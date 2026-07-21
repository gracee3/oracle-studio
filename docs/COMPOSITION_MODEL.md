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
- artifact kind and producing Git revision
- engine content ID
- exact canonical artifact JSON

JournalEntry
- stable application ID
- optional person, session, and artifact sources
- annotation | outcome
- content and caller-supplied timestamp
```

The application validates all references before creating a vault document.
Engine artifacts remain immutable snapshots; annotations and outcomes are
separate application records. Updating an artifact creates a new record rather
than rewriting its identity.

Vault document schema v2 adds ordered journal entries. Schema v1 documents are
accepted only through an explicit migration that supplies an empty journal;
all subsequent serialization writes schema v2. Journal sources must exist and
must not contradict the people or sessions attached to their source artifact.

Initial engine pins:

- Astraeus: `952a143b700ea5cad960498e7d8916a49ebb3691`
- Sibylla: `a154c32b83b110d2568a9ab10828b4f8b3dba7c7`

No sibling path dependency is permitted. The producing revision is stored per
record so future migrations can select the correct reader explicitly.
