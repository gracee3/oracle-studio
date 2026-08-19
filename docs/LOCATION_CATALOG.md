# Offline GeoNames location catalog

Oracle Studio can use the GeoNames `cities500` distribution as an optional,
local-only place index. Nothing is downloaded at application launch, vault
unlock, or search time. The native host contacts the fixed official GeoNames
URLs only after the user presses **Download and install catalog**.

The installer retrieves:

- [`cities500.zip`](https://download.geonames.org/export/dump/cities500.zip);
- [`admin1CodesASCII.txt`](https://download.geonames.org/export/dump/admin1CodesASCII.txt); and
- [`admin2Codes.txt`](https://download.geonames.org/export/dump/admin2Codes.txt).

The parser bounds archive, extracted, line, alias, and row sizes; requires the
documented 19-column place shape; validates coordinates, population, optional
elevation, country code, and IANA time zone; and rejects duplicate GeoNames IDs.
It uses administrative-code files to preserve human-readable subdivision names.

## Storage and identity

The catalog is public data and remains outside the encrypted vault. The host
stores extracted source files and strict retrieval metadata beneath the
application data directory. A catalog content ID is SHA-256 over a framed
combination of the extracted cities, admin1, and admin2 bytes. Each content ID
has its own object directory; installing a replacement atomically changes only
the active pointer and does not remove the older object.

Metadata records the retrieval time, fixed source URLs, per-source hashes,
catalog content ID, place count, distribution URL, license, and attribution.
Files are owner-only on Unix. Symbolic-link catalog targets are rejected.

Saving a result copies its label, administrative names, country code,
coordinates, optional elevation, IANA zone, GeoNames ID, and catalog content ID
into the encrypted schema-v3 vault. Later catalog replacement or saved-location
editing cannot change an immutable chart calculation's location snapshot.

## Search contract

Queries never leave the process. Each place is matched against its primary,
ASCII, and alternate names using Unicode lowercase comparison. Results are
partitioned into exact matches, prefixes, then substrings. Within each partition
they are ranked by population descending and stable GeoNames ID ascending.
Limits are bounded and an empty or oversized query is rejected.

Manual entry remains available without any catalog. It requires an application
record ID, label, ISO two-letter country code, coordinates, and IANA time zone;
administrative names and elevation are optional.

## Attribution

Contains GeoNames geographical data, available under
[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). Source:
[GeoNames distribution](https://download.geonames.org/export/dump/).
