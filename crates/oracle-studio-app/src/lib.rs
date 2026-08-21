//! Chart calculations parameterized over Astraeus' provider boundary.

use astraeus_artifacts::CalculationArtifact;
use astraeus_comparison::{ComparisonArtifact, ComparisonKind, ComparisonSpecification};
use astraeus_core::{
    AspectDefinition as AstraeusAspectDefinition, AspectDefinitions,
    AspectKind as AstraeusAspectKind, Ayanamsa, CalculationError, CalculationOptions,
    CalculationRequest, CalculationResult, CelestialObject, ChartPointId as AstraeusChartPointId,
    ChartPointSelection, EphemerisAdapter, GeographicLocation, HouseSystem,
    UtcInstant as AstraeusInstant, Zodiac,
};
use astraeus_derived::DerivedChartArtifact;
use astraeus_specifications::ChartSpecification;
use oracle_studio_chart_view::ChartScene;
use oracle_studio_core::{
    AmbiguousTimeChoice, AspectDefinition, AspectKindId, AyanamsaId, CelestialObjectId,
    ChartCalculation, ChartCalculationOptions, ChartDefinition, ChartPointId, ChartRole,
    ComparisonCalculation, HouseSystemId, LocalDateTimeInput, ResolvedLocalTime, SavedLocation,
    StableId, VaultDocument, ZodiacId, default_aspects, select_local_time,
};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EphemerisCapability {
    Unavailable,
    #[cfg(any(test, feature = "test-ephemeris"))]
    DeterministicTest,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableEphemeris;

impl EphemerisAdapter for UnavailableEphemeris {
    fn calculate(
        &self,
        _request: &CalculationRequest,
    ) -> Result<CalculationResult, CalculationError> {
        Err(CalculationError::DataUnavailable(
            "ephemeris provider unavailable in this build".into(),
        ))
    }
}

#[derive(Clone, Debug)]
pub struct ChartCalculationRequest {
    pub id: StableId,
    pub chart_definition_id: StableId,
    pub saved_location_id: StableId,
    pub ambiguous_time_choice: Option<AmbiguousTimeChoice>,
    pub calculated_at: String,
}

#[derive(Clone, Debug)]
pub struct ComparisonCalculationRequest {
    pub id: StableId,
    pub comparison_preset_id: StableId,
    pub calculated_at: String,
}

#[derive(Clone, Debug)]
pub struct WorkbenchCalculationRequest {
    pub inner_chart_definition_id: StableId,
    pub outer_chart_definition_id: StableId,
    pub inner_saved_location_id: StableId,
    pub outer_saved_location_id: StableId,
    pub outer_local_input: LocalDateTimeInput,
    pub outer_ambiguous_time_choice: Option<AmbiguousTimeChoice>,
}

#[derive(Clone, Debug)]
pub struct PreparedChartPreview {
    pub definition: ChartDefinition,
    pub local_input: LocalDateTimeInput,
    pub resolved_time: ResolvedLocalTime,
    pub location: SavedLocation,
    pub snapshot: CalculationArtifact,
}

#[derive(Clone, Debug)]
pub struct PreparedWorkbenchPreview {
    pub inner: PreparedChartPreview,
    pub outer: PreparedChartPreview,
    pub scene: ChartScene,
}

/// Calculate a render-only workbench comparison without creating immutable
/// vault records.
pub fn calculate_workbench_preview<P: EphemerisAdapter>(
    document: &VaultDocument,
    request: WorkbenchCalculationRequest,
    provider: &P,
) -> Result<PreparedWorkbenchPreview, AppError> {
    let inner_definition = chart(document, &request.inner_chart_definition_id)?.clone();
    let outer_definition = chart(document, &request.outer_chart_definition_id)?.clone();
    let inner_location = location(document, &request.inner_saved_location_id)?.clone();
    let outer_location = location(document, &request.outer_saved_location_id)?.clone();
    let inner = calculate_preview_chart(
        inner_definition.clone(),
        inner_definition.local_input().clone(),
        inner_location,
        None,
        provider,
    )?;
    let outer = calculate_preview_chart(
        outer_definition,
        request.outer_local_input,
        outer_location,
        request.outer_ambiguous_time_choice,
        provider,
    )?;

    let aspects = astraeus_aspects(&default_aspects())?;
    let points = workbench_points()?;
    let inner_options = preview_astraeus_options(inner.definition.calculation_options())?;
    let outer_options = preview_astraeus_options(outer.definition.calculation_options())?;
    let inner_specification =
        ChartSpecification::with_aspect_points(inner_options, aspects.clone(), points.clone())
            .map_err(|error| AppError::Astraeus(error.to_string()))?;
    let outer_specification =
        ChartSpecification::with_aspect_points(outer_options, aspects.clone(), points.clone())
            .map_err(|error| AppError::Astraeus(error.to_string()))?;
    let inner_derived = DerivedChartArtifact::new(inner.snapshot.clone(), inner_specification)
        .map_err(|error| AppError::Astraeus(error.to_string()))?;
    let outer_derived = DerivedChartArtifact::new(outer.snapshot.clone(), outer_specification)
        .map_err(|error| AppError::Astraeus(error.to_string()))?;
    let comparison = ComparisonArtifact::new(
        inner_derived,
        outer_derived,
        ComparisonSpecification::moving_second(
            ComparisonKind::TransitToNatal,
            aspects,
            points.clone(),
            points,
        )
        .map_err(|error| AppError::Astraeus(error.to_string()))?,
    )
    .map_err(|error| AppError::Astraeus(error.to_string()))?;
    let scene = ChartScene::from_comparison(&comparison)
        .map_err(|error| AppError::Astraeus(error.to_string()))?;
    Ok(PreparedWorkbenchPreview {
        inner,
        outer,
        scene,
    })
}

pub fn commit_workbench_update(
    document: &VaultDocument,
    preview: &PreparedWorkbenchPreview,
    calculation_id: StableId,
    calculated_at: String,
) -> Result<VaultDocument, AppError> {
    let definition = ChartDefinition::new(
        preview.outer.definition.id().clone(),
        preview.outer.definition.label(),
        preview.outer.definition.role(),
        preview.outer.definition.person_id().cloned(),
        preview.outer.local_input.clone(),
        preview.outer.definition.calculation_options().clone(),
        preview.outer.definition.ordered_points().to_vec(),
        preview.outer.definition.default_natal(),
    )?;
    let calculation = preview_calculation(
        &preview.outer,
        definition.id().clone(),
        calculation_id,
        calculated_at,
    )?;
    Ok(document
        .clone()
        .with_chart(definition)?
        .with_chart_calculation(calculation)?)
}

pub fn commit_workbench_save_as(
    document: &VaultDocument,
    preview: &PreparedWorkbenchPreview,
    chart_id: StableId,
    name: String,
    calculation_id: StableId,
    calculated_at: String,
) -> Result<VaultDocument, AppError> {
    let definition = ChartDefinition::new(
        chart_id,
        name,
        preview.outer.definition.role(),
        preview.outer.definition.person_id().cloned(),
        preview.outer.local_input.clone(),
        preview.outer.definition.calculation_options().clone(),
        preview.outer.definition.ordered_points().to_vec(),
        false,
    )?;
    let calculation = preview_calculation(
        &preview.outer,
        definition.id().clone(),
        calculation_id,
        calculated_at,
    )?;
    Ok(document
        .clone()
        .with_chart(definition)?
        .with_chart_calculation(calculation)?)
}

fn calculate_preview_chart<P: EphemerisAdapter>(
    definition: ChartDefinition,
    local_input: LocalDateTimeInput,
    location: SavedLocation,
    choice: Option<AmbiguousTimeChoice>,
    provider: &P,
) -> Result<PreparedChartPreview, AppError> {
    if local_input.time_zone() != location.time_zone() {
        return Err(AppError::ChartLocationTimeZoneMismatch);
    }
    let resolved_time = select_local_time(&local_input, choice)?;
    let calculation_request = CalculationRequest::from_options(
        AstraeusInstant::parse_rfc3339(resolved_time.utc_instant())
            .map_err(|error| AppError::Astraeus(error.to_string()))?,
        GeographicLocation::new(
            location.latitude_degrees(),
            location.longitude_degrees(),
            location.elevation_meters().unwrap_or(0.0),
        )
        .map_err(|error| AppError::Astraeus(error.to_string()))?,
        preview_astraeus_options(definition.calculation_options())?,
    );
    let result = provider
        .calculate(&calculation_request)
        .map_err(calculation_error)?;
    let snapshot = CalculationArtifact::new(calculation_request, result)
        .map_err(|error| AppError::Astraeus(error.to_string()))?;
    Ok(PreparedChartPreview {
        definition,
        local_input,
        resolved_time,
        location,
        snapshot,
    })
}

fn preview_calculation(
    preview: &PreparedChartPreview,
    chart_definition_id: StableId,
    id: StableId,
    calculated_at: String,
) -> Result<ChartCalculation, AppError> {
    Ok(ChartCalculation::new(
        id,
        chart_definition_id,
        preview.local_input.clone(),
        preview.resolved_time.clone(),
        preview.location.clone(),
        preview.snapshot.clone(),
        calculated_at,
    )?)
}

fn location<'a>(document: &'a VaultDocument, id: &StableId) -> Result<&'a SavedLocation, AppError> {
    document
        .saved_locations()
        .iter()
        .find(|location| location.id() == id)
        .ok_or(AppError::NotFound("saved location"))
}

