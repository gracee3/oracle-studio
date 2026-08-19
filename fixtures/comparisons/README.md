# Fictional transit comparison fixtures

These three compact JSON files are original, deterministic test/demo data made
for Oracle Studio. They were produced with Astraeus's `DeterministicMock` at
revision `e5d295222018178c46fb882a302a57c810bf8bd1`; they are not ephemeris
results and do not describe a person, event, or real location.

The fixed first layer uses the deliberately round timestamp
`2000-01-01T00:00:00Z`, zero-valued synthetic coordinates, and invented point
positions, nonuniform natal cusps, and all ten planets plus ASC, MC, DSC, IC,
and Vertex in an explicit comparison selection. The moving selections contain
all ten planets plus ASC and MC. Their invented 2026 positions exercise crowded
labels across 359°/0°, direct and retrograde motion, stations, a dense 12-hour
interpolation interval, a later 48-hour exact jump, angle aspects, and changing
engine-authored aspect sets.

The files contain complete Astraeus comparison envelopes so parsing also
failure-tests schema validation and aspect-tamper detection. They are licensed
AGPL-3.0-or-later with the rest of Oracle Studio.

Regenerate one canonical frame on standard output with:

```bash
cargo run -p oracle-studio-chart-view \
  --example generate_transit_fixtures --locked -- 1
```
