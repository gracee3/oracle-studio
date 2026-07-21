use std::{fs, path::PathBuf};

use fs2::FileExt;
use oracle_studio_core::{PersonKind, PersonProfile, StableId, VaultDocument};
use oracle_studio_storage::{ExpectedState, FileVault, StorageError};

const PASSWORD: &[u8] = b"fictional test password";

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let mut random = [0_u8; 16];
        getrandom::fill(&mut random).unwrap();
        let suffix = random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let path = std::env::temp_dir().join(format!("oracle-studio-storage-test-{suffix}"));
        fs::create_dir(&path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        Self(path)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn document(name: &str) -> VaultDocument {
    let profile = PersonProfile::new(
        StableId::new("person.id", "fictional_client").unwrap(),
        name,
        PersonKind::ProfessionalClient,
        None,
    )
    .unwrap();
    VaultDocument::new(vec![profile], vec![], vec![], vec![]).unwrap()
}

#[test]
fn save_load_and_atomic_revision_updates_round_trip() {
    let directory = TestDirectory::new();
    let repository = FileVault::new(directory.join("primary.vault")).unwrap();

    let first = repository
        .save(
            &document("Fictional One"),
            PASSWORD,
            &ExpectedState::Missing,
        )
        .unwrap();
    let loaded = repository.load(PASSWORD).unwrap();
    assert_eq!(loaded.document(), &document("Fictional One"));
    assert_eq!(loaded.revision(), &first);

    let second = repository
        .save(
            &document("Fictional Two"),
            PASSWORD,
            &ExpectedState::Revision(first.clone()),
        )
        .unwrap();
    assert_ne!(first, second);
    assert_eq!(
        repository.load(PASSWORD).unwrap().document(),
        &document("Fictional Two")
    );
}

#[test]
fn stale_and_create_only_writers_report_conflicts_without_data_loss() {
    let directory = TestDirectory::new();
    let repository = FileVault::new(directory.join("primary.vault")).unwrap();
    let first = repository
        .save(&document("Original"), PASSWORD, &ExpectedState::Missing)
        .unwrap();
    let current = repository
        .save(
            &document("Current"),
            PASSWORD,
            &ExpectedState::Revision(first.clone()),
        )
        .unwrap();

    assert!(matches!(
        repository.save(
            &document("Stale"),
            PASSWORD,
            &ExpectedState::Revision(first)
        ),
        Err(StorageError::Conflict)
    ));
    assert!(matches!(
        repository.save(&document("Create"), PASSWORD, &ExpectedState::Missing),
        Err(StorageError::Conflict)
    ));
    assert_eq!(repository.load(PASSWORD).unwrap().revision(), &current);
}

#[test]
fn authenticated_backup_and_transactional_import_preserve_exact_bytes() {
    let directory = TestDirectory::new();
    let source = FileVault::new(directory.join("source.vault")).unwrap();
    let source_revision = source
        .save(
            &document("Backup Source"),
            PASSWORD,
            &ExpectedState::Missing,
        )
        .unwrap();
    let backup = directory.join("backup.vault");
    let backup_revision = source.export_backup(&backup, PASSWORD).unwrap();
    assert_eq!(backup_revision, source_revision);

    let destination = FileVault::new(directory.join("destination.vault")).unwrap();
    let imported = destination
        .import_backup(&backup, PASSWORD, &ExpectedState::Missing)
        .unwrap();
    assert_eq!(imported.revision(), &source_revision);
    assert_eq!(imported.document(), &document("Backup Source"));
    assert_eq!(
        fs::read(source.path()).unwrap(),
        fs::read(destination.path()).unwrap()
    );
}

#[test]
fn invalid_imports_and_orphan_temporary_files_do_not_replace_the_live_vault() {
    let directory = TestDirectory::new();
    let repository = FileVault::new(directory.join("primary.vault")).unwrap();
    let revision = repository
        .save(&document("Still Live"), PASSWORD, &ExpectedState::Missing)
        .unwrap();
    let invalid = directory.join("invalid.vault");
    fs::write(&invalid, b"not an authenticated vault").unwrap();

    assert!(
        repository
            .import_backup(
                &invalid,
                PASSWORD,
                &ExpectedState::Revision(revision.clone())
            )
            .is_err()
    );
    fs::write(directory.join(".primary.vault.tmp-interrupted"), b"partial").unwrap();
    let loaded = repository.load(PASSWORD).unwrap();
    assert_eq!(loaded.revision(), &revision);
    assert_eq!(loaded.document(), &document("Still Live"));
}

#[test]
fn held_writer_lock_fails_fast_as_busy() {
    let directory = TestDirectory::new();
    let repository = FileVault::new(directory.join("primary.vault")).unwrap();
    let lock_path = directory.join(".primary.vault.lock");
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)
        .unwrap();
    FileExt::try_lock_exclusive(&lock).unwrap();

    assert!(matches!(
        repository.save(&document("Blocked"), PASSWORD, &ExpectedState::Missing),
        Err(StorageError::Busy)
    ));
    FileExt::unlock(&lock).unwrap();
}

#[test]
fn deletion_is_revision_checked_and_never_claims_physical_erasure() {
    let directory = TestDirectory::new();
    let repository = FileVault::new(directory.join("primary.vault")).unwrap();
    let revision = repository
        .save(&document("Delete Me"), PASSWORD, &ExpectedState::Missing)
        .unwrap();

    let report = repository
        .delete(&ExpectedState::Revision(revision))
        .unwrap();
    assert!(report.removed_live_vault());
    assert!(!report.physical_erasure_guaranteed());
    assert!(!repository.path().exists());
}

#[cfg(unix)]
#[test]
fn unix_vault_backup_lock_and_directory_permissions_are_private() {
    use std::os::unix::fs::PermissionsExt;

    let directory = TestDirectory::new();
    let repository = FileVault::new(directory.join("primary.vault")).unwrap();
    repository
        .save(&document("Private"), PASSWORD, &ExpectedState::Missing)
        .unwrap();
    let backup = directory.join("backup.vault");
    repository.export_backup(&backup, PASSWORD).unwrap();

    assert_eq!(
        fs::metadata(&directory.0).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(repository.path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(backup).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(directory.join(".primary.vault.lock"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

#[cfg(unix)]
#[test]
fn symbolic_link_targets_are_rejected() {
    use std::os::unix::fs::symlink;

    let directory = TestDirectory::new();
    let actual = directory.join("actual.vault");
    fs::write(&actual, b"unchanged").unwrap();
    let linked = directory.join("linked.vault");
    symlink(&actual, &linked).unwrap();
    let repository = FileVault::new(&linked).unwrap();

    assert!(matches!(
        repository.save(&document("No Follow"), PASSWORD, &ExpectedState::Missing),
        Err(StorageError::Symlink)
    ));
    assert_eq!(fs::read(actual).unwrap(), b"unchanged");
}

#[cfg(unix)]
#[test]
fn overly_broad_existing_directory_permissions_are_rejected() {
    use std::os::unix::fs::PermissionsExt;

    let directory = TestDirectory::new();
    fs::set_permissions(&directory.0, fs::Permissions::from_mode(0o755)).unwrap();
    let repository = FileVault::new(directory.join("primary.vault")).unwrap();

    assert!(matches!(
        repository.load(PASSWORD),
        Err(StorageError::InsecureDirectoryPermissions)
    ));
}
