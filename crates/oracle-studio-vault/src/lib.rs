//! Portable Oracle Studio vault envelope version 2.
//!
//! A password-derived Argon2id key unwraps a random vault data key. Mounted
//! callers retain only that zeroizing data key and public wrapping material.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    KeyInit, XChaCha20Poly1305, XNonce,
    aead::{Aead, Payload},
};
use oracle_studio_core::{ModelError, VaultDocument};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;
use zeroize::Zeroizing;

const MAGIC: &[u8; 8] = b"ORCLVLT\0";
pub const FORMAT_VERSION: u16 = 2;
const KDF_ARGON2ID: u8 = 1;
const WRAP_XCHACHA20_POLY1305: u8 = 1;
const DOCUMENT_XCHACHA20_POLY1305: u8 = 1;
const MEMORY_KIB: u32 = 65_536;
const TIME_COST: u32 = 3;
const LANES: u32 = 1;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;
const KEY_LEN: usize = 32;
const TAG_LEN: usize = 16;
const WRAPPED_KEY_LEN: usize = KEY_LEN + TAG_LEN;
const FIXED_LEN: usize = 42;
const MAX_TITLE_BYTES: usize = 256;
const MAX_ID_BYTES: usize = 64;
const MAX_PASSWORD_BYTES: usize = 1024;
const MAX_CIPHERTEXT_LEN: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaultHeader {
    id: String,
    title: String,
}

impl VaultHeader {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn title(&self) -> &str {
        &self.title
    }
}

#[derive(Clone)]
struct WrappingMaterial {
    salt: [u8; SALT_LEN],
    nonce: [u8; NONCE_LEN],
    wrapped_key: [u8; WRAPPED_KEY_LEN],
}

pub struct UnlockedVault {
    header: VaultHeader,
    wrapping: WrappingMaterial,
    key: Zeroizing<[u8; KEY_LEN]>,
    document: VaultDocument,
}

impl std::fmt::Debug for UnlockedVault {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UnlockedVault")
            .field("header", &self.header)
            .field("key", &"[REDACTED]")
            .field("document", &"[DECRYPTED DOCUMENT]")
            .finish()
    }
}

impl UnlockedVault {
    pub const fn header(&self) -> &VaultHeader {
        &self.header
    }
    pub const fn document(&self) -> &VaultDocument {
        &self.document
    }

    pub fn replace_document(&mut self, document: VaultDocument) -> Result<(), VaultError> {
        document.validate()?;
        self.document = document;
        Ok(())
    }

    pub fn seal(&self) -> Result<Vec<u8>, VaultError> {
        let mut nonce = [0_u8; NONCE_LEN];
        getrandom::fill(&mut nonce)?;
        seal_document_with_nonce(
            &self.header,
            &self.wrapping,
            &self.key,
            &self.document,
            nonce,
        )
    }

    pub fn seal_document(&self, document: &VaultDocument) -> Result<Vec<u8>, VaultError> {
        let mut nonce = [0_u8; NONCE_LEN];
        getrandom::fill(&mut nonce)?;
        seal_document_with_nonce(&self.header, &self.wrapping, &self.key, document, nonce)
    }
}

pub fn create(
    title: &str,
    password: &[u8],
    document: VaultDocument,
) -> Result<(UnlockedVault, Vec<u8>), VaultError> {
    validate_title(title)?;
    validate_password(password)?;
    document.validate()?;
    let header = VaultHeader {
        id: Uuid::new_v4().to_string(),
        title: title.into(),
    };
    let mut salt = [0_u8; SALT_LEN];
    let mut wrap_nonce = [0_u8; NONCE_LEN];
    let mut document_nonce = [0_u8; NONCE_LEN];
    let mut key = Zeroizing::new([0_u8; KEY_LEN]);
    getrandom::fill(&mut salt)?;
    getrandom::fill(&mut wrap_nonce)?;
    getrandom::fill(&mut document_nonce)?;
    getrandom::fill(key.as_mut())?;
    create_with_material(
        header,
        password,
        document,
        salt,
        wrap_nonce,
        document_nonce,
        key,
    )
}

