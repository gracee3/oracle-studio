# Fictional transit comparison fixtures

These three compact JSON files are original, deterministic test/demo data made
for Oracle Studio. They were produced with Astraeus's `DeterministicMock` at
revision `52d24862b8287b89b0986b7799583d82ecab21e9`; they are not ephemeris
results and do not describe a person, event, or real location.

The fixed first layer uses the deliberately round timestamp
`2000-01-01T00:00:00Z`, zero-valued synthetic coordinates, and invented point
positions. The moving frames use invented 2026 timestamps and positions chosen
to exercise direct and retrograde 359°/0° wrapping, label collisions, a dense
12-hour interpolation interval, a later 48-hour exact jump, and changing
engine-authored aspect sets.

The files contain complete Astraeus comparison envelopes so parsing also
failure-tests schema validation and aspect-tamper detection. They are licensed
AGPL-3.0-or-later with the rest of Oracle Studio.
