# Portable vault envelope v2

Envelope v2 visibly stores a random UUID vault ID and bounded UTF-8 public
title. All semantic header fields are authenticated. Every older envelope
version is rejected without migration.

1. Argon2id derives a key-encryption key from the password and a random 128-bit
   salt using 64 MiB, three iterations, and one lane.
2. A random 256-bit vault data key is wrapped with XChaCha20-Poly1305 and a
   random 192-bit nonce.
3. Canonical schema-v4 JSON is encrypted with that data key using
   XChaCha20-Poly1305 and a fresh 192-bit nonce on every save.
4. Explicit algorithm identifiers and fixed/bounded lengths fail closed.

The worker discards the password and zeroizing Argon2 result immediately after
unwrapping. A mounted vault retains only its zeroizing data key, public wrapping
material, and decrypted validated document. Resealing does not need the
password or rewrap the data key.

## Persistence

IndexedDB stores public ID/title, exact encrypted envelope, SHA-256 revision,
and timestamps. A mutation encrypts a candidate document, compares the expected
revision, commits one read-write transaction, then replaces memory. Conflicts
never silently overwrite newer bytes. Duplicate imported IDs are rejected;
confirmed replacement is whole-vault replacement, not merge or sync.

Exports contain the exact `.oracle-vault` bytes. Removing an IndexedDB record
does not claim physical erasure from browser storage, backups, or device media.

## Threat model

The envelope protects confidentiality and detects modification when an attacker
has encrypted bytes but not the password while the vault is locked. It cannot
protect against malicious browser extensions, a compromised OS/browser,
screen/memory capture while mounted, weak passwords, or an attacker with both
password and envelope. Public titles intentionally reveal metadata.