pub fn open(envelope: &[u8], password: &[u8]) -> Result<UnlockedVault, VaultError> {
    validate_password(password)?;
    let parsed = parse(envelope)?;
    let kek = derive_key(password, &parsed.wrapping.salt)?;
    let wrap_aad = wrap_aad(
        &parsed.header,
        &parsed.wrapping.salt,
        &parsed.wrapping.nonce,
    )?;
    let cipher =
        XChaCha20Poly1305::new_from_slice(&kek[..]).map_err(|_| VaultError::CryptoSetup)?;
    let key_bytes = Zeroizing::new(
        cipher
            .decrypt(
                XNonce::from_slice(&parsed.wrapping.nonce),
                Payload {
                    msg: &parsed.wrapping.wrapped_key,
                    aad: &wrap_aad,
                },
            )
            .map_err(|_| VaultError::Authentication)?,
    );
    let key: [u8; KEY_LEN] = key_bytes
        .as_slice()
        .try_into()
        .map_err(|_| VaultError::Authentication)?;
    let key = Zeroizing::new(key);
    let document = decrypt_document(
        &parsed.header,
        &parsed.wrapping,
        &key,
        &parsed.document_nonce,
        parsed.ciphertext,
    )?;
    Ok(UnlockedVault {
        header: parsed.header,
        wrapping: parsed.wrapping,
        key,
        document,
    })
}

pub fn inspect(envelope: &[u8]) -> Result<VaultHeader, VaultError> {
    Ok(parse(envelope)?.header)
}

pub fn revision(envelope: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(envelope))
}

fn create_with_material(
    header: VaultHeader,
    password: &[u8],
    document: VaultDocument,
    salt: [u8; SALT_LEN],
    wrap_nonce: [u8; NONCE_LEN],
    document_nonce: [u8; NONCE_LEN],
    key: Zeroizing<[u8; KEY_LEN]>,
) -> Result<(UnlockedVault, Vec<u8>), VaultError> {
    validate_header(&header)?;
    validate_password(password)?;
    document.validate()?;
    let kek = derive_key(password, &salt)?;
    let aad = wrap_aad(&header, &salt, &wrap_nonce)?;
    let cipher =
        XChaCha20Poly1305::new_from_slice(&kek[..]).map_err(|_| VaultError::CryptoSetup)?;
    let wrapped = cipher
        .encrypt(
            XNonce::from_slice(&wrap_nonce),
            Payload {
                msg: &key[..],
                aad: &aad,
            },
        )
        .map_err(|_| VaultError::Encryption)?;
    let wrapped_key: [u8; WRAPPED_KEY_LEN] =
        wrapped.try_into().map_err(|_| VaultError::Encryption)?;
    let wrapping = WrappingMaterial {
        salt,
        nonce: wrap_nonce,
        wrapped_key,
    };
    let envelope = seal_document_with_nonce(&header, &wrapping, &key, &document, document_nonce)?;
    Ok((
        UnlockedVault {
            header,
            wrapping,
            key,
            document,
        },
        envelope,
    ))
}

fn seal_document_with_nonce(
    header: &VaultHeader,
    wrapping: &WrappingMaterial,
    key: &[u8; KEY_LEN],
    document: &VaultDocument,
    document_nonce: [u8; NONCE_LEN],
) -> Result<Vec<u8>, VaultError> {
    validate_header(header)?;
    document.validate()?;
    let plaintext = Zeroizing::new(document.to_json()?.into_bytes());
    if plaintext.len() + TAG_LEN > MAX_CIPHERTEXT_LEN {
        return Err(VaultError::TooLarge);
    }
    let aad = document_aad(header, wrapping)?;
    let cipher = XChaCha20Poly1305::new_from_slice(key).map_err(|_| VaultError::CryptoSetup)?;
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&document_nonce),
            Payload {
                msg: &plaintext,
                aad: &aad,
            },
        )
        .map_err(|_| VaultError::Encryption)?;
    encode(header, wrapping, &document_nonce, &ciphertext)
}

