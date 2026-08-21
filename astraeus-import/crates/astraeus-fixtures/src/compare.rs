use std::{error::Error, fmt};

use astraeus_core::{CalculationResult, CelestialObject, ChartAngle};

use crate::FixtureTolerances;

// Chart-angle speeds are computed by the adapter with a documented symmetric
// 30-second sample rather than Swiss Ephemeris's internal derivative routine.
const ANGLE_SPEED_TOLERANCE_DEGREES_PER_DAY: f64 = 0.005;

#[derive(Clone, Debug, PartialEq)]
pub enum FixtureMismatch {
    MissingObject(CelestialObject),
    UnexpectedObject(CelestialObject),
    Numeric {
        path: String,
        expected: f64,
        actual: f64,
        tolerance: f64,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct FixtureComparisonError {
    mismatches: Vec<FixtureMismatch>,
}

impl FixtureComparisonError {
    pub fn mismatches(&self) -> &[FixtureMismatch] {
        &self.mismatches
    }
}

impl fmt::Display for FixtureComparisonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "fixture comparison found {} mismatch(es)",
            self.mismatches.len()
        )
    }
}

impl Error for FixtureComparisonError {}

pub(crate) fn compare_results(
    expected: &CalculationResult,
    actual: &CalculationResult,
    tolerances: FixtureTolerances,
) -> Result<(), FixtureComparisonError> {
    let mut mismatches = Vec::new();

    for (object, expected_position) in expected.positions() {
        let Some(actual_position) = actual.positions().get(object) else {
            mismatches.push(FixtureMismatch::MissingObject(*object));
            continue;
        };
        compare_angular(
            format!("positions.{object:?}.longitude_degrees"),
            expected_position.longitude_degrees(),
            actual_position.longitude_degrees(),
            tolerances.angle_degrees,
            &mut mismatches,
        );
        compare_linear(
            format!("positions.{object:?}.latitude_degrees"),
            expected_position.latitude_degrees(),
            actual_position.latitude_degrees(),
            tolerances.angle_degrees,
            &mut mismatches,
        );
        compare_linear(
            format!("positions.{object:?}.distance_au"),
            expected_position.distance_au(),
            actual_position.distance_au(),
            tolerances.distance_au,
            &mut mismatches,
        );
        compare_linear(
            format!("positions.{object:?}.longitude_speed_degrees_per_day"),
            expected_position.longitude_speed_degrees_per_day(),
            actual_position.longitude_speed_degrees_per_day(),
            tolerances.speed_degrees_per_day,
            &mut mismatches,
        );
    }
    for object in actual.positions().keys() {
        if !expected.positions().contains_key(object) {
            mismatches.push(FixtureMismatch::UnexpectedObject(*object));
        }
    }

    for (index, (expected_cusp, actual_cusp)) in expected
        .houses()
        .cusps_degrees()
        .iter()
        .zip(actual.houses().cusps_degrees())
        .enumerate()
    {
        compare_angular(
            format!("houses.cusps_degrees[{}]", index + 1),
            *expected_cusp,
            *actual_cusp,
            tolerances.angle_degrees,
            &mut mismatches,
        );
    }
    compare_angular(
        "houses.ascendant_degrees".into(),
        expected.houses().ascendant_degrees(),
        actual.houses().ascendant_degrees(),
        tolerances.angle_degrees,
        &mut mismatches,
    );
    compare_angular(
        "houses.midheaven_degrees".into(),
        expected.houses().midheaven_degrees(),
        actual.houses().midheaven_degrees(),
        tolerances.angle_degrees,
        &mut mismatches,
    );
    compare_angular(
        "houses.angles.vertex.longitude_degrees".into(),
        expected
            .houses()
            .angles()
            .get(ChartAngle::Vertex)
            .longitude_degrees(),
        actual
            .houses()
            .angles()
            .get(ChartAngle::Vertex)
            .longitude_degrees(),
        tolerances.angle_degrees,
        &mut mismatches,
    );
    for angle in [
        ChartAngle::Ascendant,
        ChartAngle::Midheaven,
        ChartAngle::Descendant,
        ChartAngle::ImumCoeli,
        ChartAngle::Vertex,
    ] {
        compare_linear(
            format!("houses.angles.{angle:?}.longitude_speed_degrees_per_day"),
            expected
                .houses()
                .angles()
                .get(angle)
                .longitude_speed_degrees_per_day(),
            actual
                .houses()
                .angles()
                .get(angle)
                .longitude_speed_degrees_per_day(),
            ANGLE_SPEED_TOLERANCE_DEGREES_PER_DAY,
            &mut mismatches,
        );
    }

    if mismatches.is_empty() {
        Ok(())
    } else {
        Err(FixtureComparisonError { mismatches })
    }
}

fn compare_angular(
    path: String,
    expected: f64,
    actual: f64,
    tolerance: f64,
    mismatches: &mut Vec<FixtureMismatch>,
) {
    let direct = (expected - actual).abs();
    let difference = direct.min(360.0 - direct);
    if difference > tolerance {
        mismatches.push(FixtureMismatch::Numeric {
            path,
            expected,
            actual,
            tolerance,
        });
    }
}

fn compare_linear(
    path: String,
    expected: f64,
    actual: f64,
    tolerance: f64,
    mismatches: &mut Vec<FixtureMismatch>,
) {
    if (expected - actual).abs() > tolerance {
        mismatches.push(FixtureMismatch::Numeric {
            path,
            expected,
            actual,
            tolerance,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angular_comparison_wraps_at_zero() {
        let mut mismatches = Vec::new();
        compare_angular(
            "longitude".into(),
            0.000_000_2,
            359.999_999_8,
            0.000_001,
            &mut mismatches,
        );
        assert!(mismatches.is_empty());
    }
}
