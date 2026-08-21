use std::collections::BTreeSet;

use astraeus_artifacts::CalculationArtifact;
use astraeus_core::{CelestialObject, EphemerisAdapter};
use astraeus_moshier::MoshierEphemerisAdapter;
use oracle_studio_public_records::{
    CatalogError, ChartReadinessStatus, PublicRecordCatalog, RecordData,
};
use serde::Deserialize;

const CATALOG: &str = include_str!("../../../catalog/public-records-v1.json");
const MOSHIER_VECTORS: &str = include_str!("../../../fixtures/public-records/moshier-v1.json");
const JSON_SCHEMA: &str = include_str!("../../../catalog/public-records-v1.schema.json");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct VectorFile {
    schema_version: u32,
    engine: String,
    engine_version: String,
    vectors: Vec<MoshierVector>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MoshierVector {
    record_id: String,
    artifact_content_id: String,
    sun_longitude_degrees: f64,
    moon_longitude_degrees: f64,
    ascendant_degrees: f64,
    midheaven_degrees: f64,
}

#[test]
fn reviewed_catalog_is_strict_canonical_and_privacy_bounded() {
    let schema: serde_json::Value = serde_json::from_str(JSON_SCHEMA).unwrap();
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(schema["properties"]["schema_version"]["const"], 1);
    assert_eq!(schema["additionalProperties"], false);

    let catalog = PublicRecordCatalog::from_json(CATALOG).unwrap();
    assert_eq!(catalog.catalog_id(), "public-record-catalog.oracle-v1");
    assert_eq!(catalog.records().len(), 7);
    assert_eq!(
        catalog.catalog_content_id(),
        "sha256:206bbe1b68efc7b4d2e838f2b63603fd173f1d906a097ed2540f451f6033107b"
    );
    assert_eq!(
        catalog.computed_content_id().unwrap(),
        catalog.catalog_content_id()
    );

    let ids = catalog
        .records()
        .iter()
        .map(|record| record.record_id())
        .collect::<BTreeSet<_>>();
    assert_eq!(ids.len(), 7);
    assert_eq!(
        catalog
            .records()
            .iter()
            .filter(|record| record.chart_readiness() == ChartReadinessStatus::ChartReady)
            .count(),
        3
    );
    assert_eq!(
        catalog
            .records()
            .iter()
            .filter(|record| matches!(record.data(), RecordData::DeceasedPerson { .. }))
            .count(),
        4
    );
    for record in catalog.records() {
        assert_eq!(record.computed_content_id().unwrap(), record.content_id());
        assert_eq!(
            record.chart_request().unwrap().is_some(),
            record.chart_readiness() == ChartReadinessStatus::ChartReady
        );
    }

    let compact = catalog.to_json().unwrap();
    assert_eq!(
        PublicRecordCatalog::from_json(&compact)
            .unwrap()
            .catalog_content_id(),
        catalog.catalog_content_id()
    );
}

#[test]
fn unknown_fields_tampering_and_old_schemas_fail_closed() {
    let unknown = CATALOG.replacen(
        "\"schema_version\": 1,",
        "\"schema_version\": 1, \"unknown\": true,",
        1,
    );
    assert!(matches!(
        PublicRecordCatalog::from_json(&unknown),
        Err(CatalogError::Json(_))
    ));

    let old = CATALOG.replacen("\"schema_version\": 1", "\"schema_version\": 2", 1);
    assert!(matches!(
        PublicRecordCatalog::from_json(&old),
        Err(CatalogError::UnsupportedSchema(2))
    ));

    let tampered = CATALOG.replacen("Ada Lovelace", "Ada Lovelace (tampered)", 1);
    assert!(matches!(
        PublicRecordCatalog::from_json(&tampered),
        Err(CatalogError::RecordContentId { .. })
    ));
}

#[test]
fn reviewed_events_match_fixed_moshier_vectors() {
    let catalog = PublicRecordCatalog::from_json(CATALOG).unwrap();
    let vectors: VectorFile = serde_json::from_str(MOSHIER_VECTORS).unwrap();
    assert_eq!(vectors.schema_version, 1);
    assert_eq!(vectors.engine, "swisseph-rs Moshier");
    assert_eq!(vectors.engine_version, "0.2.0");

    let chart_ready = catalog
        .records()
        .iter()
        .filter(|record| record.chart_request().unwrap().is_some())
        .map(|record| record.record_id())
        .collect::<BTreeSet<_>>();
    let vector_ids = vectors
        .vectors
        .iter()
        .map(|vector| vector.record_id.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(chart_ready, vector_ids);

    for vector in vectors.vectors {
        let record = catalog
            .records()
            .iter()
            .find(|record| record.record_id() == vector.record_id)
            .unwrap();
        let request = record.chart_request().unwrap().unwrap();
        let result = MoshierEphemerisAdapter::new().calculate(&request).unwrap();
        let artifact = CalculationArtifact::new(request, result).unwrap();
        let sun = artifact.result().positions()[&CelestialObject::Sun].longitude_degrees();
        let moon = artifact.result().positions()[&CelestialObject::Moon].longitude_degrees();
        let ascendant = artifact.result().houses().ascendant_degrees();
        let midheaven = artifact.result().houses().midheaven_degrees();

        if std::env::var_os("ORACLE_PRINT_PUBLIC_RECORD_VECTORS").is_some() {
            eprintln!(
                "{}|{}|{sun:?}|{moon:?}|{ascendant:?}|{midheaven:?}",
                vector.record_id,
                artifact.content_id().unwrap()
            );
            continue;
        }
        assert_eq!(artifact.content_id().unwrap(), vector.artifact_content_id);
        assert_eq!(sun, vector.sun_longitude_degrees);
        assert_eq!(moon, vector.moon_longitude_degrees);
        assert_eq!(ascendant, vector.ascendant_degrees);
        assert_eq!(midheaven, vector.midheaven_degrees);
    }
}