fn preview_astraeus_options(
    options: &ChartCalculationOptions,
) -> Result<CalculationOptions, AppError> {
    CalculationOptions::new(
        vec![
            CelestialObject::Sun,
            CelestialObject::Moon,
            CelestialObject::Mercury,
            CelestialObject::Venus,
            CelestialObject::Mars,
            CelestialObject::Jupiter,
            CelestialObject::Saturn,
            CelestialObject::Uranus,
            CelestialObject::Neptune,
            CelestialObject::Pluto,
            CelestialObject::MeanNode,
            CelestialObject::TrueNode,
        ],
        match options.zodiac() {
            ZodiacId::Tropical => Zodiac::Tropical,
            ZodiacId::Sidereal => Zodiac::Sidereal,
        },
        options.ayanamsa().map(|ayanamsa| match ayanamsa {
            AyanamsaId::FaganBradley => Ayanamsa::FaganBradley,
            AyanamsaId::Lahiri => Ayanamsa::Lahiri,
            AyanamsaId::DeLuce => Ayanamsa::DeLuce,
            AyanamsaId::Raman => Ayanamsa::Raman,
            AyanamsaId::Krishnamurti => Ayanamsa::Krishnamurti,
            AyanamsaId::Yukteshwar => Ayanamsa::Yukteshwar,
            AyanamsaId::JnBhasin => Ayanamsa::JnBhasin,
        }),
        match options.house_system() {
            HouseSystemId::Placidus => HouseSystem::Placidus,
            HouseSystemId::Koch => HouseSystem::Koch,
            HouseSystemId::Porphyry => HouseSystem::Porphyry,
            HouseSystemId::Regiomontanus => HouseSystem::Regiomontanus,
            HouseSystemId::Campanus => HouseSystem::Campanus,
            HouseSystemId::Equal => HouseSystem::Equal,
            HouseSystemId::WholeSign => HouseSystem::WholeSign,
        },
    )
    .map_err(|error| AppError::Astraeus(error.to_string()))
}

