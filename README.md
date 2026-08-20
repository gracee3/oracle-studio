# Oracle Studio

Oracle Studio is an open, browser-local astrology workspace built with Leptos
and Rust/WebAssembly. It has no application server, account, bearer token, CLI,
native filesystem storage, or runtime network dependency. A static container
serves the application; encrypted vaults, catalog objects, and settings live in
the browser's IndexedDB.

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

The production build intentionally reports **ephemeris provider unavailable**.
It never fabricates a chart result. Deterministic providers exist only behind
test/acceptance configuration; a future pure-Rust or WASM Swiss provider can be
compiled into the worker through Astraeus' existing `EphemerisAdapter`.

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
[GeoNames contract](docs/LOCATION_CATALOG.md).

## License

AGPL-3.0-or-later.
