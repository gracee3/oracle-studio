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
- Maintain chart-only schema-v5 people, saved locations, chart definitions,
  immutable calculations, comparison presets/calculations, and workspace state.
- Resolve IANA local times explicitly, including ambiguous and nonexistent
  civil times.
- Parse and search an image-pinned or user-selected GeoNames distribution in a
  Web Worker; manual location entry is always available.
- Render validated Astraeus chart presentations with additive single-wheel and
  bi-wheel Rust SVG dispatch while retaining the existing biwheel API.
- Use a full-viewport, hash-addressable Workbench, Settings, and Files shell;
  sidebars and route content scroll independently while the chart wheel remains
  the dominant surface.
- Collapse the desktop Charts and Controls sidebars independently into narrow
  rails with global `oracle-studio.layout.v1` preferences; tablet and mobile
  layouts retain their existing drawers.
- Zoom the chart from 75% to 300% with visible controls, focused-stage keyboard
  shortcuts, or pointer-relative Alt/Option-wheel input. Ctrl-wheel remains
  available to the browser for page zoom.
- Preview the fixed inner chart against a moving outer chart with exact civil-
  time and elapsed-time controls, then use Files to confirm an identity-
  preserving update or save the preview under a unique new chart name.
- Select five protected single/bi-wheel templates, duplicate them into custom
  schema-v2 visual settings, and resolve automatic chart palettes from a
  prepaint warm-light or subdued-dark theme preference.
- Select and edit global aspect sets while saved comparisons retain immutable
  rule/point snapshots.

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
entry. For an explicit catalog-enabled local build, use the shared lock-backed
workflow:

```bash
just geonames-download
just geonames-check
just geonames-build
just geonames-serve
```

The default Docker target uses the same download, verification, attribution,
and staging implementation. `acceptance-runtime` remains catalog-free. Upstream
drift fails closed; `just geonames-candidate-lock` writes only ignored review
artifacts and never edits `catalog/geonames.lock`.
Ordinary Trunk and Docker builds also contain no demo loader or demo vault.

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

## Fictional demo build

The opt-in demo build creates a fresh encrypted envelope around a deterministic,
fictional workspace. Its public, non-secret password is `oracle-demo`; never use
that password for a real vault.

```bash
just demo-verify
just demo-build
just demo-serve
```

`demo-generate`, `demo-verify`, `demo-build`, `demo-serve`, and `demo-test` keep
all generated plaintext, manifests, encrypted envelopes, and static output under
ignored `var/demo`. No stable encrypted vault is committed. See
[the demo contract](docs/DEMO.md).

See [architecture](docs/STUDIO_ARCHITECTURE.md),
[schema v5](docs/COMPOSITION_MODEL.md), [envelope v2](docs/VAULT.md),
[aspect-set contract](docs/ASPECT_SETS.md), and the
[GeoNames contract](docs/LOCATION_CATALOG.md). The presentation-only theme and
template contract is documented in [chart rendering](docs/CHART_RENDERING.md).
The [public-record catalog](docs/PUBLIC_RECORD_CATALOG.md) documents the
reviewed, non-personal fixture inventory. Development validation and the current
repository safeguards are documented in
[development policy](docs/DEVELOPMENT.md).

## License

AGPL-3.0-or-later. Swiss Ephemeris has separate dual-license requirements; see
[the consolidated engine policy](docs/astraeus/SWISS_EPHEMERIS.md) and
[third-party notices](THIRD_PARTY_NOTICES.md).