fn workbench_points() -> Result<ChartPointSelection, AppError> {
    ChartPointSelection::new(vec![
        AstraeusChartPointId::Sun,
        AstraeusChartPointId::Moon,
        AstraeusChartPointId::Mercury,
        AstraeusChartPointId::Venus,
        AstraeusChartPointId::Mars,
        AstraeusChartPointId::Jupiter,
        AstraeusChartPointId::Saturn,
        AstraeusChartPointId::Uranus,
        AstraeusChartPointId::Neptune,
        AstraeusChartPointId::Pluto,
        AstraeusChartPointId::MeanNode,
        AstraeusChartPointId::TrueNode,
        AstraeusChartPointId::MeanSouthNode,
        AstraeusChartPointId::TrueSouthNode,
        AstraeusChartPointId::Ascendant,
        AstraeusChartPointId::Midheaven,
        AstraeusChartPointId::Descendant,
        AstraeusChartPointId::ImumCoeli,
        AstraeusChartPointId::Vertex,
    ])
    .map_err(|error| AppError::Astraeus(error.to_string()))
}

fn calculation_error(error: CalculationError) -> AppError {
    match error {
        CalculationError::DataUnavailable(message) => AppError::ProviderUnavailable(message),
        error => AppError::Astraeus(error.to_string()),
    }
}

