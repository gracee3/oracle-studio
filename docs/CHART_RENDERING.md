# Transit biwheel rendering

Oracle Studio has a dependency-light presentation slice for validated Astraeus
comparison artifacts. Rust produces deterministic SVG and a self-contained
HTML player; the checked-in player uses plain JavaScript and CSS and makes no
external resource requests.

## Renderer boundary

The renderer accepts only Astraeus schema-v1 comparisons that pass
`ComparisonArtifact::from_json` at revision
`52d24862b8287b89b0986b7799583d82ecab21e9`. Oracle then requires:

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

The separate mismatch between older recorded Oracle provenance pins and the
current dependency revision is not repaired here. That needs an explicit
historical-reader/migration design; merely changing recorded revisions could
make existing vault records impossible to reopen.

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

## Current limitations

- This slice is SVG/HTML export, not a native GUI or vault migration.
- Only physical transit-to-natal comparisons are supported.
- Collision displacement is presentation-only; exact-position ticks and leader
  lines preserve the artifact coordinates.
- The player has no interpretation layer, aspect calculation, ephemeris access,
  network synchronization, or external fonts/assets.
- A standalone HTML file can be copied like any other export and is not
  encrypted by the Oracle vault.

Fixture provenance is documented in
[`fixtures/comparisons/README.md`](../fixtures/comparisons/README.md), and
adapted third-party geometry is recorded in
[`THIRD_PARTY_NOTICES.md`](../THIRD_PARTY_NOTICES.md).
