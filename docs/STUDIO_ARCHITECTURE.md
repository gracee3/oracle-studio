# Browser-local architecture

Oracle Studio is one static browser product. Nginx serves Trunk output on port
8080; there is no runtime API, account, token, filesystem vault path, writable
vault volume, or native process.

```text
Leptos UI (summaries, commands, render presentations)
                  |
                  | typed, versionless StudioPlatform messages
                  v
one Trunk-built Rust Web Worker
  decrypted documents + zeroizing keys + mutations + GeoNames + provider
                  |
                  v
IndexedDB: encrypted_vaults | catalog_objects | settings
```

`BrowserStudioPlatform` is the only implementation. Native and HTTP providers
remain possible future extensions, but no placeholder crate or versioned wire
protocol exists. The UI never receives canonical vault documents.

The worker owns multiple mounted vaults and one active workspace. Each mounted
vault locks after 15 minutes without access. Scratch is memory-only, never
silently discarded, and installs a page-exit warning while dirty. Saving scratch
commits an encrypted vault before switching active state.

Persistent browser storage is requested after the first vault save or catalog
installation. Denial is nonfatal; the UI always warns that portable exports are
required backups.

The product CSP hashes Trunk's exact inline bootstrap—including the prepaint
theme resolver—and the renderer's exact embedded wheel stylesheet at image
build time. It permits only same-origin scripts, workers, fonts, WASM, and
catalog fetches.
There are no cross-origin runtime requests. Direct non-loopback deployment
requires HTTPS supplied outside the static container.

Astraeus's pure-Rust Moshier `EphemerisAdapter` compiles directly into the
worker. Its `swisseph-rs` dependency has default and file features disabled, so
the static browser build does not fetch ephemeris files. Unsupported dates and
Chiron return explicit provider errors. Test-only deterministic adapters cannot
be selected by production UI input.

The UI receives only `WorkspaceSummary`, `WorkbenchPresentation`, global wheel
template settings, and render-ready `ChartScene` values. Theme preference and
schema-v2 wheel templates are global, unencrypted presentation settings; they
contain no chart input or result data. Preview calculations
do not mutate a document. The worker retains one pending generation together
with its source vault ID and encrypted-record revision. Hash-route navigation
does not disturb that transient record.

Chart persistence lives on Files, not beside the wheel. An update requires
confirmation and preserves the outer chart's stable identity. Save-as creates a
new stable identity, rejects case-insensitive name collisions, and never
overwrites. The pending preview can commit only to its still-active, mounted
source vault at the exact captured revision. Switching or locking the vault,
reloading the page, or losing the IndexedDB compare-and-swap invalidates it;
the UI removes the persistence actions and reports why. Scratch previews remain
renderable but cannot commit until scratch is first saved as an encrypted vault.
