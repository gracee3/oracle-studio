# Transit biwheel rendering

Oracle Studio has a dependency-light presentation slice for validated Astraeus
comparison artifacts. Rust produces deterministic SVG and a self-contained
HTML player. The player controller is Rust compiled to WebAssembly; only the
generated `wasm-bindgen` loader and bootstrap are JavaScript. Its CSP blocks
network connections and it makes no external resource requests.

## Renderer boundary

The renderer accepts only Astraeus schema-v1 comparisons that pass
`ComparisonArtifact::from_json` at revision
`e5d295222018178c46fb882a302a57c810bf8bd1`. Oracle then requires:

- `transit_to_natal` comparison kind;
- `second_moves_against_first_fixed` motion policy;
- physical first and second chart layers;
- an identical first (natal) artifact in every timeline frame;
- an identical moving-point population in every frame;
- identical transit zodiac and house-system settings in every frame; and
- unique, strictly increasing embedded transit timestamps after CLI sorting.

This validation is deliberately stricter than accepting arbitrary two-chart
data. Oracle does not calculate positions, houses, placements, aspects, phases,
or orbs. Serialized comparison aspects are revalidated by Astraeus, copied into
the render model, and changed only at exact frames.

The renderer projects only the comparison specification's ordered
`first_points` and `second_points` populations. A calculated but unselected
body or angle is never added to either visible lane. The dependency revision
adds getter-only access to those existing schema-v1 fields; canonical JSON and
content identities are unchanged. All active Oracle Studio Astraeus crates use
the same `e5d295222018178c46fb882a302a57c810bf8bd1` revision.

## Biwheel geometry

The square SVG retains a 326-unit outer radius and divides it into four
normalized radial regions:

- `0.00–0.42`: central aspect field;
- `0.42–0.66`: natal point lane;
- `0.66–0.90`: transit point lane; and
- `0.90–1.00`: natal cusp band.

House lines run from the aspect boundary to the cusp band. The ASC/DSC and
MC/IC axes are emphasized, while house numbers and a separate fixed zodiac ring
are omitted. Each natal cusp is placed at its exact longitude at radius `0.95`
and displays its rounded degree, zodiac glyph, and minute.

Point labels remain upright and use the rounded position, one sign glyph, and
one point glyph. Natal position and point-glyph radii are approximately `0.51`
and `0.61`; transit radii are `0.75` and `0.85`. Every visible selected point
has exactly one monochrome sign glyph, and every natal cusp has exactly one.
Exact-longitude ticks sit on each lane's inner boundary, and leaders connect
those ticks to any displaced label stack.

The embedded Astronomicon v1.1 font supplies all point, angle, sign, aspect,
and retrograde glyphs. Static SVG and HTML exports embed the original TTF, so
there is no system-font or emoji fallback. Font source, hashes, mapping, OFL
license, and Reserved Font Name are recorded in `THIRD_PARTY_NOTICES.md` and
`assets/astronomicon-v1.1/`.

Isolated positions display `DD°MM′`. A wrap-aware collision cluster switches
as a unit to `DD°`, then receives deterministic variable-width constrained
displacement. Rounding is to the nearest arcminute or degree as displayed and
carries correctly into the next sign. Exact longitudes remain in data
attributes and accessible titles; selection order breaks exact ties. Selected
natal ASC/DSC and MC/IC use the structural house axes instead of duplicate
natal labels. A selected natal Vertex and every selected transit angle render
as ordinary point tokens.

Inter-chart aspects never leave the central field. Their line endpoints use
the points' exact longitudes, independent of label displacement, and the
conventional aspect glyph is centered on each chord. Stable element IDs and
accessible titles retain both point IDs, kind, orb, and phase.

## CLI

Build the stateless renderer without opening a vault:

```bash
cargo build --locked --bin oracle-studio-chart
```

Render one exact comparison:

```bash
./target/debug/oracle-studio-chart svg \
  --comparison fixtures/comparisons/frame-01.json \
  --output ./chart.svg
```

Render an interactive timeline (input order does not matter):

