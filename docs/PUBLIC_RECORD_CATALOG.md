# Reviewed public-record catalog

Oracle Studio's public-record catalog is a small reviewed evidence set, not a
general celebrity database. It exists for deterministic calculation fixtures,
schema tests, provenance examples, and future demo composition. Version 1 is
stored at `catalog/public-records-v1.json`; its JSON Schema is beside it, and
`oracle-studio-public-records` applies stricter semantic and content-identity
validation offline.

## Approved records

| Record | Source precision | Catalog use |
|---|---|---|
| 2017-08-21 greatest eclipse | `18:25:30Z`, `37.0/-87.7` | Chart-ready Moshier fixture |
| 2024-04-08 greatest eclipse | `18:17:15Z`, `25.3/-104.1` | Chart-ready Moshier fixture |
| 1906 San Francisco earthquake | `13:12:26.300Z`, `37.750/-122.550` | Chart-ready Moshier fixture |
| Ada Lovelace | Day-precision birth/death; exact birth time unknown | Research-only provenance record |
| Alan Turing | Day precision; preferred and alternate birthplace statements | Research-only provenance record |
| Frida Kahlo | Day precision; preferred and alternate birthplace statements | Research-only provenance record |
| Nikola Tesla | Day-precision birth/death; exact birth time unknown | Research-only provenance record |

The earthquake record retains the source's 11.7 km hypocentral depth as event
metadata. Astrology calculation uses a surface elevation of zero meters; depth
is never converted into a negative chart elevation. The two eclipse points
retain NASA's stated 0.1-degree coordinates and one-second times rather than
implying greater accuracy.

No living people are present. A date-only person can never produce a
`CalculationRequest`: Oracle does not invent noon, coordinates, or a time zone.
All four people are therefore unsuitable for natal calculation or a demo chart
unless a separately licensed exact-time source is reviewed in the future.

## Source and rights inventory

### NASA GSFC eclipse predictions — approved with acknowledgment

The 2017 and 2024 Solar Eclipse Search Engine pages state the greatest-eclipse
instants and coordinates used here. They grant permission to reproduce the data
when accompanied by the acknowledgment “Eclipse Predictions by Fred Espenak,
NASA's GSFC”; that acknowledgment is present in every NASA record and the
third-party notice. Page precision and the stated path-edge accuracy are kept as
reliability notes. No map or page text is copied.

- 2017 source: <https://eclipse.gsfc.nasa.gov/SEsearch/SEdata.php?Ecl=20170821>
- 2024 source: <https://eclipse.gsfc.nasa.gov/SEsearch/SEdata.php?Ecl=20240408>

### USGS earthquake data — approved factual subset

The highest-weight preferred USGS origin for the 1906 event supplies the exact
time, epicenter, magnitude, and depth. Alternate historical origins remain
available at the source but are not silently blended into this fixture. USGS's
data-licensing guidance explains the U.S. public-domain status of works produced
wholly by federal employees and recommends explicit license metadata; Oracle
retains attribution and the exact origin-product revision.

- Event origin: <https://earthquake.usgs.gov/earthquakes/eventpage/iscgem16957905/origin>
- Licensing guidance: <https://www.usgs.gov/data-management/data-licensing>

### Wikidata structured data — approved CC0 factual subset

Wikidata states that structured data in its main namespace is CC0. The catalog
pins entity revisions and includes only selected labels, aliases, day-precision
dates, P19 statement IDs/ranks, and referenced place revisions. It imports no
Wikipedia/Wikidata prose or images. Preferred and alternate birthplace
statements are preserved instead of choosing a more precise-looking statement
without disclosing the source's rank.

- Licensing: <https://www.wikidata.org/wiki/Wikidata:Licensing>
- Entities: [Ada Lovelace](https://www.wikidata.org/wiki/Q7259),
  [Alan Turing](https://www.wikidata.org/wiki/Q7251),
  [Frida Kahlo](https://www.wikidata.org/wiki/Q5588), and
  [Nikola Tesla](https://www.wikidata.org/wiki/Q9036)

### Astro-Databank — reference only

Astro-Databank can be useful for identifying candidate sources and understanding
astrological time-rating practices, but public accessibility is not a license
to redistribute its database. Version 1 imports no Astro-Databank records,
ratings, article text, or images. A future import requires record-by-record
rights review and independent verification of the underlying cited source.

## Wire and identity contract

Schema v1 requires stable catalog and record IDs; explicit temporal,
coordinate, and time-scale precision; source URL and revision; rights and
attribution; reliability; ethical notes; chart readiness; and canonical
SHA-256 content IDs. Unknown fields, duplicate IDs, malformed timestamps,
non-finite coordinates, content-ID mismatches, old schemas, and inconsistent
chart readiness fail closed.

Record content IDs hash pinned compact JSON excluding only that record's
`content_id`. The catalog ID hashes the schema version, catalog ID, and ordered
records (including their content IDs), excluding only `catalog_content_id`.
JSON encoding is pinned by the workspace lock and Rust tests. Any factual or
provenance change therefore requires deliberate record and catalog identity
updates.

`fixtures/public-records/moshier-v1.json` fixes complete Astraeus calculation
artifact content IDs plus Sun, Moon, Ascendant, and Midheaven values for all
three chart-ready events. The request uses production's file-free Moshier
provider, tropical zodiac, Placidus houses, twelve supported celestial objects,
and surface elevation. Run `just public-records-check` entirely offline.

## Future candidates

High-value follow-up categories are government/open-data mundane events,
public astronomical events, calendar/time-zone edge cases, extreme latitudes,
and disputed records that explicitly exercise uncertainty without becoming
chart-ready. Each addition needs the same source revision, rights,
redistribution, precision, ethics, and deterministic-fixture review. Living
people, sensitive personal records, copied biographies, and unlicensed chart
collections remain out of scope.
