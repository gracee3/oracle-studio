//! Renderer-neutral chart data for SVG, tables, and future native clients.
//!
//! This crate formats already-calculated Astraeus artifacts. It performs no
//! astrology calculations and deliberately leaves geometry and glyph styling
//! to a renderer.

use astraeus_artifacts::CalculationArtifact;
use astraeus_core::ChartAngle;
use serde::Serialize;

pub const VIEW_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ChartViewModel {
    pub schema_version: u32,
    pub instant: String,
    pub zodiac: String,
    pub ayanamsa: Option<String>,
    pub points: Vec<PointView>,
    pub houses: Vec<f64>,
    pub angles: AnglesView,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PointView {
    pub id: String,
    pub longitude_degrees: f64,
    pub latitude_degrees: f64,
    pub distance_au: f64,
    pub longitude_speed_degrees_per_day: f64,
    pub retrograde: bool,
    pub sign_index: u8,
    pub degree_within_sign: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AnglesView {
    pub ascendant_degrees: f64,
    pub midheaven_degrees: f64,
    pub vertex_degrees: f64,
}

impl ChartViewModel {
    pub fn from_calculation(artifact: &CalculationArtifact) -> Self {
        let request = artifact.request();
        let result = artifact.result();
        let points = result
            .positions()
            .iter()
            .map(|(object, position)| {
                let longitude = position.longitude_degrees();
                PointView {
                    id: format!("{object:?}"),
                    longitude_degrees: longitude,
                    latitude_degrees: position.latitude_degrees(),
                    distance_au: position.distance_au(),
                    longitude_speed_degrees_per_day: position.longitude_speed_degrees_per_day(),
                    retrograde: position.is_retrograde(),
                    sign_index: (longitude / 30.0).floor() as u8,
                    degree_within_sign: longitude.rem_euclid(30.0),
                }
            })
            .collect();
        let angles = result.houses().angles();
        Self {
            schema_version: VIEW_SCHEMA_VERSION,
            instant: request.instant().as_datetime().to_rfc3339(),
            zodiac: format!("{:?}", request.zodiac()),
            ayanamsa: request.ayanamsa().map(|value| format!("{value:?}")),
            points,
            houses: result.houses().cusps_degrees().to_vec(),
            angles: AnglesView {
                ascendant_degrees: angles.get(ChartAngle::Ascendant).longitude_degrees(),
                midheaven_degrees: angles.get(ChartAngle::Midheaven).longitude_degrees(),
                vertex_degrees: angles.get(ChartAngle::Vertex).longitude_degrees(),
            },
        }
    }
}
