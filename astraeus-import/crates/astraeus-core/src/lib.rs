//! Validated, provider-independent astrology calculation contracts.
//!
//! This crate intentionally contains no native ephemeris implementation. An
//! adapter must either return every requested object or fail the calculation.

mod adapter;
mod aspects;
mod error;
mod placements;
mod types;

pub use adapter::{DeterministicMock, EphemerisAdapter};
pub use aspects::{
    ASPECT_EXACT_TOLERANCE_DEGREES, ASPECT_STATION_TOLERANCE_DEGREES_PER_DAY, Aspect,
    AspectDefinition, AspectDefinitions, AspectKind, AspectMeasurement, AspectPhase,
    calculate_aspects, measure_aspect,
};
pub use error::{CalculationError, ValidationError};
pub use placements::{
    ChartPointSelection, HouseNumber, PointPlacement, SignPlacement, ZodiacSign,
    calculate_placements, chart_point_positions, house_for_longitude,
};
pub use types::{
    AngularPosition, Ayanamsa, CalculationOptions, CalculationProvenance, CalculationRequest,
    CalculationResult, CelestialObject, ChartAngle, ChartAngles, ChartPointId, EphemerisSource,
    GeographicLocation, HouseCusps, HouseSystem, Position, UtcInstant, Zodiac,
};
