use std::{collections::BTreeMap, fmt::Write, str::FromStr};

use serde::Serialize;

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

const SIGNS: [&str; 12] = [
    "♈", "♉", "♊", "♋", "♌", "♍", "♎", "♏", "♐", "♑", "♒", "♓",
];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
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
    svg.push_str(STYLE_AND_GLYPH_DEFS);
    let _ = write!(
        svg,
        "<circle class=\"wheel-background\" cx=\"{CENTER}\" cy=\"{CENTER}\" r=\"{OUTER_RADIUS}\"/>"
    );
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
        let _ = write!(
            svg,
            "<text id=\"cusp-label-{}\" class=\"cusp-label\" data-role=\"cusp-label\" data-longitude=\"{cusp:.12}\" x=\"{label_x:.3}\" y=\"{label_y:.3}\">{:02}° {} {:02}′</text>",
            index + 1,
            rounded.degrees,
            SIGNS[rounded.sign_index],
            rounded
                .minutes
                .expect("arcminute precision includes minutes")
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
        "Conjunction" => "☌",
        "Sextile" => "⚹",
        "Square" => "□",
        "Trine" => "△",
        "Opposition" => "☍",
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
        "<g id=\"{layer}-point-{slug}\" class=\"chart-point chart-point--{layer}\" data-point-id=\"{}\" data-longitude=\"{:.12}\" data-display-longitude=\"{:.12}\" data-precision=\"{}\"><title>{label}</title>",
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
        "<line id=\"{layer}-tick-{slug}\" data-role=\"tick\" class=\"position-tick\" x1=\"{tick_inner_x:.3}\" y1=\"{tick_inner_y:.3}\" x2=\"{tick_outer_x:.3}\" y2=\"{tick_outer_y:.3}\"/>"
    );
    let _ = write!(
        svg,
        "<text id=\"{layer}-position-{slug}\" data-role=\"position\" data-sign-index=\"{}\" class=\"point-position\" x=\"{position_x:.3}\" y=\"{position_y:.3}\">{position}</text>",
        rounded.sign_index
    );
    if let Some(glyph_id) = glyph_id(&point.id) {
        let _ = write!(
            svg,
            "<g data-role=\"glyph\" class=\"point-glyph\" transform=\"translate({glyph_x:.3} {glyph_y:.3})\"><use href=\"#{glyph_id}\"/></g>"
        );
    } else {
        let fallback = escape_xml(glyph_fallback(&point.id));
        let _ = write!(
            svg,
            "<text data-role=\"glyph\" class=\"point-fallback\" x=\"{glyph_x:.3}\" y=\"{glyph_y:.3}\">{fallback}</text>"
        );
    }
    if point.retrograde {
        let _ = write!(
            svg,
            "<text data-role=\"motion\" class=\"motion-marker\" x=\"{:.3}\" y=\"{:.3}\">℞</text>",
            glyph_x + 9.0,
            glyph_y + 10.0
        );
    }
    svg.push_str("</g>");
}

fn is_structural_natal_angle(id: &str) -> bool {
    matches!(id, "Ascendant" | "Descendant" | "Midheaven" | "ImumCoeli")
}

fn glyph_id(id: &str) -> Option<&'static str> {
    match id {
        "Sun" => Some("glyph-sun"),
        "Moon" => Some("glyph-moon"),
        "Mercury" => Some("glyph-mercury"),
        "Venus" => Some("glyph-venus"),
        "Mars" => Some("glyph-mars"),
        "Jupiter" => Some("glyph-jupiter"),
        "Saturn" => Some("glyph-saturn"),
        "Uranus" => Some("glyph-uranus"),
        "Neptune" => Some("glyph-neptune"),
        "Pluto" => Some("glyph-pluto"),
        "Chiron" => Some("glyph-chiron"),
        "MeanNode" | "TrueNode" => Some("glyph-north-node"),
        "MeanSouthNode" | "TrueSouthNode" => Some("glyph-south-node"),
        _ => None,
    }
}

