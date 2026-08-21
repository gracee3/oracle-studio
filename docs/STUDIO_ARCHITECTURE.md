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

The product CSP hashes Trunk's exact inline bootstrap and the renderer's exact
embedded wheel stylesheet at image build time. It permits only same-origin
scripts, workers, fonts, WASM, and catalog fetches.
There are no cross-origin runtime requests. Direct non-loopback deployment
requires HTTPS supplied outside the static container.

Astraeus's pure-Rust Moshier `EphemerisAdapter` compiles directly into the
worker. Its `swisseph-rs` dependency has default and file features disabled, so
the static browser build does not fetch ephemeris files. Unsupported dates and
Chiron return explicit provider errors. Test-only deterministic adapters cannot
be selected by production UI input.

The UI receives only `WorkspaceSummary`, `WorkbenchPresentation`, global wheel
template/aspect-set settings, and render-ready `ChartScene` values. Preview
calculations do not mutate a document. The worker retains one pending generation
and commits it atomically only after Update Chart or Save As. Any aspect-set
selection or mutation invalidates that pending generation before recalculation.

Aspect-set settings are global and intentionally unencrypted. Vault documents
contain only immutable snapshots attached to saved comparison calculations, not
the mutable global collection or selection.
