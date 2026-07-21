//! Password-encrypted, authenticated bytes for an Oracle Studio vault document.
//!
//! Filesystem durability and key-recovery policy are intentionally separate
//! from this cryptographic envelope.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    KeyInit, XChaCha20Poly1305, XNonce,
    aead::{Aead, Payload},
};
use oracle_studio_core::{ModelError, VaultDocument};
use thiserror::Error;
use zeroize::Zeroizing;

const MAGIC: &[u8; 8] = b"ORCLVLT\0";
const FORMAT_VERSION: u16 = 1;
const MEMORY_KIB: u32 = 65_536;
const TIME_COST: u32 = 3;
const LANES: u32 = 1;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;
const HEADER_LEN: usize = 8 + 2 + 4 + 4 + 4 + SALT_LEN + NONCE_LEN;
const TAG_LEN: usize = 16;
const MAX_CIPHERTEXT_LEN: usize = 64 * 1024 * 1024;

/// Encrypts a validated document using fresh operating-system randomness.
pub fn seal(document: &VaultDocument, password: &[u8]) -> Result<Vec<u8>, VaultError> {
    validate_password(password)?;
    let mut salt = [0_u8; SALT_LEN];
    let mut nonce = [0_u8; NONCE_LEN];
    getrandom::fill(&mut salt)?;
    getrandom::fill(&mut nonce)?;
    seal_with_material(document, password, salt, nonce)
}

/// Authenticates, decrypts, and validates a vault document.
pub fn open(envelope: &[u8], password: &[u8]) -> Result<VaultDocument, VaultError> {
    validate_password(password)?;
    if envelope.len() < HEADER_LEN + TAG_LEN {
        return Err(VaultError::Truncated);
    }
    if &envelope[..MAGIC.len()] != MAGIC {
        return Err(VaultError::InvalidMagic);
    }
    let version = read_u16(envelope, 8)?;
    if version != FORMAT_VERSION {
        return Err(VaultError::UnsupportedVersion(version));
    }
    let memory_kib = read_u32(envelope, 10)?;
    let time_cost = read_u32(envelope, 14)?;
    let lanes = read_u32(envelope, 18)?;
    if (memory_kib, time_cost, lanes) != (MEMORY_KIB, TIME_COST, LANES) {
        return Err(VaultError::UnsupportedKdfParameters);
    }
    let ciphertext = &envelope[HEADER_LEN..];
    if ciphertext.len() > MAX_CIPHERTEXT_LEN {
        return Err(VaultError::TooLarge);
    }
    let salt: [u8; SALT_LEN] = envelope[22..22 + SALT_LEN]
        .try_into()
        .map_err(|_| VaultError::Truncated)?;
    let nonce: [u8; NONCE_LEN] = envelope[22 + SALT_LEN..HEADER_LEN]
        .try_into()
        .map_err(|_| VaultError::Truncated)?;
    let key = derive_key(password, &salt)?;
    let cipher =
        XChaCha20Poly1305::new_from_slice(&key[..]).map_err(|_| VaultError::CryptoSetup)?;
    let plaintext = Zeroizing::new(
        cipher
            .decrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: ciphertext,
                    aad: &envelope[..HEADER_LEN],
                },
            )
            .map_err(|_| VaultError::Authentication)?,
    );
    let json = std::str::from_utf8(&plaintext).map_err(|_| VaultError::InvalidPlaintext)?;
    VaultDocument::from_json(json).map_err(VaultError::InvalidDocument)
}

fn seal_with_material(
    document: &VaultDocument,
    password: &[u8],
    salt: [u8; SALT_LEN],
    nonce: [u8; NONCE_LEN],
) -> Result<Vec<u8>, VaultError> {
    let plaintext = Zeroizing::new(document.to_json()?.into_bytes());
    seal_plaintext_with_material(&plaintext, password, salt, nonce)
}

fn seal_plaintext_with_material(
    plaintext: &[u8],
    password: &[u8],
    salt: [u8; SALT_LEN],
    nonce: [u8; NONCE_LEN],
) -> Result<Vec<u8>, VaultError> {
    if plaintext.len() + TAG_LEN > MAX_CIPHERTEXT_LEN {
        return Err(VaultError::TooLarge);
    }
    let header = header(&salt, &nonce);
    let key = derive_key(password, &salt)?;
    let cipher =
        XChaCha20Poly1305::new_from_slice(&key[..]).map_err(|_| VaultError::CryptoSetup)?;
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: &header,
            },
        )
        .map_err(|_| VaultError::Encryption)?;
    let mut envelope = Vec::with_capacity(HEADER_LEN + ciphertext.len());
    envelope.extend_from_slice(&header);
    envelope.extend_from_slice(&ciphertext);
    Ok(envelope)
}

fn derive_key(password: &[u8], salt: &[u8; SALT_LEN]) -> Result<Zeroizing<[u8; 32]>, VaultError> {
    let params =
        Params::new(MEMORY_KIB, TIME_COST, LANES, Some(32)).map_err(|_| VaultError::CryptoSetup)?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = Zeroizing::new([0_u8; 32]);
    argon2
        .hash_password_into(password, salt, key.as_mut())
        .map_err(|_| VaultError::KeyDerivation)?;
    Ok(key)
}

