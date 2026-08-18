# Contributor and agent guidance

Oracle Studio is the local-first composition application for Astraeus astrology
artifacts and Sibylla tarot artifacts. It owns people and professional-client
profiles, cross-domain sessions, journaling, deck-pack indexes, encrypted local
storage, backups, search, and deletion workflows. It does not recalculate
astrology, reinterpret tarot domain records, or require Magnolia.

Before changing implementation, read `README.md`, `docs/COMPOSITION_MODEL.md`,
`docs/VAULT.md`, and `docs/PHASE_5_PLAN.md`. Read `docs/CLI_TESTING.md` for CLI
work and `docs/DECK_PACKS.md` for local asset-pack behavior.

## Validation boundary

No narrow ordinary implementation check has yet been reviewed for this
repository. For instruction-only changes, run:

```bash
git diff --check
```

Before changing code, select and record the smallest relevant locked test
command. Do not infer permission to access personal data, deck images, sibling
worktrees, local vaults, or exceptional resources.

## Privacy, storage, and delivery

- Never commit secrets, passwords, vaults, backups, personal/client records,
  readings, charts, deck scans, copyrighted text or art, local asset indexes,
  model weights, or local paths.
- Consume pinned public Astraeus and Sibylla revisions through their published
  contracts; do not copy their domain types or use sibling path dependencies.
- Encryption APIs authenticate before deserializing plaintext. Keep atomic
  no-overwrite publication, key handling, permissions, backup recovery, and
  permanent-deletion behavior failure-tested with fictional data.
- Preserve AGPL-3.0-or-later obligations and record provenance and rights for
  every imported schema, fixture, or asset description.
- Use a focused feature branch. Commit and push the validated change and open a
  pull request; storage, encryption, schema, or dependency changes stay
  reviewable and must not auto-merge.
- After publication, send the exact commit, PR, validation, outcome, risks, and
  next action to the repository's external coordination record. Do not claim
  completion until that remote handoff is verified.