pub fn calculate_chart<P: EphemerisAdapter>(
    document: &VaultDocument,
    request: ChartCalculationRequest,
    provider: &P,
) -> Result<VaultDocument, AppError> {
    let chart = document
        .chart_definitions()
        .iter()
        .find(|chart| chart.id() == &request.chart_definition_id)
        .ok_or(AppError::NotFound("chart definition"))?;
    let location = document
        .saved_locations()
        .iter()
        .find(|location| location.id() == &request.saved_location_id)
        .ok_or(AppError::NotFound("saved location"))?;
    if chart.local_input().time_zone() != location.time_zone() {
        return Err(AppError::ChartLocationTimeZoneMismatch);
    }
    let resolved = select_local_time(chart.local_input(), request.ambiguous_time_choice)?;
    let calculation_request = CalculationRequest::from_options(
        AstraeusInstant::parse_rfc3339(resolved.utc_instant())
            .map_err(|error| AppError::Astraeus(error.to_string()))?,
        GeographicLocation::new(
            location.latitude_degrees(),
            location.longitude_degrees(),
            location.elevation_meters().unwrap_or(0.0),
        )
        .map_err(|error| AppError::Astraeus(error.to_string()))?,
        astraeus_options(chart.calculation_options())?,
    );
    let result = provider
        .calculate(&calculation_request)
        .map_err(|error| match error {
            CalculationError::DataUnavailable(message) => AppError::ProviderUnavailable(message),
            error => AppError::Astraeus(error.to_string()),
        })?;
    let snapshot = CalculationArtifact::new(calculation_request, result)
        .map_err(|error| AppError::Astraeus(error.to_string()))?;
    let calculation = ChartCalculation::new(
        request.id,
        chart.id().clone(),
        chart.local_input().clone(),
        resolved,
        location.clone(),
        snapshot,
        request.calculated_at,
    )?;
    Ok(document.clone().with_chart_calculation(calculation)?)
}

pub fn calculate_comparison(
    document: &VaultDocument,
    request: ComparisonCalculationRequest,
) -> Result<VaultDocument, AppError> {
    let preset = document
        .comparison_presets()
        .iter()
        .find(|preset| preset.id() == &request.comparison_preset_id)
        .ok_or(AppError::NotFound("comparison preset"))?;
    let inner_chart = chart(document, preset.inner_chart_definition_id())?;
    let outer_chart = chart(document, preset.outer_chart_definition_id())?;
    let inner = current_calculation(document, inner_chart)?;
    let outer = current_calculation(document, outer_chart)?;
    let aspects = astraeus_aspects(preset.aspects())?;
    let inner_points = astraeus_points(preset.inner_points())?;
    let outer_points = astraeus_points(preset.outer_points())?;
    let inner_specification = ChartSpecification::with_aspect_points(
        astraeus_options(inner_chart.calculation_options())?,
        aspects.clone(),
        inner_points.clone(),
    )
    .map_err(|error| AppError::Astraeus(error.to_string()))?;
    let outer_specification = ChartSpecification::with_aspect_points(
        astraeus_options(outer_chart.calculation_options())?,
        aspects.clone(),
        outer_points.clone(),
    )
    .map_err(|error| AppError::Astraeus(error.to_string()))?;
    let inner_derived = DerivedChartArtifact::new(inner.snapshot().clone(), inner_specification)
        .map_err(|error| AppError::Astraeus(error.to_string()))?;
    let outer_derived = DerivedChartArtifact::new(outer.snapshot().clone(), outer_specification)
        .map_err(|error| AppError::Astraeus(error.to_string()))?;
    let specification =
        if inner_chart.role() == ChartRole::Natal && outer_chart.role() == ChartRole::Natal {
            ComparisonSpecification::synastry(aspects, inner_points, outer_points)
        } else {
            ComparisonSpecification::moving_second(
                comparison_kind(inner_chart.role(), outer_chart.role()),
                aspects,
                inner_points,
                outer_points,
            )
        }
        .map_err(|error| AppError::Astraeus(error.to_string()))?;
    let snapshot = ComparisonArtifact::new(inner_derived, outer_derived, specification)
        .map_err(|error| AppError::Astraeus(error.to_string()))?;
    let calculation = ComparisonCalculation::new(
        request.id,
        preset.id().clone(),
        inner.id().clone(),
        outer.id().clone(),
        snapshot,
        request.calculated_at,
    )?;
    Ok(document.clone().with_comparison_calculation(calculation)?)
}

fn chart<'a>(
    document: &'a VaultDocument,
    id: &StableId,
) -> Result<&'a oracle_studio_core::ChartDefinition, AppError> {
    document
        .chart_definitions()
        .iter()
        .find(|chart| chart.id() == id)
        .ok_or(AppError::NotFound("chart definition"))
}