fn header(salt: &[u8; SALT_LEN], nonce: &[u8; NONCE_LEN]) -> [u8; HEADER_LEN] {
    let mut header = [0_u8; HEADER_LEN];
    header[..8].copy_from_slice(MAGIC);
    header[8..10].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
    header[10..14].copy_from_slice(&MEMORY_KIB.to_le_bytes());
    header[14..18].copy_from_slice(&TIME_COST.to_le_bytes());
    header[18..22].copy_from_slice(&LANES.to_le_bytes());
    header[22..22 + SALT_LEN].copy_from_slice(salt);
    header[22 + SALT_LEN..].copy_from_slice(nonce);
    header
}

fn validate_password(password: &[u8]) -> Result<(), VaultError> {
    if password.is_empty() {
        Err(VaultError::EmptyPassword)
    } else {
        Ok(())
    }
}

fn read_u16(input: &[u8], offset: usize) -> Result<u16, VaultError> {
    let bytes = input
        .get(offset..offset + 2)
        .ok_or(VaultError::Truncated)?
        .try_into()
        .map_err(|_| VaultError::Truncated)?;
    Ok(u16::from_le_bytes(bytes))
}

fn read_u32(input: &[u8], offset: usize) -> Result<u32, VaultError> {
    let bytes = input
        .get(offset..offset + 4)
        .ok_or(VaultError::Truncated)?
        .try_into()
        .map_err(|_| VaultError::Truncated)?;
    Ok(u32::from_le_bytes(bytes))
}

#[derive(Debug, Error)]
pub enum VaultError {
    #[error("vault password must not be empty")]
    EmptyPassword,
    #[error("vault envelope is truncated")]
    Truncated,
    #[error("vault envelope has invalid magic")]
    InvalidMagic,
    #[error("unsupported vault envelope version {0}")]
    UnsupportedVersion(u16),
    #[error("unsupported vault KDF parameters")]
    UnsupportedKdfParameters,
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
    #[error("operating-system randomness failed: {0}")]
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
    use oracle_studio_core::{PersonKind, PersonProfile, Session, StableId, VaultDocument};

    use super::*;

    fn document() -> VaultDocument {
        let person_id = StableId::new("person.id", "fictional_client").unwrap();
        let person = PersonProfile::new(
            person_id.clone(),
            "Fictional Client",
            PersonKind::ProfessionalClient,
            Some("Original test-only data.".into()),
        )
        .unwrap();
        let session = Session::new(
            StableId::new("session.id", "fictional_session").unwrap(),
            Some(person_id),
            "Fictional Session",
            None,
            "2026-07-21T14:00:00Z",
            "2026-07-21T14:00:00Z",
        )
        .unwrap();
        VaultDocument::new(vec![person], vec![session], vec![], vec![]).unwrap()
    }

    #[test]
    fn fixed_material_round_trip_is_reproducible() {
        let first = seal_with_material(&document(), b"test password", [1; 16], [2; 24]).unwrap();
        let second = seal_with_material(&document(), b"test password", [1; 16], [2; 24]).unwrap();
        assert_eq!(first, second);
        assert_eq!(open(&first, b"test password").unwrap(), document());
        assert!(!String::from_utf8_lossy(&first).contains("Fictional Client"));
    }

    #[test]
    fn wrong_password_and_tampering_fail_authentication() {
        let envelope = seal_with_material(&document(), b"correct", [3; 16], [4; 24]).unwrap();
        assert!(matches!(
            open(&envelope, b"wrong"),
            Err(VaultError::Authentication)
        ));

        let mut tampered = envelope;
        *tampered.last_mut().unwrap() ^= 1;
        assert!(matches!(
            open(&tampered, b"correct"),
            Err(VaultError::Authentication)
        ));
    }

    #[test]
    fn header_tampering_and_unsupported_versions_fail() {
        let envelope = seal_with_material(&document(), b"correct", [5; 16], [6; 24]).unwrap();

        let mut bad_parameters = envelope.clone();
        bad_parameters[10] ^= 1;
        assert!(matches!(
            open(&bad_parameters, b"correct"),
            Err(VaultError::UnsupportedKdfParameters)
        ));

        let mut bad_version = envelope;
        bad_version[8..10].copy_from_slice(&2_u16.to_le_bytes());
        assert!(matches!(
            open(&bad_version, b"correct"),
            Err(VaultError::UnsupportedVersion(2))
        ));
    }

    #[test]
    fn production_sealing_uses_fresh_random_material() {
        let first = seal(&document(), b"test password").unwrap();
        let second = seal(&document(), b"test password").unwrap();
        assert_ne!(first, second);
        assert_eq!(open(&first, b"test password").unwrap(), document());
        assert_eq!(open(&second, b"test password").unwrap(), document());
    }

    #[test]
    fn malformed_envelopes_and_empty_passwords_fail() {
        assert!(matches!(
            seal(&document(), b""),
            Err(VaultError::EmptyPassword)
        ));
        assert!(matches!(
            open(b"short", b"password"),
            Err(VaultError::Truncated)
        ));
        let mut envelope = seal_with_material(&document(), b"password", [7; 16], [8; 24]).unwrap();
        envelope[0] ^= 1;
        assert!(matches!(
            open(&envelope, b"password"),
            Err(VaultError::InvalidMagic)
        ));
    }

    #[test]
    fn authenticated_schema_one_plaintext_migrates_after_decryption() {
        let legacy = br#"{"schema_version":1,"people":[],"sessions":[],"artifacts":[]}"#;
        let envelope =
            seal_plaintext_with_material(legacy, b"test password", [9; 16], [10; 24]).unwrap();
        let migrated = open(&envelope, b"test password").unwrap();
        assert!(migrated.journal_entries().is_empty());
        assert!(
            migrated
                .to_json()
                .unwrap()
                .starts_with("{\"schema_version\":2,")
        );
    }
}
