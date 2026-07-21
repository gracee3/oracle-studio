# Encrypted vault contract and threat model

## Version 1 envelope

Vault schema version 1 serializes validated composition data to canonical JSON,
then encrypts and authenticates it with XChaCha20-Poly1305. A 256-bit key is
derived from the caller's password using Argon2id with a fresh 128-bit salt.
Every seal uses a fresh operating-system-generated 192-bit nonce. The fixed
header, version, KDF parameters, salt, and nonce are authenticated as associated
data. Plaintext is never parsed until authentication succeeds.

The binary format has explicit magic and version bytes, bounded lengths, and
fixed accepted KDF parameters. Unknown versions and parameters fail rather than
being guessed or silently migrated.

## Threat model

The vault protects confidentiality and detects modification when an attacker
obtains the encrypted file but not the password while the application is
closed. It does not protect data from malware, a compromised operating system,
screen capture, memory inspection while unlocked, weak passwords, or an
attacker who has both the file and password.

Passwords are never stored. Derived key buffers are zeroized on drop. Callers
remain responsible for protecting and zeroizing their password input.

## Storage and deletion boundary

Phase 5A produces and consumes authenticated bytes; it does not claim durable
filesystem semantics. Phase 5B will add atomic replacement, file permissions,
locking, backup, and crash recovery.

Deleting a file cannot guarantee physical erasure on SSDs, snapshots, or cloud
backups. Permanent deletion therefore means removing all live records and
backups under application control and, where possible, destroying the unique
vault key. The UI must describe this limit honestly.
