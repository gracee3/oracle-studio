# Oracle Studio

Oracle Studio is the local-first composition application for Astraeus astrology
artifacts and Sibylla tarot artifacts. It owns people and professional-client
profiles, cross-domain sessions, journaling, and encrypted private storage.

The application is independent of Magnolia and remains useful offline. It does
not recalculate astrology or reinterpret tarot domain records.

## Status

Phase 5C offline application workflow: validated composition records, encrypted
atomic persistence, reusable tarot/chart/journal services, in-memory search, and
a guided command-line interface. There is no graphical UI, synchronization,
account system, AI layer, or camera recognition yet.

## CLI

Build with `cargo build --locked --bin oracle-studio`. Every command requires an
explicit `--vault` path. Passwords are hidden terminal prompts by default; for
automation, `--password-file` accepts an owner-only file and never places the
password in arguments or environment variables.

```text
oracle-studio --vault ./private/journal.vault init
oracle-studio --vault ./private/journal.vault person-add "Client Name" --professional-client
oracle-studio --vault ./private/journal.vault deck-import ./deck-manifest.json
oracle-studio --vault ./private/journal.vault reading-new --deck RECORD_ID --method manual
oracle-studio --vault ./private/journal.vault search "theme"
```

`reading-new` is a guided one-card, three-card, or freeform workflow. Manual
mode records confirmed physical cards and orientations; software mode always
uses Sibylla's operating-system-random production shuffle.

See the [Phase 5 plan](docs/PHASE_5_PLAN.md),
[composition model](docs/COMPOSITION_MODEL.md), and
[vault threat model](docs/VAULT.md).

## License

AGPL-3.0-or-later.
