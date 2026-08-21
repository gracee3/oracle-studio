# Browser-local architecture

Oracle Studio is one static browser product. Nginx serves Trunk output on port
8080; there is no runtime API, account, token, filesystem vault path, writable
vault volume, or native process.

The current-format inventory, accepted long-term storage boundaries, public
object model, recovery rules, and staged IndexedDB adoption are defined in
[`PERSISTENCE_ARCHITECTURE.md`](PERSISTENCE_ARCHITECTURE.md).

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

The separate `demo` UI feature may fetch one generated same-origin encrypted
vault with the reviewed public demo UUID. Before allowing replacement, the UI
checks the envelope-v2 public ID and title against compiled demo constants. It
then uses the ordinary worker import and unlock commands. Ordinary production
Trunk and Docker targets do not compile the loader, password, or asset path.
Demo load/reset never writes global settings and replacement can target only the
demo UUID.

Astraeus's pure-Rust Moshier `EphemerisAdapter` compiles directly into the
worker. Its `swisseph-rs` dependency has default and file features disabled, so
the static browser build does not fetch ephemeris files. Unsupported dates and
Chiron return explicit provider errors. Test-only deterministic adapters cannot
be selected by production UI input.

The UI receives only `WorkspaceSummary`, `WorkbenchPresentation`, global wheel
template/aspect-set settings, and render-ready `ChartScene` values. Theme
preference and schema-v2 wheel templates are global, unencrypted presentation
settings; they contain no chart input or result data. Preview calculations do
not mutate a document. The worker retains one pending generation
together with its source vault ID and encrypted-record revision. Hash-route
navigation does not disturb that transient record. Any aspect-set selection or
mutation invalidates that pending generation before recalculation.

Aspect-set settings are global and intentionally unencrypted. Vault documents
contain only immutable snapshots attached to saved comparison calculations, not
the mutable global collection or selection.

Chart zoom is session-only presentation state and never enters a worker message,
calculation artifact, or vault. Desktop sidebar collapse preferences are global,
unencrypted browser settings stored as `oracle-studio.layout.v1`; they affect only
the responsive shell. Both states leave the active chart, filters, selections,
preview generation, and encrypted workspace untouched.

Chart persistence lives on Files, not beside the wheel. An update requires
confirmation and preserves the outer chart's stable identity. Save-as creates a
new stable identity, rejects case-insensitive name collisions, and never
overwrites. The pending preview can commit only to its still-active, mounted
source vault at the exact captured revision. Switching or locking the vault,
reloading the page, or losing the IndexedDB compare-and-swap invalidates it;
the UI removes the persistence actions and reports why. Scratch previews remain
renderable but cannot commit until scratch is first saved as an encrypted vault.
