# Contributor and agent guidance

Astraeus is a headless, validation-first Rust astrology and ephemeris engine.
It owns calculation contracts, providers, provenance, canonical artifacts,
derived policies, comparisons, techniques, and events. It does not own people,
tarot, encrypted application storage, a GUI, or a Magnolia runtime.

This is a historical copy of the standalone repository's guidance. It is not an
active instruction file. Active consolidated instructions are in the repository
root `AGENTS.md`. Within this directory, read `README.md` and the affected
contracts in `VALIDATION.md`, `PROVENANCE.md`, `ARTIFACTS.md`, and
`SWISS_EPHEMERIS.md`. Read the more specific artifact, policy, comparison,
technique, or event document for that surface.

## Ordinary validation

```bash
cargo test --locked
git diff --check
```

Uncached crates may use ordinary network access. The default suite must not
require ephemeris downloads, personal charts, a GUI, Docker, models, datasets,
or special hardware. External Swiss-file or reference-tool validation requires
an explicit selected suite and separately reviewed fixture provenance.

## Correctness, provenance, and delivery

- Validate public constructors and every deserialization path; reject partial,
  unknown, unsupported, or tampered artifacts explicitly.
- Keep UTC, location, provider, ephemeris mode, version, data revision, schema,
  ordering, and content-ID semantics explicit and deterministic.
- Use only fictional or non-personal public fixtures. Do not commit charts,
  client data, ephemeris binaries, secrets, local paths, or private source
  material.
- Preserve AGPL-3.0-or-later obligations and document Swiss Ephemeris and other
  reference licenses before copying code or fixtures.
- Use a focused feature branch. Commit and push the validated change and open a
  pull request; incomplete or higher-risk work stays draft.
- The final two delivery bullets above describe the standalone repository's
  historical process. Consolidated Oracle Studio work follows the root
  `AGENTS.md` and has no external coordination-record requirement.
