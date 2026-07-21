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

## Durable filesystem repository

Phase 5B stores only authenticated envelope bytes. Writers take a nonblocking
advisory lock, compare the caller's expected encrypted revision, write a
randomly named same-directory temporary file, flush it, atomically rename it,
and sync the parent directory. A stale caller receives a conflict instead of
silently replacing newer data. Readers take a shared lock and authenticate the
envelope before returning a document.

On Unix, application-created directories are mode `0700`; an existing directory
is rejected if group or other permission bits are set. Vault, lock, temporary,
and backup files are mode `0600`. Symbolic-link storage targets are rejected.
Other platforms use their native permission model and require a platform-specific
review before production release.

Backups contain the exact encrypted envelope and are authenticated before
export or import. Import uses the same optimistic revision check and atomic
replacement path as a normal save. Filesystem revision IDs hash encrypted bytes
for concurrency control; they are distinct from engine artifact content IDs.

## Deletion boundary

Deleting a file cannot guarantee physical erasure on SSDs, snapshots, or cloud
backups. Permanent deletion therefore means removing all live records and
explicitly selected backups under application control. The storage API reports
that physical erasure is not guaranteed. Password-derived vault v1 has no
separate unique data key to destroy; key-wrapping and rotation remain a future
format version and must not be implied by the UI.
