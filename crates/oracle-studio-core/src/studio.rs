use std::collections::BTreeSet;

use astraeus_artifacts::CalculationArtifact;
use astraeus_comparison::{
    ComparisonArtifact, InterChartAspect, calculate_phase_aware_inter_chart_aspects,
};
use chrono::{LocalResult, NaiveDate, NaiveTime, Offset, SecondsFormat, TimeZone, Utc};
use chrono_tz::Tz;
use oracle_studio_aspect_sets::AspectSetSnapshot;
use serde::{Deserialize, Serialize};

use super::{ModelError, StableId, normalize_timestamp, validate_content_id, validate_text};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LocationProvenance {
    Manual,
    GeoNames {
        geonames_id: u64,
        catalog_content_id: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedLocation {
    id: StableId,
    label: String,
    administrative_names: Vec<String>,
    country_code: String,
    latitude_degrees: f64,
    longitude_degrees: f64,
    elevation_meters: Option<f64>,
    time_zone: String,
    provenance: LocationProvenance,
}

impl SavedLocation {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: StableId,
        label: impl Into<String>,
        administrative_names: Vec<String>,
        country_code: impl Into<String>,
        latitude_degrees: f64,
        longitude_degrees: f64,
        elevation_meters: Option<f64>,
        time_zone: impl Into<String>,
        provenance: LocationProvenance,
    ) -> Result<Self, ModelError> {
        let location = Self {
            id,
            label: label.into(),
            administrative_names,
            country_code: country_code.into(),
            latitude_degrees,
            longitude_degrees,
            elevation_meters,
            time_zone: time_zone.into(),
            provenance,
        };
        location.validate()?;
        Ok(location)
    }

    pub(crate) fn validate(&self) -> Result<(), ModelError> {
        validate_text("saved_location.label", &self.label)?;
        for name in &self.administrative_names {
            validate_text("saved_location.administrative_names", name)?;
        }
        if self.country_code.len() != 2
            || !self
                .country_code
                .bytes()
                .all(|byte| byte.is_ascii_uppercase())
        {
            return Err(ModelError::InvalidValue("saved_location.country_code"));
        }
        if !self.latitude_degrees.is_finite() || !(-90.0..=90.0).contains(&self.latitude_degrees) {
            return Err(ModelError::InvalidValue("saved_location.latitude_degrees"));
        }
        if !self.longitude_degrees.is_finite()
            || !(-180.0..=180.0).contains(&self.longitude_degrees)
        {
            return Err(ModelError::InvalidValue("saved_location.longitude_degrees"));
        }
        if self
            .elevation_meters
            .is_some_and(|value| !value.is_finite() || !(-500.0..=10_000.0).contains(&value))
        {
            return Err(ModelError::InvalidValue("saved_location.elevation_meters"));
        }
        validate_time_zone(&self.time_zone)?;
        if let LocationProvenance::GeoNames {
            geonames_id,
            catalog_content_id,
        } = &self.provenance
        {
            if *geonames_id == 0 {
                return Err(ModelError::InvalidValue(
                    "saved_location.provenance.geonames_id",
                ));
            }
            validate_content_id(catalog_content_id)
                .map_err(|_| ModelError::InvalidValue("saved_location.catalog_content_id"))?;
        }
        Ok(())
    }

    pub fn id(&self) -> &StableId {
        &self.id
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn administrative_names(&self) -> &[String] {
        &self.administrative_names
    }
    pub fn country_code(&self) -> &str {
        &self.country_code
    }
    pub fn latitude_degrees(&self) -> f64 {
        self.latitude_degrees
    }
    pub fn longitude_degrees(&self) -> f64 {
        self.longitude_degrees
    }
    pub fn elevation_meters(&self) -> Option<f64> {
        self.elevation_meters
    }
    pub fn time_zone(&self) -> &str {
        &self.time_zone
    }
    pub fn provenance(&self) -> &LocationProvenance {
        &self.provenance
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalDateTimeInput {
    local_date: String,
    local_time: String,
    time_zone: String,
}

impl LocalDateTimeInput {
    pub fn new(
        local_date: impl Into<String>,
        local_time: impl Into<String>,
        time_zone: impl Into<String>,
    ) -> Result<Self, ModelError> {
        let local_date = local_date.into();
        let local_time = local_time.into();
        let time_zone = time_zone.into();
        let date = NaiveDate::parse_from_str(&local_date, "%Y-%m-%d")
            .map_err(|_| ModelError::InvalidValue("chart.local_date"))?;
        let time = parse_local_time(&local_time)?;
        validate_time_zone(&time_zone)?;
        Ok(Self {
            local_date: date.format("%Y-%m-%d").to_string(),
            local_time: time.format("%H:%M:%S").to_string(),
            time_zone,
        })
    }

    pub(crate) fn validate(&self) -> Result<(), ModelError> {
        let normalized = Self::new(&self.local_date, &self.local_time, &self.time_zone)?;
        if normalized == *self {
            Ok(())
        } else {
            Err(ModelError::InvalidValue("chart.local_input"))
        }
    }

    pub fn local_date(&self) -> &str {
        &self.local_date
    }
    pub fn local_time(&self) -> &str {
        &self.local_time
    }
    pub fn time_zone(&self) -> &str {
        &self.time_zone
    }

    fn naive(&self) -> Result<chrono::NaiveDateTime, ModelError> {
        let date = NaiveDate::parse_from_str(&self.local_date, "%Y-%m-%d")
            .map_err(|_| ModelError::InvalidValue("chart.local_date"))?;
        Ok(date.and_time(parse_local_time(&self.local_time)?))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedLocalTime {
    local_input: LocalDateTimeInput,
    abbreviation: String,
    utc_offset_seconds: i32,
    utc_instant: String,
}

impl ResolvedLocalTime {
    pub fn local_input(&self) -> &LocalDateTimeInput {
        &self.local_input
    }
    pub fn abbreviation(&self) -> &str {
        &self.abbreviation
    }
    pub const fn utc_offset_seconds(&self) -> i32 {
        self.utc_offset_seconds
    }
    pub fn utc_offset_display(&self) -> String {
        format_utc_offset(self.utc_offset_seconds)
    }
    pub fn utc_instant(&self) -> &str {
        &self.utc_instant
    }

    pub(crate) fn validate(&self) -> Result<(), ModelError> {
        self.local_input.validate()?;
        validate_text("chart.abbreviation", &self.abbreviation)?;
        let candidates = resolve_local_time(&self.local_input)?;
        let valid = match candidates {
            LocalTimeResolution::Unique(candidate) => candidate == *self,
            LocalTimeResolution::Ambiguous { earlier, later } => earlier == *self || later == *self,
            LocalTimeResolution::Nonexistent => false,
        };
        if valid {
            Ok(())
        } else {
            Err(ModelError::InvalidValue("chart.resolved_local_time"))
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LocalTimeResolution {
    Unique(ResolvedLocalTime),
    Ambiguous {
        earlier: ResolvedLocalTime,
        later: ResolvedLocalTime,
    },
    Nonexistent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AmbiguousTimeChoice {
    Earlier,
    Later,
}

pub fn resolve_local_time(input: &LocalDateTimeInput) -> Result<LocalTimeResolution, ModelError> {
    input.validate()?;
    let zone = input
        .time_zone
        .parse::<Tz>()
        .map_err(|_| ModelError::InvalidValue("chart.time_zone"))?;
    let resolved = match zone.from_local_datetime(&input.naive()?) {
        LocalResult::Single(value) => LocalTimeResolution::Unique(resolved(input, value)),
        LocalResult::Ambiguous(first, second) => {
            let mut candidates = [resolved(input, first), resolved(input, second)];
            candidates.sort_by(|first, second| first.utc_instant.cmp(&second.utc_instant));
            let [earlier, later] = candidates;
            LocalTimeResolution::Ambiguous { earlier, later }
        }
        LocalResult::None => LocalTimeResolution::Nonexistent,
    };
    Ok(resolved)
}

pub fn select_local_time(
    input: &LocalDateTimeInput,
    choice: Option<AmbiguousTimeChoice>,
) -> Result<ResolvedLocalTime, ModelError> {
    match resolve_local_time(input)? {
        LocalTimeResolution::Unique(value) if choice.is_none() => Ok(value),
        LocalTimeResolution::Unique(_) => Err(ModelError::UnexpectedAmbiguousTimeChoice),
        LocalTimeResolution::Ambiguous { earlier, later } => match choice {
            Some(AmbiguousTimeChoice::Earlier) => Ok(earlier),
            Some(AmbiguousTimeChoice::Later) => Ok(later),
            None => Err(ModelError::AmbiguousLocalTime),
        },
        LocalTimeResolution::Nonexistent => Err(ModelError::NonexistentLocalTime),
    }
}

fn resolved<T: TimeZone>(
    input: &LocalDateTimeInput,
    value: chrono::DateTime<T>,
) -> ResolvedLocalTime
where
    T::Offset: std::fmt::Display,
{
    ResolvedLocalTime {
        local_input: input.clone(),
        abbreviation: value.format("%Z").to_string(),
        utc_offset_seconds: value.offset().fix().local_minus_utc(),
        utc_instant: value
            .with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::Secs, true),
    }
}

pub fn format_utc_offset(seconds: i32) -> String {
    let sign = if seconds < 0 { '-' } else { '+' };
    let seconds = seconds.unsigned_abs();
    format!(
        "UTC{sign}{:02}:{:02}",
        seconds / 3600,
        (seconds % 3600) / 60
    )
}

fn parse_local_time(value: &str) -> Result<NaiveTime, ModelError> {
    NaiveTime::parse_from_str(value, "%H:%M:%S")
        .or_else(|_| NaiveTime::parse_from_str(value, "%H:%M"))
        .map_err(|_| ModelError::InvalidValue("chart.local_time"))
}

fn validate_time_zone(value: &str) -> Result<(), ModelError> {
    value
        .parse::<Tz>()
        .map(|_| ())
        .map_err(|_| ModelError::InvalidValue("chart.time_zone"))
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CelestialObjectId {
    Moon,
    Sun,
    Mercury,
    Venus,
    Mars,
    Jupiter,
    Saturn,
    Uranus,
    Neptune,
    Pluto,
    MeanNode,
    TrueNode,
    Chiron,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChartPointId {
    Moon,
    Sun,
    Mercury,
    Venus,
    Mars,
    Jupiter,
    Saturn,
    Uranus,
    Neptune,
    Pluto,
    MeanNode,
    TrueNode,
    Chiron,
    MeanSouthNode,
    TrueSouthNode,
    Ascendant,
    Midheaven,
    Descendant,
    ImumCoeli,
    Vertex,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZodiacId {
    Tropical,
    Sidereal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AyanamsaId {
    FaganBradley,
    Lahiri,
    DeLuce,
    Raman,
    Krishnamurti,
    Yukteshwar,
    JnBhasin,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HouseSystemId {
    Placidus,
    Koch,
    Porphyry,
    Regiomontanus,
    Campanus,
    Equal,
    WholeSign,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChartCalculationOptions {
    zodiac: ZodiacId,
    ayanamsa: Option<AyanamsaId>,
    house_system: HouseSystemId,
    ordered_objects: Vec<CelestialObjectId>,
}

impl Default for ChartCalculationOptions {
    fn default() -> Self {
        Self {
            zodiac: ZodiacId::Tropical,
            ayanamsa: None,
            house_system: HouseSystemId::Placidus,
            ordered_objects: vec![
                CelestialObjectId::Moon,
                CelestialObjectId::Sun,
                CelestialObjectId::Mercury,
                CelestialObjectId::Venus,
                CelestialObjectId::Mars,
                CelestialObjectId::Jupiter,
                CelestialObjectId::Saturn,
                CelestialObjectId::Uranus,
                CelestialObjectId::Neptune,
                CelestialObjectId::Pluto,
            ],
        }
    }
}

impl ChartCalculationOptions {
    pub fn new(
        zodiac: ZodiacId,
        ayanamsa: Option<AyanamsaId>,
        house_system: HouseSystemId,
        ordered_objects: Vec<CelestialObjectId>,
    ) -> Result<Self, ModelError> {
        let options = Self {
            zodiac,
            ayanamsa,
            house_system,
            ordered_objects,
        };
        options.validate()?;
        Ok(options)
    }

    pub(crate) fn validate(&self) -> Result<(), ModelError> {
        if self.ordered_objects.is_empty() {
            return Err(ModelError::InvalidValue("chart.ordered_objects"));
        }
        validate_unique_values(&self.ordered_objects, "chart.ordered_objects")?;
        match (self.zodiac, self.ayanamsa) {
            (ZodiacId::Tropical, None) | (ZodiacId::Sidereal, Some(_)) => Ok(()),
            _ => Err(ModelError::InvalidValue("chart.ayanamsa")),
        }
    }

    pub const fn zodiac(&self) -> ZodiacId {
        self.zodiac
    }
    pub const fn ayanamsa(&self) -> Option<AyanamsaId> {
        self.ayanamsa
    }
    pub const fn house_system(&self) -> HouseSystemId {
        self.house_system
    }
    pub fn ordered_objects(&self) -> &[CelestialObjectId] {
        &self.ordered_objects
    }
}

pub fn default_chart_points() -> Vec<ChartPointId> {
    vec![
        ChartPointId::Moon,
        ChartPointId::Sun,
        ChartPointId::Mercury,
        ChartPointId::Venus,
        ChartPointId::Mars,
        ChartPointId::Jupiter,
        ChartPointId::Saturn,
        ChartPointId::Uranus,
        ChartPointId::Neptune,
        ChartPointId::Pluto,
        ChartPointId::Ascendant,
        ChartPointId::Midheaven,
    ]
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChartRole {
    Natal,
    Event,
    Transit,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChartDefinition {
    id: StableId,
    label: String,
    role: ChartRole,
    person_id: Option<StableId>,
    local_input: LocalDateTimeInput,
    calculation_options: ChartCalculationOptions,
    ordered_points: Vec<ChartPointId>,
    default_natal: bool,
    current_calculation_id: Option<StableId>,
}

impl ChartDefinition {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: StableId,
        label: impl Into<String>,
        role: ChartRole,
        person_id: Option<StableId>,
        local_input: LocalDateTimeInput,
        calculation_options: ChartCalculationOptions,
        ordered_points: Vec<ChartPointId>,
        default_natal: bool,
    ) -> Result<Self, ModelError> {
        let chart = Self {
            id,
            label: label.into(),
            role,
            person_id,
            local_input,
            calculation_options,
            ordered_points,
            default_natal,
            current_calculation_id: None,
        };
        chart.validate()?;
        Ok(chart)
    }

    pub(crate) fn validate(&self) -> Result<(), ModelError> {
        validate_text("chart_definition.label", &self.label)?;
        self.local_input.validate()?;
        self.calculation_options.validate()?;
        if self.ordered_points.is_empty() {
            return Err(ModelError::InvalidValue("chart_definition.ordered_points"));
        }
        validate_unique_values(&self.ordered_points, "chart_definition.ordered_points")?;
        if self.default_natal && (self.role != ChartRole::Natal || self.person_id.is_none()) {
            return Err(ModelError::InvalidDefaultNatal);
        }
        for point in &self.ordered_points {
            if let Some(object) = point_object(*point)
                && !self.calculation_options.ordered_objects.contains(&object)
            {
                return Err(ModelError::InvalidValue("chart_definition.ordered_points"));
            }
        }
        Ok(())
    }

    pub fn set_current_calculation(&mut self, id: StableId) {
        self.current_calculation_id = Some(id);
    }

    pub fn id(&self) -> &StableId {
        &self.id
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    pub const fn role(&self) -> ChartRole {
        self.role
    }
    pub fn person_id(&self) -> Option<&StableId> {
        self.person_id.as_ref()
    }
    pub fn local_input(&self) -> &LocalDateTimeInput {
        &self.local_input
    }
    pub fn calculation_options(&self) -> &ChartCalculationOptions {
        &self.calculation_options
    }
    pub fn ordered_points(&self) -> &[ChartPointId] {
        &self.ordered_points
    }
    pub const fn default_natal(&self) -> bool {
        self.default_natal
    }
    pub fn current_calculation_id(&self) -> Option<&StableId> {
        self.current_calculation_id.as_ref()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChartCalculation {
    id: StableId,
    chart_definition_id: StableId,
    local_input_snapshot: LocalDateTimeInput,
    resolved_time: ResolvedLocalTime,
    location_snapshot: SavedLocation,
    snapshot: CalculationArtifact,
    calculated_at: String,
}

impl ChartCalculation {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: StableId,
        chart_definition_id: StableId,
        local_input_snapshot: LocalDateTimeInput,
        resolved_time: ResolvedLocalTime,
        location_snapshot: SavedLocation,
        snapshot: CalculationArtifact,
        calculated_at: impl Into<String>,
    ) -> Result<Self, ModelError> {
        let calculation = Self {
            id,
            chart_definition_id,
            local_input_snapshot,
            resolved_time,
            location_snapshot,
            snapshot,
            calculated_at: normalize_timestamp(
                "chart_calculation.calculated_at",
                calculated_at.into(),
            )?,
        };
        calculation.validate()?;
        Ok(calculation)
    }

    pub(crate) fn validate(&self) -> Result<(), ModelError> {
        self.local_input_snapshot.validate()?;
        self.resolved_time.validate()?;
        self.location_snapshot.validate()?;
        if self.resolved_time.local_input != self.local_input_snapshot {
            return Err(ModelError::InvalidValue(
                "chart_calculation.local_input_snapshot",
            ));
        }
        if self.local_input_snapshot.time_zone != self.location_snapshot.time_zone {
            return Err(ModelError::InvalidValue(
                "chart_calculation.location_snapshot",
            ));
        }
        let normalized = normalize_timestamp(
            "chart_calculation.calculated_at",
            self.calculated_at.clone(),
        )?;
        if normalized != self.calculated_at {
            return Err(ModelError::InvalidTimestamp(
                "chart_calculation.calculated_at",
            ));
        }
        Ok(())
    }

    pub fn id(&self) -> &StableId {
        &self.id
    }
    pub fn chart_definition_id(&self) -> &StableId {
        &self.chart_definition_id
    }
    pub fn local_input_snapshot(&self) -> &LocalDateTimeInput {
        &self.local_input_snapshot
    }
    pub fn resolved_time(&self) -> &ResolvedLocalTime {
        &self.resolved_time
    }
    pub fn location_snapshot(&self) -> &SavedLocation {
        &self.location_snapshot
    }
    pub const fn snapshot(&self) -> &CalculationArtifact {
        &self.snapshot
    }
    pub fn calculated_at(&self) -> &str {
        &self.calculated_at
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AspectKindId {
    Conjunction,
    Opposition,
    Square,
    Trine,
    Sextile,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AspectDefinition {
    kind: AspectKindId,
    orb_degrees: f64,
}

impl AspectDefinition {
    pub fn new(kind: AspectKindId, orb_degrees: f64) -> Result<Self, ModelError> {
        if !orb_degrees.is_finite() || !(0.0..=30.0).contains(&orb_degrees) {
            return Err(ModelError::InvalidValue("comparison.aspect_orb"));
        }
        Ok(Self { kind, orb_degrees })
    }
    pub const fn kind(&self) -> AspectKindId {
        self.kind
    }
    pub fn orb_degrees(&self) -> f64 {
        self.orb_degrees
    }
}

pub fn default_aspects() -> Vec<AspectDefinition> {
    vec![
        AspectDefinition::new(AspectKindId::Conjunction, 8.0).expect("valid default"),
        AspectDefinition::new(AspectKindId::Opposition, 8.0).expect("valid default"),
        AspectDefinition::new(AspectKindId::Square, 6.0).expect("valid default"),
        AspectDefinition::new(AspectKindId::Trine, 6.0).expect("valid default"),
        AspectDefinition::new(AspectKindId::Sextile, 4.0).expect("valid default"),
    ]
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WheelOrientation {
    AscendantLeft,
    AriesTop,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComparisonPreset {
    id: StableId,
    label: String,
    inner_chart_definition_id: StableId,
    outer_chart_definition_id: StableId,
    inner_points: Vec<ChartPointId>,
    outer_points: Vec<ChartPointId>,
    aspects: Vec<AspectDefinition>,
    orientation: WheelOrientation,
    current_calculation_id: Option<StableId>,
}

impl ComparisonPreset {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: StableId,
        label: impl Into<String>,
        inner_chart_definition_id: StableId,
        outer_chart_definition_id: StableId,
        inner_points: Vec<ChartPointId>,
        outer_points: Vec<ChartPointId>,
        aspects: Vec<AspectDefinition>,
        orientation: WheelOrientation,
    ) -> Result<Self, ModelError> {
        let preset = Self {
            id,
            label: label.into(),
            inner_chart_definition_id,
            outer_chart_definition_id,
            inner_points,
            outer_points,
            aspects,
            orientation,
            current_calculation_id: None,
        };
        preset.validate()?;
        Ok(preset)
    }

    pub(crate) fn validate(&self) -> Result<(), ModelError> {
        validate_text("comparison_preset.label", &self.label)?;
        if self.inner_chart_definition_id == self.outer_chart_definition_id {
            return Err(ModelError::InvalidComparisonSources);
        }
        if self.inner_points.is_empty() || self.outer_points.is_empty() || self.aspects.is_empty() {
            return Err(ModelError::InvalidValue("comparison_preset.selection"));
        }
        validate_unique_values(&self.inner_points, "comparison_preset.inner_points")?;
        validate_unique_values(&self.outer_points, "comparison_preset.outer_points")?;
        validate_unique_values(
            &self
                .aspects
                .iter()
                .map(AspectDefinition::kind)
                .collect::<Vec<_>>(),
            "comparison_preset.aspects",
        )?;
        for aspect in &self.aspects {
            AspectDefinition::new(aspect.kind, aspect.orb_degrees)?;
        }
        Ok(())
    }

    pub fn set_current_calculation(&mut self, calculation_id: StableId) {
        self.current_calculation_id = Some(calculation_id);
    }

    pub fn id(&self) -> &StableId {
        &self.id
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn inner_chart_definition_id(&self) -> &StableId {
        &self.inner_chart_definition_id
    }
    pub fn outer_chart_definition_id(&self) -> &StableId {
        &self.outer_chart_definition_id
    }
    pub fn inner_points(&self) -> &[ChartPointId] {
        &self.inner_points
    }
    pub fn outer_points(&self) -> &[ChartPointId] {
        &self.outer_points
    }
    pub fn aspects(&self) -> &[AspectDefinition] {
        &self.aspects
    }
    pub const fn orientation(&self) -> WheelOrientation {
        self.orientation
    }
    pub fn current_calculation_id(&self) -> Option<&StableId> {
        self.current_calculation_id.as_ref()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComparisonCalculation {
    id: StableId,
    comparison_preset_id: StableId,
    inner_calculation_id: StableId,
    outer_calculation_id: StableId,
    snapshot: ComparisonArtifact,
    aspect_set_snapshot: AspectSetSnapshot,
    phase_aware_aspects: Vec<InterChartAspect>,
    calculated_at: String,
}

impl ComparisonCalculation {
    pub fn new(
        id: StableId,
        comparison_preset_id: StableId,
        inner_calculation_id: StableId,
        outer_calculation_id: StableId,
        snapshot: ComparisonArtifact,
        calculated_at: impl Into<String>,
    ) -> Result<Self, ModelError> {
        let specification = snapshot.specification();
        let aspect_set_snapshot = AspectSetSnapshot::legacy_uniform(
            specification.aspects(),
            specification.first_points(),
            specification.second_points(),
        )
        .map_err(|_| ModelError::InvalidValue("comparison_calculation.aspect_set"))?;
        let phase_aware_aspects = calculate_phase_aware_inter_chart_aspects(
            snapshot.first(),
            snapshot.second(),
            &aspect_set_snapshot
                .phase_aware_definitions()
                .map_err(|_| ModelError::InvalidValue("comparison_calculation.aspect_set"))?,
            specification.first_points(),
            specification.second_points(),
            specification.motion(),
        )
        .map_err(|_| ModelError::InvalidValue("comparison_calculation.aspect_set"))?;
        Self::new_with_aspect_set(
            id,
            comparison_preset_id,
            inner_calculation_id,
            outer_calculation_id,
            snapshot,
            aspect_set_snapshot,
            phase_aware_aspects,
            calculated_at,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_aspect_set(
        id: StableId,
        comparison_preset_id: StableId,
        inner_calculation_id: StableId,
        outer_calculation_id: StableId,
        snapshot: ComparisonArtifact,
        aspect_set_snapshot: AspectSetSnapshot,
        phase_aware_aspects: Vec<InterChartAspect>,
        calculated_at: impl Into<String>,
    ) -> Result<Self, ModelError> {
        let calculation = Self {
            id,
            comparison_preset_id,
            inner_calculation_id,
            outer_calculation_id,
            snapshot,
            aspect_set_snapshot,
            phase_aware_aspects,
            calculated_at: normalize_timestamp(
                "comparison_calculation.calculated_at",
                calculated_at.into(),
            )?,
        };
        calculation.validate()?;
        Ok(calculation)
    }

    pub(crate) fn validate(&self) -> Result<(), ModelError> {
        if self.inner_calculation_id == self.outer_calculation_id {
            return Err(ModelError::InvalidComparisonSources);
        }
        let specification = self.snapshot.specification();
        let mut expected_points = specification.first_points().as_slice().to_vec();
        for point in specification.second_points().as_slice() {
            if !expected_points.contains(point) {
                expected_points.push(*point);
            }
        }
        if self.aspect_set_snapshot.points() != expected_points {
            return Err(ModelError::InvalidValue(
                "comparison_calculation.aspect_set_points",
            ));
        }
        let expected = calculate_phase_aware_inter_chart_aspects(
            self.snapshot.first(),
            self.snapshot.second(),
            &self
                .aspect_set_snapshot
                .phase_aware_definitions()
                .map_err(|_| ModelError::InvalidValue("comparison_calculation.aspect_set"))?,
            specification.first_points(),
            specification.second_points(),
            specification.motion(),
        )
        .map_err(|_| ModelError::InvalidValue("comparison_calculation.aspect_set"))?;
        if expected != self.phase_aware_aspects {
            return Err(ModelError::InvalidValue(
                "comparison_calculation.phase_aware_aspects",
            ));
        }
        let normalized = normalize_timestamp(
            "comparison_calculation.calculated_at",
            self.calculated_at.clone(),
        )?;
        if normalized != self.calculated_at {
            return Err(ModelError::InvalidTimestamp(
                "comparison_calculation.calculated_at",
            ));
        }
        Ok(())
    }

    pub fn id(&self) -> &StableId {
        &self.id
    }
    pub fn comparison_preset_id(&self) -> &StableId {
        &self.comparison_preset_id
    }
    pub fn inner_calculation_id(&self) -> &StableId {
        &self.inner_calculation_id
    }
    pub fn outer_calculation_id(&self) -> &StableId {
        &self.outer_calculation_id
    }
    pub const fn snapshot(&self) -> &ComparisonArtifact {
        &self.snapshot
    }
    pub const fn aspect_set_snapshot(&self) -> &AspectSetSnapshot {
        &self.aspect_set_snapshot
    }
    pub fn phase_aware_aspects(&self) -> &[InterChartAspect] {
        &self.phase_aware_aspects
    }
    pub fn calculated_at(&self) -> &str {
        &self.calculated_at
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceState {
    active_person_id: Option<StableId>,
    active_comparison_preset_id: Option<StableId>,
}

impl WorkspaceState {
    pub const fn new(
        active_person_id: Option<StableId>,
        active_comparison_preset_id: Option<StableId>,
    ) -> Self {
        Self {
            active_person_id,
            active_comparison_preset_id,
        }
    }
    pub fn active_person_id(&self) -> Option<&StableId> {
        self.active_person_id.as_ref()
    }
    pub fn active_comparison_preset_id(&self) -> Option<&StableId> {
        self.active_comparison_preset_id.as_ref()
    }
}

pub(crate) fn validate_studio_records(
    people: &BTreeSet<&StableId>,
    locations: &[SavedLocation],
    charts: &[ChartDefinition],
    calculations: &[ChartCalculation],
    comparisons: &[ComparisonPreset],
    comparison_calculations: &[ComparisonCalculation],
    workspace: &WorkspaceState,
) -> Result<(), ModelError> {
    validate_unique_ids(locations.iter().map(SavedLocation::id), "saved location")?;
    validate_unique_ids(charts.iter().map(ChartDefinition::id), "chart definition")?;
    validate_unique_ids(
        calculations.iter().map(ChartCalculation::id),
        "chart calculation",
    )?;
    validate_unique_ids(
        comparisons.iter().map(ComparisonPreset::id),
        "comparison preset",
    )?;
    validate_unique_ids(
        comparison_calculations
            .iter()
            .map(ComparisonCalculation::id),
        "comparison calculation",
    )?;
    for location in locations {
        location.validate()?;
    }
    let chart_ids = charts
        .iter()
        .map(ChartDefinition::id)
        .collect::<BTreeSet<_>>();
    let comparison_ids = comparisons
        .iter()
        .map(ComparisonPreset::id)
        .collect::<BTreeSet<_>>();
    let comparison_calculation_ids = comparison_calculations
        .iter()
        .map(ComparisonCalculation::id)
        .collect::<BTreeSet<_>>();
    let mut default_people = BTreeSet::new();
    for chart in charts {
        chart.validate()?;
        if chart.person_id().is_some_and(|id| !people.contains(id)) {
            return Err(ModelError::DanglingReference("chart_definition.person_id"));
        }
        if chart.default_natal()
            && !default_people.insert(chart.person_id().expect("validated default person"))
        {
            return Err(ModelError::DuplicateDefaultNatal);
        }
        if let Some(calculation_id) = chart.current_calculation_id() {
            let calculation = calculations
                .iter()
                .find(|candidate| candidate.id() == calculation_id)
                .ok_or(ModelError::DanglingReference(
                    "chart_definition.current_calculation_id",
                ))?;
            if calculation.chart_definition_id() != chart.id() {
                return Err(ModelError::CalculationChartMismatch);
            }
        }
    }
    for calculation in calculations {
        calculation.validate()?;
        if !chart_ids.contains(calculation.chart_definition_id()) {
            return Err(ModelError::DanglingReference(
                "chart_calculation.chart_definition_id",
            ));
        }
    }
    for comparison in comparisons {
        comparison.validate()?;
        let inner_chart = charts
            .iter()
            .find(|chart| chart.id() == comparison.inner_chart_definition_id())
            .ok_or(ModelError::DanglingReference(
                "comparison_preset.inner_chart_definition_id",
            ))?;
        let outer_chart = charts
            .iter()
            .find(|chart| chart.id() == comparison.outer_chart_definition_id())
            .ok_or(ModelError::DanglingReference(
                "comparison_preset.outer_chart_definition_id",
            ))?;
        if comparison
            .inner_points()
            .iter()
            .any(|point| !inner_chart.ordered_points().contains(point))
            || comparison
                .outer_points()
                .iter()
                .any(|point| !outer_chart.ordered_points().contains(point))
        {
            return Err(ModelError::InvalidValue(
                "comparison_preset.point_selection",
            ));
        }
        if comparison
            .current_calculation_id()
            .is_some_and(|id| !comparison_calculation_ids.contains(id))
        {
            return Err(ModelError::DanglingReference(
                "comparison_preset.current_calculation_id",
            ));
        }
    }
    for comparison_calculation in comparison_calculations {
        comparison_calculation.validate()?;
        let preset = comparisons
            .iter()
            .find(|preset| preset.id() == comparison_calculation.comparison_preset_id())
            .ok_or(ModelError::DanglingReference(
                "comparison_calculation.comparison_preset_id",
            ))?;
        let inner = calculations
            .iter()
            .find(|calculation| calculation.id() == comparison_calculation.inner_calculation_id())
            .ok_or(ModelError::DanglingReference(
                "comparison_calculation.inner_calculation_id",
            ))?;
        let outer = calculations
            .iter()
            .find(|calculation| calculation.id() == comparison_calculation.outer_calculation_id())
            .ok_or(ModelError::DanglingReference(
                "comparison_calculation.outer_calculation_id",
            ))?;
        if inner.chart_definition_id() != preset.inner_chart_definition_id()
            || outer.chart_definition_id() != preset.outer_chart_definition_id()
        {
            return Err(ModelError::InvalidComparisonSources);
        }
    }
    if workspace
        .active_person_id()
        .is_some_and(|id| !people.contains(id))
    {
        return Err(ModelError::DanglingReference(
            "workspace_state.active_person_id",
        ));
    }
    if workspace
        .active_comparison_preset_id()
        .is_some_and(|id| !comparison_ids.contains(id))
    {
        return Err(ModelError::DanglingReference(
            "workspace_state.active_comparison_preset_id",
        ));
    }
    Ok(())
}

fn validate_unique_ids<'a>(
    ids: impl Iterator<Item = &'a StableId>,
    kind: &'static str,
) -> Result<(), ModelError> {
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(id) {
            return Err(ModelError::DuplicateId(kind));
        }
    }
    Ok(())
}

fn validate_unique_values<T: Ord + Copy>(
    values: &[T],
    field: &'static str,
) -> Result<(), ModelError> {
    let mut seen = BTreeSet::new();
    if values.iter().any(|value| !seen.insert(*value)) {
        Err(ModelError::InvalidValue(field))
    } else {
        Ok(())
    }
}

fn point_object(point: ChartPointId) -> Option<CelestialObjectId> {
    match point {
        ChartPointId::Moon => Some(CelestialObjectId::Moon),
        ChartPointId::Sun => Some(CelestialObjectId::Sun),
        ChartPointId::Mercury => Some(CelestialObjectId::Mercury),
        ChartPointId::Venus => Some(CelestialObjectId::Venus),
        ChartPointId::Mars => Some(CelestialObjectId::Mars),
        ChartPointId::Jupiter => Some(CelestialObjectId::Jupiter),
        ChartPointId::Saturn => Some(CelestialObjectId::Saturn),
        ChartPointId::Uranus => Some(CelestialObjectId::Uranus),
        ChartPointId::Neptune => Some(CelestialObjectId::Neptune),
        ChartPointId::Pluto => Some(CelestialObjectId::Pluto),
        ChartPointId::MeanNode => Some(CelestialObjectId::MeanNode),
        ChartPointId::TrueNode => Some(CelestialObjectId::TrueNode),
        ChartPointId::Chiron => Some(CelestialObjectId::Chiron),
        ChartPointId::MeanSouthNode => Some(CelestialObjectId::MeanNode),
        ChartPointId::TrueSouthNode => Some(CelestialObjectId::TrueNode),
        ChartPointId::Ascendant
        | ChartPointId::Midheaven
        | ChartPointId::Descendant
        | ChartPointId::ImumCoeli
        | ChartPointId::Vertex => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_ambiguous_and_nonexistent_local_times_are_explicit() {
        let normal = LocalDateTimeInput::new("2026-08-17", "16:20", "America/New_York").unwrap();
        let resolved = select_local_time(&normal, None).unwrap();
        assert_eq!(resolved.abbreviation(), "EDT");
        assert_eq!(resolved.utc_offset_display(), "UTC-04:00");
        assert_eq!(resolved.utc_instant(), "2026-08-17T20:20:00Z");

        let ambiguous = LocalDateTimeInput::new("2026-11-01", "01:30", "America/New_York").unwrap();
        let LocalTimeResolution::Ambiguous { earlier, later } =
            resolve_local_time(&ambiguous).unwrap()
        else {
            panic!("fall-back time must be ambiguous");
        };
        assert_eq!(earlier.abbreviation(), "EDT");
        assert_eq!(earlier.utc_instant(), "2026-11-01T05:30:00Z");
        assert_eq!(later.abbreviation(), "EST");
        assert_eq!(later.utc_instant(), "2026-11-01T06:30:00Z");
        assert!(matches!(
            select_local_time(&ambiguous, None),
            Err(ModelError::AmbiguousLocalTime)
        ));

        let gap = LocalDateTimeInput::new("2026-03-08", "02:30", "America/New_York").unwrap();
        assert_eq!(
            resolve_local_time(&gap).unwrap(),
            LocalTimeResolution::Nonexistent
        );
        assert!(matches!(
            select_local_time(&gap, None),
            Err(ModelError::NonexistentLocalTime)
        ));
    }

    #[test]
    fn historical_new_york_offsets_keep_est_and_edt() {
        let winter = LocalDateTimeInput::new("1985-01-21", "03:09", "America/New_York").unwrap();
        let summer = LocalDateTimeInput::new("1985-07-21", "03:09", "America/New_York").unwrap();
        let winter = select_local_time(&winter, None).unwrap();
        let summer = select_local_time(&summer, None).unwrap();
        assert_eq!(
            (winter.abbreviation(), winter.utc_offset_display()),
            ("EST", "UTC-05:00".into())
        );
        assert_eq!(
            (summer.abbreviation(), summer.utc_offset_display()),
            ("EDT", "UTC-04:00".into())
        );
    }

    #[test]
    fn chart_defaults_match_the_studio_contract() {
        let options = ChartCalculationOptions::default();
        assert_eq!(options.zodiac(), ZodiacId::Tropical);
        assert_eq!(options.house_system(), HouseSystemId::Placidus);
        assert_eq!(
            default_chart_points()[..2],
            [ChartPointId::Moon, ChartPointId::Sun]
        );
        assert_eq!(
            default_aspects()
                .iter()
                .map(AspectDefinition::orb_degrees)
                .collect::<Vec<_>>(),
            vec![8.0, 8.0, 6.0, 6.0, 4.0]
        );
    }
}
