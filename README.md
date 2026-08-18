# Oracle Studio

Oracle Studio is the local-first composition application for Astraeus astrology
artifacts and Sibylla tarot artifacts. It owns people and professional-client
profiles, cross-domain sessions, journaling, and encrypted private storage.

The application is independent of Magnolia and remains useful offline. It does
not recalculate astrology or reinterpret tarot domain records.

## Status

**Lifecycle:** Active pre-1.0 application. The CLI and storage contracts are implemented; interfaces and the future graphical workspace may evolve.

Phase 5C/5D integration checkpoint: validated composition records, encrypted
atomic persistence, reusable tarot/chart/journal services, validated local
Sibylla deck-pack indexes, in-memory search, a guided command-line interface,
and a stateless SVG/HTML transit-biwheel renderer. There is no native graphical
UI, synchronization, account system, AI layer, or camera recognition yet.

## CLI

The CLI is the current offline interface. Build and run it from this repository:

```bash
cargo build --locked --bin oracle-studio
./target/debug/oracle-studio --help
```

Every command needs an explicit `--vault` path. Passwords are hidden terminal
prompts by default. For non-interactive local testing, `--password-file` reads
an owner-only file; on Unix, files readable by group or others are rejected.
Passwords are never accepted as arguments or environment variables.

The core workflow is:

1. `init` an encrypted vault.
2. Add a person or professional client and an optional session.
3. Import a Sibylla deck manifest (raw manifest or deck artifact envelope).
4. Optionally verify a local deck-pack sidecar with `deck-pack-verify`.
5. Run `reading-new` with `--method manual` for physical cards or `--method software` for an OS-random shuffle.
6. Add annotations or outcomes, search the unlocked vault, and export an encrypted backup.

`reading-new` guides one-card, three-card, and freeform spreads. Manual mode
records confirmed deck-card IDs and upright, reversed, or unspecified
orientation. Software mode always uses Sibylla's operating-system-random
production shuffle; it has no deterministic production switch.

For a copy/paste walkthrough, minimal deck manifest, prompts, backup recovery,
and command reference, see [CLI testing guide](docs/CLI_TESTING.md).

Deck-pack indexes and image files remain application-owned. See the
[deck-pack contract](docs/DECK_PACKS.md) for the sidecar format; workspace-local
asset packs are documented separately from this public repository.

See the [Phase 5 plan](docs/PHASE_5_PLAN.md),
[composition model](docs/COMPOSITION_MODEL.md), and
[vault threat model](docs/VAULT.md).

## Transit chart export

`oracle-studio-chart` renders validated physical Astraeus transit-to-natal
comparison artifacts without opening a vault or recalculating astrology. It can
produce a deterministic SVG or a self-contained plain-JavaScript HTML player.
See the [renderer boundary, CLI, animation semantics, and privacy
notes](docs/CHART_RENDERING.md).

## License

AGPL-3.0-or-later.
