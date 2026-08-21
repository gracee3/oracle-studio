# Browser-local delivery status

Implemented foundation:

- chart-only document schema v4 with strict rejection of schemas 1–3;
- wrapped-key portable envelope v2 with fixed Argon2id policy;
- versionless `StudioPlatform` and one browser worker implementation;
- transactional IndexedDB vault/catalog/settings stores;
- volatile scratch, multiple mounts, active switching, idle lock, import/export;
- pure GeoNames parsing/search with upload and image-pinned same-origin inputs;
- open Leptos vault library and chart workflow shell;
- static unprivileged container and generated bootstrap/wheel-style CSP hashes.

Review gates before merge include browser IndexedDB/Chrome acceptance and the
usual locked Rust, WASM, Clippy, rustdoc, dependency, Trunk, Docker, security
header, responsiveness, accessibility, and no-external-request checks.

Later work: production pure-Rust/WASM ephemeris, TLS deployment guidance, and
PWA installation. A dynamic Swiss plugin ABI is intentionally not designed.
