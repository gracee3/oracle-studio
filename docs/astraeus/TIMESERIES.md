# Aspect time series

`astraeus-timeseries` is the headless schema-v1 boundary for plotting and
mathematical analysis of one aspect between one pair. It calculates either a
moving object against a fixed point in an embedded `CalculationArtifact`, or
two distinct moving objects in an explicit `EventCoordinateFrame`. Rendering,
interpretation, multi-pair batching, statistical scoring, and aggregate
exposure models are downstream concerns.

## Request and samples

An `AspectTimelineRequest` contains one subject, one `AspectDefinition`,
inclusive UTC `start` and `end` instants, and a positive cadence in whole
seconds. The end cannot precede the start and the orb must be greater than zero.
The fixed point must exist in the embedded chart. A moving/moving pair cannot
repeat the same object.

For a moving/fixed subject, the first position is the selected chart-point
longitude with speed zero and the second is the moving object. Transit samples
use the chart's tropical or configured sidereal zodiac. For moving/moving, the
listed objects determine first and second position order.

Every regular sample records both positions plus:

- signed and absolute shortest separation;
- signed error from the nearest directed branch of the exact aspect;
- absolute angular error;
- second-minus-first relative longitude speed and instantaneous phase;
- inclusive `within_orb`; and
- `proximity = clamp(1 - angular_error / orb, 0, 1)`.

Regular samples occur at the requested cadence and always include both query
endpoints. If the interval is not an exact multiple of the cadence, the final
step is shorter. At most 100,000 output samples are accepted.

## Exact passes and orb windows

Event discovery is independent of graph cadence. The internal scan step is
`min(cadence_seconds, 21600)` and is limited to 100,000 intervals. The solver
unwraps continuous second-minus-first relative longitude, enumerates both
directed branches for nonzero, non-opposition aspects, and bisects crossings.
Relative-speed reversals add stationary candidates so retrograde multiple
passes and tangencies are not discarded.

Every exact pass has a final bracket no wider than one second and angular
residual no greater than `1e-5` degrees. Refinement is capped at 80 iterations,
and candidates within one second are deduplicated deterministically. The same
rules cover conjunction wrap at 0°/360°, opposition, query-edge roots, and
stationary tangencies.

`AspectWindow` records refined inclusive-orb boundaries. A boundary clipped by
the requested interval is marked `start_truncated` or `end_truncated`.
An outside-to-outside touch at exactly the orb is represented as a deterministic
zero-duration window.

## Artifact and graph workflow

`AspectTimelineArtifact` contains schema version 1, the complete request, one
provider provenance record, regular samples, exact passes, windows, and solver
metadata. Compact JSON is canonical and feeds the SHA-256 content ID; pretty
JSON is display-only. Unknown fields and unsupported versions fail. During
deserialization, sample measurements, proximity, phase, fixed positions,
ordering, boundaries, limits, and solver constants are reconstructed and
checked. Astraeus enables lossless JSON float parsing so canonical bytes survive
a round trip.

A graph consumer normally plots `instant` against `signed_aspect_error_degrees`
for a signed waveform or against `proximity` for a normalized 0–1 curve. It can
overlay `exact_passes` as markers and `windows` as shaded regions without
rerunning an ephemeris.

## Provider selection

Both CLI calculation commands require an explicit provider:

```text
astraeus chart cast REQUEST --ephemeris moshier|swiss-files
astraeus timeline aspect REQUEST --ephemeris moshier|swiss-files [--ephemeris-path PATH] [--pretty]
```

Swiss mode resolves its directory from `--ephemeris-path`, then
`ASTRAEUS_SWISS_EPHEMERIS_PATH`, and verifies the pinned three-file bundle
before calculation. Moshier ignores that environment variable and rejects
Swiss-only objects such as Chiron.

## Fictional requests

The repository's [moving/moving example](../../examples/astraeus/timeline-moving-moving.json)
tracks fictional Sun/Moon conjunction data over one day and is directly usable
with Moshier:

```text
cargo run -p astraeus-cli -- timeline aspect \
  examples/astraeus/timeline-moving-moving.json --ephemeris moshier --pretty
```

A moving/fixed request uses the same outer fields and embeds an ordinary,
validated fictional chart:

```json
{
  "subject": {
    "kind": "moving_fixed",
    "chart": { "schema_version": 1, "request": "…", "result": "…" },
    "fixed_point": "ascendant",
    "moving_object": "saturn"
  },
  "aspect": { "kind": "square", "orb_degrees": 2.0 },
  "start": "2030-01-01T00:00:00Z",
  "end": "2030-02-01T00:00:00Z",
  "cadence_seconds": 21600
}
```

The ellipses above abbreviate the embedded `CalculationArtifact`; they are not
accepted literal request values.
