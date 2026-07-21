# CLI testing guide

This guide exercises the complete offline tarot workflow with a throwaway,
encrypted vault. It uses an original one-card metadata fixture: it contains no
personal data, card artwork, guidebook text, or commercial-deck material.

Run all commands from the Oracle Studio repository. The `cargo run` form avoids
installing a binary globally. Substitute `./target/debug/oracle-studio` after
building if preferred.

## 1. Create a private test directory

On Unix, create a directory only you can access:

```bash
mkdir -m 700 ./oracle-studio-cli-test
```

Choose a path inside it for the test vault. `init` creates an encrypted file and
prompts twice for a new password; use a unique throwaway password for this
walkthrough.

```bash
cargo run --locked --bin oracle-studio -- \
  --vault ./oracle-studio-cli-test/journal.vault init
```

The vault must be supplied to every later command. Entering a password at the
hidden terminal prompt is the normal workflow.

For non-interactive local experiments only, create an owner-only password file:

```bash
umask 077
printf '%s\n' 'replace-this-throwaway-password' > ./oracle-studio-cli-test/password
```

Then add `--password-file ./oracle-studio-cli-test/password` after the vault
argument in every command below. Do not put a real vault password in a shell
history, command argument, or environment variable. On Unix, the CLI rejects a
password file with group or other read permissions.

## 2. Create a minimal deck manifest

Save this original metadata-only manifest as
`./oracle-studio-cli-test/minimal-deck.json`:

```json
{
  "schema_version": 1,
  "id": "cli_test_deck",
  "name": "CLI Test Deck",
  "attribution": {
    "author": "Oracle Studio contributors",
    "artist": null,
    "publisher": null
  },
  "tradition": "Original metadata-only test fixture",
  "rights": {
    "license": "AGPL-3.0-or-later",
    "source": null,
    "notes": "No artwork."
  },
  "reversal_rate_basis_points": 0,
  "cards": [
    {
      "id": "fool",
      "identity": { "kind": "conventional", "id": "fool" },
      "printed_title": "The Fool",
      "printed_number": null,
      "printed_suit": null,
      "printed_rank": null,
      "enabled": true,
      "asset_id": null,
      "correspondences": [],
      "notes": null
    }
  ]
}
```

For a deck you own, use your own metadata-only manifest or an authorized
Sibylla deck artifact. Do not commit, distribute, or upload scanned deck
images, guidebook text, or other copyrighted deck material.

## 3. Add a client, session, and deck

The explicit IDs below make the next commands easy to repeat. Names and notes
are encrypted in the vault after the command succeeds.

```bash
cargo run --locked --bin oracle-studio -- \
  --vault ./oracle-studio-cli-test/journal.vault \
  person-add "Test Client" --id test_client --professional-client

cargo run --locked --bin oracle-studio -- \
  --vault ./oracle-studio-cli-test/journal.vault \
  session-add "CLI test session" --id test_session --person test_client \
  --context "A fictional local test session."

cargo run --locked --bin oracle-studio -- \
  --vault ./oracle-studio-cli-test/journal.vault \
  deck-import ./oracle-studio-cli-test/minimal-deck.json --id test_deck

cargo run --locked --bin oracle-studio -- \
  --vault ./oracle-studio-cli-test/journal.vault deck-list
```

If using `--password-file`, place it immediately after the `--vault` argument
in each command. The CLI prints the encrypted vault revision after every saved
change; that value is an encrypted-file concurrency revision, not a tarot
artifact ID.

## 4. Record a manual reading

Start the guided workflow:

```bash
cargo run --locked --bin oracle-studio -- \
  --vault ./oracle-studio-cli-test/journal.vault \
  reading-new --deck test_deck --person test_client --session test_session \
  --method manual
```

Answer the prompts as follows:

| Prompt | Suggested test answer |
| --- | --- |
| Spread | press Enter for `one` |
| Question/intention | `What should this test demonstrate?` |
| Background/context | `Fictional CLI test.` |
| Reader notes | optional; press Enter to skip |
| Interpretation | optional; press Enter to skip |
| Deck card ID | `fool` |
| Orientation | `unspecified`, `upright`, or `reversed` |
| Card notes | optional; press Enter to skip |
| Save this reading? | `yes` |

The preview appears before saving. Answer `no` to prove that cancellation does
not change the vault.

To exercise the secure software path, rerun the same command with
`--method software`. Choose a one-card spread; this minimal deck has one
enabled card, so its identity is predictable, but the recorded draw provenance
still comes from Sibylla's operating-system-random shuffle. Use a fuller
authorized manifest to observe a varied draw.

## 5. Add outcomes and search

Add a source-linked follow-up and then search the unlocked in-memory vault:

```bash
cargo run --locked --bin oracle-studio -- \
  --vault ./oracle-studio-cli-test/journal.vault \
  annotate "Fictional follow-up: the workflow completed." \
  --id test_outcome --person test_client --session test_session --kind outcome

cargo run --locked --bin oracle-studio -- \
  --vault ./oracle-studio-cli-test/journal.vault \
  search "workflow"
```

Search is case-insensitive and runs only after the vault is decrypted in
memory. It creates no plaintext index, cache, query log, or temporary file.
Search results are intentionally plaintext on your terminal, so treat terminal
history and screen capture as part of your local threat model.

## 6. Test encrypted backup and recovery

Export creates an exact authenticated copy of the encrypted vault. Import first
authenticates the backup and then performs the same revision-checked atomic
replacement used by normal saves.

```bash
cargo run --locked --bin oracle-studio -- \
  --vault ./oracle-studio-cli-test/journal.vault \
  backup-export ./oracle-studio-cli-test/journal.backup

cargo run --locked --bin oracle-studio -- \
  --vault ./oracle-studio-cli-test/journal.vault \
  backup-import ./oracle-studio-cli-test/journal.backup
```

Use `person-list`, `session-list`, `deck-list`, and `search` after import to
confirm recovery. An incorrect password, a tampered vault, or a tampered backup
must fail authentication rather than producing data.

## Commands available today

```text
init
person-add | person-update | person-list
session-add | session-update | session-list
deck-import | deck-list
chart-import
reading-new
annotate
search
backup-export | backup-import
```

Use `--help` at any level for the exact supported flags, for example:

```bash
cargo run --locked --bin oracle-studio -- --help
cargo run --locked --bin oracle-studio -- reading-new --help
```

`chart-import` accepts a validated Astraeus calculation artifact; Oracle Studio
stores the imported artifact and its provenance but does not calculate or
repair charts. There is not yet a graphical interface, cloud sync, AI
interpretation, camera recognition, or a permanent-delete command in the CLI.