```bash
./target/debug/oracle-studio-chart timeline \
  --comparison fixtures/comparisons/frame-03.json \
               fixtures/comparisons/frame-01.json \
               fixtures/comparisons/frame-02.json \
  --title "Fictional transit demonstration" \
  --natal-name "Example natal" \
  --natal-datetime "2000-01-01T01:00:00+01:00" \
  --natal-location "Fictional test location" \
  --transit-name "Example transits" \
  --transit-datetime "2025-12-31T19:00:00-05:00" \
  --transit-location "Fictional test location" \
  --output ./timeline.html
```

The optional natal and transit date-time flags are independent RFC 3339 local
representations of the instants embedded in their respective artifacts. They
must include a numeric UTC offset and must resolve to the exact artifact
instant; a mismatched date is rejected rather than silently relabeling a
different chart. When omitted, the header displays the artifact time in UTC.
The transit offset is retained while the player advances so its outer-chart
date and time update with the sampled timestamp. It is a fixed numeric offset,
not a daylight-saving timezone database.

Names and locations are caller-supplied display labels. Astraeus coordinates
are never reverse-geocoded or exposed as location names. The HTML always
identifies the natal chart as the inner wheel and transits as the outer wheel,
and shows each artifact's zodiac and house system. The SVG remains chart-only;
all chart information stays in the surrounding HTML page. The serialized
presentation timeline is schema version 2 because it now carries the natal
instant and both charts' zodiac/house-system labels.

Both commands default to `--orientation ascendant-left`; use
`--orientation zodiac-zero-top` for a fixed zodiac wheel. Outputs are published
atomically with owner-only `0600` permissions on Unix. An existing destination
is refused unless `--overwrite` explicitly authorizes atomic replacement.

## Animation semantics

The timeline keeps exact frames as the source of truth. For adjacent frames no
more than 24 hours apart, only moving-point longitude and instantaneous speed
are interpolated. Direction selects the correct direct or retrograde path
across 359°/0°; a station uses the available endpoint direction or the shortest
path. Gaps greater than 24 hours hold the earlier exact scene and jump at the
next exact timestamp.

Aspect sets are never interpolated or inferred. During a dense transition the
earlier frame's aspect set remains visible until the next exact frame. Previous
and next controls always step to exact frames.

The HTML contains the minimal render timeline: natal houses/points, moving
points, motion, timestamps, zodiac and house-system names, inter-chart aspects,
and any caller-supplied chart names or location labels. It does not embed raw
comparison envelopes, input paths, calculation provenance, or source
coordinates. It is still derived chart data and carries an in-file privacy
warning.

At each sample the WASM controller calls the same Rust interpolation and SVG
renderer used by static export. Rounding, adaptive precision, wraparound
clustering, variable-width displacement, upright token placement, ticks,
leaders, retrograde markers, aspect glyphs, and transit header time therefore
have one implementation. The natal
visibility control hides both the natal point lane and natal house/cusp
structure; the other controls remain scoped to transits and aspects.

Names, chart-information headers, dates, and locations are not rendered inside
the SVG. The HTML wrapper shows artifact-grounded chart information and the
caller-supplied display labels described above. A companion aspect/position
table is deliberately outside this renderer pass; fixture table images are
used only to verify point and aspect semantics.

## Current limitations

- This slice remains the stateless SVG/HTML export boundary; the native host
  now stores schema-v3 chart and comparison artifacts separately.
- Only physical transit-to-natal comparisons are supported.
- Collision displacement is presentation-only; exact-position ticks and leader
  lines preserve the artifact coordinates.
- The dark presentation palette assigns accessible identity colors to planets;
  lane backgrounds, leader styles, and filled/hollow exact ticks preserve the
  natal/transit distinction without relying on color.
- A companion aspect/position table remains intentionally excluded.
- The player has no interpretation layer, aspect calculation, ephemeris access,
  network synchronization, or external fonts/assets.
- A standalone HTML file can be copied like any other export and is not
  encrypted by the Oracle vault.

Fixture provenance is documented in
[`fixtures/comparisons/README.md`](../fixtures/comparisons/README.md), and
adapted third-party geometry is recorded in
[`THIRD_PARTY_NOTICES.md`](../THIRD_PARTY_NOTICES.md).
