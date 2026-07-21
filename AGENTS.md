# Oracle Studio contributor guidance

Oracle Studio is the composition application for the Oracle family. Before
working here, read `../../internal/AGENTS.md` and
`../../internal/docs/CURRENT_HANDOFF.md`.

## Boundaries

- Oracle Studio owns people/client profiles, cross-domain sessions, journaling,
  encrypted local storage, backup/import/export, and deletion workflows.
- Astraeus owns astrology calculations and artifacts. Sibylla owns tarot
  readings and artifacts. Consume pinned public revisions; do not reproduce
  their domain types here.
- Magnolia integration is optional and domain-neutral.
- Never commit sibling path dependencies, secrets, personal data, charts, deck
  scans, copyrighted text/art, model weights, or ephemeris binaries.
- Encryption APIs must authenticate before deserializing plaintext. Tests use
  fictional data and explicit deterministic randomness seams only.
