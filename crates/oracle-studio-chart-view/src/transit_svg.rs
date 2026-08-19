use std::{collections::BTreeMap, fmt::Write, str::FromStr};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};

use crate::{ChartPoint, ChartScene, transit::stable_slug};

const SIZE: f64 = 720.0;
const CENTER: f64 = SIZE / 2.0;
const OUTER_RADIUS: f64 = 326.0;
const ASPECT_RADIUS: f64 = OUTER_RADIUS * 0.42;
const NATAL_INNER_RADIUS: f64 = ASPECT_RADIUS;
const NATAL_POSITION_RADIUS: f64 = OUTER_RADIUS * 0.51;
const NATAL_GLYPH_RADIUS: f64 = OUTER_RADIUS * 0.61;
const TRANSIT_INNER_RADIUS: f64 = OUTER_RADIUS * 0.66;
const TRANSIT_POSITION_RADIUS: f64 = OUTER_RADIUS * 0.75;
const TRANSIT_GLYPH_RADIUS: f64 = OUTER_RADIUS * 0.85;
const CUSP_INNER_RADIUS: f64 = OUTER_RADIUS * 0.90;
const CUSP_LABEL_RADIUS: f64 = OUTER_RADIUS * 0.95;
const LABEL_PADDING: f64 = 4.0;

const ASTRONOMICON_TTF: &[u8] =
    include_bytes!("../../../assets/astronomicon-v1.1/Astronomicon.ttf");
const SIGN_GLYPHS: [&str; 12] = ["A", "B", "C", "D", "E", "F", "G", "H", "I", "\\", "K", "L"];
const SIGN_NAMES: [&str; 12] = [
    "Aries",
    "Taurus",
    "Gemini",
    "Cancer",
    "Leo",
    "Virgo",
    "Libra",
    "Scorpio",
    "Sagittarius",
    "Capricorn",
    "Aquarius",
    "Pisces",
];

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WheelOrientation {
    #[default]
    AscendantLeft,
    ZodiacZeroTop,
}

impl FromStr for WheelOrientation {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "ascendant-left" => Ok(Self::AscendantLeft),
            "zodiac-zero-top" => Ok(Self::ZodiacZeroTop),
            _ => Err(format!(
                "unknown wheel orientation {value:?}; expected ascendant-left or zodiac-zero-top"
            )),
        }
    }
}

