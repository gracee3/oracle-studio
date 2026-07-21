//! Durable filesystem persistence for authenticated Oracle Studio vault bytes.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use fs2::FileExt;
use oracle_studio_core::VaultDocument;
use sha2::{Digest, Sha256};
use thiserror::Error;

const MAX_ENVELOPE_BYTES: u64 = 64 * 1024 * 1024 + 128;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaultRevision(String);

impl VaultRevision {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn from_bytes(bytes: &[u8]) -> Self {
        Self(format!("sha256:{:x}", Sha256::digest(bytes)))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpectedState {
    Missing,
    Revision(VaultRevision),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedVault {
    document: VaultDocument,
    revision: VaultRevision,
}

impl LoadedVault {
    pub fn document(&self) -> &VaultDocument {
        &self.document
    }

    pub fn into_document(self) -> VaultDocument {
        self.document
    }

    pub fn revision(&self) -> &VaultRevision {
        &self.revision
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeletionReport {
    removed_live_vault: bool,
    physical_erasure_guaranteed: bool,
}

impl DeletionReport {
    pub const fn removed_live_vault(&self) -> bool {
        self.removed_live_vault
    }

    pub const fn physical_erasure_guaranteed(&self) -> bool {
        self.physical_erasure_guaranteed
    }
}

#[derive(Clone, Debug)]
pub struct FileVault {
    path: PathBuf,
    lock_path: PathBuf,
}

impl FileVault {
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, StorageError> {
        let path = path.into();
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty() && *name != "." && *name != "..")
            .ok_or(StorageError::InvalidPath)?;
        let parent = path.parent().filter(|path| !path.as_os_str().is_empty());
        if parent.is_none() {
            return Err(StorageError::InvalidPath);
        }
        let lock_path = path.with_file_name(format!(".{file_name}.lock"));
        Ok(Self { path, lock_path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self, password: &[u8]) -> Result<LoadedVault, StorageError> {
        let lock = self.acquire_lock(false)?;
        let bytes = read_bounded(&self.path)?;
        let document = oracle_studio_vault::open(&bytes, password)?;
        let loaded = LoadedVault {
            revision: VaultRevision::from_bytes(&bytes),
            document,
        };
        FileExt::unlock(&lock)?;
        Ok(loaded)
    }

    pub fn save(
        &self,
        document: &VaultDocument,
        password: &[u8],
        expected: &ExpectedState,
    ) -> Result<VaultRevision, StorageError> {
        let envelope = oracle_studio_vault::seal(document, password)?;
        self.write_authenticated(envelope, expected)
    }

    pub fn export_backup(
        &self,
        destination: impl AsRef<Path>,
        password: &[u8],
    ) -> Result<VaultRevision, StorageError> {
        let destination = destination.as_ref();
        if destination == self.path || destination == self.lock_path {
            return Err(StorageError::InvalidPath);
        }
        let lock = self.acquire_lock(false)?;
        let bytes = read_bounded(&self.path)?;
        oracle_studio_vault::open(&bytes, password)?;
        atomic_replace(destination, &bytes)?;
        let revision = VaultRevision::from_bytes(&bytes);
        FileExt::unlock(&lock)?;
        Ok(revision)
    }

    pub fn import_backup(
        &self,
        source: impl AsRef<Path>,
        password: &[u8],
        expected: &ExpectedState,
    ) -> Result<LoadedVault, StorageError> {
        let bytes = read_bounded(source.as_ref())?;
        let document = oracle_studio_vault::open(&bytes, password)?;
        let revision = self.write_authenticated(bytes, expected)?;
        Ok(LoadedVault { document, revision })
    }

    pub fn delete(&self, expected: &ExpectedState) -> Result<DeletionReport, StorageError> {
        let lock = self.acquire_lock(true)?;
        verify_expected(&self.path, expected)?;
        let removed_live_vault = match fs::remove_file(&self.path) {
            Ok(()) => true,
            Err(error) if error.kind() == io::ErrorKind::NotFound => false,
            Err(error) => return Err(error.into()),
        };
        sync_parent(&self.path)?;
        FileExt::unlock(&lock)?;
        Ok(DeletionReport {
            removed_live_vault,
            physical_erasure_guaranteed: false,
        })
    }

    fn write_authenticated(
        &self,
        envelope: Vec<u8>,
        expected: &ExpectedState,
    ) -> Result<VaultRevision, StorageError> {
        let lock = self.acquire_lock(true)?;
        verify_expected(&self.path, expected)?;
        atomic_replace(&self.path, &envelope)?;
        let revision = VaultRevision::from_bytes(&envelope);
        FileExt::unlock(&lock)?;
        Ok(revision)
    }

    fn acquire_lock(&self, exclusive: bool) -> Result<File, StorageError> {
        ensure_parent(&self.path)?;
        reject_symlink(&self.lock_path)?;
        let lock = private_open(&self.lock_path, false)?;
        let result = if exclusive {
            FileExt::try_lock_exclusive(&lock)
        } else {
            FileExt::try_lock_shared(&lock)
        };
        match result {
            Ok(()) => Ok(lock),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Err(StorageError::Busy),
            Err(error) => Err(error.into()),
        }
    }
}

fn verify_expected(path: &Path, expected: &ExpectedState) -> Result<(), StorageError> {
    match (expected, read_optional(path)?) {
        (ExpectedState::Missing, None) => Ok(()),
        (ExpectedState::Missing, Some(_)) => Err(StorageError::Conflict),
        (ExpectedState::Revision(_), None) => Err(StorageError::Conflict),
        (ExpectedState::Revision(expected), Some(bytes))
            if VaultRevision::from_bytes(&bytes) == *expected =>
        {
            Ok(())
        }
        (ExpectedState::Revision(_), Some(_)) => Err(StorageError::Conflict),
    }
}

fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<(), StorageError> {
    ensure_parent(path)?;
    reject_symlink(path)?;
    let parent = path.parent().ok_or(StorageError::InvalidPath)?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(StorageError::InvalidPath)?;
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random).map_err(|error| StorageError::Randomness(error.to_string()))?;
    let suffix = random
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let temporary = parent.join(format!(".{name}.tmp-{suffix}"));
    let result = (|| {
        let mut file = private_open(&temporary, true)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        set_private_permissions(path)?;
        sync_parent(path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, StorageError> {
    match read_bounded(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(StorageError::Io(error)) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, StorageError> {
    reject_symlink(path)?;
    let file = File::open(path)?;
    if file.metadata()?.len() > MAX_ENVELOPE_BYTES {
        return Err(StorageError::TooLarge);
    }
    let mut bytes = Vec::new();
    file.take(MAX_ENVELOPE_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_ENVELOPE_BYTES {
        return Err(StorageError::TooLarge);
    }
    Ok(bytes)
}

fn ensure_parent(path: &Path) -> Result<(), StorageError> {
    let parent = path.parent().ok_or(StorageError::InvalidPath)?;
    reject_symlink(parent)?;
    match fs::create_dir(parent) {
        Ok(()) => set_private_directory_permissions(parent),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            validate_private_directory_permissions(parent)
        }
        Err(error) => Err(error.into()),
    }
}

fn reject_symlink(path: &Path) -> Result<(), StorageError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(StorageError::Symlink),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn private_open(path: &Path, create_new: bool) -> Result<File, StorageError> {
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    if create_new {
        options.create_new(true);
    } else {
        options.create(true);
    }
    set_private_open_mode(&mut options);
    let file = options.open(path)?;
    set_private_permissions(path)?;
    Ok(file)
}

#[cfg(unix)]
fn set_private_open_mode(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
}

#[cfg(not(unix))]
fn set_private_open_mode(_options: &mut OpenOptions) {}

#[cfg(unix)]
fn set_private_permissions(path: &Path) -> Result<(), StorageError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_permissions(_path: &Path) -> Result<(), StorageError> {
    Ok(())
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) -> Result<(), StorageError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_directory_permissions(_path: &Path) -> Result<(), StorageError> {
    Ok(())
}

#[cfg(unix)]
fn validate_private_directory_permissions(path: &Path) -> Result<(), StorageError> {
    use std::os::unix::fs::PermissionsExt;
    if fs::metadata(path)?.permissions().mode() & 0o077 == 0 {
        Ok(())
    } else {
        Err(StorageError::InsecureDirectoryPermissions)
    }
}

#[cfg(not(unix))]
fn validate_private_directory_permissions(_path: &Path) -> Result<(), StorageError> {
    Ok(())
}

fn sync_parent(path: &Path) -> Result<(), StorageError> {
    let parent = path.parent().ok_or(StorageError::InvalidPath)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("invalid vault path")]
    InvalidPath,
    #[error("vault operation conflicts with the current revision")]
    Conflict,
    #[error("vault is busy in another reader or writer")]
    Busy,
    #[error("symbolic links are not accepted for vault storage paths")]
    Symlink,
    #[error("vault directory permissions allow access outside the owner")]
    InsecureDirectoryPermissions,
    #[error("vault file exceeds the configured size bound")]
    TooLarge,
    #[error("operating-system randomness failed: {0}")]
    Randomness(String),
    #[error("vault envelope failed authentication or validation: {0}")]
    Vault(#[from] oracle_studio_vault::VaultError),
    #[error("filesystem operation failed: {0}")]
    Io(#[from] io::Error),
}
