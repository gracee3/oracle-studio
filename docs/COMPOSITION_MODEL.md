# Schema-v4 chart document

Vault document schema v4 is an intentional reset. Schemas 1–3 are rejected
without migration, including when older plaintext is validly authenticated.
Canonical JSON contains exactly these record families:

```text
PersonProfile
SavedLocation
ChartDefinition -> optional current ChartCalculation
ChartCalculation -> embedded validated Astraeus calculation snapshot
ComparisonPreset -> optional current ComparisonCalculation
ComparisonCalculation -> exact inner/outer calculation IDs + embedded validated Astraeus comparison snapshot
WorkspaceState
```

Sessions, generic artifacts, tarot, journals, deck packs, and external artifact
references do not exist in v4. Astraeus snapshots are typed values inside their
immutable calculation records rather than separate generic records.

Validation enforces unique stable IDs, reference integrity, one default natal
per person, exact local-input/resolved-offset/location snapshots, calculation
ownership, comparison source IDs, and current-result pointers. Recalculation
appends history and advances only the owning definition/preset pointer. Editing
a saved location never changes a calculation's embedded snapshot.

Text and collections are bounded, unknown fields are rejected, and hostile but
valid Unicode/HTML-like text round-trips as data. The current Astraeus revision
is `e5d295222018178c46fb882a302a57c810bf8bd1`; no sibling path dependency is
used.
