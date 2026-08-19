use astraeus_comparison::{
    ChartLayerArtifact, ComparisonArtifact, ComparisonKind, ComparisonMotionPolicy,
};
use astraeus_core::{ChartAngle, ChartPointSelection, chart_point_positions};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const TRANSIT_TIMELINE_SCHEMA_VERSION: u32 = 2;
const MAX_INTERPOLATION_GAP: Duration = Duration::hours(24);
const STATION_EPSILON: f64 = 1.0e-12;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChartPoint {
    pub id: String,
    pub longitude_degrees: f64,
    pub longitude_speed_degrees_per_day: f64,
    pub retrograde: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChartAspect {
    pub id: String,
    pub natal_point_id: String,
    pub transit_point_id: String,
    pub kind: String,
    pub orb_degrees: f64,
    pub phase: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChartRing {
    pub timestamp: String,
    pub zodiac: String,
    pub house_system: String,
    pub points: Vec<ChartPoint>,
    pub houses: Vec<f64>,
    pub ascendant_degrees: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChartScene {
    pub timestamp: String,
    pub natal: ChartRing,
    pub transit_zodiac: String,
    pub transit_house_system: String,
    pub transit: Vec<ChartPoint>,
    pub aspects: Vec<ChartAspect>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TransitFrame {
    pub timestamp: String,
    pub points: Vec<ChartPoint>,
    pub aspects: Vec<ChartAspect>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TransitTimeline {
    pub schema_version: u32,
    pub natal: ChartRing,
    pub transit_zodiac: String,
    pub transit_house_system: String,
    pub frames: Vec<TransitFrame>,
}

#[derive(Debug, Error)]
pub enum TransitTimelineError {
    #[error("at least one comparison artifact is required")]
    EmptyTimeline,
    #[error("comparison kind must be transit_to_natal")]
    UnsupportedComparisonKind,
    #[error("comparison motion must be second_moves_against_first_fixed")]
    UnsupportedMotionPolicy,
    #[error("both comparison layers must be physical charts")]
    NonPhysicalLayer,
    #[error("invalid transit timestamp {0:?}")]
    InvalidTimestamp(String),
    #[error("all frames must contain the identical natal artifact")]
    NatalChanged,
    #[error("all frames must contain the identical moving-point population")]
    MovingPointPopulationChanged,
    #[error("all frames must use the same transit zodiac and house system")]
    TransitCalculationSettingsChanged,
    #[error("transit timestamps must be unique; duplicate {0}")]
    DuplicateTimestamp(String),
    #[error("transit timestamps must be strictly increasing: {previous} then {current}")]
    ReversedChronology { previous: String, current: String },
    #[error("Astraeus chart point data is invalid: {0}")]
    InvalidPointData(String),
}

impl ChartScene {
    pub fn from_comparison(comparison: &ComparisonArtifact) -> Result<Self, TransitTimelineError> {
        validate_policy(comparison)?;
        let (natal, transit) = physical_layers(comparison)?;
        let natal_calculation = natal.calculation();
        let transit_calculation = transit.calculation();
        let natal_request = natal_calculation.request();
        let transit_request = transit_calculation.request();
        let timestamp = transit_calculation
            .request()
            .instant()
            .as_datetime()
            .to_rfc3339();
        parse_timestamp(&timestamp)?;

        let natal_points = chart_point_positions(natal_calculation.result())
            .map_err(|error| TransitTimelineError::InvalidPointData(error.to_string()))?;
        let transit_points = chart_point_positions(transit_calculation.result())
            .map_err(|error| TransitTimelineError::InvalidPointData(error.to_string()))?;
        let specification = comparison.specification();
        let houses = natal_calculation.result().houses();

        Ok(Self {
            timestamp,
            natal: ChartRing {
                timestamp: natal_request.instant().as_datetime().to_rfc3339(),
                zodiac: format!("{:?}", natal_request.zodiac()),
                house_system: format!("{:?}", natal_request.house_system()),
                points: selected_points(specification.first_points(), &natal_points)?,
                houses: houses.cusps_degrees().to_vec(),
                ascendant_degrees: houses
                    .angles()
                    .get(ChartAngle::Ascendant)
                    .longitude_degrees(),
            },
            transit_zodiac: format!("{:?}", transit_request.zodiac()),
            transit_house_system: format!("{:?}", transit_request.house_system()),
            transit: selected_points(specification.second_points(), &transit_points)?,
            aspects: comparison
                .aspects()
                .iter()
                .copied()
                .map(|aspect| {
                    let natal_point_id = format!("{:?}", aspect.first());
                    let transit_point_id = format!("{:?}", aspect.second());
                    let kind = format!("{:?}", aspect.kind());
                    ChartAspect {
                        id: format!(
                            "aspect--{}--{}--{}",
                            stable_slug(&natal_point_id),
                            stable_slug(&transit_point_id),
                            stable_slug(&kind)
                        ),
                        natal_point_id,
                        transit_point_id,
                        kind,
                        orb_degrees: aspect.orb_degrees(),
                        phase: aspect.phase().map(|phase| format!("{phase:?}")),
                    }
                })
                .collect(),
        })
    }
}

fn selected_points(
    selection: &ChartPointSelection,
    available: &std::collections::BTreeMap<
        astraeus_core::ChartPointId,
        astraeus_core::AngularPosition,
    >,
) -> Result<Vec<ChartPoint>, TransitTimelineError> {
    selection
        .as_slice()
        .iter()
        .map(|id| {
            available
                .get(id)
                .copied()
                .map(|position| point(format!("{id:?}"), position))
                .ok_or_else(|| {
                    TransitTimelineError::InvalidPointData(format!(
                        "selected point {id:?} is absent from its validated chart"
                    ))
                })
        })
        .collect()
}

impl TransitTimeline {
    pub fn from_comparisons(
        comparisons: &[ComparisonArtifact],
    ) -> Result<Self, TransitTimelineError> {
        let first = comparisons
            .first()
            .ok_or(TransitTimelineError::EmptyTimeline)?;
        validate_policy(first)?;
        let (first_natal, _) = physical_layers(first)?;
        let first_scene = ChartScene::from_comparison(first)?;
        let expected_point_population = point_population(&first_scene.transit);
        let mut previous_time = parse_timestamp(&first_scene.timestamp)?;
        let mut frames = vec![frame_from_scene(&first_scene)];

        for comparison in &comparisons[1..] {
            validate_policy(comparison)?;
            let (natal, _) = physical_layers(comparison)?;
            if natal != first_natal {
                return Err(TransitTimelineError::NatalChanged);
            }
            let scene = ChartScene::from_comparison(comparison)?;
            if point_population(&scene.transit) != expected_point_population {
                return Err(TransitTimelineError::MovingPointPopulationChanged);
            }
            if scene.transit_zodiac != first_scene.transit_zodiac
                || scene.transit_house_system != first_scene.transit_house_system
            {
                return Err(TransitTimelineError::TransitCalculationSettingsChanged);
            }
            let current_time = parse_timestamp(&scene.timestamp)?;
            if current_time == previous_time {
                return Err(TransitTimelineError::DuplicateTimestamp(scene.timestamp));
            }
            if current_time < previous_time {
                return Err(TransitTimelineError::ReversedChronology {
                    previous: frames.last().expect("first frame exists").timestamp.clone(),
                    current: scene.timestamp,
                });
            }
            previous_time = current_time;
            frames.push(frame_from_scene(&scene));
        }

        Ok(Self {
            schema_version: TRANSIT_TIMELINE_SCHEMA_VERSION,
            natal: first_scene.natal,
            transit_zodiac: first_scene.transit_zodiac,
            transit_house_system: first_scene.transit_house_system,
            frames,
        })
    }

    /// Sample a presentation scene without calculating positions or aspects.
    ///
    /// Moving points are interpolated only when adjacent exact frames are no
    /// more than 24 hours apart. Aspect lists always come from the preceding
    /// exact frame and switch only when an exact frame is reached.
    pub fn scene_at(&self, timestamp: DateTime<Utc>) -> ChartScene {
        let first = self.frames.first().expect("validated timeline has a frame");
        let last = self.frames.last().expect("validated timeline has a frame");
        let first_time = parse_timestamp(&first.timestamp).expect("validated timestamp");
        let last_time = parse_timestamp(&last.timestamp).expect("validated timestamp");
        if timestamp <= first_time {
            return self.exact_scene(first);
        }
        if timestamp >= last_time {
            return self.exact_scene(last);
        }

        for pair in self.frames.windows(2) {
            let left_time = parse_timestamp(&pair[0].timestamp).expect("validated timestamp");
            let right_time = parse_timestamp(&pair[1].timestamp).expect("validated timestamp");
            if timestamp == right_time {
                return self.exact_scene(&pair[1]);
            }
            if timestamp > left_time && timestamp < right_time {
                let gap = right_time - left_time;
                if gap > MAX_INTERPOLATION_GAP {
                    return self.exact_scene(&pair[0]);
                }
                let elapsed = timestamp - left_time;
                let ratio = elapsed.num_milliseconds() as f64 / gap.num_milliseconds() as f64;
                let transit = pair[0]
                    .points
                    .iter()
                    .zip(&pair[1].points)
                    .map(|(left, right)| interpolate_point(left, right, ratio))
                    .collect();
                return ChartScene {
                    timestamp: timestamp.to_rfc3339(),
                    natal: self.natal.clone(),
                    transit_zodiac: self.transit_zodiac.clone(),
                    transit_house_system: self.transit_house_system.clone(),
                    transit,
                    aspects: pair[0].aspects.clone(),
                };
            }
        }
        self.exact_scene(last)
    }

    fn exact_scene(&self, frame: &TransitFrame) -> ChartScene {
        ChartScene {
            timestamp: frame.timestamp.clone(),
            natal: self.natal.clone(),
            transit_zodiac: self.transit_zodiac.clone(),
            transit_house_system: self.transit_house_system.clone(),
            transit: frame.points.clone(),
            aspects: frame.aspects.clone(),
        }
    }
}

fn validate_policy(comparison: &ComparisonArtifact) -> Result<(), TransitTimelineError> {
    if comparison.specification().kind() != ComparisonKind::TransitToNatal {
        return Err(TransitTimelineError::UnsupportedComparisonKind);
    }
    if comparison.specification().motion() != ComparisonMotionPolicy::SecondMovesAgainstFirstFixed {
        return Err(TransitTimelineError::UnsupportedMotionPolicy);
    }
    Ok(())
}

fn physical_layers(
    comparison: &ComparisonArtifact,
) -> Result<
    (
        &astraeus_comparison::ChartLayerArtifact,
        &astraeus_comparison::ChartLayerArtifact,
    ),
    TransitTimelineError,
> {
    match (comparison.first(), comparison.second()) {
        (ChartLayerArtifact::Physical(_), ChartLayerArtifact::Physical(_)) => {
            Ok((comparison.first(), comparison.second()))
        }
        _ => Err(TransitTimelineError::NonPhysicalLayer),
    }
}

trait PhysicalLayer {
    fn calculation(&self) -> &astraeus_artifacts::CalculationArtifact;
}

impl PhysicalLayer for ChartLayerArtifact {
    fn calculation(&self) -> &astraeus_artifacts::CalculationArtifact {
        match self {
            Self::Physical(chart) => chart.calculation(),
            _ => unreachable!("physical_layers validated the variant"),
        }
    }
}

fn point(id: String, position: astraeus_core::AngularPosition) -> ChartPoint {
    let speed = position.longitude_speed_degrees_per_day();
    ChartPoint {
        id,
        longitude_degrees: position.longitude_degrees(),
        longitude_speed_degrees_per_day: speed,
        retrograde: speed < -STATION_EPSILON,
    }
}

fn frame_from_scene(scene: &ChartScene) -> TransitFrame {
    TransitFrame {
        timestamp: scene.timestamp.clone(),
        points: scene.transit.clone(),
        aspects: scene.aspects.clone(),
    }
}

fn point_population(points: &[ChartPoint]) -> Vec<&str> {
    points.iter().map(|point| point.id.as_str()).collect()
}

fn parse_timestamp(value: &str) -> Result<DateTime<Utc>, TransitTimelineError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| TransitTimelineError::InvalidTimestamp(value.to_owned()))
}

fn interpolate_point(left: &ChartPoint, right: &ChartPoint, ratio: f64) -> ChartPoint {
    debug_assert_eq!(left.id, right.id);
    let speed = left.longitude_speed_degrees_per_day
        + (right.longitude_speed_degrees_per_day - left.longitude_speed_degrees_per_day) * ratio;
    let delta = directed_delta(left, right);
    ChartPoint {
        id: left.id.clone(),
        longitude_degrees: (left.longitude_degrees + delta * ratio).rem_euclid(360.0),
        longitude_speed_degrees_per_day: speed,
        retrograde: speed < -STATION_EPSILON,
    }
}

fn directed_delta(left: &ChartPoint, right: &ChartPoint) -> f64 {
    let left_direction = direction(left.longitude_speed_degrees_per_day);
    let right_direction = direction(right.longitude_speed_degrees_per_day);
    let direction = if left_direction == right_direction {
        left_direction
    } else if left_direction == 0 {
        right_direction
    } else if right_direction == 0 {
        left_direction
    } else {
        0
    };
    match direction {
        1 => (right.longitude_degrees - left.longitude_degrees).rem_euclid(360.0),
        -1 => -((left.longitude_degrees - right.longitude_degrees).rem_euclid(360.0)),
        _ => {
            let forward = (right.longitude_degrees - left.longitude_degrees).rem_euclid(360.0);
            if forward > 180.0 {
                forward - 360.0
            } else {
                forward
            }
        }
    }
}

fn direction(speed: f64) -> i8 {
    if speed > STATION_EPSILON {
        1
    } else if speed < -STATION_EPSILON {
        -1
    } else {
        0
    }
}

pub(crate) fn stable_slug(value: &str) -> String {
    let mut slug = String::new();
    let mut separator = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            if separator && !slug.is_empty() {
                slug.push('-');
            }
            separator = false;
            slug.push(character.to_ascii_lowercase());
        } else {
            separator = true;
        }
    }
    if slug.is_empty() {
        "unknown".to_owned()
    } else {
        slug
    }
}