fn decrypt_document(
    header: &VaultHeader,
    wrapping: &WrappingMaterial,
    key: &[u8; KEY_LEN],
    document_nonce: &[u8; NONCE_LEN],
    ciphertext: &[u8],
) -> Result<VaultDocument, VaultError> {
    let aad = document_aad(header, wrapping)?;
    let cipher = XChaCha20Poly1305::new_from_slice(key).map_err(|_| VaultError::CryptoSetup)?;
    let plaintext = Zeroizing::new(
        cipher
            .decrypt(
                XNonce::from_slice(document_nonce),
                Payload {
                    msg: ciphertext,
                    aad: &aad,
                },
            )
            .map_err(|_| VaultError::Authentication)?,
    );
    let json = std::str::from_utf8(&plaintext).map_err(|_| VaultError::InvalidPlaintext)?;
    VaultDocument::from_json(json).map_err(VaultError::InvalidDocument)
}

fn wrap_aad(
    header: &VaultHeader,
    salt: &[u8; SALT_LEN],
    nonce: &[u8; NONCE_LEN],
) -> Result<Vec<u8>, VaultError> {
    validate_header(header)?;
    let mut aad = Vec::with_capacity(64 + header.id.len() + header.title.len());
    aad.extend_from_slice(MAGIC);
    aad.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    aad.extend_from_slice(&[
        KDF_ARGON2ID,
        WRAP_XCHACHA20_POLY1305,
        DOCUMENT_XCHACHA20_POLY1305,
    ]);
    aad.extend_from_slice(&MEMORY_KIB.to_le_bytes());
    aad.extend_from_slice(&TIME_COST.to_le_bytes());
    aad.extend_from_slice(&LANES.to_le_bytes());
    aad.extend_from_slice(&(header.id.len() as u16).to_le_bytes());
    aad.extend_from_slice(header.id.as_bytes());
    aad.extend_from_slice(&(header.title.len() as u16).to_le_bytes());
    aad.extend_from_slice(header.title.as_bytes());
    aad.extend_from_slice(salt);
    aad.extend_from_slice(nonce);
    Ok(aad)
}

fn document_aad(header: &VaultHeader, wrapping: &WrappingMaterial) -> Result<Vec<u8>, VaultError> {
    let mut aad = wrap_aad(header, &wrapping.salt, &wrapping.nonce)?;
    aad.extend_from_slice(&wrapping.wrapped_key);
    Ok(aad)
}

fn encode(
    header: &VaultHeader,
    wrapping: &WrappingMaterial,
    document_nonce: &[u8; NONCE_LEN],
    ciphertext: &[u8],
) -> Result<Vec<u8>, VaultError> {
    let ciphertext_len = u32::try_from(ciphertext.len()).map_err(|_| VaultError::TooLarge)?;
    let mut output = Vec::with_capacity(
        FIXED_LEN
            + header.id.len()
            + header.title.len()
            + SALT_LEN
            + NONCE_LEN
            + WRAPPED_KEY_LEN
            + NONCE_LEN
            + ciphertext.len(),
    );
    output.extend_from_slice(MAGIC);
    output.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    output.extend_from_slice(&[
        KDF_ARGON2ID,
        WRAP_XCHACHA20_POLY1305,
        DOCUMENT_XCHACHA20_POLY1305,
        0,
    ]);
    output.extend_from_slice(&MEMORY_KIB.to_le_bytes());
    output.extend_from_slice(&TIME_COST.to_le_bytes());
    output.extend_from_slice(&LANES.to_le_bytes());
    for length in [
        header.id.len(),
        header.title.len(),
        SALT_LEN,
        NONCE_LEN,
        WRAPPED_KEY_LEN,
        NONCE_LEN,
    ] {
        output.extend_from_slice(&(length as u16).to_le_bytes());
    }
    output.extend_from_slice(&ciphertext_len.to_le_bytes());
    output.extend_from_slice(header.id.as_bytes());
    output.extend_from_slice(header.title.as_bytes());
    output.extend_from_slice(&wrapping.salt);
    output.extend_from_slice(&wrapping.nonce);
    output.extend_from_slice(&wrapping.wrapped_key);
    output.extend_from_slice(document_nonce);
    output.extend_from_slice(ciphertext);
    Ok(output)
}

