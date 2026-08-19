# Composition model

Oracle Studio owns identifiers and relationships around engine artifacts:

```text
PersonProfile
- stable application ID
- display name
- personal | professional_client
- optional notes

Session
- stable application ID
- optional person ID
- title and optional context
- caller-supplied created/modified timestamps

ArtifactRecord
- stable application ID
- optional person and session IDs
- engine: astraeus | sibylla
- artifact kind, engine artifact schema version, and producing Git revision
- engine content ID
- exact canonical artifact JSON
- optional verified deck-pack ID and deck content ID snapshot

DeckPackManifest
- application-owned pack ID
- exact Sibylla deck artifact content ID
- card asset IDs, relative paths, hashes, dimensions, and source/license data

JournalEntry
- stable application ID
- optional person, session, and artifact sources
- annotation | outcome
- content and caller-supplied timestamp

SavedLocation
- encrypted place snapshot with label, administrative names, country, coordinates,
  optional elevation, and IANA time zone
- manual or GeoNames provenance; GeoNames snapshots retain the catalog content ID

ChartDefinition
- natal | event | transit role and optional person
- editable local date, local time, IANA zone, calculation policy, and ordered points
- optional current calculation and a per-person default-natal marker

ChartCalculation
- immutable local-input, resolved-offset, exact-UTC, and saved-location snapshots
- exact Astraeus calculation artifact and calculation timestamp

ComparisonPreset
- ordered inner/outer chart sources and point selections
- editable aspect definitions and wheel orientation
- exact calculation IDs and Astraeus comparison artifact for its current result

WorkspaceState
- active person and active comparison preset
```

The application validates all references before creating a vault document.
Engine artifacts remain immutable snapshots; annotations and outcomes are
separate application records. Updating an artifact creates a new record rather
than rewriting its identity.

Vault document schema v3 is an intentional pre-1.0 reset. It adds saved
locations, chart definitions, immutable calculations, comparison presets, and
workspace state. Schema v1 and v2 documents are rejected without migration;
all serialization writes canonical schema v3. The authenticated-encryption
envelope is unchanged.

References, unique IDs, one-default-natal-per-person, current calculation
ownership, comparison source calculations, and artifact kinds are validated on
construction and reopen. Editing a saved location cannot mutate a calculation's
embedded location snapshot. Recalculation appends a new calculation and
canonical artifact, then advances only the chart's current-result pointer.

Initial engine pins:

- Astraeus: `e5d295222018178c46fb882a302a57c810bf8bd1`
- Sibylla: `a154c32b83b110d2568a9ab10828b4f8b3dba7c7`

No sibling path dependency is permitted. The producing revision is stored per
record so future migrations can select the correct reader explicitly.

Deck-pack metadata is verified separately from the immutable artifact record.
It does not change Sibylla content IDs, enter the tarot reading snapshot, or
place image bytes in the encrypted vault document.
