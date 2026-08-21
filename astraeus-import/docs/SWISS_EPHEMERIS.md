# Swiss Ephemeris integration policy

## Pure-Rust Moshier adapter

`astraeus-moshier` uses `swisseph-rs` 0.2.0 with default features disabled.
The resulting provider is stateless, file-free, and browser-compatible. Its
successful provenance names `swisseph-rs Moshier` version `0.2.0` and the
`moshier` source. It rejects Chiron and epochs outside the shared Moshier
planetary/lunar range rather than falling back or returning partial output.

The dependency is an AGPL-3.0-or-later pure-Rust port of Swiss Ephemeris
2.10.03. Astraeus distributes it under the same AGPL path described below.

## Native Swiss adapter

The `astraeus-swiss` crate uses the exactly pinned `sweph-sys` 0.3.0 raw
binding, which vendors Swiss Ephemeris C source. No executable or `.se1` data
file is committed.

## Licensing

Swiss Ephemeris uses a dual-license model: a developer must choose the GNU
Affero General Public License path or obtain a Swiss Ephemeris Professional
License before distributing software containing it or activating a public
service that uses it. The authoritative terms are maintained by Astrodienst:
<https://www.astro.com/swisseph/swephinfo_e.htm>.

Astraeus is AGPL-3.0-or-later and the planned open-source adapter will use the
AGPL path. A different licensing choice requires an explicit project decision
before native integration or distribution. This document is project policy,
not legal advice.

Ephemeris data files and compiled binaries must not be committed. Users of a
future adapter will provide their own data path, and tests requiring those
files will be an explicitly selected integration suite.

## Native global state

Swiss Ephemeris configuration includes process-global state such as the
ephemeris path and sidereal mode. The first native adapter must therefore:

1. serialize every library interaction behind one process-wide lock;
2. apply the complete path, zodiac, ayanamsa, and calculation flags while the
   lock is held for each request;
3. request speed explicitly and check the returned flags;
4. reject unsupported objects and every per-object failure;
5. treat Swiss-file-to-Moshier fallback as an error when Swiss-file mode was
   selected; and
6. keep all native calls, including cleanup, inside the same synchronization
   boundary.

The adapter implements this policy with one outer lock across configuration,
all requested objects, fallback checks, and houses. Applications must not call
`sweph-sys` directly in the same process because that would bypass the lock.
Swiss mode rejects any returned Moshier source flag. Moshier mode is explicit
and rejects Chiron because it requires external data.

## Declared file bundle

`SwissEphemerisAdapter::pinned_swiss_files` verifies `sepl_18.se1`,
`semo_18.se1`, and `seas_18.se1` against the SHA-256 values in
`fixtures/swetest-v2.10.03/SWISS_PROVENANCE.md`, then records revision
`cae9ecd4b201544d85e411aced17660932514d43`. The CLI uses only this constructor
for Swiss mode; undeclared or modified bundles are not accepted in schema v1.
