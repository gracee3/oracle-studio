# Transit biwheel rendering

Oracle Studio has a dependency-light presentation slice for validated Astraeus
comparison artifacts. Rust produces deterministic SVG and a self-contained
HTML player; the checked-in player uses plain JavaScript and CSS and makes no
external resource requests.

## Renderer boundary

The renderer accepts only Astraeus schema-v1 comparisons that pass
`ComparisonArtifact::from_json` at revision
`e5d295222018178c46fb882a302a57c810bf8bd1`. Oracle then requires:

- `transit_to_natal` comparison kind;
- `second_moves_against_first_fixed` motion policy;
- physical first and second chart layers;
- an identical first (natal) artifact in every timeline frame;
- an identical moving-point population in every frame; and
- unique, strictly increasing embedded transit timestamps after CLI sorting.

This validation is deliberately stricter than accepting arbitrary two-chart
data. Oracle does not calculate positions, houses, placements, aspects, phases,
or orbs. Serialized comparison aspects are revalidated by Astraeus, copied into
the render model, and changed only at exact frames.

The renderer projects only the comparison specification's ordered
`first_points` and `second_points` populations. A calculated but unselected
body or angle is never added to either visible lane. The dependency revision
adds getter-only access to those existing schema-v1 fields; canonical JSON and
content identities are unchanged. The separate historical
`oracle-studio-core` provenance pin remains at `eb9a756…` pending an explicit
reader/migration design.

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

Point labels remain upright and use three radial tokens, ordered from the
center outward as sign, position, and point glyph. Natal token radii are
approximately `0.46`, `0.54`, and `0.62`; transit token radii are `0.70`,
`0.78`, and `0.86`. Exact-longitude ticks sit on each lane's inner boundary,
and leaders connect those ticks to any displaced label stack.

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
  --output ./timeline.html
```

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
points, motion, timestamps, and inter-chart aspects. It does not embed raw
comparison envelopes, input paths, calculation provenance, or locations. It is
still derived chart data and carries an in-file privacy warning.

The player reads all geometry constants from the generated SVG. At each sample
it mirrors Rust's rounding, adaptive precision, wraparound clustering,
variable-width displacement, upright token placement, ticks, leaders,
retrograde markers, and aspect glyphs. The natal visibility control hides both
the natal point lane and natal house/cusp structure; the other controls remain
scoped to transits and aspects.

Names, chart-information headers, dates, and locations are not rendered inside
the SVG. The HTML wrapper keeps only its caller-supplied title, timestamp
readout, and privacy notice as chart metadata. A companion aspect/position
table is deliberately outside this renderer pass; fixture table images are
used only to verify point and aspect semantics.

## Current limitations

- This slice is SVG/HTML export, not a native GUI or vault migration.
- Only physical transit-to-natal comparisons are supported.
- Collision displacement is presentation-only; exact-position ticks and leader
  lines preserve the artifact coordinates.
- Existing color tokens are retained pending a separate visual-design pass.
- A metadata header and table view are intentionally excluded from this pass.
- The player has no interpretation layer, aspect calculation, ephemeris access,
  network synchronization, or external fonts/assets.
- A standalone HTML file can be copied like any other export and is not
  encrypted by the Oracle vault.

Fixture provenance is documented in
[`fixtures/comparisons/README.md`](../fixtures/comparisons/README.md), and
adapted third-party geometry is recorded in
[`THIRD_PARTY_NOTICES.md`](../THIRD_PARTY_NOTICES.md).
