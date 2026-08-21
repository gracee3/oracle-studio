# Browser chart rendering

`oracle-studio-chart-view` retains the renderer-neutral model and deterministic
SVG transit biwheel. The existing `render_biwheel_svg` API remains available;
additive `render_single_wheel_svg`, `WheelMode`, `WheelLayout`,
`ChartRenderOptions`, and `render_chart_svg` surfaces provide general dispatch.
`oracle-studio-chart-player` retains the Rust/WASM animated HTML controller.
The deleted native chart-export binary is not part of the browser product;
downloads use browser Blob URLs.

The renderer accepts only validated Astraeus schema-v1 comparison snapshots at
revision `8637ceb64fa11a06c8680b46cb4b57c71d94d37f`. It does not calculate or
invent positions, houses, aspects, phases, or orbs. Production presentations
exist only when the worker's compiled Moshier provider has produced a validated
preview or immutable snapshot.

The square biwheel preserves separate central-aspect, natal-point,
transit-point, and cusp regions; stable element IDs; exact-longitude data and
accessible titles; wrap-aware label collision behavior; wheel orientation;
structural angles; palette and label-density options; keyboard-focusable point
and aspect metadata; and ordered selected-point populations. The embedded
Astronomicon v1.1 TTF supplies point, sign, aspect, and retrograde glyphs with no
external font request. Provenance and license details remain in
`THIRD_PARTY_NOTICES.md`.

Single-wheel mode presents Chart 1 (the fixed inner chart) with its points,
houses, and cusps. It deliberately omits Chart 2 and inter-chart aspects. This
is presentation dispatch over the same `ChartScene`; it never requests a new
calculation or changes an Astraeus artifact, canonical byte sequence, content
ID, provider provenance, or vault record.

The workbench keeps wheel identity deliberately quiet: only `Chart 1`/`Chart 2`
and their names appear behind the desktop wheel, with the full date, location,
and time-zone label available as an accessible tooltip. On small screens the
same identities become a short strip above the wheel so they cannot overlap the
chart. A compact status block reports local calculation time, ephemeris mode,
the active aspect set, and wheel template. Its tooltip carries exact provider,
aspect revision, and content-ID detail without exposing filesystem paths.

Displayed and aspected point selections are independent. Displayed points are
a session presentation filter. Aspected points are part of the calculation
rules and immutable saved snapshot. A hidden point may therefore continue to
participate in calculated results and tables, while its glyph and any aspect
line touching it are omitted from the rendered wheel. Neither selection alters
the underlying Astraeus artifact format.

## Themes and template settings v2

`oracle-studio.theme.v1` is an optional browser-local `light` or `dark`
preference. A CSP-hashed script resolves it before paint, falling back to the
system color-scheme preference. The top navigation toggles explicit warm light
and subdued dark schemes. Settings can remove the explicit value and return to
the system preference. Semantic tokens cover page and panel surfaces, controls,
borders, focus, success/error status, chart staging, and responsive drawers.

Wheel settings schema v2 adds presentation-only mode, automatic or explicit
palette selection, label density, orientation, and layout emphasis. Schema-v1
custom template IDs, names, values, order, and selected ID migrate in place as
biwheel/balanced records; the migration also installs five protected built-ins:

- Studio Biwheel
- Compact Biwheel
- High Contrast Biwheel
- Classic Single
- Data-forward Single

Protected templates may be selected and duplicated, but not edited or removed.
Custom records remain editable. Automatic palettes resolve to Paper Light in
the light theme and Studio Dark in the dark theme; High Contrast is always
explicit. Template and theme selection are global unencrypted settings and do
not enter encrypted documents or calculation snapshots.

SVG and animated-HTML exports are self-contained and deterministic for the same
validated input and presentation options. Tests cover glyph population, lane
placement, orientation, single/bi-wheel dispatch, layout metadata, theme-aware
palettes, collision behavior, accessibility names, and fixture hashes using
fictional snapshots. Browser acceptance covers both themes and representative
templates at 390×844, 768×1024, and 1440×900.
