//! Renderer-neutral chart data for SVG, tables, and future native clients.
//!
//! This crate formats already-calculated Astraeus artifacts. It performs no
//! astrology calculations and deliberately leaves geometry and glyph styling
//! to a renderer.

use astraeus_artifacts::CalculationArtifact;
use astraeus_core::{Aspect, ChartAngle};
use serde::Serialize;
use std::collections::BTreeSet;

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
pub struct PlacementRow {
    pub id: String,
    pub sign_index: u8,
    pub degree_within_sign: f64,
    pub house: u8,
    pub longitude_speed_degrees_per_day: f64,
    pub retrograde: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AspectRow {
    pub first: String,
    pub second: String,
    pub kind: String,
    pub separation_degrees: f64,
    pub orb_degrees: f64,
    pub phase: String,
}

impl AspectRow {
    pub fn from_aspects(aspects: &[Aspect]) -> Vec<Self> {
        aspects
            .iter()
            .map(|aspect| Self {
                first: format!("{:?}", aspect.first()),
                second: format!("{:?}", aspect.second()),
                kind: format!("{:?}", aspect.kind()),
                separation_degrees: aspect.separation_degrees(),
                orb_degrees: aspect.orb_degrees(),
                phase: format!("{:?}", aspect.phase()),
            })
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AnglesView {
    pub ascendant_degrees: f64,
    pub midheaven_degrees: f64,
    pub vertex_degrees: f64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct ChartSelection {
    selected_ids: BTreeSet<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ChartWorkspace {
    pub chart: ChartViewModel,
    pub placements: Vec<PlacementRow>,
    pub aspects: Vec<AspectRow>,
    pub selection: ChartSelection,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ChartExport {
    pub svg: String,
    pub placements: Vec<PlacementRow>,
    pub aspects: Vec<AspectRow>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LayerRole {
    Natal,
    Transit,
    Progressed,
    Synastry,
    Return,
    Custom,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ChartLayer {
    pub id: String,
    pub role: LayerRole,
    pub chart: ChartViewModel,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LayeredWorkspace {
    pub layers: Vec<ChartLayer>,
    pub selection: ChartSelection,
}

impl LayeredWorkspace {
    pub fn new(layers: Vec<ChartLayer>) -> Self {
        Self {
            layers,
            selection: ChartSelection::default(),
        }
    }

    pub fn layer(&self, id: &str) -> Option<&ChartLayer> {
        self.layers.iter().find(|layer| layer.id == id)
    }
}

impl ChartWorkspace {
    pub fn new(chart: ChartViewModel, aspects: Vec<AspectRow>) -> Self {
        let placements = chart.placement_rows();
        Self {
            chart,
            placements,
            aspects,
            selection: ChartSelection::default(),
        }
    }

    pub fn export(&self) -> ChartExport {
        ChartExport {
            svg: render_svg_with_selection(&self.chart, &self.selection),
            placements: self.placements.clone(),
            aspects: self.aspects.clone(),
        }
    }
}

impl ChartSelection {
    pub fn select(&mut self, id: impl Into<String>) {
        self.selected_ids.insert(id.into());
    }

    pub fn deselect(&mut self, id: &str) {
        self.selected_ids.remove(id);
    }

    pub fn clear(&mut self) {
        self.selected_ids.clear();
    }

    pub fn is_selected(&self, id: &str) -> bool {
        self.selected_ids.contains(id)
    }

    pub fn selected_ids(&self) -> impl Iterator<Item = &str> {
        self.selected_ids.iter().map(String::as_str)
    }
}

/// Render a deterministic, presentation-only SVG wheel.
///
/// The output intentionally uses plain SVG primitives so it can be embedded
/// in a web view, exported, or wrapped by a native client without introducing
/// a GUI dependency into the domain engine.
pub fn render_svg(view: &ChartViewModel) -> String {
    render_svg_with_selection(view, &ChartSelection::default())
}

/// Render an SVG wheel with selected points visually emphasized.
pub fn render_svg_with_selection(view: &ChartViewModel, selection: &ChartSelection) -> String {
    use std::fmt::Write;

    const SIZE: f64 = 600.0;
    const CENTER: f64 = SIZE / 2.0;
    const RADIUS: f64 = 220.0;
    let mut svg = String::new();
    let _ = write!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {SIZE} {SIZE}\" role=\"img\" aria-label=\"Astrology chart\">"
    );
    let _ = write!(
        svg,
        "<circle cx=\"{CENTER}\" cy=\"{CENTER}\" r=\"{RADIUS}\" fill=\"white\" stroke=\"black\"/>"
    );
    for (index, cusp) in view.houses.iter().enumerate() {
        let (x, y) = polar(*cusp, RADIUS);
        let _ = write!(
            svg,
            "<line x1=\"{CENTER}\" y1=\"{CENTER}\" x2=\"{x:.3}\" y2=\"{y:.3}\" stroke=\"#999\"/><text x=\"{x:.3}\" y=\"{y:.3}\" font-size=\"10\" text-anchor=\"middle\">{}</text>",
            index + 1
        );
    }
    for point in &view.points {
        let (x, y) = polar(point.longitude_degrees, RADIUS - 24.0);
        let label = escape_xml(&point.id);
        let fill = if selection.is_selected(&point.id) {
            "#c62828"
        } else {
            "black"
        };
        let marker_radius = if selection.is_selected(&point.id) {
            6
        } else {
            4
        };
        let _ = write!(
            svg,
            "<circle cx=\"{x:.3}\" cy=\"{y:.3}\" r=\"{marker_radius}\" fill=\"{fill}\"/><text x=\"{x:.3}\" y=\"{:.3}\" font-size=\"11\" text-anchor=\"middle\">{label}</text>",
            y - 7.0,
        );
    }
    svg.push_str("</svg>");
    svg
}

fn polar(longitude: f64, radius: f64) -> (f64, f64) {
    let radians = (90.0 - longitude).to_radians();
    (
        300.0 + radius * radians.cos(),
        300.0 - radius * radians.sin(),
    )
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
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

    /// Produce an accessible table representation from this same view model.
    pub fn placement_rows(&self) -> Vec<PlacementRow> {
        self.points
            .iter()
            .map(|point| PlacementRow {
                id: point.id.clone(),
                sign_index: point.sign_index,
                degree_within_sign: point.degree_within_sign,
                house: house_for_view_longitude(point.longitude_degrees, &self.houses),
                longitude_speed_degrees_per_day: point.longitude_speed_degrees_per_day,
                retrograde: point.retrograde,
            })
            .collect()
    }
}

fn house_for_view_longitude(longitude: f64, cusps: &[f64]) -> u8 {
    if cusps.len() != 12 {
        return 0;
    }
    for (index, start) in cusps.iter().copied().enumerate() {
        let end = cusps[(index + 1) % 12];
        let arc = (end - start).rem_euclid(360.0);
        let offset = (longitude - start).rem_euclid(360.0);
        if offset < arc {
            return index as u8 + 1;
        }
    }
    0
}
