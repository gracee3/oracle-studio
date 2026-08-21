# Calculation provenance

Every successful `CalculationResult` contains validated provenance. Consumers
must persist it with the positions and houses rather than infer the calculation
source from configuration or deployment state.

The record contains:

- `provider`: the implementation that produced the result;
- `provider_version`: the native provider version reported at runtime;
- `ephemeris_source`: `synthetic`, `moshier`, or `swiss_files`; and
- `data_revision`: the caller-supplied immutable data revision when known.

The deterministic mock identifies synthetic results. The native adapter reads
the Swiss Ephemeris version from the linked library for every atomic
calculation. `swiss_files_with_revision` should be used when data came from a
pinned distribution; the path itself is intentionally omitted so machine- or
user-specific filesystem details do not leak into serialized artifacts.

Provenance describes a successful calculation, not an attempted one. Failures
return no partial result and therefore no success provenance. Operational logs
may separately record failed attempts without changing this domain contract.

An aspect timeline records one provider provenance value for all transit
samples. Calculation fails if the provider identity, version, source, or data
revision changes during a request. A moving/fixed request also embeds the fixed
chart's independent calculation provenance.