fn current_calculation<'a>(
    document: &'a VaultDocument,
    chart: &oracle_studio_core::ChartDefinition,
) -> Result<&'a ChartCalculation, AppError> {
    let id = chart
        .current_calculation_id()
        .ok_or(AppError::MissingCurrentCalculation)?;
    document
        .chart_calculations()
        .iter()
        .find(|calculation| calculation.id() == id)
        .ok_or(AppError::NotFound("chart calculation"))
}

fn astraeus_options(options: &ChartCalculationOptions) -> Result<CalculationOptions, AppError> {
    CalculationOptions::new(
        options
            .ordered_objects()
            .iter()
            .copied()
            .map(astraeus_object)
            .collect(),
        match options.zodiac() {
            ZodiacId::Tropical => Zodiac::Tropical,
            ZodiacId::Sidereal => Zodiac::Sidereal,
        },
        options.ayanamsa().map(|ayanamsa| match ayanamsa {
            AyanamsaId::FaganBradley => Ayanamsa::FaganBradley,
            AyanamsaId::Lahiri => Ayanamsa::Lahiri,
            AyanamsaId::DeLuce => Ayanamsa::DeLuce,
            AyanamsaId::Raman => Ayanamsa::Raman,
            AyanamsaId::Krishnamurti => Ayanamsa::Krishnamurti,
            AyanamsaId::Yukteshwar => Ayanamsa::Yukteshwar,
            AyanamsaId::JnBhasin => Ayanamsa::JnBhasin,
        }),
        match options.house_system() {
            HouseSystemId::Placidus => HouseSystem::Placidus,
            HouseSystemId::Koch => HouseSystem::Koch,
            HouseSystemId::Porphyry => HouseSystem::Porphyry,
            HouseSystemId::Regiomontanus => HouseSystem::Regiomontanus,
            HouseSystemId::Campanus => HouseSystem::Campanus,
            HouseSystemId::Equal => HouseSystem::Equal,
            HouseSystemId::WholeSign => HouseSystem::WholeSign,
        },
    )
    .map_err(|error| AppError::Astraeus(error.to_string()))
}

fn astraeus_object(object: CelestialObjectId) -> CelestialObject {
    match object {
        CelestialObjectId::Moon => CelestialObject::Moon,
        CelestialObjectId::Sun => CelestialObject::Sun,
        CelestialObjectId::Mercury => CelestialObject::Mercury,
        CelestialObjectId::Venus => CelestialObject::Venus,
        CelestialObjectId::Mars => CelestialObject::Mars,
        CelestialObjectId::Jupiter => CelestialObject::Jupiter,
        CelestialObjectId::Saturn => CelestialObject::Saturn,
        CelestialObjectId::Uranus => CelestialObject::Uranus,
        CelestialObjectId::Neptune => CelestialObject::Neptune,
        CelestialObjectId::Pluto => CelestialObject::Pluto,
        CelestialObjectId::MeanNode => CelestialObject::MeanNode,
        CelestialObjectId::TrueNode => CelestialObject::TrueNode,
        CelestialObjectId::Chiron => CelestialObject::Chiron,
    }
}

fn astraeus_point(point: ChartPointId) -> AstraeusChartPointId {
    match point {
        ChartPointId::Moon => AstraeusChartPointId::Moon,
        ChartPointId::Sun => AstraeusChartPointId::Sun,
        ChartPointId::Mercury => AstraeusChartPointId::Mercury,
        ChartPointId::Venus => AstraeusChartPointId::Venus,
        ChartPointId::Mars => AstraeusChartPointId::Mars,
        ChartPointId::Jupiter => AstraeusChartPointId::Jupiter,
        ChartPointId::Saturn => AstraeusChartPointId::Saturn,
        ChartPointId::Uranus => AstraeusChartPointId::Uranus,
        ChartPointId::Neptune => AstraeusChartPointId::Neptune,
        ChartPointId::Pluto => AstraeusChartPointId::Pluto,
        ChartPointId::MeanNode => AstraeusChartPointId::MeanNode,
        ChartPointId::TrueNode => AstraeusChartPointId::TrueNode,
        ChartPointId::Chiron => AstraeusChartPointId::Chiron,
        ChartPointId::MeanSouthNode => AstraeusChartPointId::MeanSouthNode,
        ChartPointId::TrueSouthNode => AstraeusChartPointId::TrueSouthNode,
        ChartPointId::Ascendant => AstraeusChartPointId::Ascendant,
        ChartPointId::Midheaven => AstraeusChartPointId::Midheaven,
        ChartPointId::Descendant => AstraeusChartPointId::Descendant,
        ChartPointId::ImumCoeli => AstraeusChartPointId::ImumCoeli,
        ChartPointId::Vertex => AstraeusChartPointId::Vertex,
    }
}

