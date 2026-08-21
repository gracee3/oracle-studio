# Local persistence architecture

Status: accepted design for the pre-release browser product, 2026-08-21.

Oracle Studio has three deliberately separate storage domains. Secret vaults
remain independently encrypted and portable. Non-secret preferences are typed,
versioned public objects. Large licensed or reproducible data remains a local,
content-addressed cache. A single container must not blur those security and
lifecycle boundaries.

## Current state

| Owner / location | Current contents | Lifetime and portability |
|---|---|---|
| Worker memory | Zeroizing mounted-vault keys, decrypted schema-v5 documents, scratch document, parsed GeoNames catalog, wheel/aspect settings, pending preview and source revision | Lost on lock/reload; decrypted documents never cross into the UI |
| UI memory | Summaries, render scenes, forms, point/aspect filters, selections, zoom, route and transient errors | Session-only except the explicit preferences below |
| IndexedDB `encrypted_vaults` | JSON-encoded `VaultRecord`: public UUID/title, exact envelope-v2 bytes, SHA-256 revision and timestamps | One independently locked/exportable record per vault; writes use revision CAS |
| IndexedDB `catalog_objects` | GeoNames source bytes plus verified metadata, keyed by content ID | Device-local cache; not a user-data backup or sync object |
| IndexedDB `settings` | Active catalog content ID, wheel-template settings v2 JSON, aspect-set settings v1 containing aspect-set v2 objects | Global, non-secret, browser-local state |
| `localStorage` | `oracle-studio.theme.v1` and `oracle-studio.layout.v1` | Small prepaint/UI preferences; neither contains chart data |
| Worker/UI session only | Chart zoom and unsaved preview generation | Intentionally never persisted |
| Repository fixtures | Reviewed public records, schema files, GeoNames lock/attribution, deterministic fictional demo lock | Build/test inputs; ordinary production builds contain no demo loader or vault |
| Ignored `var/` output | Downloaded/staged GeoNames data, demo plaintext/manifest/site and fresh encrypted demo envelope | Reproducible local output, never committed |

The portable secret format is the exact binary `.oracle-vault` envelope v2.
Aspect sets export as strict JSON v2. SVG/HTML chart exports are presentation
artifacts, not restorable application state. There is currently no complete
backup format for global non-secret settings and no aggregate multi-vault
backup.

## Canonical model

```text
LocalStudioRepository
├── EncryptedVaultStore       independent opaque envelope-v2 objects
├── PublicObjectStore         typed non-secret objects and selections
├── CatalogCache              content-addressed source objects + active pointer
└── DurabilityService         browser persistence request/status only

WorkerSession
├── MountedVault              zeroizing data key + validated plaintext document
├── ScratchDocument           memory-only
├── PendingPreview            source vault ID + exact encrypted revision
└── ParsedCatalog             derived from a verified cache object
```

These are provider boundaries, not one universal database document. IndexedDB
is the only provider today. A future local filesystem or optional sync provider
may implement selected repositories without gaining access to decrypted vault
documents.

### Encrypted vault objects

- One stable public UUID identifies one vault. Each vault remains independently
  imported, mounted, active, locked, removed and exported.
- The stored value is the exact authenticated binary envelope. Public database
  metadata is a cache and must match `inspect(envelope)` before use.
- The SHA-256 of the complete envelope is its storage revision and CAS token.
  A successful mutation seals a candidate with a fresh document nonce, commits
  it against the expected revision in one transaction, and only then replaces
  decrypted memory.
- Envelope v2 remains binary. Re-encoding it as readable JSON would not make
  encrypted material debuggable, would increase size, and would create a second
  cryptographic parser. A JSON backup container may carry the exact bytes as
  base64, but it does not replace the envelope.
- The public UUID and title are intentionally visible. No chart identity, date,
  location, person, calculation summary or decrypted schema version may be
  copied into public metadata.

### Public objects

Global non-secret state moves toward independently addressable public objects,
not an ever-growing settings singleton. The proposed wire sketch is
[`local-settings-bundle-v1.schema.json`](../schemas/local-settings-bundle-v1.schema.json).
Each object has:

- a stable `(scope, object_type, object_id)` key;
- its own payload schema version and strict type-specific validator;
- a monotonic revision, prior content ID for causal/CAS checks, update time and
  canonical SHA-256 content ID;
- an explicit `device_local`, `portable`, or `syncable` policy.

Current object families map as follows:

