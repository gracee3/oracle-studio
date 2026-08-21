use std::collections::BTreeMap;

use astraeus_core::{
    AngularPosition, CalculationError, CalculationProvenance, CalculationRequest,
    CalculationResult, CelestialObject, ChartAngles, EphemerisSource, HouseCusps, Position,
};
use chrono::{Datelike, Timelike};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SwetestParseError {
    #[error("line {line}: expected at least {expected} CSV fields, got {actual}")]
    FieldCount {
        line: usize,
        expected: usize,
        actual: usize,
    },
    #[error("line {line}: invalid number in {field}: {value}")]
    Number {
        line: usize,
        field: &'static str,
        value: String,
    },
    #[error("line {line}: unsupported swetest row {0}", .row)]
    UnsupportedRow { line: usize, row: String },
    #[error("line {line}: duplicate row {0}", .row)]
    DuplicateRow { line: usize, row: String },
    #[error("line {line}: expected timestamp {expected}, got {actual}")]
    Timestamp {
        line: usize,
        expected: String,
        actual: String,
    },
    #[error("missing house row {0}")]
    MissingHouse(usize),
    #[error("missing Ascendant row")]
    MissingAscendant,
    #[error("missing MC row")]
    MissingMidheaven,
    #[error("missing Vertex row")]
    MissingVertex,
    #[error(transparent)]
    InvalidValue(#[from] astraeus_core::ValidationError),
    #[error(transparent)]
    InvalidResult(#[from] CalculationError),
}

pub fn parse_swetest_output(
    request: &CalculationRequest,
    input: &str,
) -> Result<CalculationResult, SwetestParseError> {
    let mut positions = BTreeMap::new();
    let mut cusps: [Option<f64>; 12] = [None; 12];
    let mut ascendant = None;
    let mut midheaven = None;
    let mut vertex = None;
    let instant = request.instant().as_datetime();
    let expected_timestamp = format!(
        "{:02}.{:02}.{} {:02}:{:02}:{:02} UT",
        instant.day(),
        instant.month(),
        instant.year(),
        instant.hour(),
        instant.minute(),
        instant.second(),
    );

    for (index, raw_line) in input.lines().enumerate() {
        let line = index + 1;
        if raw_line.trim().is_empty() {
            continue;
        }
        let fields: Vec<_> = raw_line.split(',').map(str::trim).collect();
        require_fields(line, &fields, 2)?;
        if fields[1] != expected_timestamp {
            return Err(SwetestParseError::Timestamp {
                line,
                expected: expected_timestamp.clone(),
                actual: fields[1].into(),
            });
        }
        let name = fields[0];
        if let Some(object) = object_from_name(name) {
            require_fields(line, &fields, 6)?;
            let position = Position::new(
                number(line, "longitude", fields[2])?,
                number(line, "latitude", fields[3])?,
                number(line, "distance", fields[4])?,
                number(line, "longitude speed", fields[5])?,
            )?;
            if positions.insert(object, position).is_some() {
                return Err(SwetestParseError::DuplicateRow {
                    line,
                    row: name.into(),
                });
            }
        } else if let Some(house_number) = name.strip_prefix("house ") {
            require_fields(line, &fields, 3)?;
            let house_number: usize =
                house_number
                    .trim()
                    .parse()
                    .map_err(|_| SwetestParseError::Number {
                        line,
                        field: "house number",
                        value: house_number.into(),
                    })?;
            if !(1..=12).contains(&house_number) {
                return Err(SwetestParseError::UnsupportedRow {
                    line,
                    row: name.into(),
                });
            }
            let value = number(line, "house cusp", fields[2])?;
            if cusps[house_number - 1].replace(value).is_some() {
                return Err(SwetestParseError::DuplicateRow {
                    line,
                    row: name.into(),
                });
            }
        } else if name == "Ascendant" {
            require_fields(line, &fields, 4)?;
            if ascendant
                .replace(AngularPosition::new(
                    number(line, "ascendant", fields[2])?,
                    number(line, "ascendant speed", fields[3])?,
                )?)
                .is_some()
            {
                return Err(SwetestParseError::DuplicateRow {
                    line,
                    row: name.into(),
                });
            }
        } else if name == "MC" {
            require_fields(line, &fields, 4)?;
            if midheaven
                .replace(AngularPosition::new(
                    number(line, "midheaven", fields[2])?,
                    number(line, "midheaven speed", fields[3])?,
                )?)
                .is_some()
            {
                return Err(SwetestParseError::DuplicateRow {
                    line,
                    row: name.into(),
                });
            }
        } else if name == "Vertex" {
            require_fields(line, &fields, 4)?;
            if vertex
                .replace(AngularPosition::new(
                    number(line, "vertex", fields[2])?,
                    number(line, "vertex speed", fields[3])?,
                )?)
                .is_some()
            {
                return Err(SwetestParseError::DuplicateRow {
                    line,
                    row: name.into(),
                });
            }
        } else if !matches!(
            name,
            "ARMC" | "equat. Asc." | "co-Asc. W.Koch" | "co-Asc Munkasey" | "Polar Asc."
        ) {
            return Err(SwetestParseError::UnsupportedRow {
                line,
                row: name.into(),
            });
        }
    }

    let cusps = cusps
        .into_iter()
        .enumerate()
        .map(|(index, value)| value.ok_or(SwetestParseError::MissingHouse(index + 1)))
        .collect::<Result<Vec<_>, _>>()?;
    let houses = HouseCusps::new(
        cusps,
        ChartAngles::new(
            ascendant.ok_or(SwetestParseError::MissingAscendant)?,
            midheaven.ok_or(SwetestParseError::MissingMidheaven)?,
            vertex.ok_or(SwetestParseError::MissingVertex)?,
        )?,
    )?;
    let provenance =
        CalculationProvenance::new("swetest", "unknown", EphemerisSource::Synthetic, None)?;
    Ok(CalculationResult::new(
        request, positions, houses, provenance,
    )?)
}

fn object_from_name(name: &str) -> Option<CelestialObject> {
    match name {
        "Sun" => Some(CelestialObject::Sun),
        "Moon" => Some(CelestialObject::Moon),
        "Mercury" => Some(CelestialObject::Mercury),
        "Venus" => Some(CelestialObject::Venus),
        "Mars" => Some(CelestialObject::Mars),
        "Jupiter" => Some(CelestialObject::Jupiter),
        "Saturn" => Some(CelestialObject::Saturn),
        "Uranus" => Some(CelestialObject::Uranus),
        "Neptune" => Some(CelestialObject::Neptune),
        "Pluto" => Some(CelestialObject::Pluto),
        "mean Node" => Some(CelestialObject::MeanNode),
        "true Node" => Some(CelestialObject::TrueNode),
        "Chiron" => Some(CelestialObject::Chiron),
        _ => None,
    }
}

fn require_fields(line: usize, fields: &[&str], expected: usize) -> Result<(), SwetestParseError> {
    if fields.len() >= expected {
        Ok(())
    } else {
        Err(SwetestParseError::FieldCount {
            line,
            expected,
            actual: fields.len(),
        })
    }
}

fn number(line: usize, field: &'static str, value: &str) -> Result<f64, SwetestParseError> {
    value.parse().map_err(|_| SwetestParseError::Number {
        line,
        field,
        value: value.into(),
    })
}