impl std::fmt::Display for WheelOrientation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::AscendantLeft => "ascendant-left",
            Self::ZodiacZeroTop => "zodiac-zero-top",
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderOptions {
    pub orientation: WheelOrientation,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            orientation: WheelOrientation::AscendantLeft,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PositionPrecision {
    ArcMinute,
    Degree,
}

impl PositionPrecision {
    const fn data_value(self) -> &'static str {
        match self {
            Self::ArcMinute => "arcminute",
            Self::Degree => "degree",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct PointGeometry {
    inner_radius: f64,
    position_radius: f64,
    glyph_radius: f64,
}

#[derive(Clone, Copy, Debug)]
struct PointLayout {
    display_longitude: f64,
    precision: PositionPrecision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RoundedPosition {
    sign_index: usize,
    degrees: u16,
    minutes: Option<u16>,
}

/// Render a deterministic SVG biwheel from a validated presentation scene.
pub fn render_biwheel_svg(scene: &ChartScene, options: &RenderOptions) -> String {
    let mut svg = String::with_capacity(48_000);
    let _ = write!(
        svg,
        "<svg id=\"oracle-transit-biwheel\" xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {SIZE:.0} {SIZE:.0}\" role=\"img\" aria-labelledby=\"chart-title chart-description\" data-orientation=\"{}\" data-ascendant=\"{:.12}\" data-center=\"{CENTER:.3}\" data-outer-radius=\"{OUTER_RADIUS:.3}\" data-aspect-radius=\"{ASPECT_RADIUS:.3}\" data-natal-inner-radius=\"{NATAL_INNER_RADIUS:.3}\" data-natal-position-radius=\"{NATAL_POSITION_RADIUS:.3}\" data-natal-glyph-radius=\"{NATAL_GLYPH_RADIUS:.3}\" data-transit-inner-radius=\"{TRANSIT_INNER_RADIUS:.3}\" data-transit-position-radius=\"{TRANSIT_POSITION_RADIUS:.3}\" data-transit-glyph-radius=\"{TRANSIT_GLYPH_RADIUS:.3}\" data-cusp-inner-radius=\"{CUSP_INNER_RADIUS:.3}\" data-cusp-label-radius=\"{CUSP_LABEL_RADIUS:.3}\" data-label-padding=\"{LABEL_PADDING:.3}\">",
        options.orientation, scene.natal.ascendant_degrees,
    );
    svg.push_str("<title id=\"chart-title\">Transit biwheel</title><desc id=\"chart-description\">Selected natal and transit points, natal cusps, and engine-authored inter-chart aspects.</desc>");
    render_font_and_style(&mut svg);
    let _ = write!(
        svg,
        "<circle class=\"wheel-background\" cx=\"{CENTER}\" cy=\"{CENTER}\" r=\"{OUTER_RADIUS}\"/>"
    );
    render_lane_backgrounds(&mut svg);
    render_houses(&mut svg, scene, options.orientation);
    render_aspects(&mut svg, scene, options.orientation);
    render_point_layer(
        &mut svg,
        "natal",
        &scene.natal.points,
        PointGeometry {
            inner_radius: NATAL_INNER_RADIUS,
            position_radius: NATAL_POSITION_RADIUS,
            glyph_radius: NATAL_GLYPH_RADIUS,
        },
        scene.natal.ascendant_degrees,
        options.orientation,
    );
    render_point_layer(
        &mut svg,
        "transit",
        &scene.transit,
        PointGeometry {
            inner_radius: TRANSIT_INNER_RADIUS,
            position_radius: TRANSIT_POSITION_RADIUS,
            glyph_radius: TRANSIT_GLYPH_RADIUS,
        },
        scene.natal.ascendant_degrees,
        options.orientation,
    );
    svg.push_str("</svg>");
    svg
}

fn render_font_and_style(svg: &mut String) {
    svg.push_str("<defs><style>@font-face{font-family:Astronomicon;src:url(data:font/ttf;base64,");
    svg.push_str(&BASE64.encode(ASTRONOMICON_TTF));
    svg.push_str(") format('truetype');font-style:normal;font-weight:400}");
    svg.push_str(CHART_STYLE);
    svg.push_str("</style></defs>");
}

fn render_lane_backgrounds(svg: &mut String) {
    let lane_width = OUTER_RADIUS * 0.24;
    let natal_radius = OUTER_RADIUS * 0.54;
    let transit_radius = OUTER_RADIUS * 0.78;
    let _ = write!(
        svg,
        "<g id=\"lane-backgrounds\" aria-hidden=\"true\"><circle class=\"lane-background lane-background--natal\" cx=\"{CENTER:.3}\" cy=\"{CENTER:.3}\" r=\"{natal_radius:.3}\" stroke-width=\"{lane_width:.3}\"/><circle class=\"lane-background lane-background--transit\" cx=\"{CENTER:.3}\" cy=\"{CENTER:.3}\" r=\"{transit_radius:.3}\" stroke-width=\"{lane_width:.3}\"/></g>"
    );
}

fn render_houses(svg: &mut String, scene: &ChartScene, orientation: WheelOrientation) {
    svg.push_str("<g id=\"natal-structure-layer\" class=\"layer layer--natal\" aria-label=\"Natal houses and cusp band\">");
    let _ = write!(
        svg,
        "<circle class=\"ring ring--aspect-boundary\" cx=\"{CENTER:.3}\" cy=\"{CENTER:.3}\" r=\"{ASPECT_RADIUS:.3}\"/><circle class=\"ring ring--cusp-boundary\" cx=\"{CENTER:.3}\" cy=\"{CENTER:.3}\" r=\"{CUSP_INNER_RADIUS:.3}\"/>"
    );
    for (index, cusp) in scene.natal.houses.iter().copied().enumerate() {
        let visual = visual_longitude(cusp, scene.natal.ascendant_degrees, orientation);
        let (x1, y1) = polar(visual, ASPECT_RADIUS);
        let (x2, y2) = polar(visual, CUSP_INNER_RADIUS);
        let axis_class = if matches!(index, 0 | 3 | 6 | 9) {
            " house-cusp--axis"
        } else {
            ""
        };
        let _ = write!(
            svg,
            "<line id=\"house-cusp-{}\" class=\"house-cusp{axis_class}\" data-longitude=\"{cusp:.12}\" x1=\"{x1:.3}\" y1=\"{y1:.3}\" x2=\"{x2:.3}\" y2=\"{y2:.3}\"><title>House {} cusp at {cusp:.12}°</title></line>",
            index + 1,
            index + 1
        );
        let rounded = round_position(cusp, PositionPrecision::ArcMinute);
        let (label_x, label_y) = polar(visual, CUSP_LABEL_RADIUS);
        let minutes = rounded
            .minutes
            .expect("arcminute precision includes minutes");
        let _ = write!(
            svg,
            "<g id=\"cusp-label-{}\" class=\"cusp-label\" data-role=\"cusp-label\" data-longitude=\"{cusp:.12}\" transform=\"translate({label_x:.3} {label_y:.3})\"><title>House {} cusp at {:02}°{:02}′ {}</title><text class=\"cusp-position\" y=\"-6\">{:02}°{:02}′</text><text data-role=\"cusp-sign\" class=\"astronomicon cusp-sign\" y=\"7\"><title>{} sign</title>{}</text></g>",
            index + 1,
            index + 1,
            rounded.degrees,
            minutes,
            SIGN_NAMES[rounded.sign_index],
            rounded.degrees,
            minutes,
            SIGN_NAMES[rounded.sign_index],
            SIGN_GLYPHS[rounded.sign_index]
        );
    }
    svg.push_str("</g>");
}

fn render_aspects(svg: &mut String, scene: &ChartScene, orientation: WheelOrientation) {
    let natal: BTreeMap<&str, f64> = scene
        .natal
        .points
        .iter()
        .map(|point| (point.id.as_str(), point.longitude_degrees))
        .collect();
    let transit: BTreeMap<&str, f64> = scene
        .transit
        .iter()
        .map(|point| (point.id.as_str(), point.longitude_degrees))
        .collect();
    svg.push_str(
        "<g id=\"aspect-layer\" class=\"layer layer--aspects\" aria-label=\"Inter-chart aspects\">",
    );
    for aspect in &scene.aspects {
        let (Some(natal_longitude), Some(transit_longitude)) = (
            natal.get(aspect.natal_point_id.as_str()),
            transit.get(aspect.transit_point_id.as_str()),
        ) else {
            continue;
        };
        let natal_visual =
            visual_longitude(*natal_longitude, scene.natal.ascendant_degrees, orientation);
        let transit_visual = visual_longitude(
            *transit_longitude,
            scene.natal.ascendant_degrees,
            orientation,
        );
        let (x1, y1) = polar(natal_visual, ASPECT_RADIUS);
        let (x2, y2) = polar(transit_visual, ASPECT_RADIUS);
        let kind = stable_slug(&aspect.kind);
        let title = escape_xml(&format!(
            "{} {} {} (orb {:.6}°, phase {})",
            aspect.natal_point_id,
            aspect.kind,
            aspect.transit_point_id,
            aspect.orb_degrees,
            aspect.phase.as_deref().unwrap_or("not supplied")
        ));
        let midpoint_x = (x1 + x2) / 2.0;
        let midpoint_y = (y1 + y2) / 2.0;
        let id = escape_xml(&aspect.id);
        let natal_id = escape_xml(&aspect.natal_point_id);
        let transit_id = escape_xml(&aspect.transit_point_id);
        let glyph = aspect_glyph(&aspect.kind);
        let _ = write!(
            svg,
            "<g id=\"{id}\" class=\"aspect aspect--{kind}\" data-natal-id=\"{natal_id}\" data-transit-id=\"{transit_id}\" data-kind=\"{kind}\"><title>{title}</title><line id=\"{id}--line\" data-role=\"aspect-line\" x1=\"{x1:.3}\" y1=\"{y1:.3}\" x2=\"{x2:.3}\" y2=\"{y2:.3}\"/><text id=\"{id}--glyph\" data-role=\"aspect-glyph\" class=\"aspect-glyph\" x=\"{midpoint_x:.3}\" y=\"{midpoint_y:.3}\">{glyph}</text></g>"
        );
    }
    svg.push_str("</g>");
}

fn aspect_glyph(kind: &str) -> &'static str {
    match kind {
        "Conjunction" => "!",
        "Sextile" => "%",
        "Square" => "#",
        "Trine" => "$",
        "Opposition" => "\"",
        _ => "·",
    }
}

#[allow(clippy::too_many_arguments)]
fn render_point_layer(
    svg: &mut String,
    layer: &str,
    points: &[ChartPoint],
    geometry: PointGeometry,
    ascendant: f64,
    orientation: WheelOrientation,
) {
    let visible: Vec<_> = points
        .iter()
        .filter(|point| layer != "natal" || !is_structural_natal_angle(&point.id))
        .collect();
    let layouts = layout_point_labels(&visible, geometry);
    let _ = write!(
        svg,
        "<g id=\"{layer}-layer\" class=\"layer layer--{layer}\" aria-label=\"{layer} points\">"
    );
    if layer == "transit" {
        let _ = write!(
            svg,
            "<circle class=\"ring ring--transit-boundary\" cx=\"{CENTER:.3}\" cy=\"{CENTER:.3}\" r=\"{TRANSIT_INNER_RADIUS:.3}\"/>"
        );
    }
    for (point, layout) in visible.into_iter().zip(layouts) {
        render_point(svg, layer, point, layout, geometry, ascendant, orientation);
    }
    svg.push_str("</g>");
}

#[allow(clippy::too_many_arguments)]
fn render_point(
    svg: &mut String,
    layer: &str,
    point: &ChartPoint,
    layout: PointLayout,
    geometry: PointGeometry,
    ascendant: f64,
    orientation: WheelOrientation,
) {
    let slug = stable_slug(&point.id);
    let actual_visual = visual_longitude(point.longitude_degrees, ascendant, orientation);
    let display_visual = visual_longitude(layout.display_longitude, ascendant, orientation);
    let (tick_inner_x, tick_inner_y) = polar(actual_visual, geometry.inner_radius - 4.0);
    let (tick_outer_x, tick_outer_y) = polar(actual_visual, geometry.inner_radius + 4.0);
    let (leader_x, leader_y) = polar(actual_visual, geometry.inner_radius + 4.0);
    let (leader_end_x, leader_end_y) = polar(display_visual, geometry.position_radius - 9.0);
    let (position_x, position_y) = polar(display_visual, geometry.position_radius);
    let (sign_x, sign_y) = polar(display_visual, geometry.position_radius + 12.0);
    let (glyph_x, glyph_y) = polar(display_visual, geometry.glyph_radius);
    let rounded = round_position(point.longitude_degrees, layout.precision);
    let position = match rounded.minutes {
        Some(minutes) => format!("{:02}°{minutes:02}′", rounded.degrees),
        None => format!("{:02}°", rounded.degrees),
    };
    let label = escape_xml(&format!(
        "{} {} at {:.6}°, speed {:.6}° per day{}",
        if layer == "natal" { "Natal" } else { "Transit" },
        point.id,
        point.longitude_degrees,
        point.longitude_speed_degrees_per_day,
        if point.retrograde { ", retrograde" } else { "" }
    ));
    let _ = write!(
        svg,
        "<g id=\"{layer}-point-{slug}\" class=\"chart-point chart-point--{layer} point--{slug}\" data-point-id=\"{}\" data-longitude=\"{:.12}\" data-display-longitude=\"{:.12}\" data-precision=\"{}\"><title>{label}</title>",
        escape_xml(&point.id),
        point.longitude_degrees,
        layout.display_longitude,
        layout.precision.data_value()
    );
    let _ = write!(
        svg,
        "<line id=\"{layer}-leader-{slug}\" data-role=\"leader\" class=\"point-leader\" x1=\"{leader_x:.3}\" y1=\"{leader_y:.3}\" x2=\"{leader_end_x:.3}\" y2=\"{leader_end_y:.3}\"/>"
    );
    let _ = write!(
        svg,
        "<line id=\"{layer}-tick-line-{slug}\" aria-hidden=\"true\" class=\"position-tick-line\" x1=\"{tick_inner_x:.3}\" y1=\"{tick_inner_y:.3}\" x2=\"{tick_outer_x:.3}\" y2=\"{tick_outer_y:.3}\"/><circle id=\"{layer}-tick-{slug}\" data-role=\"tick\" class=\"position-tick position-tick--{layer}\" cx=\"{:.3}\" cy=\"{:.3}\" r=\"3\"/>",
        (tick_inner_x + tick_outer_x) / 2.0,
        (tick_inner_y + tick_outer_y) / 2.0,
    );
    let _ = write!(
        svg,
        "<text id=\"{layer}-position-{slug}\" data-role=\"position\" data-sign-index=\"{}\" class=\"point-position\" x=\"{position_x:.3}\" y=\"{position_y:.3}\">{position}</text>",
        rounded.sign_index
    );
    let _ = write!(
        svg,
        "<text data-role=\"sign\" class=\"astronomicon point-sign\" x=\"{sign_x:.3}\" y=\"{sign_y:.3}\"><title>{} sign</title>{}</text>",
        SIGN_NAMES[rounded.sign_index], SIGN_GLYPHS[rounded.sign_index]
    );
    let glyph = point_glyph(&point.id);
    let _ = write!(
        svg,
        "<text data-role=\"glyph\" class=\"astronomicon point-glyph\" x=\"{glyph_x:.3}\" y=\"{glyph_y:.3}\"><title>{}</title>{glyph}</text>",
        escape_xml(&point.id)
    );
    if point.retrograde {
        let _ = write!(
            svg,
            "<text data-role=\"motion\" class=\"astronomicon motion-marker\" x=\"{:.3}\" y=\"{:.3}\"><title>Retrograde</title>N</text>",
            glyph_x + 9.0,
            glyph_y + 10.0
        );
    }
    svg.push_str("</g>");
}

fn is_structural_natal_angle(id: &str) -> bool {
    matches!(id, "Ascendant" | "Descendant" | "Midheaven" | "ImumCoeli")
}

fn point_glyph(id: &str) -> &'static str {
    match id {
        "Moon" => "R",
        "Mercury" => "S",
        "Venus" => "T",
        "Sun" => "Q",
        "Mars" => "U",
        "Jupiter" => "V",
        "Saturn" => "W",
        "Uranus" => "X",
        "Neptune" => "Y",
        "Pluto" => "Z",
        "Chiron" => "q",
        "MeanNode" | "TrueNode" => "g",
        "MeanSouthNode" | "TrueSouthNode" => "i",
        "Ascendant" => "c",
        "Midheaven" => "d",
        "Descendant" => "f",
        "ImumCoeli" => "e",
        "Vertex" => "k",
        _ => unreachable!("validated Astraeus chart-point identifier {id:?}"),
    }
}

fn round_position(longitude: f64, precision: PositionPrecision) -> RoundedPosition {
    match precision {
        PositionPrecision::ArcMinute => {
            let total = (longitude.rem_euclid(360.0) * 60.0).round() as i64;
            let total = total.rem_euclid(360 * 60) as u16;
            let sign_index = usize::from(total / (30 * 60));
            let within_sign = total % (30 * 60);
            RoundedPosition {
                sign_index,
                degrees: within_sign / 60,
                minutes: Some(within_sign % 60),
            }
        }
        PositionPrecision::Degree => {
            let total = longitude.rem_euclid(360.0).round() as i64;
            let total = total.rem_euclid(360) as u16;
            RoundedPosition {
                sign_index: usize::from(total / 30),
                degrees: total % 30,
                minutes: None,
            }
        }
    }
}

fn layout_point_labels(points: &[&ChartPoint], geometry: PointGeometry) -> Vec<PointLayout> {
    if points.is_empty() {
        return Vec::new();
    }
    if points.len() == 1 {
        return vec![PointLayout {
            display_longitude: points[0].longitude_degrees.rem_euclid(360.0),
            precision: PositionPrecision::ArcMinute,
        }];
    }

    let mut sorted: Vec<(usize, f64)> = points
        .iter()
        .enumerate()
        .map(|(index, point)| (index, point.longitude_degrees.rem_euclid(360.0)))
        .collect();
    sorted.sort_by(|left, right| {
        left.1
            .total_cmp(&right.1)
            .then_with(|| left.0.cmp(&right.0))
    });

    let mut cut_after = 0;
    let mut largest_gap = f64::NEG_INFINITY;
    for index in 0..sorted.len() {
        let gap = (sorted[(index + 1) % sorted.len()].1 - sorted[index].1).rem_euclid(360.0);
        if gap >= largest_gap {
            largest_gap = gap;
            cut_after = index;
        }
    }
    let start = (cut_after + 1) % sorted.len();
    let mut unwrapped = Vec::with_capacity(sorted.len());
    for offset in 0..sorted.len() {
        let entry = sorted[(start + offset) % sorted.len()];
        let mut angle = entry.1;
        while unwrapped
            .last()
            .is_some_and(|(_, previous): &(usize, f64)| angle < *previous)
        {
            angle += 360.0;
        }
        unwrapped.push((entry.0, angle));
    }

    let mut result = points
        .iter()
        .map(|point| PointLayout {
            display_longitude: point.longitude_degrees.rem_euclid(360.0),
            precision: PositionPrecision::ArcMinute,
        })
        .collect::<Vec<_>>();
    let mut cluster_start = 0;
    for index in 1..=unwrapped.len() {
        let continues = index < unwrapped.len()
            && unwrapped[index].1 - unwrapped[index - 1].1
                < required_gap_degrees(
                    points[unwrapped[index - 1].0],
                    points[unwrapped[index].0],
                    geometry,
                    PositionPrecision::ArcMinute,
                );
        if continues {
            continue;
        }
        if index - cluster_start > 1 {
            resolve_label_cluster(
                &unwrapped[cluster_start..index],
                points,
                geometry,
                &mut result,
            );
        }
        cluster_start = index;
    }
    result
}

fn resolve_label_cluster(
    cluster: &[(usize, f64)],
    points: &[&ChartPoint],
    geometry: PointGeometry,
    result: &mut [PointLayout],
) {
    let mut offsets = vec![0.0; cluster.len()];
    for index in 1..cluster.len() {
        offsets[index] = offsets[index - 1]
            + required_gap_degrees(
                points[cluster[index - 1].0],
                points[cluster[index].0],
                geometry,
                PositionPrecision::Degree,
            );
    }
    let adjusted: Vec<_> = cluster
        .iter()
        .enumerate()
        .map(|(index, (_, angle))| angle - offsets[index])
        .collect();
    let mean = adjusted.iter().sum::<f64>() / adjusted.len() as f64;
    for (index, (original_index, _)) in cluster.iter().enumerate() {
        result[*original_index] = PointLayout {
            display_longitude: (mean + offsets[index]).rem_euclid(360.0),
            precision: PositionPrecision::Degree,
        };
    }
}

fn required_gap_degrees(
    left: &ChartPoint,
    right: &ChartPoint,
    geometry: PointGeometry,
    precision: PositionPrecision,
) -> f64 {
    let left_widths = token_widths(left, precision);
    let right_widths = token_widths(right, precision);
    [
        (geometry.position_radius, left_widths[0], right_widths[0]),
        (geometry.glyph_radius, left_widths[1], right_widths[1]),
    ]
    .into_iter()
    .map(|(radius, left_width, right_width)| {
        let distance = (left_width + right_width) / 2.0 + LABEL_PADDING;
        2.0 * (distance / (2.0 * radius))
            .clamp(0.0, 1.0)
            .asin()
            .to_degrees()
    })
    .fold(0.0, f64::max)
}

fn token_widths(point: &ChartPoint, precision: PositionPrecision) -> [f64; 2] {
    let position = match precision {
        PositionPrecision::ArcMinute => 40.0,
        PositionPrecision::Degree => 24.0,
    };
    let glyph = if point.retrograde { 28.0 } else { 18.0 };
    [position, glyph]
}

fn visual_longitude(longitude: f64, ascendant: f64, orientation: WheelOrientation) -> f64 {
    match orientation {
        WheelOrientation::AscendantLeft => (longitude - ascendant + 270.0).rem_euclid(360.0),
        WheelOrientation::ZodiacZeroTop => longitude.rem_euclid(360.0),
    }
}

fn polar(longitude: f64, radius: f64) -> (f64, f64) {
    let radians = (longitude - 90.0).to_radians();
    (
        CENTER + radius * radians.cos(),
        CENTER + radius * radians.sin(),
    )
}

/// Deterministically displace labels on a circular ring while preserving order.
///
/// The largest empty arc is used as the cut, so a collision cluster spanning
/// 359°/0° remains together. Isotonic regression then gives colliding labels
/// the requested chord distance with the smallest centered displacement.
pub fn resolve_circular_collisions(
    longitudes: &[f64],
    radius: f64,
    minimum_distance: f64,
) -> Vec<f64> {
    if longitudes.len() < 2 || radius <= 0.0 || minimum_distance <= 0.0 {
        return longitudes
            .iter()
            .map(|value| value.rem_euclid(360.0))
            .collect();
    }
    let ratio = (minimum_distance / (2.0 * radius)).clamp(0.0, 1.0);
    let minimum_gap = 2.0 * ratio.asin().to_degrees();
    if minimum_gap * longitudes.len() as f64 >= 360.0 {
        let even_gap = 360.0 / longitudes.len() as f64;
        return (0..longitudes.len())
            .map(|index| index as f64 * even_gap)
            .collect();
    }

    let mut sorted: Vec<(usize, f64)> = longitudes
        .iter()
        .copied()
        .enumerate()
        .map(|(index, value)| (index, value.rem_euclid(360.0)))
        .collect();
    sorted.sort_by(|left, right| {
        left.1
            .total_cmp(&right.1)
            .then_with(|| left.0.cmp(&right.0))
    });
    let mut cut_after = 0;
    let mut largest_gap = f64::NEG_INFINITY;
    for index in 0..sorted.len() {
        let current = sorted[index].1;
        let next = sorted[(index + 1) % sorted.len()].1;
        let gap = (next - current).rem_euclid(360.0);
        if gap > largest_gap {
            largest_gap = gap;
            cut_after = index;
        }
    }

    let mut unwrapped: Vec<(usize, f64)> = Vec::with_capacity(sorted.len());
    let start = (cut_after + 1) % sorted.len();
    for offset in 0..sorted.len() {
        let entry = sorted[(start + offset) % sorted.len()];
        let mut angle = entry.1;
        if offset > 0 {
            while angle < unwrapped.last().expect("an earlier angle exists").1 {
                angle += 360.0;
            }
        }
        unwrapped.push((entry.0, angle));
    }

    #[derive(Clone, Copy)]
    struct Block {
        first: usize,
        last: usize,
        sum: f64,
        count: usize,
    }
    impl Block {
        fn mean(self) -> f64 {
            self.sum / self.count as f64
        }
    }

    let adjusted: Vec<f64> = unwrapped
        .iter()
        .enumerate()
        .map(|(index, (_, angle))| angle - index as f64 * minimum_gap)
        .collect();
    let mut blocks: Vec<Block> = Vec::new();
    for (index, value) in adjusted.iter().copied().enumerate() {
        blocks.push(Block {
            first: index,
            last: index,
            sum: value,
            count: 1,
        });
        while blocks.len() >= 2 {
            let last = blocks[blocks.len() - 1];
            let previous = blocks[blocks.len() - 2];
            if previous.mean() <= last.mean() {
                break;
            }
            blocks.pop();
            blocks.pop();
            blocks.push(Block {
                first: previous.first,
                last: last.last,
                sum: previous.sum + last.sum,
                count: previous.count + last.count,
            });
        }
    }
    let mut result = vec![0.0; longitudes.len()];
    for block in blocks {
        for index in block.first..=block.last {
            let display = block.mean() + index as f64 * minimum_gap;
            result[unwrapped[index].0] = display.rem_euclid(360.0);
        }
    }
    result
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

const CHART_STYLE: &str = r##"
:root{--wheel-bg:#10131c;--lane-natal:#202b3d;--lane-transit:#251f35;--ink:#f3f5f7;--muted:#8791a4;--natal:#a9c7f5;--transit:#e4b6ff;--aspect:#b6a6d9;--ring:#536075}
.wheel-background{fill:var(--wheel-bg);stroke:var(--ink);stroke-width:1.5}.lane-background{fill:none}.lane-background--natal{stroke:var(--lane-natal)}.lane-background--transit{stroke:var(--lane-transit)}
.ring{fill:none;stroke:var(--ring);stroke-width:1}.ring--aspect-boundary{stroke-dasharray:2 4}.ring--transit-boundary{stroke:var(--transit)}
.house-cusp,.position-tick-line,.point-leader{stroke:var(--muted);fill:none}.house-cusp{stroke-width:.8}.house-cusp--axis{stroke:var(--ink);stroke-width:1.8}.position-tick-line{stroke-width:.8}.position-tick{stroke:currentColor;stroke-width:1.4}.position-tick--natal{fill:currentColor}.position-tick--transit{fill:var(--wheel-bg)}
.point-leader{stroke-width:.8;opacity:.88}.chart-point--transit .point-leader{stroke-dasharray:3 3}.chart-point--natal .point-leader{stroke-dasharray:none}
.astronomicon{font-family:Astronomicon}.cusp-label,.point-position,.point-sign,.point-glyph,.motion-marker,.aspect-glyph{text-anchor:middle;dominant-baseline:middle}.cusp-label{color:var(--natal)}.cusp-position{font:8px system-ui,sans-serif;fill:currentColor}.cusp-sign{font-size:12px;fill:currentColor}.point-position{font:9px system-ui,sans-serif;fill:currentColor}.point-sign{font-size:10px;fill:currentColor}.point-glyph{font-size:22px;fill:currentColor}.motion-marker{font-size:10px;fill:currentColor}
.chart-point{color:var(--ink)}.point--moon{color:#d9e5ff}.point--sun{color:#ffd166}.point--mercury{color:#72ddf7}.point--venus{color:#ffafcc}.point--mars{color:#ff7b72}.point--jupiter{color:#c7a6ff}.point--saturn{color:#d6c7b0}.point--uranus{color:#80ed99}.point--neptune{color:#70b7e6}.point--pluto{color:#ff9f68}.point--meannode,.point--truenode,.point--meansouthnode,.point--truesouthnode{color:#b8c0ff}.point--chiron{color:#e6ccb2}.point--ascendant,.point--midheaven,.point--descendant,.point--imumcoeli,.point--vertex{color:#f1f3f5}
.aspect{color:var(--aspect);opacity:.82}.aspect line{stroke:currentColor;stroke-width:1}.aspect-glyph{font-family:Astronomicon;fill:currentColor;stroke:none;font-size:15px;paint-order:stroke;stroke:var(--wheel-bg);stroke-width:3px}.aspect--conjunction{color:#b6a6d9}.aspect--opposition{color:#ff7b72}.aspect--square{color:#ffad66}.aspect--trine{color:#80ed99}.aspect--sextile{color:#72ddf7}
.is-hidden{display:none}
"##;

#[cfg(test)]
mod tests {
    use super::*;

    fn point(id: &str, longitude_degrees: f64, retrograde: bool) -> ChartPoint {
        ChartPoint {
            id: id.to_owned(),
            longitude_degrees,
            longitude_speed_degrees_per_day: if retrograde { -1.0 } else { 1.0 },
            retrograde,
        }
    }

    fn transit_geometry() -> PointGeometry {
        PointGeometry {
            inner_radius: TRANSIT_INNER_RADIUS,
            position_radius: TRANSIT_POSITION_RADIUS,
            glyph_radius: TRANSIT_GLYPH_RADIUS,
        }
    }

    #[test]
    fn positions_round_to_arcminutes_and_roll_signs_forward() {
        assert_eq!(
            round_position(29.999_9, PositionPrecision::ArcMinute),
            RoundedPosition {
                sign_index: 1,
                degrees: 0,
                minutes: Some(0),
            }
        );
        assert_eq!(
            round_position(359.999_9, PositionPrecision::ArcMinute),
            RoundedPosition {
                sign_index: 0,
                degrees: 0,
                minutes: Some(0),
            }
        );
        assert_eq!(
            round_position(89.501, PositionPrecision::Degree),
            RoundedPosition {
                sign_index: 3,
                degrees: 0,
                minutes: None,
            }
        );
    }

    #[test]
    fn adaptive_layout_keeps_isolated_minutes_and_spreads_wrap_cluster_in_order() {
        let points = [
            point("Ascendant", 359.5, false),
            point("Mercury", 0.0, true),
            point("Sun", 0.5, false),
            point("Moon", 110.25, false),
        ];
        let references = points.iter().collect::<Vec<_>>();
        let layout = layout_point_labels(&references, transit_geometry());
        assert_eq!(layout[3].precision, PositionPrecision::ArcMinute);
        assert!(
            layout[..3]
                .iter()
                .all(|entry| entry.precision == PositionPrecision::Degree)
        );
        assert!(
            layout
                .iter()
                .all(|entry| entry.display_longitude.is_finite())
        );
        let unwrapped = [
            layout[0].display_longitude,
            if layout[1].display_longitude < layout[0].display_longitude {
                layout[1].display_longitude + 360.0
            } else {
                layout[1].display_longitude
            },
            if layout[2].display_longitude < layout[0].display_longitude {
                layout[2].display_longitude + 360.0
            } else {
                layout[2].display_longitude
            },
        ];
        assert!(unwrapped[0] < unwrapped[1] && unwrapped[1] < unwrapped[2]);
    }

    #[test]
    fn exact_ties_use_selection_order() {
        let points = [
            point("Vertex", 42.0, false),
            point("Ascendant", 42.0, false),
            point("Moon", 42.0, false),
        ];
        let references = points.iter().collect::<Vec<_>>();
        let layout = layout_point_labels(&references, transit_geometry());
        assert!(
            layout[0].display_longitude < layout[1].display_longitude
                && layout[1].display_longitude < layout[2].display_longitude
        );
    }
}
