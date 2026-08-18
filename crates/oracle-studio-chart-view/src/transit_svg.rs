use std::{collections::BTreeMap, fmt::Write, str::FromStr};

use serde::Serialize;

use crate::{ChartPoint, ChartScene, transit::stable_slug};

const SIZE: f64 = 720.0;
const CENTER: f64 = SIZE / 2.0;
const OUTER_RADIUS: f64 = 326.0;
const TRANSIT_RADIUS: f64 = 302.0;
const ZODIAC_RADIUS: f64 = 270.0;
const NATAL_RADIUS: f64 = 224.0;
const HOUSE_RADIUS: f64 = 202.0;
const ASPECT_RADIUS: f64 = 142.0;
const GLYPH_MIN_DISTANCE: f64 = 21.0;

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
    pub title: String,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            orientation: WheelOrientation::AscendantLeft,
            title: "Natal chart with transits".to_owned(),
        }
    }
}

/// Render a deterministic SVG biwheel from a validated presentation scene.
pub fn render_biwheel_svg(scene: &ChartScene, options: &RenderOptions) -> String {
    let mut svg = String::with_capacity(48_000);
    let title = escape_xml(&options.title);
    let description = escape_xml(&format!(
        "Natal houses and points with an outer transit layer and engine-authored inter-chart aspects at {}. Orientation: {}.",
        scene.timestamp, options.orientation
    ));
    let _ = write!(
        svg,
        "<svg id=\"oracle-transit-biwheel\" xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {SIZE:.0} {SIZE:.0}\" role=\"img\" aria-labelledby=\"chart-title chart-description\" data-orientation=\"{}\" data-ascendant=\"{:.12}\">",
        options.orientation, scene.natal.ascendant_degrees
    );
    let _ = write!(
        svg,
        "<title id=\"chart-title\">{title}</title><desc id=\"chart-description\">{description}</desc>"
    );
    svg.push_str(STYLE_AND_GLYPH_DEFS);
    let _ = write!(
        svg,
        "<circle class=\"wheel-background\" cx=\"{CENTER}\" cy=\"{CENTER}\" r=\"{OUTER_RADIUS}\"/>"
    );
    render_zodiac(&mut svg, scene, options.orientation);
    render_houses(&mut svg, scene, options.orientation);
    render_aspects(&mut svg, scene, options.orientation);
    render_point_layer(
        &mut svg,
        "natal",
        &scene.natal.points,
        NATAL_RADIUS,
        NATAL_RADIUS + 13.0,
        scene.natal.ascendant_degrees,
        options.orientation,
    );
    render_point_layer(
        &mut svg,
        "transit",
        &scene.transit,
        TRANSIT_RADIUS,
        TRANSIT_RADIUS - 13.0,
        scene.natal.ascendant_degrees,
        options.orientation,
    );
    svg.push_str("</svg>");
    svg
}

fn render_zodiac(svg: &mut String, scene: &ChartScene, orientation: WheelOrientation) {
    const SIGNS: [&str; 12] = [
        "♈", "♉", "♊", "♋", "♌", "♍", "♎", "♏", "♐", "♑", "♒", "♓",
    ];
    svg.push_str("<g id=\"zodiac-layer\" aria-label=\"Zodiac\">");
    for degree in (0..360).step_by(5) {
        let longitude = f64::from(degree);
        let visual = visual_longitude(longitude, scene.natal.ascendant_degrees, orientation);
        let length = if degree % 30 == 0 {
            12.0
        } else if degree % 10 == 0 {
            8.0
        } else {
            4.0
        };
        let (x1, y1) = polar(visual, ZODIAC_RADIUS - length / 2.0);
        let (x2, y2) = polar(visual, ZODIAC_RADIUS + length / 2.0);
        let _ = write!(
            svg,
            "<line id=\"zodiac-tick-{degree}\" class=\"zodiac-tick\" x1=\"{x1:.3}\" y1=\"{y1:.3}\" x2=\"{x2:.3}\" y2=\"{y2:.3}\"/>"
        );
    }
    for (index, sign) in SIGNS.iter().enumerate() {
        let longitude = index as f64 * 30.0 + 15.0;
        let visual = visual_longitude(longitude, scene.natal.ascendant_degrees, orientation);
        let (x, y) = polar(visual, ZODIAC_RADIUS);
        let _ = write!(
            svg,
            "<text id=\"zodiac-sign-{index}\" class=\"zodiac-sign zodiac-sign--{}\" x=\"{x:.3}\" y=\"{y:.3}\">{sign}</text>",
            index % 4
        );
    }
    svg.push_str("<circle class=\"ring\" cx=\"360\" cy=\"360\" r=\"258\"/><circle class=\"ring\" cx=\"360\" cy=\"360\" r=\"282\"/></g>");
}

