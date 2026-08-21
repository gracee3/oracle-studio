# Schema-v5 chart document

Vault document schema v5 is an intentional storage break. Schemas 1–4 are
rejected without migration, including when older plaintext is validly
authenticated. In particular, a v4 open fails after decryption without
overwriting, deleting, or resealing the encrypted envelope bytes.
Canonical JSON contains exactly these record families:

```text
PersonProfile
SavedLocation
ChartDefinition -> optional current ChartCalculation
ChartCalculation -> embedded validated Astraeus calculation snapshot
ComparisonPreset -> optional current ComparisonCalculation
ComparisonCalculation -> exact inner/outer calculation IDs + embedded validated Astraeus comparison snapshot + immutable aspect-set snapshot and phase-aware result
WorkspaceState
```

Sessions, generic artifacts, tarot, journals, deck packs, and external artifact
references do not exist in v5. Astraeus snapshots are typed values inside their
immutable calculation records rather than separate generic records.

The aspect-set snapshot records its stable ID, revision, canonical content ID,
all five rules, and participating points. Validation recomputes the
phase/category-aware result from those rules. The nested Astraeus comparison
artifact remains schema v1 and byte-compatible with the legacy uniform-orb API.

Validation enforces unique stable IDs, reference integrity, one default natal
per person, exact local-input/resolved-offset/location snapshots, calculation
ownership, comparison source IDs, and current-result pointers. Recalculation
appends history and advances only the owning definition/preset pointer. Editing
a saved location never changes a calculation's embedded snapshot.

Text and collections are bounded, unknown fields are rejected, and hostile but
valid Unicode/HTML-like text round-trips as data. The current Astraeus revision
is `44af176ef8a85db2bbd7b57228710855a8fe6f3b`; no sibling path dependency is
used.
