# Browser chart rendering

`oracle-studio-chart-view` retains the renderer-neutral model and deterministic
SVG transit biwheel. `oracle-studio-chart-player` retains the Rust/WASM animated
HTML controller. The deleted native chart-export binary is not part of the
browser product; downloads use browser Blob URLs.

The renderer accepts only validated Astraeus schema-v1 comparison snapshots at
revision `e5d295222018178c46fb882a302a57c810bf8bd1`. It does not calculate or
invent positions, houses, aspects, phases, or orbs. Production presentations
exist only when a compiled ephemeris provider has produced immutable snapshots.

The square biwheel preserves separate central-aspect, natal-point,
transit-point, and cusp regions; stable element IDs; exact-longitude data and
accessible titles; wrap-aware label collision behavior; wheel orientation;
structural angles; and ordered selected-point populations. The embedded
Astronomicon v1.1 TTF supplies point, sign, aspect, and retrograde glyphs with no
external font request. Provenance and license details remain in
`THIRD_PARTY_NOTICES.md`.

SVG and animated-HTML exports are self-contained and deterministic for the same
validated input. Tests cover glyph population, lane placement, orientation,
collision behavior, accessibility names, and fixture hashes using fictional
snapshots.
