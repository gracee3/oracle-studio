# Fictional demo workspace

Oracle Studio's demo is an opt-in static build, not a production seed, account,
or privileged data path. The title is **Oracle Studio Demo** and the password is
the intentionally public, non-secret value `oracle-demo`. Never reuse it for a
real vault.

## Fixed records

The canonical native builder uses only Moshier calculations and the current
Standard aspect defaults. It creates:

- Avery North at fictional Juniper Harbor (`40.7128`, `-74.0060`,
  `America/New_York`), natal `1988-04-12 10:32:00` local;
- Mira Vale at fictional Cedar Observatory (`51.5074`, `-0.1278`,
  `Europe/London`), natal `1992-09-23 07:45:00` local;
- Harbor Transit at `2026-08-21 12:00:00` local;
- Cedar Equinox Event at `2026-03-20 14:00:00` local; and
- one synastry, one transit-to-natal, and one event-to-natal comparison.

Every record, definition, calculation, and comparison has a stable demo-only ID.
The compact canonical document, document SHA-256, and seven Astraeus artifact
content IDs must match
`fixtures/demo/oracle-studio-demo.lock.json`. The current branch builds the
schema-v4 document available on remote `main`; integrating a later vault-schema
branch requires deliberate regeneration and review of this lock.

## Commands

```bash
just demo-generate  # ignored plaintext, manifest, and fresh envelope
just demo-verify    # lock, deterministic plaintext, fresh randomness, reopen
just demo-build     # feature-gated static site in ignored var/demo/site
just demo-serve     # local loopback review server
just demo-test      # Rust, lock, Docker, and pinned-Chrome acceptance
```

`demo-generate` writes beneath `var/demo/generated`. Each invocation encrypts
the same reviewed fictional document with fresh cryptographic randomness, so
the `.oracle-vault` bytes are expected to differ. Generated plaintext and
encrypted files are ignored and must not be committed.

## Browser safety

Only demo builds show **Load demo workspace** and **Reset demo workspace**.
Both require confirmation. The same-origin asset's public envelope ID and title
must match the compiled demo identity before import. Load rejects an existing
demo ID; reset explicitly replaces only that ID. The ordinary vault engine then
imports and unlocks it, preserving unrelated vaults and all global preferences.
The browser acceptance suite exercises decline, load, record counts, Moshier
rendering, export, lock/unlock, reset, unrelated-vault preservation, and CSP.