| Object family | Scope | Policy | Notes |
|---|---|---|---|
| Theme and desktop layout | Global | `syncable` | `localStorage` may retain a derived prepaint mirror; the public object is authoritative after startup |
| Wheel-template collection/selection | Global | `syncable` | Payload remains strict wheel settings v2 |
| Aspect-set collection/selection | Global | `syncable` | Payload remains settings v1 with aspect-set objects v2 |
| Active catalog pointer | Global | `device_local` | References only an installed local cache content ID |
| Future vault-specific presentation | Vault scope | Explicit per type | Must contain no private chart data; deletion does not alter the encrypted vault |

The bundle is a portable snapshot of public objects, not their live transaction
unit. Stores update one object at a time by expected content ID. Import validates
every envelope and payload before committing all accepted objects in one local
transaction. Unknown object types are preserved only by backup tools; the live
application does not activate data it cannot validate.

Canonical identity uses RFC 8785 JSON Canonicalization Scheme bytes and SHA-256
for the public-object envelope excluding `content_id`. Payload validators may
also retain their existing content identities. The outer identity protects
against accidental corruption and supports conflict detection; it is not an
authentication signature and must never be described as one.

### Catalog cache

GeoNames source files, attribution and manifest are reproducible, licensed,
large device-local cache objects. They are keyed and revalidated by content ID.
They are excluded from settings sync and normal backup. A backup may include
only the active content ID and provenance; restoration reports a missing cache
and asks for the reviewed source files rather than silently downloading data.

### Optional aggregate backup

The proposed
[`local-backup-container-v1.schema.json`](../schemas/local-backup-container-v1.schema.json)
can aggregate exact encrypted envelopes and a public-settings bundle. It is a
human-readable manifest around opaque encrypted bytes, not a new vault format.
On import, title hints and revisions are untrusted until each embedded envelope
is inspected; duplicate vault IDs require explicit per-vault replacement. The
container has no password and does not decrypt or re-encrypt its entries.

## Transactions, conflicts and recovery

1. Validate bounds, schema and canonical content IDs before opening a write
   transaction.
2. Read the current object revision inside that transaction.
3. Reject a missing or mismatched expected revision; never last-writer-wins.
4. Commit all records and pointers atomically, then update memory/UI state.
5. Keep the prior encrypted bytes or public object untouched on any validation,
   encryption, quota, transaction or revision failure.

Public sync conflicts are object-level. Equal content IDs converge. A direct
parent/child advances automatically. Divergent descendants remain two explicit
candidates for user choice; no field-level merge is attempted for aspect or
template collections. Encrypted vault conflicts remain whole-envelope choices
because a provider cannot inspect plaintext. Cloud support is optional and may
receive only public objects marked `syncable` and opaque encrypted envelopes.

Corrupt local records are not replaced with defaults in storage. Startup may
use an in-memory default while reporting the rejected object and offering
export/removal. Vault corruption fails closed. Catalog corruption disables the
catalog. Backup restore validates the complete candidate set before mutation
and produces a receipt listing imported, skipped, conflicting and rejected
objects.

## Version and migration policy

During pre-release, a schema may break deliberately, but the application must
still fail clearly and preserve original bytes. The IndexedDB database version,
store layout, record envelope version and payload schema version are independent
numbers.

The first adopting release should create new public-object records alongside
the existing settings keys, validate round trips, and only then mark migration
complete. The old records remain readable for one pre-release cycle and are not
deleted automatically. Theme/layout prepaint mirrors are rewritten only after
the authoritative object commits. Vault envelope v2 and document v5 are not
changed by this architecture decision.

After a stable release, readers support the current and immediately previous
public-object payload versions with explicit, tested migrations. Unknown newer
versions fail closed. Encrypted document migrations always happen only after
successful authentication and retain the original envelope until a separately
confirmed save/export succeeds.

## Staged adoption

1. Introduce typed `EncryptedVaultStore`, `PublicObjectStore`, `CatalogCache`
   and `DurabilityService` traits behind the current browser façade; keep
   behavior and IndexedDB v1 bytes unchanged.
2. Add IndexedDB v2 public-object records, migrate wheel/aspect/theme/layout
   settings transactionally, and add corruption quarantine/receipts.
3. Add explicit public-settings export/import. Do not include catalogs.
4. Add the optional aggregate backup container only after size limits,
   streaming base64 handling and per-vault conflict UX have browser tests.
5. Add an optional sync provider only after object-level conflict UX exists.
   Accounts and cloud services remain unnecessary for local use.

Each stage is independently reviewable. Stages 2–5 change storage or portable
schemas and require explicit approval plus native, WASM, browser reload,
corruption, quota, CAS, import/export and non-mutating-failure tests.
