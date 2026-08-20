# Browser-local GeoNames catalog

The worker accepts either an image-pinned same-origin pack or three local files:
`cities500.zip`, `admin1CodesASCII.txt`, and `admin2Codes.txt`. Ordinary Trunk
builds contain no pack and retain upload/manual entry. The default Docker build
fetches official bytes, verifies `catalog/geonames.lock`, and exposes them only
on the application's static origin. No cross-origin runtime download occurs.

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
