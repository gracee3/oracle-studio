use std::collections::BTreeMap;

use astraeus_artifacts::CalculationArtifact;
use astraeus_core::{
    AngularPosition, AspectDefinitions, AspectKind, CalculationRequest, CelestialObject,
    ChartAngles, ChartPointId, DeterministicMock, EphemerisAdapter, GeographicLocation, HouseCusps,
    HouseSystem, Position, UtcInstant, Zodiac, calculate_aspects,
};
use oracle_studio_chart_view::{
    AspectRow, ChartLayer, ChartSelection, ChartViewModel, ChartWorkspace, LayerRole,
    LayeredWorkspace, render_svg, render_svg_with_selection,
};

#[test]
fn view_model_formats_calculated_points_without_recalculation() {
    let request = CalculationRequest::new(
        UtcInstant::parse_rfc3339("2000-01-01T12:00:00Z").unwrap(),
        GeographicLocation::new(51.4779, 0.0, 46.0).unwrap(),
        vec![CelestialObject::Sun],
        Zodiac::Tropical,
        None,
        HouseSystem::Placidus,
    )
    .unwrap();
    let positions = BTreeMap::from([(
        CelestialObject::Sun,
        Position::new(280.3689197, 0.0002323, 0.983327645, -0.0194321).unwrap(),
    )]);
    let houses = HouseCusps::new(
        (0..12).map(|index| f64::from(index) * 30.0).collect(),
        ChartAngles::new(
            AngularPosition::new(0.0, 360.0).unwrap(),
            AngularPosition::new(270.0, 360.0).unwrap(),
            AngularPosition::new(180.0, 360.0).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let result = DeterministicMock::new(positions, houses)
        .calculate(&request)
        .unwrap();
    let artifact = CalculationArtifact::new(request, result).unwrap();
    let view = ChartViewModel::from_calculation(&artifact);
    assert_eq!(view.points.len(), 1);
    assert!(view.points[0].retrograde);
    assert_eq!(view.points[0].sign_index, 9);
    assert_eq!(view.houses.len(), 12);
    assert_eq!(view.placement_rows()[0].house, 10);
    let svg = render_svg(&view);
    assert!(svg.starts_with("<svg "));
    assert!(svg.contains("Sun"));
    assert_eq!(svg, render_svg(&view));
    let mut selection = ChartSelection::default();
    selection.select("Sun");
    assert!(render_svg_with_selection(&view, &selection).contains("#c62828"));
}

#[test]
fn selection_state_is_shared_by_wheel_and_table_clients() {
    let mut selection = ChartSelection::default();
    selection.select("sun");
    selection.select("moon");
    selection.select("sun");
    assert!(selection.is_selected("sun"));
    assert_eq!(
        selection.selected_ids().collect::<Vec<_>>(),
        vec!["moon", "sun"]
    );
    selection.deselect("moon");
    selection.clear();
    assert!(!selection.is_selected("sun"));
}

#[test]
fn aspect_rows_preserve_engine_results_for_tables() {
    let positions = std::collections::BTreeMap::from([
        (
            ChartPointId::from(CelestialObject::Sun),
            AngularPosition::new(0.0, 1.0).unwrap(),
        ),
        (
            ChartPointId::from(CelestialObject::Moon),
            AngularPosition::new(90.0, 1.0).unwrap(),
        ),
    ]);
    let aspects = calculate_aspects(&positions, &AspectDefinitions::ptolemaic(1.0).unwrap());
    let rows = AspectRow::from_aspects(&aspects);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kind, format!("{:?}", AspectKind::Square));
    assert_eq!(rows[0].orb_degrees, 0.0);
}

#[test]
fn workspace_keeps_wheel_tables_and_selection_together() {
    let chart = ChartViewModel {
        schema_version: 1,
        instant: "2000-01-01T12:00:00Z".into(),
        zodiac: "Tropical".into(),
        ayanamsa: None,
        points: Vec::new(),
        houses: (0..12).map(|index| f64::from(index) * 30.0).collect(),
        angles: oracle_studio_chart_view::AnglesView {
            ascendant_degrees: 0.0,
            midheaven_degrees: 270.0,
            vertex_degrees: 180.0,
        },
    };
    let mut workspace = ChartWorkspace::new(chart, Vec::new());
    workspace.selection.select("sun");
    assert!(workspace.placements.is_empty());
    assert!(workspace.aspects.is_empty());
    assert!(workspace.selection.is_selected("sun"));
    let export = workspace.export();
    assert!(export.svg.starts_with("<svg "));
    assert!(export.placements.is_empty());
}

#[test]
fn layered_workspace_keeps_roles_and_layer_identity_explicit() {
    let chart = ChartViewModel {
        schema_version: 1,
        instant: "2000-01-01T12:00:00Z".into(),
        zodiac: "Tropical".into(),
        ayanamsa: None,
        points: Vec::new(),
        houses: (0..12).map(|index| f64::from(index) * 30.0).collect(),
        angles: oracle_studio_chart_view::AnglesView {
            ascendant_degrees: 0.0,
            midheaven_degrees: 270.0,
            vertex_degrees: 180.0,
        },
    };
    let workspace = LayeredWorkspace::new(vec![ChartLayer {
        id: "natal".into(),
        role: LayerRole::Natal,
        chart,
    }]);
    assert_eq!(workspace.layer("natal").unwrap().role, LayerRole::Natal);
    assert!(workspace.layer("transit").is_none());
}
