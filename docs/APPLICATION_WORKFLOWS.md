# Offline application workflows

`oracle-studio-app` is a UI-neutral use-case layer. Each operation takes a
validated `VaultDocument` and returns a newly validated document; it does not
read files, retain passwords, or bypass optimistic storage revisions. The CLI
loads an authenticated revision, applies exactly one transformation, previews
guided readings, and saves only after confirmation.

The current CLI walkthrough and exact command reference are in
[CLI testing guide](CLI_TESTING.md). It uses an original metadata-only fixture
and a throwaway encrypted vault so the complete first tarot workflow can be
tested without artwork or personal data.

Deck images are not embedded in Sibylla artifacts or the encrypted vault
document. The `oracle-studio-assets` crate validates an application-owned
sidecar, binds it to the exact Sibylla deck content ID, rejects unsafe paths and
symbolic links, and verifies every local file's SHA-256 before a UI can render
it. The CLI exposes this as `deck-pack-verify`.

## Engine boundaries

- Deck import accepts a Sibylla deck envelope or raw manifest and stores the
  canonical deck envelope.
- Manual readings map confirmed physical deck-card IDs to declared spread
  positions and preserve upright, reversed, or unspecified orientation.
- Software readings call `sibylla-shuffle::shuffle`, which always obtains OS
  entropy. The CLI has no deterministic production switch.
- Chart import accepts only the pinned Astraeus calculation artifact and never
  recalculates or repairs it.
- Engine artifacts are immutable. Later annotations and outcomes are separate
  source-linked journal entries.
- Person and session commands may create or edit composition records, while
  `deck-list`, `person-list`, and `session-list` provide the IDs needed by the
  guided workflows.
- Backup export and import copy authenticated encrypted envelope bytes; they do
  not create a plaintext export format.

## Search and confidentiality

Search performs a case-insensitive scan of the already decrypted in-memory
document and returns bounded snippets. It writes no index, cache, query log, or
decrypted temporary file. Artifact searches include verified deck pack IDs and
deck content IDs when present. CLI output is intentionally plaintext visible to
the unlocked user and remains subject to terminal history, screen capture, and
the host threat model.

## Fixture provenance

Phase 5C tests contain only original fictional metadata. The synthetic chart
fixture follows the public Astraeus API pinned at
`952a143b700ea5cad960498e7d8916a49ebb3691`; the metadata-only tarot fixture
targets Sibylla `a154c32b83b110d2568a9ab10828b4f8b3dba7c7`. No personal data,
deck art, guidebook text, ephemeris binary, or model asset is included.