struct ParsedEnvelope<'a> {
    header: VaultHeader,
    wrapping: WrappingMaterial,
    document_nonce: [u8; NONCE_LEN],
    ciphertext: &'a [u8],
}

fn parse(envelope: &[u8]) -> Result<ParsedEnvelope<'_>, VaultError> {
    if envelope.len() < FIXED_LEN {
        return Err(VaultError::Truncated);
    }
    if &envelope[..8] != MAGIC {
        return Err(VaultError::InvalidMagic);
    }
    let version = read_u16(envelope, 8)?;
    if version != FORMAT_VERSION {
        return Err(VaultError::UnsupportedVersion(version));
    }
    if envelope[10..14]
        != [
            KDF_ARGON2ID,
            WRAP_XCHACHA20_POLY1305,
            DOCUMENT_XCHACHA20_POLY1305,
            0,
        ]
    {
        return Err(VaultError::UnsupportedAlgorithms);
    }
    if (
        read_u32(envelope, 14)?,
        read_u32(envelope, 18)?,
        read_u32(envelope, 22)?,
    ) != (MEMORY_KIB, TIME_COST, LANES)
    {
        return Err(VaultError::UnsupportedKdfParameters);
    }
    let lengths = [
        read_u16(envelope, 26)? as usize,
        read_u16(envelope, 28)? as usize,
        read_u16(envelope, 30)? as usize,
        read_u16(envelope, 32)? as usize,
        read_u16(envelope, 34)? as usize,
        read_u16(envelope, 36)? as usize,
    ];
    let ciphertext_len = read_u32(envelope, 38)? as usize;
    if lengths[0] == 0
        || lengths[0] > MAX_ID_BYTES
        || lengths[1] == 0
        || lengths[1] > MAX_TITLE_BYTES
        || lengths[2..] != [SALT_LEN, NONCE_LEN, WRAPPED_KEY_LEN, NONCE_LEN]
        || !(TAG_LEN..=MAX_CIPHERTEXT_LEN).contains(&ciphertext_len)
    {
        return Err(VaultError::InvalidLengths);
    }
    let expected = FIXED_LEN
        .checked_add(lengths.iter().sum::<usize>())
        .and_then(|value| value.checked_add(ciphertext_len))
        .ok_or(VaultError::TooLarge)?;
    if envelope.len() != expected {
        return Err(VaultError::InvalidLengths);
    }
    let mut cursor = FIXED_LEN;
    let id = read_utf8(envelope, &mut cursor, lengths[0])?;
    let title = read_utf8(envelope, &mut cursor, lengths[1])?;
    let header = VaultHeader { id, title };
    validate_header(&header)?;
    let salt = take_array(envelope, &mut cursor)?;
    let nonce = take_array(envelope, &mut cursor)?;
    let wrapped_key = take_array(envelope, &mut cursor)?;
    let document_nonce = take_array(envelope, &mut cursor)?;
    let ciphertext = &envelope[cursor..];
    Ok(ParsedEnvelope {
        header,
        wrapping: WrappingMaterial {
            salt,
            nonce,
            wrapped_key,
        },
        document_nonce,
        ciphertext,
    })
}

fn derive_key(
    password: &[u8],
    salt: &[u8; SALT_LEN],
) -> Result<Zeroizing<[u8; KEY_LEN]>, VaultError> {
    let params = Params::new(MEMORY_KIB, TIME_COST, LANES, Some(KEY_LEN))
        .map_err(|_| VaultError::CryptoSetup)?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = Zeroizing::new([0_u8; KEY_LEN]);
    argon2
        .hash_password_into(password, salt, key.as_mut())
        .map_err(|_| VaultError::KeyDerivation)?;
    Ok(key)
}

fn validate_header(header: &VaultHeader) -> Result<(), VaultError> {
    if header.id.len() > MAX_ID_BYTES || Uuid::parse_str(&header.id).is_err() {
        return Err(VaultError::InvalidVaultId);
    }
    validate_title(&header.title)
}

