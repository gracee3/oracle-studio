# Astraeus

**Status:** Active research software. Core artifact and validation contracts are implemented, while interfaces remain pre-1.0. Automatic CI is currently manual-dispatch, so published checkpoints should be tied to a passing run or documented local acceptance.

Astraeus is a validation-first Rust astrology and ephemeris engine.

The project is intentionally beginning with a clean history. The former
`gracee3/aphrodite-rust` repository remains the legacy source and provenance
record; code and fixtures will be imported only after review.

## Initial scope

- A small headless `astraeus-core` crate.
- Explicit calculation inputs, outputs, validation, and failure semantics.
- A provider boundary for Swiss Ephemeris and possible future alternatives.
- Deterministic golden tests against pinned `swetest` output in explicit
  Moshier and Swiss-file modes.
- No GUI, HTTP service, database, Oracle Studio, tarot, or Magnolia dependency
  until the calculation foundation is independently validated.

## Track B handoff

Start with [the project organization and Track B handoff](docs/PROJECT_ORGANIZATION.md).
It records the repository boundaries, legacy sources, known defects, first
checkpoint, and non-goals.

The calculation contract lives in `astraeus-core`. The non-published
`astraeus-fixtures` crate verifies versioned external reference output without
adding a native ephemeris dependency. See [validation fixtures](docs/VALIDATION.md)
and the [Swiss Ephemeris integration policy](docs/SWISS_EPHEMERIS.md).

`astraeus-swiss` implements the provider contract with explicit Moshier and
Swiss-file modes. Swiss-file mode requires a caller-supplied data directory
and rejects silent fallback; no ephemeris data is bundled.

`astraeus-moshier` implements the same chart provider contract through the
pure-Rust `swisseph-rs` Moshier backend. It disables all file/JPL features,
performs no I/O, compiles for browser WebAssembly, and explicitly rejects
Chiron and dates outside the analytical ephemeris range.

Every successful result includes validated [calculation provenance](docs/PROVENANCE.md)
covering its provider, runtime version, ephemeris source, and optional pinned
data revision.

`astraeus-artifacts` provides the versioned, content-addressed
[calculation artifact format](docs/ARTIFACTS.md) for safe hand-off to storage,
APIs, and future composition applications.

The core also provides deterministic [aspect detection](docs/ASPECTS.md) over
validated positions, with explicit per-aspect orbs and canonical pair ordering.

`astraeus-timeseries` produces schema-v1 [aspect timelines](docs/TIMESERIES.md)
for one moving/fixed or moving/moving pair. Its canonical JSON includes regular
waveform samples, refined exact passes, inclusive orb windows, provider
provenance, and solver guarantees; rendering and interpretation stay downstream.

`astraeus-specifications` provides reusable schema-v1
[chart specifications](docs/CHART_SPECIFICATIONS.md) that combine calculation
choices and aspect policy without changing calculation artifact schema v1.

`astraeus-derived` combines a calculation artifact and matching specification
into a separately versioned, content-addressed
[derived chart artifact](docs/DERIVED_ARTIFACTS.md) with typed angles, derived
South Nodes, sign/house placements, and revalidated aspects.

`astraeus-western` adds separately versioned
[Western policy artifacts](docs/WESTERN_POLICIES.md) for traditional/modern
rulership, essential dignity, and selectable Chaldean/triplicity decans.

`astraeus-comparison` provides content-addressed
[two-chart comparisons](docs/COMPARISONS.md) with explicit layer identities and
motion semantics for synastry, transits, progressions, returns, and research.

`astraeus-techniques` implements versioned [Western chart
techniques](docs/TECHNIQUES.md): progressions, direct solar arcs, harmonics,
midpoint composites, and Davison charts.

`astraeus-events` provides [exact event solving](docs/EVENTS.md) for returns,
lunations, ingresses, seasonal points, and global eclipse maxima, producing
ordinary charts at the resolved instant.

The completed Western milestone is summarized in the [engine requirements
status](docs/ENGINE_REQUIREMENTS.md), including explicit Oracle Studio and
licensing boundaries.

## CLI

The `astraeus` binary is a deliberately thin JSON boundary for validated
calculation artifacts. It does not store people or client data and does not
perform local-time or location normalization; callers provide an exact UTC
request in the artifact.

```text
cargo run -p astraeus-cli -- chart cast request.json --ephemeris moshier
cargo run -p astraeus-cli -- timeline aspect examples/timeline-moving-moving.json --ephemeris moshier --pretty
cargo run -p astraeus-cli -- artifact canonicalize chart.json
cargo run -p astraeus-cli -- artifact canonicalize chart.json --pretty
cargo run -p astraeus-cli -- artifact inspect chart.json
```

Use `-` or omit the path to read standard input. Invalid, unknown, or
tampered artifact fields fail before anything is emitted. The canonicalize
command emits stable compact JSON by default; inspect emits content ID and
basic request metadata for scripts and composition clients.

`chart cast` accepts a strict `CalculationRequest` with an exact UTC instant
and coordinates. `timeline aspect` accepts a strict `AspectTimelineRequest`.
Both commands require `--ephemeris moshier|swiss-files`. Moshier ignores the
Swiss path environment and rejects file-only objects such as Chiron. Swiss mode
uses `--ephemeris-path`, then `ASTRAEUS_SWISS_EPHEMERIS_PATH`, and refuses to
run unless the three declared files match their pinned SHA-256 hashes. Neither
command resolves local time zones.

## Swiss-file setup

The optional Swiss-file adapter uses three files pinned by revision and SHA-256
in [the fixture provenance](fixtures/swetest-v2.10.03/SWISS_PROVENANCE.md).
Download them to the configured XDG data directory and run the selected adapter
test with:

```text
just swiss-setup
```

The default is `${XDG_DATA_HOME:-$HOME/.local/share}/astraeus/swisseph`. Set
`ASTRAEUS_SWISS_EPHEMERIS_PATH` or pass a directory to the recipe to use a
different location. To configure the current shell for direct Cargo commands:

```text
eval "$(just swiss-env)"
```

`just swiss-check` verifies the installed files without network access, and
`just swiss-test` reruns the ignored Swiss-file integration test. No `.se1`
file is committed to this repository.

## License

AGPL-3.0-or-later. Swiss Ephemeris has its own dual-license requirements; its
adapter and distribution implications must be documented before integration.
