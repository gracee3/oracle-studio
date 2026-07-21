# Phase 5: Oracle Studio MVP

## Phase 5A: foundation

- Bootstrap pinned Rust, CI, dependency policy, and repository boundaries.
- Define validated people/client, session, and artifact-record contracts.
- Define a versioned authenticated-encryption envelope and recovery tests.
- Keep engine integration at pinned public revisions without sibling paths.

Exit: fictional composition data can be serialized, encrypted with a password,
authenticated, reopened offline, and rejected on tampering or a wrong password.

## Phase 5B: durable local repository

Status: implemented.

- Add atomic writes, fsync strategy, lock handling, and crash recovery.
- Add encrypted backup/export and transactional import with conflict reporting.
- Add explicit deletion reporting without claiming physical media erasure.
- Keep all records inside the encrypted document; no plaintext index is
  written in this phase.

Exit: a vault survives process interruption and supports tested backup,
recovery, import, export, and deletion behavior.

## Phase 5C: application workflow

Status: implemented as an offline CLI checkpoint.

- Add deck management and manual/software reading flows using Sibylla.
- Add saved-chart ingestion using a selected Astraeus artifact contract.
- Add people, professional-client, session, annotation, and follow-up screens.
- Add a basic offline search and journal interface.

Exit: a user can complete and recover the first encrypted tarot workflow and
associate tarot and astrology artifacts with a client/session.

## Phase 5D: memory and practitioner controls

- Add visible, editable, source-linked memory records.
- Add practitioner-private visibility and export controls.
- Add correction, forgetting, retention, and audit behavior.

AI interpretation, camera recognition, online accounts, synchronization,
subscriptions, and commercial deck packs remain later projects.