fn glyph_fallback(id: &str) -> &str {
    match id {
        "Ascendant" => "As",
        "Midheaven" => "Mc",
        "Descendant" => "Ds",
        "ImumCoeli" => "Ic",
        "Vertex" => "Vx",
        _ => "•",
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
    let glyph = if glyph_id(&point.id).is_some() {
        if point.retrograde { 28.0 } else { 18.0 }
    } else if point.retrograde {
        34.0
    } else {
        24.0
    };
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

// Planetary path geometry and the 0° collision-cut idea are adapted from
// AstroDraw/AstroChart project/src/svg.ts and project/src/utils.ts at commit
// d8fb56fc7855ec4ea089710dba99f728c9b01918 (MIT). See THIRD_PARTY_NOTICES.md.
const STYLE_AND_GLYPH_DEFS: &str = r##"<defs>
<style>
:root{--wheel-bg:#fffdf8;--ink:#24202d;--muted:#80778b;--natal:#315d86;--transit:#a23e48;--aspect:#806f98;--ring:#c9c0ce;--fire:#b84b3b;--earth:#7d6540;--air:#3f7892;--water:#477861}
.wheel-background{fill:var(--wheel-bg);stroke:var(--ink);stroke-width:1.5}.ring{fill:none;stroke:var(--ring);stroke-width:1}.ring--aspect-boundary{stroke-dasharray:2 4}.ring--transit-boundary{stroke:var(--transit)}
.house-cusp,.position-tick,.point-leader{stroke:var(--muted);fill:none}.house-cusp{stroke-width:.8}.house-cusp--axis{stroke:var(--ink);stroke-width:1.8}.position-tick{stroke-width:2}.point-leader{stroke-width:.75;opacity:.75}
.cusp-label,.point-position,.point-fallback,.motion-marker,.aspect-glyph{font-family:serif;text-anchor:middle;dominant-baseline:middle;fill:var(--ink)}.cusp-label{font-size:10px;fill:var(--natal)}.point-position{font:10px sans-serif}
.point-glyph{fill:none;stroke:currentColor;stroke-width:1.7;stroke-linecap:round;stroke-linejoin:round}.chart-point--natal{color:var(--natal)}.chart-point--transit{color:var(--transit)}.point-fallback{font:bold 11px sans-serif;fill:currentColor}.motion-marker{font-size:9px;fill:currentColor}
.aspect{color:var(--aspect);opacity:.78}.aspect line{stroke:currentColor;stroke-width:1}.aspect-glyph{fill:currentColor;stroke:none;font-size:13px;paint-order:stroke;stroke:var(--wheel-bg);stroke-width:3px}.aspect--conjunction{color:#6e6578}.aspect--opposition{color:#9b3b42}.aspect--square{color:#b06535}.aspect--trine{color:#39805e}.aspect--sextile{color:#3a7290}
.is-hidden{display:none}
</style>
<g id="glyph-sun"><path d="m-1,-8 -2.18182,.727268 -2.181819,1.454543 -1.454552,2.18182 -.727268,2.181819 0,2.181819 .727268,2.181819 1.454552,2.18182 2.181819,1.454544 2.18182,.727276 2.18181,0 2.18182,-.727276 2.181819,-1.454544 1.454552,-2.18182 .727268,-2.181819 0,-2.181819 -.727268,-2.181819 -1.454552,-2.18182 -2.181819,-1.454543 -2.18182,-.727268 -2.18181,0 m.727267,6.54545 -.727267,.727276 0,.727275 .727267,.727268 .727276,0 .727267,-.727268 0,-.727275 -.727267,-.727276 -.727276,0"/></g>
<g id="glyph-moon"><path d="m-2,-7 a7.4969283,7.4969283 0 0 1 0,14.327462 7.4969283,7.4969283 0 1 0 0,-14.327462z"/></g>
<g id="glyph-mercury"><path d="m-2,7 4.26011,0 m-2.13005,-2.98207 0,5.11213 m4.70312,-9.7983a4.70315,4.70315 0 0 1-4.70315,4.70314 4.70315,4.70315 0 0 1-4.70314,-4.70314 4.70315,4.70315 0 0 1 4.70314,-4.70315 4.70315,4.70315 0 0 1 4.70315,4.70315z"/><path d="m4,-9a3.9717855,3.9717855 0 0 1-3.95541,3.59054 3.9717855,3.9717855 0 0 1-3.95185,-3.59445"/></g>
<g id="glyph-venus"><path d="m2,7 -4.937669,.03973m2.448972,2.364607 0,-5.79014c-3.109546,-.0085-5.624617,-2.534212-5.620187,-5.64208.0044,-3.107706 2.526514,-5.621689 5.635582,-5.621689 3.109068,0 5.631152,2.513983 5.635582,5.621689.0044,3.107868-2.510641,5.633586-5.620187,5.64208"/></g>
<g id="glyph-mars"><path d="m2,-2c-5.247438,-4.150623-11.6993,3.205518-7.018807,7.886007 4.680494,4.680488 12.036628,-1.771382 7.885999,-7.018816zm0,0 .433597,.433595 3.996566,-4.217419m-3.239802,-.05521 3.295015,0 .110427,3.681507"/></g>
<g id="glyph-jupiter"><path d="m-5,-2c-.43473,0-1.30422,-.40572-1.30422,-2.02857 0,-1.62285 1.73897,-3.2457 3.47792,-3.2457 1.73897,0 3.47792,1.21715 3.47792,4.05713 0,2.83999-2.1737,7.30283-6.52108,7.30283m12.17269,0-12.60745,0m9.99902,-11.76567 0,15.82279"/></g>
<g id="glyph-saturn"><path d="m5,10c-.52222,.52221-1.04445,1.04444-1.56666,1.04444-.52222,0-1.56667,-.52223-1.56667,-1.56667 0,-1.04443.52223,-2.08887 1.56667,-3.13332 1.04444,-1.04443 2.08888,-3.13331 2.08888,-5.22219 0,-2.08888-1.04444,-4.17776-3.13332,-4.17776-1.97566,0-3.65555,1.04444-4.69998,3.13333m-2.55515,-5.87499 6.26664,0m-3.71149,-2.48054 0,15.14438"/></g>
<g id="glyph-uranus"><path d="m-5,-7 0,10.23824m10.23633,-10.32764 0,10.23824m-10.26606,-4.6394 10.23085,0m-5.06415,-5.51532 0,11.94985"/><path d="m2,7.5a1.8384377,1.8384377 0 0 1-1.83844,1.83843 1.8384377,1.8384377 0 0 1-1.83842,-1.83843 1.8384377,1.8384377 0 0 1 1.83842,-1.83844 1.8384377,1.8384377 0 0 1 1.83844,1.83844z"/></g>
<g id="glyph-neptune"><path d="m3,-5 1.77059,-2.36312 2.31872,1.8045m-14.44264,-.20006 2.34113,-1.77418 1.74085,2.38595m-1.80013,-1.77265c-1.23776,8.40975.82518,9.67121 4.95106,9.67121 4.12589,0 6.18883,-1.26146 4.95107,-9.67121m-7.05334,3.17005 2.03997,-2.12559 2.08565,2.07903m-5.32406,9.91162 6.60142,0m-3.30071,-12.19414 0,15.55803"/></g>
<g id="glyph-pluto"><path d="m5,-5a5.7676856,5.7676856 0 0 1-2.88385,4.99496 5.7676856,5.7676856 0 0 1-5.76768,0 5.7676856,5.7676856 0 0 1-2.88385,-4.99496m5.76771,13.93858 0,-8.17088m-3.84512,4.32576 7.69024,0"/><path d="m2.7,-5a3.3644834,3.3644834 0 0 1-3.36448,3.36449 3.3644834,3.3644834 0 0 1-3.36448,-3.36449 3.3644834,3.3644834 0 0 1 3.36448,-3.36448 3.3644834,3.3644834 0 0 1 3.36448,3.36448z"/></g>
<g id="glyph-chiron"><path d="m3,5a3.8764725,3.0675249 0 0 1-3.876473,3.067525 3.8764725,3.0675249 0 0 1-3.876472,-3.067525 3.8764725,3.0675249 0 0 1 3.876472,-3.067525 3.8764725,3.0675249 0 0 1 3.876473,3.067525z"/><path d="m3,-8-3.942997,4.243844 4.110849,3.656151m-4.867569,-9.009468 0,11.727251"/></g>
<g id="glyph-north-node"><path d="m-2,3-1.3333334,-.6666667-.6666666,0-1.3333334,.6666667-.6666667,1.3333333 0,.6666667.6666667,1.3333333 1.3333334,.6666667.6666666,0 1.3333334,-.6666667.6666666,-1.3333333 0,-.6666667-.6666666,-1.3333333-2,-2.66666665-.6666667,-1.99999995 0,-1.3333334.6666667,-2 1.3333333,-1.3333333 2,-.6666667 2.6666666,0 2,.6666667 1.3333333,1.3333333.6666667,2 0,1.3333334-.6666667,1.99999995-2,2.66666665-.6666666,1.3333333 0,.6666667.6666666,1.3333333 1.3333334,.6666667.6666666,0 1.3333334,-.6666667.6666667,-1.3333333 0,-.6666667-.6666667,-1.3333333-1.3333334,-.6666667-.6666666,0-1.3333334,.6666667m-8,-6 .6666667,-1.3333333 1.3333333,-1.3333333 2,-.6666667 2.6666666,0 2,.6666667 1.3333333,1.3333333.6666667,1.3333333"/></g>
<g id="glyph-south-node" transform="rotate(180)"><use href="#glyph-north-node"/></g>
</defs>"##;

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
