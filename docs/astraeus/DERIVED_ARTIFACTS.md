# Derived chart artifacts

`astraeus-derived` defines schema version 1 for deterministic values derived
from a completed calculation and a chart specification. Its envelope contains:

- the unchanged calculation artifact schema v1;
- the unchanged chart specification schema v1;
- the complete ordered sign and house placement set for celestial objects,
  derived South Nodes, and chart angles;
- the complete ordered aspect result, including motion phase.

Construction requires the specification's calculation options to exactly
match the calculation request. House assignment follows half-open forward
zodiac arcs, so a point exactly on cusp N belongs to house N. Deserialization
revalidates both nested envelopes, recalculates every placement and aspect, and
rejects any difference. Compact JSON is content-addressed with SHA-256; display
formatting is never digest input.

This new outer schema does not add aspects to calculation artifact v1. A raw
ephemeris calculation therefore retains its established identity, while a
derived chart receives a separate identity that includes its aspect policy.
