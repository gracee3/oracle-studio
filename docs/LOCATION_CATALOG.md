# Browser-local GeoNames catalog

The worker accepts either an image-pinned same-origin pack or three local files:
`cities500.zip`, `admin1CodesASCII.txt`, and `admin2Codes.txt`. Ordinary Trunk
builds contain no pack and retain upload/manual entry. `just geonames-download`
fetches official bytes through a temporary directory into ignored
`var/geonames/source` only after exact SHA-256 and byte-length verification.
`just geonames-check` is fully offline. `geonames-build` and `geonames-serve`
stage the same verified bytes, a strict worker manifest, and `ATTRIBUTION.txt`
into Trunk output. No cross-origin runtime download occurs.

The default Docker target calls the same `scripts/geonames.py` download and
stage implementation. The `acceptance-runtime` target is deliberately
catalog-free. A changed daily upstream file fails without replacing a verified
cache or altering the tracked lock. `just geonames-candidate-lock` downloads
current bytes beneath ignored `var/geonames/candidate` and writes an ignored
candidate lock for explicit review.

Parsing bounds archive, extracted, admin, line, alias, query, result, and row
sizes; validates the 19-column cities shape, IDs, coordinates, elevation,
country, and IANA zones; and rejects duplicate place IDs. Search matches Unicode
lowercase primary, ASCII, and alternate names, ranked deterministically by
exact/prefix/substring, population descending, then GeoNames ID ascending.

IndexedDB catalog objects retain source bytes, per-file hashes and lengths,
retrieval kind/time, content ID, license metadata, and an active pointer.
Replacing the pointer has no merge semantics. Selected location snapshots alone
enter encrypted vaults and retain GeoNames/content provenance.

Contains GeoNames geographical data, available under
[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). Source:
[GeoNames distribution](https://download.geonames.org/export/dump/).
GeoNames states that reuse, including commercial use, is allowed with credit.
Distributed catalog builds therefore keep the attribution in both the Settings
interface and the staged pack. This is project documentation, not legal advice.

## Swiss dependency and data audit

Production chart calculation remains file-free Moshier only.
`astraeus-moshier` uses `swisseph-rs` 0.2.0 with default and file features
disabled; it is an AGPL-3.0-or-later pure-Rust port and compiles into WASM.
`astraeus-swiss` uses the native-only `sweph-sys` 0.3.0 binding, which vendors
Swiss Ephemeris C source and remains outside the worker graph. Neither crate
causes `.se1` files to enter ordinary Trunk or production Docker builds.

The existing `astraeus-swiss-*` recipes remain the only optional Swiss-file
workflow. They download three files to the configured local data directory,
verify the pinned hashes, and run the selected native suite. The files are never
committed, copied into Trunk output, or embedded in Oracle's image. A local
user-initiated download does not make those bytes repository content, but it
does not waive Swiss Ephemeris license obligations. Astrodienst requires a
developer to choose the AGPL or Professional License before distributing
software containing Swiss Ephemeris or activating a public service. Oracle
Studio uses the AGPL path; any embedded `.se1` distribution or different
license choice requires a new explicit legal and release review under the
[official terms](https://www.astro.com/swisseph/swephinfo_e.htm).
