# Local deck-pack contract

Sibylla deck manifests may contain opaque `asset_id` values, but Sibylla does
not own artwork, filesystem paths, licensing records, or image processing.
Oracle Studio resolves those references through an application-owned sidecar.

## Sidecar schema v1

`oracle-studio-assets` validates this shape:

```text
DeckPackManifest
- schema_version: 1
- pack_id
- deck_content_id: exact Sibylla deck artifact content ID (`sha256:...`)
- assets[]

DeckAsset
- asset_id: matches a Sibylla deck-card asset reference
- local_path: relative path below the supplied pack root
- sha256: lowercase/uppercase hexadecimal file digest
- mime
- width_pixels / height_pixels
- source

AssetSource
- file_page
- original_url
- license
- optional usage_terms
```

The sidecar rejects duplicate IDs, blank or malformed hashes, absolute paths,
`..` traversal, invalid source URLs, zero dimensions, and unknown JSON fields.
Verification refuses symbolic links and compares every file's SHA-256. A pack
cannot be used with a different deck manifest even if its card IDs happen to
match.

## CLI verification

After importing a deck artifact into a vault, verify a pack before rendering or
recognition:

```bash
cargo run --locked --bin oracle-studio -- \
  --vault /path/to/private/journal.vault \
  deck-pack-verify \
  --deck rider_waite_smith_geldard \
  /path/to/pack/assets.json \
  /path/to/pack
```

The pack JSON's `deck_content_id` must equal the canonical content ID of the
imported Sibylla deck artifact. The command reads files only for hashing; it
does not copy artwork into the vault or alter the immutable deck record.

After verification, bind the pack to the imported deck record so future
readings can inherit the verified asset context:

```bash
cargo run --locked --bin oracle-studio -- \
  --vault /path/to/private/journal.vault \
  deck-pack-bind \
  --deck rider_waite_smith_geldard \
  /path/to/pack/assets.json \
  /path/to/pack
```

Binding stores only the pack ID and exact deck content ID in the application
record. The image files remain outside the encrypted vault document.

## Asset retention boundary

The asset sidecar is metadata and a verification index. Future Oracle Studio
storage work must decide whether user-owned images are encrypted individually,
included in backups, or kept as external references. A missing asset must be a
visible UI state, never a reason to fabricate a card identity or rewrite a
reading.

Do not add proprietary scans, guidebook text, fonts, recognition weights, or
personal deck photographs to a public repository. Any redistribution requires
per-asset rights verification and pack provenance.
