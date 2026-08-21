# Contributor and agent guidance

Oracle Studio is a browser-first Leptos/WASM astrology workspace. Read
`README.md`, `docs/STUDIO_ARCHITECTURE.md`, `docs/COMPOSITION_MODEL.md`,
`docs/VAULT.md`, and `docs/LOCATION_CATALOG.md` before implementation work.

## Astraeus engine contracts and provenance

The `crates/astraeus-*` packages are the headless Astraeus engine subsystem.
Before changing them, read `docs/astraeus/README.md` and the affected contracts
in `docs/astraeus/VALIDATION.md`, `docs/astraeus/PROVENANCE.md`,
`docs/astraeus/ARTIFACTS.md`, and `docs/astraeus/SWISS_EPHEMERIS.md`.

- Preserve every imported crate and public symbol name unless a separately
  reviewed compatibility change explicitly says otherwise.
- Keep calculation artifact schema v1, canonical bytes, content IDs, provider
  provenance, ordering, fixtures, and deserialization validation deterministic.
- Use only fictional or non-personal public fixtures. Never commit charts,
  ephemeris binaries, private source material, secrets, or local data paths.
- Keep `astraeus-swiss`, the CLI, fixtures, events, policies, and time-series
  code outside the Oracle Web Worker dependency graph. Browser calculation uses
  the file-free `astraeus-moshier` adapter and must fail instead of falling back.
- Preserve the AGPL path and document Swiss Ephemeris provenance and licensing
  before copying code, data, or fixtures.

## Validation boundary

Ordinary checks are CPU-only and use fictional data. They may access the network
only to resolve locked Rust dependencies:

```bash
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --locked
cargo check --locked --target wasm32-unknown-unknown \
  -p astraeus-moshier -p oracle-studio-worker -p oracle-studio-ui \
  -p oracle-studio-chart-player
(cd crates/oracle-studio-ui && trunk build --release --locked=true)
cargo deny check
git diff --check
```

Docker, Chrome, catalog downloads, and networked acceptance are exceptional;
run them only when explicitly authorized. Never run models, GPU work, or use
personal charts/vaults for validation.

## Oracle worker, vault, and privacy

- Never commit credentials, vaults, browser profiles, screenshots, personal
  charts, GeoNames source bytes, generated WASM, or acceptance artifacts.
- Keep decrypted documents, passwords, and keys inside the worker. UI messages
  may contain summaries and render presentations, never canonical vault JSON.
- Passwords and Argon2 results are discarded after data-key unwrapping. Mounted
  data keys must remain zeroizing values.
- Persist encrypted mutations transactionally with revision compare-and-swap;
  replace in-memory state only after the IndexedDB transaction commits.
- Production has no deterministic ephemeris fallback and no dynamic Swiss ABI.

## Delivery

- Use a focused branch. Encryption, schema, dependency, storage, and container
  changes stay reviewable and must not auto-merge.
- Publish the exact commit and draft PR, then record validation, risks, and next
  action in the external portfolio handoff before claiming completion.