fn astraeus_points(points: &[ChartPointId]) -> Result<ChartPointSelection, AppError> {
    ChartPointSelection::new(points.iter().copied().map(astraeus_point).collect())
        .map_err(|error| AppError::Astraeus(error.to_string()))
}

fn astraeus_aspects(aspects: &[AspectDefinition]) -> Result<AspectDefinitions, AppError> {
    let definitions = aspects
        .iter()
        .map(|aspect| {
            AstraeusAspectDefinition::new(
                match aspect.kind() {
                    AspectKindId::Conjunction => AstraeusAspectKind::Conjunction,
                    AspectKindId::Opposition => AstraeusAspectKind::Opposition,
                    AspectKindId::Square => AstraeusAspectKind::Square,
                    AspectKindId::Trine => AstraeusAspectKind::Trine,
                    AspectKindId::Sextile => AstraeusAspectKind::Sextile,
                },
                aspect.orb_degrees(),
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| AppError::Astraeus(error.to_string()))?;
    AspectDefinitions::new(definitions).map_err(|error| AppError::Astraeus(error.to_string()))
}

fn comparison_kind(inner: ChartRole, outer: ChartRole) -> ComparisonKind {
    match (inner, outer) {
        (ChartRole::Natal, ChartRole::Transit) => ComparisonKind::TransitToNatal,
        (ChartRole::Natal, ChartRole::Event) => ComparisonKind::EventToNatal,
        (ChartRole::Transit, ChartRole::Transit) => ComparisonKind::TransitToTransit,
        _ => ComparisonKind::Generic,
    }
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0} was not found")]
    NotFound(&'static str),
    #[error("chart has no current calculation")]
    MissingCurrentCalculation,
    #[error("chart and saved location must use the same IANA time zone")]
    ChartLocationTimeZoneMismatch,
    #[error("ephemeris provider unavailable: {0}")]
    ProviderUnavailable(String),
    #[error("Astraeus calculation failed: {0}")]
    Astraeus(String),
    #[error(transparent)]
    Model(#[from] oracle_studio_core::ModelError),
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use astraeus_comparison::ChartLayerArtifact;
    use astraeus_core::{AngularPosition, ChartAngles, DeterministicMock, HouseCusps, Position};
    use oracle_studio_core::{
        ChartDefinition, ChartRole, ComparisonPreset, LocalDateTimeInput, LocationProvenance,
        SavedLocation, WheelOrientation, default_aspects, default_chart_points,
    };

    use super::*;

    fn id(value: &str) -> StableId {
        StableId::new("test.id", value).unwrap()
    }

    fn provider(offset: f64) -> DeterministicMock {
        let objects = [
            CelestialObject::Sun,
            CelestialObject::Moon,
            CelestialObject::Mercury,
            CelestialObject::Venus,
            CelestialObject::Mars,
            CelestialObject::Jupiter,
            CelestialObject::Saturn,
            CelestialObject::Uranus,
            CelestialObject::Neptune,
            CelestialObject::Pluto,
            CelestialObject::MeanNode,
            CelestialObject::TrueNode,
        ];
        let positions = objects
            .iter()
            .enumerate()
            .map(|(index, object)| {
                (
                    *object,
                    Position::new(
                        (offset + index as f64 * 31.0).rem_euclid(360.0),
                        0.0,
                        1.0,
                        if index % 3 == 0 { -0.1 } else { 0.5 },
                    )
                    .unwrap(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let houses = HouseCusps::new(
            (0..12).map(|index| index as f64 * 30.0).collect(),
            ChartAngles::new(
                AngularPosition::new(0.0, 0.0).unwrap(),
                AngularPosition::new(270.0, 0.0).unwrap(),
                AngularPosition::new(180.0, 0.0).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        DeterministicMock::new(positions, houses)
    }

    fn chart(id_: &str, label: &str, role: ChartRole, date: &str) -> ChartDefinition {
        ChartDefinition::new(
            id(id_),
            label,
            role,
            None,
            LocalDateTimeInput::new(date, "12:00", "America/New_York").unwrap(),
            ChartCalculationOptions::default(),
            default_chart_points(),
            false,
        )
        .unwrap()
    }

    #[test]
    fn production_provider_never_fabricates_results() {
        let request = CalculationRequest::from_options(
            AstraeusInstant::parse_rfc3339("2026-08-19T12:00:00Z").unwrap(),
            GeographicLocation::new(0.0, 0.0, 0.0).unwrap(),
            CalculationOptions::new(
                vec![CelestialObject::Sun],
                Zodiac::Tropical,
                None,
                HouseSystem::WholeSign,
            )
            .unwrap(),
        );
        assert!(matches!(
            UnavailableEphemeris.calculate(&request),
            Err(CalculationError::DataUnavailable(_))
        ));
    }

    #[test]
    fn deterministic_acceptance_provider_populates_immutable_chart_and_comparison_snapshots() {
        let location = SavedLocation::new(
            id("fictional_harbor"),
            "Fictional Harbor",
            vec!["Example County".into()],
            "US",
            40.0,
            -75.0,
            None,
            "America/New_York",
            LocationProvenance::Manual,
        )
        .unwrap();
        let natal = chart("natal", "Fictional natal", ChartRole::Natal, "2000-01-15");
        let transit = chart(
            "transit",
            "Fictional transit",
            ChartRole::Transit,
            "2026-08-17",
        );
        let preset = ComparisonPreset::new(
            id("natal_transit"),
            "Fictional natal + transit",
            natal.id().clone(),
            transit.id().clone(),
            default_chart_points(),
            default_chart_points(),
            default_aspects(),
            WheelOrientation::AscendantLeft,
        )
        .unwrap();
        let document = VaultDocument::empty()
            .with_location(location.clone())
            .unwrap()
            .with_chart(natal)
            .unwrap()
            .with_chart(transit)
            .unwrap()
            .with_comparison(preset)
            .unwrap();
        let document = calculate_chart(
            &document,
            ChartCalculationRequest {
                id: id("natal_1"),
                chart_definition_id: id("natal"),
                saved_location_id: location.id().clone(),
                ambiguous_time_choice: None,
                calculated_at: "2026-08-19T12:00:00Z".into(),
            },
            &provider(5.0),
        )
        .unwrap();
        let document = calculate_chart(
            &document,
            ChartCalculationRequest {
                id: id("transit_1"),
                chart_definition_id: id("transit"),
                saved_location_id: location.id().clone(),
                ambiguous_time_choice: None,
                calculated_at: "2026-08-19T12:01:00Z".into(),
            },
            &provider(12.0),
        )
        .unwrap();
        let document = calculate_comparison(
            &document,
            ComparisonCalculationRequest {
                id: id("comparison_1"),
                comparison_preset_id: id("natal_transit"),
                calculated_at: "2026-08-19T12:02:00Z".into(),
            },
        )
        .unwrap();

        assert_eq!(document.chart_calculations().len(), 2);
        assert_eq!(document.comparison_calculations().len(), 1);
        let comparison = &document.comparison_calculations()[0];
        assert_eq!(comparison.inner_calculation_id().as_str(), "natal_1");
        assert_eq!(comparison.outer_calculation_id().as_str(), "transit_1");
        let ChartLayerArtifact::Physical(inner_snapshot) = comparison.snapshot().first() else {
            panic!("deterministic comparison uses a physical chart layer")
        };
        assert_eq!(
            inner_snapshot.calculation(),
            document.chart_calculations()[0].snapshot()
        );
        assert_eq!(
            VaultDocument::from_json(&document.to_json().unwrap()).unwrap(),
            document
        );

        let duplicate = calculate_chart(
            &document,
            ChartCalculationRequest {
                id: id("natal_1"),
                chart_definition_id: id("natal"),
                saved_location_id: location.id().clone(),
                ambiguous_time_choice: None,
                calculated_at: "2026-08-19T12:03:00Z".into(),
            },
            &provider(20.0),
        );
        assert!(matches!(
            duplicate,
            Err(AppError::Model(
                oracle_studio_core::ModelError::ImmutableRecord("chart calculation")
            ))
        ));
    }

    #[test]
    fn workbench_preview_is_transient_and_update_and_save_as_commit_distinctly() {
        let location = SavedLocation::new(
            id("fictional_harbor"),
            "Fictional Harbor",
            Vec::new(),
            "US",
            40.0,
            -75.0,
            None,
            "America/New_York",
            LocationProvenance::Manual,
        )
        .unwrap();
        let document = VaultDocument::empty()
            .with_location(location.clone())
            .unwrap()
            .with_chart(chart(
                "inner",
                "Fictional inner",
                ChartRole::Natal,
                "2000-01-15",
            ))
            .unwrap()
            .with_chart(chart(
                "outer",
                "Fictional outer",
                ChartRole::Transit,
                "2026-08-17",
            ))
            .unwrap();
        let moved_time =
            LocalDateTimeInput::new("2026-08-20", "12:00:00", "America/New_York").unwrap();
        let preview = calculate_workbench_preview(
            &document,
            WorkbenchCalculationRequest {
                inner_chart_definition_id: id("inner"),
                outer_chart_definition_id: id("outer"),
                inner_saved_location_id: location.id().clone(),
                outer_saved_location_id: location.id().clone(),
                outer_local_input: moved_time.clone(),
                outer_ambiguous_time_choice: None,
            },
            &provider(10.0),
        )
        .unwrap();
        assert!(document.chart_calculations().is_empty());
        assert_eq!(preview.outer.local_input, moved_time);
        assert!(!preview.scene.transit.is_empty());

        let updated = commit_workbench_update(
            &document,
            &preview,
            id("outer_calculation"),
            "2026-08-20T16:00:00Z".into(),
        )
        .unwrap();
        assert_eq!(updated.chart_definitions().len(), 2);
        assert_eq!(updated.chart_calculations().len(), 1);
        let outer = updated
            .chart_definitions()
            .iter()
            .find(|chart| chart.id().as_str() == "outer")
            .unwrap();
        assert_eq!(outer.local_input(), &moved_time);
        assert_eq!(
            outer.current_calculation_id().unwrap().as_str(),
            "outer_calculation"
        );
        let updated_again = commit_workbench_update(
            &updated,
            &preview,
            id("outer_calculation_2"),
            "2026-08-20T16:00:30Z".into(),
        )
        .unwrap();
        assert_eq!(updated_again.chart_calculations().len(), 2);
        assert_eq!(
            updated_again
                .chart_definitions()
                .iter()
                .find(|chart| chart.id().as_str() == "outer")
                .unwrap()
                .current_calculation_id()
                .unwrap()
                .as_str(),
            "outer_calculation_2"
        );
        assert!(
            updated_again
                .chart_calculations()
                .iter()
                .any(|calculation| calculation.id().as_str() == "outer_calculation")
        );

        let saved_as = commit_workbench_save_as(
            &document,
            &preview,
            id("outer_copy"),
            "Fictional outer copy".into(),
            id("outer_copy_calculation"),
            "2026-08-20T16:01:00Z".into(),
        )
        .unwrap();
        assert_eq!(saved_as.chart_definitions().len(), 3);
        assert_eq!(saved_as.chart_calculations().len(), 1);
        assert!(
            saved_as
                .chart_definitions()
                .iter()
                .any(|chart| chart.id().as_str() == "outer_copy"
                    && chart.label() == "Fictional outer copy")
        );
    }

    #[test]
    fn workbench_provider_failure_is_explicit_and_never_fabricates_a_scene() {
        let location = SavedLocation::new(
            id("fictional_harbor"),
            "Fictional Harbor",
            Vec::new(),
            "US",
            40.0,
            -75.0,
            None,
            "America/New_York",
            LocationProvenance::Manual,
        )
        .unwrap();
        let document = VaultDocument::empty()
            .with_location(location.clone())
            .unwrap()
            .with_chart(chart(
                "inner",
                "Fictional inner",
                ChartRole::Natal,
                "2000-01-15",
            ))
            .unwrap()
            .with_chart(chart(
                "outer",
                "Fictional outer",
                ChartRole::Transit,
                "2026-08-17",
            ))
            .unwrap();
        let error = calculate_workbench_preview(
            &document,
            WorkbenchCalculationRequest {
                inner_chart_definition_id: id("inner"),
                outer_chart_definition_id: id("outer"),
                inner_saved_location_id: location.id().clone(),
                outer_saved_location_id: location.id().clone(),
                outer_local_input: document.chart_definitions()[1].local_input().clone(),
                outer_ambiguous_time_choice: None,
            },
            &UnavailableEphemeris,
        )
        .unwrap_err();
        assert!(matches!(error, AppError::ProviderUnavailable(_)));
        assert!(document.chart_calculations().is_empty());
    }
}