fn validate_title(title: &str) -> Result<(), VaultError> {
    if title.trim().is_empty() {
        Err(VaultError::EmptyTitle)
    } else if title.len() > MAX_TITLE_BYTES {
        Err(VaultError::TitleTooLong)
    } else {
        Ok(())
    }
}

fn validate_password(password: &[u8]) -> Result<(), VaultError> {
    if password.is_empty() {
        Err(VaultError::EmptyPassword)
    } else if password.len() > MAX_PASSWORD_BYTES {
        Err(VaultError::PasswordTooLong)
    } else {
        Ok(())
    }
}

fn read_utf8(input: &[u8], cursor: &mut usize, length: usize) -> Result<String, VaultError> {
    let end = cursor.checked_add(length).ok_or(VaultError::Truncated)?;
    let value = std::str::from_utf8(input.get(*cursor..end).ok_or(VaultError::Truncated)?)
        .map_err(|_| VaultError::InvalidHeaderUtf8)?;
    *cursor = end;
    Ok(value.into())
}

fn take_array<const N: usize>(input: &[u8], cursor: &mut usize) -> Result<[u8; N], VaultError> {
    let end = cursor.checked_add(N).ok_or(VaultError::Truncated)?;
    let value = input
        .get(*cursor..end)
        .ok_or(VaultError::Truncated)?
        .try_into()
        .map_err(|_| VaultError::Truncated)?;
    *cursor = end;
    Ok(value)
}

fn read_u16(input: &[u8], offset: usize) -> Result<u16, VaultError> {
    Ok(u16::from_le_bytes(
        input
            .get(offset..offset + 2)
            .ok_or(VaultError::Truncated)?
            .try_into()
            .map_err(|_| VaultError::Truncated)?,
    ))
}

fn read_u32(input: &[u8], offset: usize) -> Result<u32, VaultError> {
    Ok(u32::from_le_bytes(
        input
            .get(offset..offset + 4)
            .ok_or(VaultError::Truncated)?
            .try_into()
            .map_err(|_| VaultError::Truncated)?,
    ))
}

