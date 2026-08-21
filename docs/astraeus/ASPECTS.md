# Aspect calculations

`astraeus-core` detects aspects from validated typed chart-point longitudes
without calling an ephemeris provider. Points may be requested celestial
objects, derived mean/true South Nodes, or ASC, MC, DSC, IC, and Vertex. The
contract supports conjunction, sextile, square, trine, and opposition with an
explicit orb per aspect.

The additive phase-aware API accepts four orb values per aspect: luminary
applying/separating and other applying/separating. Either endpoint being Sun or
Moon selects the luminary category. Phase is measured before the applicable orb
is selected; exact and stationary cases use the documented deterministic
policy. The original single-orb API delegates through uniform four-value rules
and retains identical results and serialization.

Detection uses the shortest separation on the 360-degree circle. Orb limits
are inclusive. Results use canonical point ordering and contain the measured
separation and absolute distance from exactitude. When configured windows
overlap, only the closest aspect is returned for a pair; configuration order
does not affect results, and canonical aspect-kind order breaks an exact tie.

`measure_aspect` exposes the provider-independent measurement used by detection
and time-series consumers. `AspectMeasurement` contains signed and absolute
separation, signed error from the nearest directed branch of the requested
aspect, absolute angular error, relative longitude speed, and phase.

## Motion phase

Each detected aspect records signed separation from the canonically first
point to the second, their relative longitude speed (`second - first`), and an
auditable phase: `applying`, `exact`, `separating`, or `stationary`.

The engine selects the signed branch of the exact aspect angle nearest the
current signed separation. Motion is applying when the instantaneous relative
speed reduces the signed deviation from that target and separating when it
increases the deviation. Circular normalization uses the interval
`(-180°, 180°]`, so motion remains consistent across the 0° conjunction and
180° opposition boundaries.

Exactitude takes precedence when absolute deviation is at most `1e-9` degrees.
Otherwise, relative speed at or below `1e-12` degrees/day is stationary. These
thresholds are public constants. Phase is instantaneous, not a prediction that
an aspect will perfect before either object's motion changes. Serialized
aspects revalidate separation, orb, relative speed, and phase consistency.

Swiss chart-angle speed uses a symmetric 30-second sample. Golden comparison
allows 0.005 degrees/day for this numerical derivative while retaining the
tighter configured tolerance for celestial objects.
