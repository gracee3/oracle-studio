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

Workbench previews are not schema-v4 records. Files can commit one either by
updating the preview's source chart without changing its stable ID, or by
creating a separately named chart with a new stable ID. Both operations append
an immutable calculation; neither mutates an existing calculation record.

Text and collections are bounded, unknown fields are rejected, and hostile but
valid Unicode/HTML-like text round-trips as data. The current Astraeus revision
is `8637ceb64fa11a06c8680b46cb4b57c71d94d37f`; no sibling path dependency is
used.