#[derive(Debug, Error)]
pub enum VaultError {
    #[error("vault password must not be empty")]
    EmptyPassword,
    #[error("vault password exceeds its size bound")]
    PasswordTooLong,
    #[error("vault title must not be blank")]
    EmptyTitle,
    #[error("vault title exceeds its UTF-8 size bound")]
    TitleTooLong,
    #[error("vault ID is invalid")]
    InvalidVaultId,
    #[error("vault envelope is truncated")]
    Truncated,
    #[error("vault envelope has invalid magic")]
    InvalidMagic,
    #[error("unsupported vault envelope version {0}")]
    UnsupportedVersion(u16),
    #[error("unsupported vault algorithm identifiers")]
    UnsupportedAlgorithms,
    #[error("unsupported vault KDF parameters")]
    UnsupportedKdfParameters,
    #[error("vault header lengths are invalid")]
    InvalidLengths,
    #[error("vault header text is not UTF-8")]
    InvalidHeaderUtf8,
    #[error("vault document exceeds the configured size bound")]
    TooLarge,
    #[error("vault authentication failed")]
    Authentication,
    #[error("vault encryption failed")]
    Encryption,
    #[error("vault cryptographic setup failed")]
    CryptoSetup,
    #[error("vault key derivation failed")]
    KeyDerivation,
    #[error("decrypted vault is not UTF-8")]
    InvalidPlaintext,
    #[error("decrypted vault document is invalid: {0}")]
    InvalidDocument(ModelError),
    #[error("browser randomness failed: {0}")]
    Randomness(String),
    #[error("vault document serialization failed: {0}")]
    Document(#[from] ModelError),
}

impl From<getrandom::Error> for VaultError {
    fn from(error: getrandom::Error) -> Self {
        Self::Randomness(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> VaultHeader {
        VaultHeader {
            id: "550e8400-e29b-41d4-a716-446655440000".into(),
            title: "Fictional studio".into(),
        }
    }

    fn deterministic() -> (UnlockedVault, Vec<u8>) {
        create_with_material(
            header(),
            b"correct horse battery staple",
            VaultDocument::empty(),
            [1; SALT_LEN],
            [2; NONCE_LEN],
            [3; NONCE_LEN],
            Zeroizing::new([4; KEY_LEN]),
        )
        .unwrap()
    }

    #[test]
    fn vector_round_trip_wraps_a_random_data_key() {
        let (_, first) = deterministic();
        let (_, second) = deterministic();
        assert_eq!(first, second);
        assert_eq!(
            revision(&first),
            "sha256:85f7ecfebba87687ec34c60a08216cf543ab530c38473032c487c9eecd2c9575"
        );
        let opened = open(&first, b"correct horse battery staple").unwrap();
        assert_eq!(opened.header(), &header());
        assert_eq!(opened.document(), &VaultDocument::empty());
        assert!(!String::from_utf8_lossy(&first).contains("schema_version"));
    }

    #[test]
    fn wrong_password_tampering_and_title_changes_fail_authentication() {
        let (_, envelope) = deterministic();
        assert!(matches!(
            open(&envelope, b"wrong"),
            Err(VaultError::Authentication)
        ));
        let mut ciphertext = envelope.clone();
        *ciphertext.last_mut().unwrap() ^= 1;
        assert!(matches!(
            open(&ciphertext, b"correct horse battery staple"),
            Err(VaultError::Authentication)
        ));
        let mut title = envelope;
        let title_offset = FIXED_LEN + header().id.len();
        title[title_offset] ^= 1;
        assert!(matches!(
            open(&title, b"correct horse battery staple"),
            Err(VaultError::Authentication)
        ));
    }

    #[test]
    fn every_save_uses_a_fresh_document_nonce_without_rewrapping_the_key() {
        let (mounted, first) = deterministic();
        let second = mounted.seal().unwrap();
        let third = mounted.seal().unwrap();
        assert_ne!(first, second);
        assert_ne!(second, third);
        let first = parse(&first).unwrap();
        let second = parse(&second).unwrap();
        assert_eq!(first.wrapping.wrapped_key, second.wrapping.wrapped_key);
        assert_ne!(first.document_nonce, second.document_nonce);
    }

    #[test]
    fn old_versions_algorithms_and_bounds_are_rejected() {
        let (_, envelope) = deterministic();
        let mut v1 = envelope.clone();
        v1[8..10].copy_from_slice(&1_u16.to_le_bytes());
        assert!(matches!(
            inspect(&v1),
            Err(VaultError::UnsupportedVersion(1))
        ));
        let mut algorithm = envelope.clone();
        algorithm[10] = 9;
        assert!(matches!(
            inspect(&algorithm),
            Err(VaultError::UnsupportedAlgorithms)
        ));
        let mut oversized_title = envelope;
        oversized_title[28..30].copy_from_slice(&((MAX_TITLE_BYTES + 1) as u16).to_le_bytes());
        assert!(matches!(
            inspect(&oversized_title),
            Err(VaultError::InvalidLengths)
        ));
    }

    #[test]
    fn old_document_schemas_are_rejected_even_when_authenticated() {
        let (mounted, _) = deterministic();
        let mut plaintext = VaultDocument::empty().to_json().unwrap();
        plaintext = plaintext.replacen("\"schema_version\":4", "\"schema_version\":3", 1);
        let aad = document_aad(&mounted.header, &mounted.wrapping).unwrap();
        let cipher = XChaCha20Poly1305::new_from_slice(&mounted.key[..]).unwrap();
        let nonce = [8; NONCE_LEN];
        let encrypted = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: plaintext.as_bytes(),
                    aad: &aad,
                },
            )
            .unwrap();
        let envelope = encode(&mounted.header, &mounted.wrapping, &nonce, &encrypted).unwrap();
        assert!(matches!(
            open(&envelope, b"correct horse battery staple"),
            Err(VaultError::InvalidDocument(ModelError::UnsupportedSchema(
                3
            )))
        ));
    }
}
