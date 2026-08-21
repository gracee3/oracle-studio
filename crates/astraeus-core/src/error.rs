use crate::{CelestialObject, ChartPointId};
use thiserror::Error;

/// A malformed domain value, rejected before an ephemeris is invoked.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ValidationError {
    #[error("{field} must be finite")]
    NonFinite { field: &'static str },
    #[error("{field} must be in {minimum}..={maximum}, got {value}")]
    OutOfRange {
        field: &'static str,
        minimum: i32,
        maximum: i32,
        value: String,
    },
    #[error("at least one celestial object must be requested")]
    EmptyObjectSet,
    #[error("celestial objects must not contain duplicates: {0:?}")]
    DuplicateObject(CelestialObject),
    #[error("chart points must not contain duplicates: {0:?}")]
    DuplicateChartPoint(ChartPointId),
    #[error("sidereal calculations require an ayanamsa")]
    MissingAyanamsa,
    #[error("tropical calculations must not specify an ayanamsa")]
    UnexpectedAyanamsa,
    #[error("invalid RFC 3339 timestamp: {0}")]
    InvalidUtcInstant(String),
    #[error("house cusps must contain exactly 12 values, got {0}")]
    InvalidHouseCount(usize),
    #[error("house number must be in 1..=12, got {0}")]
    InvalidHouseNumber(u8),
    #[error("house cusps must be distinct and ordered through exactly one zodiac revolution")]
    InvalidHouseTopology,
    #[error("{field} must not be empty")]
    EmptyText { field: &'static str },
    #[error("aspect orb must be finite and in 0..=180 degrees, got {0}")]
    InvalidAspectOrb(String),
    #[error("aspect definitions must not contain duplicates: {0:?}")]
    DuplicateAspect(crate::AspectKind),
    #[error("aspect objects must be distinct and canonically ordered")]
    InvalidAspectPair,
    #[error("aspect separation must be finite and in 0..=180 degrees, got {0}")]
    InvalidAspectSeparation(String),
    #[error("aspect orb {actual} does not match the {kind:?} separation; expected {expected}")]
    InconsistentAspectOrb {
        kind: crate::AspectKind,
        expected: String,
        actual: String,
    },
    #[error("signed aspect separation must be finite and in (-180, 180], got {0}")]
    InvalidSignedAspectSeparation(String),
    #[error("signed aspect separation does not match unsigned separation")]
    InconsistentAspectSeparation,
    #[error("relative longitude speed must be finite, got {0}")]
    InvalidRelativeSpeed(String),
    #[error("aspect phase {actual:?} does not match calculated phase {expected:?}")]
    InconsistentAspectPhase {
        expected: crate::AspectPhase,
        actual: crate::AspectPhase,
    },
}

/// A complete calculation failed; partial results are never successful.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum CalculationError {
    #[error(transparent)]
    InvalidInput(#[from] ValidationError),
    #[error("ephemeris data is unavailable: {0}")]
    DataUnavailable(String),
    #[error("the provider does not support {0:?}")]
    UnsupportedObject(CelestialObject),
    #[error("calculation failed for {object:?}: {message}")]
    ObjectCalculation {
        object: CelestialObject,
        message: String,
    },
    #[error("provider returned no position for requested object {0:?}")]
    MissingObject(CelestialObject),
    #[error("provider returned an unrequested position for {0:?}")]
    UnexpectedObject(CelestialObject),
    #[error("ephemeris provider failed: {0}")]
    Provider(String),
}
