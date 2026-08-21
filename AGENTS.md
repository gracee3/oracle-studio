# Contributor and agent guidance

Oracle Studio is a browser-first Leptos/WASM astrology workspace. Read
`README.md`, `docs/STUDIO_ARCHITECTURE.md`, `docs/COMPOSITION_MODEL.md`,
`docs/VAULT.md`, and `docs/LOCATION_CATALOG.md` before implementation work.

## Validation boundary

Ordinary checks are CPU-only and use fictional data. They may access the network
only to resolve locked Rust dependencies:

```bash
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --locked
cargo check --locked --target wasm32-unknown-unknown \
  -p oracle-studio-worker -p oracle-studio-ui -p oracle-studio-chart-player
(cd crates/oracle-studio-ui && trunk build --release --locked=true)
cargo deny check
git diff --check
```

Docker, Chrome, catalog downloads, and networked acceptance are exceptional;
run them only when explicitly authorized. Never run models, GPU work, or use
personal charts/vaults for validation.

## Privacy and delivery

- Never commit credentials, vaults, browser profiles, screenshots, personal
  charts, GeoNames source bytes, generated WASM, or acceptance artifacts.
- Keep decrypted documents, passwords, and keys inside the worker. UI messages
  may contain summaries and render presentations, never canonical vault JSON.
- Passwords and Argon2 results are discarded after data-key unwrapping. Mounted
  data keys must remain zeroizing values.
- Persist encrypted mutations transactionally with revision compare-and-swap;
  replace in-memory state only after the IndexedDB transaction commits.
- Production has no deterministic ephemeris fallback and no dynamic Swiss ABI.
- Use a focused branch. Encryption, schema, dependency, storage, and container
  changes stay reviewable and must not auto-merge.
- Publish the exact commit and draft PR, then record validation, risks, and next
  action in the external portfolio handoff before claiming completion.
