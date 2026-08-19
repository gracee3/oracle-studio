# Studio application architecture

Oracle Studio's graphical application is a local-first Rust application. A
Leptos client-side-rendered interface compiles to WebAssembly and a native Axum
process serves it on a per-launch loopback address. There is no Node, React,
Vite, Tailwind, application JavaScript, remote account, or hosted API in this
path.

## Boundaries

The application is split into four boundaries:

- `oracle-studio-ui` contains Leptos routes and presenters compiled for
  `wasm32-unknown-unknown`. Components depend on `StudioPlatform`, not on HTTP,
  Axum, storage crates, or Leptos server functions.
- `oracle-studio-protocol` owns strict, versioned JSON request and response
  types. All structs deny unknown fields. Password-bearing request types redact
  their `Debug` output.
- `oracle-studio-server` is the initial `StudioPlatform` provider. It owns vault
  paths, authenticated persistence, decrypted records, passwords, and native
  services. The current browser adapter calls it over same-origin loopback HTTP.
- A separate offline location-catalog crate will own GeoNames parsing, indexing,
  content addressing, retrieval metadata, and attribution. Catalog bytes stay
  outside both Git and encrypted vault documents; selected places become
  encrypted snapshots.

This division deliberately leaves room for a Tauri adapter and a browser-local
adapter. Those are deferred until the loopback browser application is usable;
mobile portability does not require components to know which adapter supplies a
service.

## Local session security

The host always requests an operating-system-assigned port on `127.0.0.1`; its
bind validation rejects non-loopback addresses. Every launch creates 32 random
bytes and prints a URL containing the hex bearer token in the fragment. URL
fragments are not sent in the initial HTTP request. The UI validates the token,
keeps it only in memory, removes the fragment from browser history, and sends it
in the `Authorization` header on API requests.

Every API operation is a JSON `POST` and must pass all three checks:

1. the `Host` header is the bound `127.0.0.1:PORT` authority;
2. the `Origin` header is the exact launch origin;
3. the bearer token matches in constant time.

Static and API responses receive a no-store policy, a same-origin resource and
opener policy, `nosniff`, no-referrer, and an offline Content Security Policy.
The policy disallows frames, forms, objects, external connections, and external
assets. `wasm-unsafe-eval` is limited to the same-origin generated WebAssembly
bootstrap.

After unlock, the native session owns the decrypted document, optimistic vault
revision, and a zeroizing password allocation. Explicit lock drops that state.
Any API access first expires state that has been idle for 15 minutes. Every
accepted person, location, chart, comparison, or workspace mutation saves
through `FileVault::save` with the session revision, then replaces the in-memory
document and revision only after the atomic write succeeds.

The browser necessarily handles the password while the user fills and submits
the unlock form. The input is cleared immediately after submission; no browser
storage, URL, log, component state, or server response retains it.

## Routes

The CSR shell owns these stable route families:

- `/vault` — create, unlock, and lock;
- `/people` and `/people/:id` — people and person detail;
- `/charts/:id` — chart entry and calculation settings;
- `/locations` — saved locations and offline catalog settings;
- `/workspace` — the active natal/transit comparison.

The native protocol already exposes schema-v3 list, save, calculate, and
workspace operations. The foundation views remain explicit empty states until
the chart-first form and renderer integration land. They are responsive,
keyboard-focusable, and use live regions for vault and form errors.

## Build and run

Install the Rust WASM target and Trunk 0.21.14, then build from the repository
root:

```bash
rustup target add wasm32-unknown-unknown
(cd crates/oracle-studio-ui && trunk build --release)
cargo run --locked -p oracle-studio-server --bin oracle-studio-host -- \
  --dist crates/oracle-studio-ui/dist
```

Open only the complete per-launch URL printed by the host. The host is the
supported way to serve the UI; opening `index.html` directly cannot satisfy the
origin or bearer checks.