fn render_houses(svg: &mut String, scene: &ChartScene, orientation: WheelOrientation) {
    svg.push_str("<g id=\"house-layer\" aria-label=\"Natal houses\">");
    for (index, cusp) in scene.natal.houses.iter().copied().enumerate() {
        let visual = visual_longitude(cusp, scene.natal.ascendant_degrees, orientation);
        let (x1, y1) = polar(visual, ASPECT_RADIUS + 10.0);
        let (x2, y2) = polar(visual, HOUSE_RADIUS);
        let _ = write!(
            svg,
            "<line id=\"house-cusp-{}\" class=\"house-cusp\" x1=\"{x1:.3}\" y1=\"{y1:.3}\" x2=\"{x2:.3}\" y2=\"{y2:.3}\"><title>House {} cusp at {cusp:.6}°</title></line>",
            index + 1,
            index + 1
        );
        let next = scene.natal.houses[(index + 1) % scene.natal.houses.len()];
        let arc = (next - cusp).rem_euclid(360.0);
        let midpoint = (cusp + arc / 2.0).rem_euclid(360.0);
        let visual_midpoint =
            visual_longitude(midpoint, scene.natal.ascendant_degrees, orientation);
        let (label_x, label_y) = polar(visual_midpoint, HOUSE_RADIUS - 17.0);
        let _ = write!(
            svg,
            "<text id=\"house-label-{}\" class=\"house-label\" x=\"{label_x:.3}\" y=\"{label_y:.3}\">{}</text>",
            index + 1,
            index + 1
        );
    }
    svg.push_str("<circle class=\"ring ring--aspect\" cx=\"360\" cy=\"360\" r=\"142\"/><circle class=\"ring\" cx=\"360\" cy=\"360\" r=\"202\"/></g>");
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
        let _ = write!(
            svg,
            "<line id=\"{}\" class=\"aspect aspect--{kind}\" data-natal-id=\"{}\" data-transit-id=\"{}\" data-kind=\"{kind}\" x1=\"{x1:.3}\" y1=\"{y1:.3}\" x2=\"{x2:.3}\" y2=\"{y2:.3}\"><title>{title}</title></line>",
            escape_xml(&aspect.id),
            escape_xml(&aspect.natal_point_id),
            escape_xml(&aspect.transit_point_id)
        );
    }
    svg.push_str("</g>");
}

#[allow(clippy::too_many_arguments)]
fn render_point_layer(
    svg: &mut String,
    layer: &str,
    points: &[ChartPoint],
    glyph_radius: f64,
    tick_radius: f64,
    ascendant: f64,
    orientation: WheelOrientation,
) {
    let longitudes: Vec<f64> = points.iter().map(|point| point.longitude_degrees).collect();
    let display_longitudes =
        resolve_circular_collisions(&longitudes, glyph_radius, GLYPH_MIN_DISTANCE);
    let _ = write!(
        svg,
        "<g id=\"{layer}-layer\" class=\"layer layer--{layer}\" aria-label=\"{layer} points\">"
    );
    let _ = write!(
        svg,
        "<circle class=\"ring ring--{layer}\" cx=\"{CENTER}\" cy=\"{CENTER}\" r=\"{glyph_radius}\"/>"
    );
    for (point, display_longitude) in points.iter().zip(display_longitudes) {
        render_point(
            svg,
            layer,
            point,
            display_longitude,
            glyph_radius,
            tick_radius,
            ascendant,
            orientation,
        );
    }
    svg.push_str("</g>");
}

