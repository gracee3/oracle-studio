# Contributor and agent guidance

Astraeus is a headless, validation-first Rust astrology and ephemeris engine.
It owns calculation contracts, providers, provenance, canonical artifacts,
derived policies, comparisons, techniques, and events. It does not own people,
tarot, encrypted application storage, a GUI, or a Magnolia runtime.

Before changing implementation, read `README.md` and the affected contracts in
`docs/VALIDATION.md`, `docs/PROVENANCE.md`, `docs/ARTIFACTS.md`, and
`docs/SWISS_EPHEMERIS.md`. Read the more specific artifact, policy, comparison,
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
- After publication, send the exact commit, PR, validation, outcome, risks, and
  next action to the repository's external coordination record. Do not claim
  completion until that remote handoff is verified.
