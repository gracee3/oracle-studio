# Aspect sets

Aspect sets are versioned, global, unencrypted preferences for new Oracle
Studio previews. They never contain people, dates, locations, vault IDs, or
calculation results. The selected set controls only new previews and newly
saved comparisons. Every vault-v5 comparison calculation stores an immutable
snapshot of the resolved set ID, revision, content ID, complete rules, and
calculation-participating points, so later global edits cannot change a saved result.

## Calculation semantics

Each of the five Ptolemaic rules has an enabled flag and four inclusive orb
limits: luminary applying, luminary separating, other applying, and other
separating. A pair is luminary-classified when either endpoint is the Sun or
Moon. Astraeus measures instantaneous phase before choosing the applicable
orb. Exact aspects pass whenever the rule is enabled. Stationary or unavailable
phase uses the wider of the applying and separating values.

The legacy single-orb Astraeus APIs delegate through uniform four-value rules.
Their schema-v1 artifacts, canonical JSON, content IDs, provenance, and output
ordering are unchanged. The phase-aware result is additive and is stored by
Oracle beside—not inside—the schema-v1 comparison artifact.

## Oracle presets

Values are `luminary applying / luminary separating / other applying / other
separating`, in degrees.

| Preset | Conjunction / Opposition | Square / Trine | Sextile | Points |
|---|---:|---:|---:|---|
| Tight | 2 / 2 / 1 / 1 | 2 / 2 / 1 / 1 | 1.5 / 1.5 / 1 / 1 | Original 12 |
| Standard | 8 / 8 / 8 / 8 | 6 / 6 / 6 / 6 | 4 / 4 / 4 / 4 | Original 12 |
| Synastry | 10 / 8 / 8 / 6 | 8 / 6 / 6 / 5 | 6 / 5 / 4 / 3 | 19 browser points |
| Synwide | 12 / 10 / 10 / 8 | 10 / 8 / 8 / 6 | 8 / 6 / 6 / 4 | 19 browser points |

Standard is the compatibility default. Tight and Standard use Sun, Moon,
Mercury through Pluto, Ascendant, and Midheaven. Synastry and Synwide add mean
and true North/South Nodes, Descendant, IC, and Vertex. Chiron is excluded
because the production Moshier adapter cannot supply it.

These are Oracle project defaults, not universal astrological standards. The
Huber method uses a more specific system in which aspect effectiveness depends
on planet/aspect combinations, while traditional/medieval moiety systems derive
reach from the planetary orbs of both endpoints. Those models do not map
faithfully to four per-aspect values. Oracle therefore documents but does not
ship a guessed “Huber,” “Medieval,” or “moiety” preset. Research references:
[Astrodienst Huber Method](https://www.astro.com/astrowiki/en/Huber_Method),
[Deborah Houlding on traditional aspects](https://www.skyscript.co.uk/aspects.html),
and the [School of Traditional Astrology moiety definition](https://sta.co/portal/mod/glossary/showentry.php?eid=440).

## Global workflows and JSON v2

Settings supports selecting, creating by duplication, editing rules and independent
displayed/aspected point selections,
renaming, deleting, resetting built-ins, importing, and exporting. Built-ins are
immutable but duplicable. Reset restores only the four reviewed built-ins and
retains user sets.

The exchange format is defined by
[`schemas/aspect-set-v2.schema.json`](../schemas/aspect-set-v2.schema.json).
Canonical identity covers every field except `content_id`, which is the
lowercase SHA-256 of compact canonical JSON. Imports are limited to 64 KiB,
deny unknown fields, require all five unique rules and display orders, allow
only finite `0..=30` degree values and the 19 supported points, reject reserved
`builtin.*` IDs, and never replace an existing ID. An exported built-in is
useful for inspection; create an editable copy in Settings rather than trying
to import its reserved identity.

Global selection is stored in IndexedDB under settings schema v1. Changing,
editing, importing, deleting, or resetting a set invalidates any worker-held
pending preview. A new preview must finish under the resolved selection before
it can be committed.

The browser store trait owns persistence, while the aspect-set model remains
independent of IndexedDB. This keeps local storage as the only current provider
without coupling the wire model to a future optional synchronization provider.
Demo builds expose the same four reviewed built-ins and do not overwrite an
existing global selection when a demo vault is loaded or reset.

Version 1 imports and locally persisted sets migrate by copying their single
`points` selection into both v2 selections. Displayed-only points render without
entering aspect calculation. Aspected-but-hidden points remain in calculated
data, but aspect lines connected to hidden endpoints are omitted from the wheel.