#[allow(clippy::too_many_arguments)]
fn render_point(
    svg: &mut String,
    layer: &str,
    point: &ChartPoint,
    display_longitude: f64,
    glyph_radius: f64,
    tick_radius: f64,
    ascendant: f64,
    orientation: WheelOrientation,
) {
    let slug = stable_slug(&point.id);
    let actual_visual = visual_longitude(point.longitude_degrees, ascendant, orientation);
    let display_visual = visual_longitude(display_longitude, ascendant, orientation);
    let (tick_inner_x, tick_inner_y) = polar(actual_visual, tick_radius - 4.0);
    let (tick_outer_x, tick_outer_y) = polar(actual_visual, tick_radius + 4.0);
    let (leader_x, leader_y) = polar(actual_visual, glyph_radius);
    let (glyph_x, glyph_y) = polar(display_visual, glyph_radius);
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
        "<g id=\"{layer}-point-{slug}\" class=\"chart-point chart-point--{layer}\" data-point-id=\"{}\" data-longitude=\"{:.12}\"><title>{label}</title>",
        escape_xml(&point.id),
        point.longitude_degrees
    );
    let _ = write!(
        svg,
        "<line id=\"{layer}-leader-{slug}\" data-role=\"leader\" class=\"point-leader\" x1=\"{leader_x:.3}\" y1=\"{leader_y:.3}\" x2=\"{glyph_x:.3}\" y2=\"{glyph_y:.3}\"/>"
    );
    let _ = write!(
        svg,
        "<line id=\"{layer}-tick-{slug}\" data-role=\"tick\" class=\"position-tick\" x1=\"{tick_inner_x:.3}\" y1=\"{tick_inner_y:.3}\" x2=\"{tick_outer_x:.3}\" y2=\"{tick_outer_y:.3}\"/>"
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
.wheel-background{fill:var(--wheel-bg);stroke:var(--ink);stroke-width:1.5}.ring{fill:none;stroke:var(--ring);stroke-width:1}.ring--aspect{stroke-dasharray:2 4}.ring--natal{stroke:var(--natal)}.ring--transit{stroke:var(--transit)}
.zodiac-tick,.house-cusp,.position-tick,.point-leader{stroke:var(--muted);fill:none}.zodiac-tick{stroke-width:.7}.house-cusp{stroke-width:.8}.position-tick{stroke-width:2}.point-leader{stroke-width:.75;opacity:.75}
.zodiac-sign,.house-label,.point-fallback,.motion-marker{font-family:serif;text-anchor:middle;dominant-baseline:middle;fill:var(--ink)}.zodiac-sign{font-size:19px}.zodiac-sign--0{fill:var(--fire)}.zodiac-sign--1{fill:var(--earth)}.zodiac-sign--2{fill:var(--air)}.zodiac-sign--3{fill:var(--water)}.house-label{font:11px sans-serif;fill:var(--muted)}
.point-glyph{fill:none;stroke:currentColor;stroke-width:1.7;stroke-linecap:round;stroke-linejoin:round}.chart-point--natal{color:var(--natal)}.chart-point--transit{color:var(--transit)}.point-fallback{font:bold 11px sans-serif;fill:currentColor}.motion-marker{font-size:9px;fill:currentColor}
.aspect{stroke:var(--aspect);stroke-width:1;opacity:.72}.aspect--conjunction{stroke:#6e6578}.aspect--opposition{stroke:#9b3b42}.aspect--square{stroke:#b06535}.aspect--trine{stroke:#39805e}.aspect--sextile{stroke:#3a7290}
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
