# Calculation artifacts

`astraeus-artifacts` defines the stable boundary between a calculation engine
and downstream storage, APIs, or applications. It contains no database or
Oracle Studio types.

Schema version 1 is a JSON object containing:

- `schema_version`;
- the complete validated `CalculationRequest`; and
- the complete `CalculationResult`, including provenance.

Because no Astraeus release or tag existed, schema v1 was finalized before its
first release to include validated ASC, MC, DSC, IC, and Vertex positions with
instantaneous longitude speeds. The earlier pre-release JSON shape and content
hashes are intentionally not supported.

Deserialization reconstructs every domain value through its validation path
and rechecks that the result contains exactly the request's object set. Unknown
fields and unsupported schema versions fail explicitly.

## Identity

`to_json` emits canonical compact JSON. Struct field order is fixed and object
positions use a `BTreeMap`, so identical artifacts produce identical bytes.
`content_sha256` hashes those compact bytes; `content_id` prefixes the lowercase
digest with `sha256:`. Pretty JSON is for display only and is never digest
input.

The workspace pins the JSON encoder version and tests a fixed schema-v1 digest
vector. Any future change to field order, number formatting, or canonical bytes
must introduce an explicit migration decision rather than silently changing
content identifiers.

Artifacts deliberately omit creation time, storage identifiers, people,
sessions, and filesystem paths. Applications may wrap an artifact with those
concerns without changing the calculation identity.

Aspect time series use their own schema-v1 envelope and content identity rather
than extending calculation artifact schema v1. See [aspect time
series](TIMESERIES.md).
