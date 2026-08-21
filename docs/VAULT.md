# Portable vault envelope v2

Envelope v2 visibly stores a random UUID vault ID and bounded UTF-8 public
title. All semantic header fields are authenticated. Every older envelope
version is rejected without migration.

1. Argon2id derives a key-encryption key from the password and a random 128-bit
   salt using 64 MiB, three iterations, and one lane.
2. A random 256-bit vault data key is wrapped with XChaCha20-Poly1305 and a
   random 192-bit nonce.
3. Canonical schema-v5 JSON is encrypted with that data key using
   XChaCha20-Poly1305 and a fresh 192-bit nonce on every save.
4. Explicit algorithm identifiers and fixed/bounded lengths fail closed.

The worker discards the password and zeroizing Argon2 result immediately after
unwrapping. A mounted vault retains only its zeroizing data key, public wrapping
material, and decrypted validated document. Resealing does not need the
password or rewrap the data key.

Envelope v2 did not change for document schema v5. An authenticated v4
document is rejected clearly after decryption and its original encrypted bytes
remain untouched; there is deliberately no v4-to-v5 migration.

## Persistence

IndexedDB stores public ID/title, exact encrypted envelope, SHA-256 revision,
and timestamps. A mutation encrypts a candidate document, compares the expected
revision, commits one read-write transaction, then replaces memory. Conflicts
never silently overwrite newer bytes. Duplicate imported IDs are rejected;
confirmed replacement is whole-vault replacement, not merge or sync.

A transient chart preview also captures the source record revision. Its Files
update/save-as transaction requires that exact revision in the currently active
unlocked vault. A revision conflict invalidates the preview and leaves both the
decrypted in-memory document and encrypted stored bytes unchanged.

Exports contain the exact `.oracle-vault` bytes. Removing an IndexedDB record
does not claim physical erasure from browser storage, backups, or device media.

## Demo envelope boundary

The native-only `oracle-studio-demo` builder enables a narrowly feature-gated
vault constructor so every generated demo envelope carries one reviewed public
UUID. That constructor is absent from ordinary vault and production browser
builds. Stable public identity does not make encryption deterministic: each
generation still uses a fresh Argon2 salt, wrapping nonce, document nonce, and
random data key. The repository tracks only deterministic fictional plaintext
hashes and calculation content IDs, never an encrypted demo envelope.

## Threat model

The envelope protects confidentiality and detects modification when an attacker
has encrypted bytes but not the password while the vault is locked. It cannot
protect against malicious browser extensions, a compromised OS/browser,
screen/memory capture while mounted, weak passwords, or an attacker with both
password and envelope. Public titles intentionally reveal metadata.
