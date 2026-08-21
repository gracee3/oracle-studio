//! Versioned, provenance-aware golden fixtures for ephemeris validation.

mod compare;
mod fixture;
mod swetest;

pub use compare::{FixtureComparisonError, FixtureMismatch};
pub use fixture::{FixtureError, FixtureSource, FixtureTolerances, GoldenFixture};
pub use swetest::{SwetestParseError, parse_swetest_output};
