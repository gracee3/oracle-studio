# Astraeus engine requirements status

This matrix records the completed Western-engine milestone on `main`. It is an
Astraeus scope document, not an Oracle Studio application roadmap.

| Capability | Status | Implementation boundary |
| --- | --- | --- |
| Validated tropical/sidereal chart calculations | Complete | Core contracts plus explicit Moshier/Swiss-file adapter modes |
| One-pair aspect time series | Complete | Canonical schema-v1 samples, exact passes, orb windows, and explicit provider provenance |
| Angles, houses, signs, South Nodes, aspects, and motion phase | Complete | Typed derived chart artifacts with strict revalidation |
| Traditional/modern rulership, dignity, and two decan policies | Complete | Versioned Western policy artifacts |
| Synastry, transit, event, return, progressed, harmonic, and research comparisons | Complete | Physical, progressed, and synthetic chart layers with explicit motion policy |
| Secondary, tertiary-I, tertiary-II, and minor progressions | Complete | Direct methods with natal-fixed or recast angle policy |
| Naibod and true solar-arc directions | Complete | Direct only; all-points or angles-only application |
| Harmonics, midpoint composites, and Davison charts | Complete | No fabricated synthetic houses; antipodal Davison locations fail |
| Planetary returns | Complete | Configured-zodiac and engine-computed birth-epoch-ecliptic frames |
| New/full moons, ingresses, equinoxes, and solstices | Complete | Previous/nearest/next exact-time solving and event charts |
| Global solar/lunar eclipse maxima | Complete | Swiss-native maximum and classification; no local circumstances or paths |
| Canonical JSON, content IDs, provenance, and tamper checks | Complete | Separate schema-v1 artifacts per calculation/derived feature family |
| Local Rust verification and dependency policy | Complete | Format, tests, Clippy, Rustdoc, and `cargo-deny`; workflows remain manual-only |

## Deliberate boundaries

- People, professional clients, sessions, journaling, encryption, durable
  storage, and application workflows belong to Oracle Studio.
- Wheel/chart rendering, web/native UI, and visualization clients do not belong
  in the headless Astraeus engine.
- Jyotish methods, converse directions, eclipse magnitude/local visibility,
  contact times, and geographic eclipse paths are outside this milestone.
- Swiss Ephemeris file distribution remains a licensing/deployment decision;
  Astraeus bundles no `.se1` data and keeps file-backed verification opt-in.
- Oracle Studio remains pinned to its existing Astraeus revision until a
  separate application integration change intentionally upgrades it.
