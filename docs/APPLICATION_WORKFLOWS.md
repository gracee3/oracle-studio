# Offline application workflows

`oracle-studio-app` is a UI-neutral use-case layer. Each operation takes a
validated `VaultDocument` and returns a newly validated document; it does not
read files, retain passwords, or bypass optimistic storage revisions. The CLI
loads an authenticated revision, applies exactly one transformation, previews
guided readings, and saves only after confirmation.

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

## Search and confidentiality

Search performs a case-insensitive scan of the already decrypted in-memory
document and returns bounded snippets. It writes no index, cache, query log, or
decrypted temporary file. CLI output is intentionally plaintext visible to the
unlocked user and remains subject to terminal history, screen capture, and the
host threat model.

## Fixture provenance

Phase 5C tests contain only original fictional metadata. The synthetic chart
fixture follows the public Astraeus API pinned at
`952a143b700ea5cad960498e7d8916a49ebb3691`; the metadata-only tarot fixture
targets Sibylla `a154c32b83b110d2568a9ab10828b4f8b3dba7c7`. No personal data,
deck art, guidebook text, ephemeris binary, or model asset is included.
