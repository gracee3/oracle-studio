# Oracle Studio

Oracle Studio is an open, browser-local astrology workspace built with Leptos
and Rust/WebAssembly. It has no application server, account, bearer token, CLI,
native filesystem storage, or runtime network dependency. A static container
serves the application; encrypted vaults, catalog objects, and settings live in
the browser's IndexedDB.

The complete Astraeus calculation engine is maintained in this repository as
non-publishable `astraeus-*` workspace crates. The subsystem retains its crate
names, public Rust APIs, schema-v1 calculation artifacts, content identifiers,
provider provenance, fixtures, CLI, native Swiss adapter, and pure-Rust Moshier
adapter. Its engine documentation begins at [Astraeus](docs/astraeus/README.md),
and the full-history import is recorded in [the migration record](docs/astraeus/MIGRATION.md).

## Current boundary

- Start immediately in a volatile scratch workspace.
- Save scratch as a portable envelope-v2 `.oracle-vault` using a public title
  and password.
- Import, unlock, switch, lock, export, unload, replace, and remove multiple
  independent vaults.
- Maintain chart-only schema-v4 people, saved locations, chart definitions,
  immutable calculations, comparison presets/calculations, and workspace state.
- Resolve IANA local times explicitly, including ambiguous and nonexistent
  civil times.
- Parse and search an image-pinned or user-selected GeoNames distribution in a
  Web Worker; manual location entry is always available.
- Render validated Astraeus chart presentations with the retained Rust SVG and
  animated-HTML renderer.
- Use a full-viewport, hash-addressable Workbench, Settings, and Files shell;
  sidebars and route content scroll independently while the chart wheel remains
  the dominant surface.
- Preview the fixed inner chart against a moving outer chart with exact civil-
  time and elapsed-time controls, then use Files to confirm an identity-
  preserving update or save the preview under a unique new chart name.
- Save versioned, global wheel templates containing visual options only.

The production worker compiles Astraeus's pure-Rust Moshier adapter using
`swisseph-rs` with file and default features disabled. Results explicitly carry
Moshier provenance. Unsupported dates and Chiron fail visibly; Oracle Studio
never substitutes a provider or fabricates a chart result. Decrypted documents,
calculation work, and immutable commits remain worker-owned.

The native `astraeus-swiss`, CLI, fixtures, events, Western policy, and
time-series crates remain ordinary workspace members but are deliberately
outside the Web Worker's dependency graph.

## Build

Install the pinned Rust target and Trunk release, then build the static product:

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk --locked --version 0.21.14
cd crates/oracle-studio-ui
trunk build --release --locked=true
```

Ordinary Trunk builds contain no GeoNames bytes and offer local upload/manual
entry. The Docker build fetches the three official GeoNames inputs, verifies the
hashes in `catalog/geonames.lock`, and publishes them on the same static origin.

```bash
docker build -t oracle-studio:browser-local .
docker run --rm --read-only --tmpfs /tmp --publish 127.0.0.1:8080:8080 \
  oracle-studio:browser-local
```

Open `http://127.0.0.1:8080`. A ThinkPad can use the stable forward
`ssh -N -L 127.0.0.1:8080:127.0.0.1:8080 HOST`; there is no launch token.
Non-loopback deployments require HTTPS outside the container.

Portable exports are the backup boundary. Browser eviction or profile deletion
can remove IndexedDB even after persistent storage is granted.

See [architecture](docs/STUDIO_ARCHITECTURE.md),
[schema v4](docs/COMPOSITION_MODEL.md), [envelope v2](docs/VAULT.md), and the
[GeoNames contract](docs/LOCATION_CATALOG.md). Development validation and the
current repository safeguards are documented in
[development policy](docs/DEVELOPMENT.md).

## License

AGPL-3.0-or-later. Swiss Ephemeris has separate dual-license requirements; see
[the consolidated engine policy](docs/astraeus/SWISS_EPHEMERIS.md) and
[third-party notices](THIRD_PARTY_NOTICES.md).
