//! Fictional demo identity plus the opt-in native canonical builder.

pub const DEMO_VAULT_ID: &str = "7b6d8275-b622-4d47-a7f2-66fca6f84d2d";
pub const DEMO_TITLE: &str = "Oracle Studio Demo";
pub const DEMO_PASSWORD: &str = "oracle-demo";
pub const DEMO_ASSET_PATH: &str = "demo/oracle-studio-demo.oracle-vault";

#[cfg(feature = "builder")]
mod builder {
    use std::{collections::BTreeMap, fs, path::Path};

    use astraeus_moshier::MoshierEphemerisAdapter;
    use oracle_studio_app::{
        AppError, ChartCalculationRequest, ComparisonCalculationRequest, calculate_chart,
        calculate_comparison,
    };
    use oracle_studio_core::{
        ASTRAEUS_IMPORT_REVISION, ChartCalculationOptions, ChartDefinition, ChartRole,
        ComparisonPreset, LocalDateTimeInput, LocationProvenance, ModelError, PersonKind,
        PersonProfile, SavedLocation, StableId, VAULT_DOCUMENT_SCHEMA_VERSION, VaultDocument,
        WheelOrientation, WorkspaceState, default_aspects, default_chart_points,
    };
    use oracle_studio_vault::{
        FORMAT_VERSION as ENVELOPE_FORMAT_VERSION, VaultError, create_with_id_for_demo, inspect,
        open,
    };
    use serde::{Deserialize, Serialize};
    use sha2::{Digest, Sha256};
    use thiserror::Error;

    use crate::{DEMO_PASSWORD, DEMO_TITLE, DEMO_VAULT_ID};

    pub const DOCUMENT_FILENAME: &str = "oracle-studio-demo.document.json";
    pub const MANIFEST_FILENAME: &str = "oracle-studio-demo.manifest.json";
    pub const ENVELOPE_FILENAME: &str = "oracle-studio-demo.oracle-vault";
    const CALCULATED_AT: &str = "2026-08-21T16:00:00Z";

    #[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct DemoManifest {
        pub schema_version: u32,
        pub vault_id: String,
        pub title: String,
        pub vault_document_schema_version: u32,
        pub envelope_format_version: u16,
        pub astraeus_import_revision: String,
        pub ephemeris: String,
        pub aspect_set: String,
        pub document_sha256: String,
        pub people: usize,
        pub locations: usize,
        pub charts: usize,
        pub comparisons: usize,
        pub chart_artifact_content_ids: BTreeMap<String, String>,
        pub comparison_artifact_content_ids: BTreeMap<String, String>,
    }

    #[derive(Clone, Debug, PartialEq)]
    pub struct DemoBundle {
        pub document: VaultDocument,
        pub document_json: String,
        pub manifest: DemoManifest,
    }

    #[derive(Debug, Error)]
    pub enum DemoError {
        #[error(transparent)]
        Model(#[from] ModelError),
        #[error(transparent)]
        App(#[from] AppError),
        #[error(transparent)]
        Vault(#[from] VaultError),
        #[error("demo JSON error: {0}")]
        Json(#[from] serde_json::Error),
        #[error("demo filesystem error: {0}")]
        Io(#[from] std::io::Error),
        #[error("demo artifact content ID error: {0}")]
        Artifact(String),
        #[error("demo verification failed: {0}")]
        Verification(String),
    }

    pub fn build_demo_bundle() -> Result<DemoBundle, DemoError> {
        let avery_id = id("demo.person.avery-north")?;
        let mira_id = id("demo.person.mira-vale")?;
        let harbor_id = id("demo.location.juniper-harbor")?;
        let cedar_id = id("demo.location.cedar-observatory")?;
        let avery_chart_id = id("demo.chart.avery-north-natal")?;
        let mira_chart_id = id("demo.chart.mira-vale-natal")?;
        let harbor_transit_id = id("demo.chart.harbor-transit")?;
        let cedar_event_id = id("demo.chart.cedar-equinox-event")?;
        let synastry_id = id("demo.comparison.avery-mira-synastry")?;
        let transit_id = id("demo.comparison.avery-harbor-transit")?;
        let event_id = id("demo.comparison.mira-cedar-event")?;

        let people = vec![
            PersonProfile::new(
                avery_id.clone(),
                "Avery North",
                PersonKind::Personal,
                Some("Fictional demo record; not a real person.".into()),
            )?,
            PersonProfile::new(
                mira_id.clone(),
                "Mira Vale",
                PersonKind::Personal,
                Some("Fictional demo record; not a real person.".into()),
            )?,
        ];
        let harbor = SavedLocation::new(
            harbor_id.clone(),
            "Juniper Harbor",
            Vec::new(),
            "US",
            40.7128,
            -74.0060,
            None,
            "America/New_York",
            LocationProvenance::Manual,
        )?;
        let cedar = SavedLocation::new(
            cedar_id.clone(),
            "Cedar Observatory",
            Vec::new(),
            "GB",
            51.5074,
            -0.1278,
            None,
            "Europe/London",
            LocationProvenance::Manual,
        )?;
        let options = ChartCalculationOptions::default();
        let points = default_chart_points();
        let charts = vec![
            chart(
                avery_chart_id.clone(),
                "Avery North natal",
                ChartRole::Natal,
                Some(avery_id.clone()),
                "1988-04-12",
                "10:32:00",
                "America/New_York",
                &options,
                &points,
                true,
            )?,
            chart(
                mira_chart_id.clone(),
                "Mira Vale natal",
                ChartRole::Natal,
                Some(mira_id.clone()),
                "1992-09-23",
                "07:45:00",
                "Europe/London",
                &options,
                &points,
                true,
            )?,
            chart(
                harbor_transit_id.clone(),
                "Harbor Transit",
                ChartRole::Transit,
                None,
                "2026-08-21",
                "12:00:00",
                "America/New_York",
                &options,
                &points,
                false,
            )?,
            chart(
                cedar_event_id.clone(),
                "Cedar Equinox Event",
                ChartRole::Event,
                None,
                "2026-03-20",
                "14:00:00",
                "Europe/London",
                &options,
                &points,
                false,
            )?,
        ];
        let aspects = default_aspects();
        let comparisons = vec![
            comparison(
                synastry_id.clone(),
                "Avery North + Mira Vale synastry",
                avery_chart_id.clone(),
                mira_chart_id.clone(),
                &points,
                &aspects,
            )?,
            comparison(
                transit_id.clone(),
                "Avery North + Harbor Transit",
                avery_chart_id.clone(),
                harbor_transit_id.clone(),
                &points,
                &aspects,
            )?,
            comparison(
                event_id.clone(),
                "Mira Vale + Cedar Equinox Event",
                mira_chart_id.clone(),
                cedar_event_id.clone(),
                &points,
                &aspects,
            )?,
        ];
        let mut document = VaultDocument::new(
            people,
            vec![harbor, cedar],
            charts,
            Vec::new(),
            comparisons,
            Vec::new(),
            WorkspaceState::new(Some(avery_id), Some(synastry_id.clone())),
        )?;
        let provider = MoshierEphemerisAdapter::new();
        for (calculation, chart, location) in [
            (
                "demo.calculation.avery-north-natal",
                avery_chart_id,
                harbor_id.clone(),
            ),
            (
                "demo.calculation.mira-vale-natal",
                mira_chart_id,
                cedar_id.clone(),
            ),
            (
                "demo.calculation.harbor-transit",
                harbor_transit_id,
                harbor_id,
            ),
            (
                "demo.calculation.cedar-equinox-event",
                cedar_event_id,
                cedar_id,
            ),
        ] {
            document = calculate_chart(
                &document,
                ChartCalculationRequest {
                    id: id(calculation)?,
                    chart_definition_id: chart,
                    saved_location_id: location,
                    ambiguous_time_choice: None,
                    calculated_at: CALCULATED_AT.into(),
                },
                &provider,
            )?;
        }
        for (calculation, comparison) in [
            ("demo.calculation.avery-mira-synastry", synastry_id),
            ("demo.calculation.avery-harbor-transit", transit_id),
            ("demo.calculation.mira-cedar-event", event_id),
        ] {
            document = calculate_comparison(
                &document,
                ComparisonCalculationRequest {
                    id: id(calculation)?,
                    comparison_preset_id: comparison,
                    calculated_at: CALCULATED_AT.into(),
                },
            )?;
        }

        let document_json = document.to_json()?;
        let manifest = manifest(&document, &document_json)?;
        Ok(DemoBundle {
            document,
            document_json,
            manifest,
        })
    }

    pub fn create_demo_envelope(document: VaultDocument) -> Result<Vec<u8>, DemoError> {
        let (_, envelope) = create_with_id_for_demo(
            DEMO_VAULT_ID,
            DEMO_TITLE,
            DEMO_PASSWORD.as_bytes(),
            document,
        )?;
        Ok(envelope)
    }

    pub fn generate(output: &Path) -> Result<DemoManifest, DemoError> {
        let bundle = build_demo_bundle()?;
        let envelope = create_demo_envelope(bundle.document.clone())?;
        fs::create_dir_all(output)?;
        fs::write(output.join(DOCUMENT_FILENAME), &bundle.document_json)?;
        fs::write(
            output.join(MANIFEST_FILENAME),
            serde_json::to_string_pretty(&bundle.manifest)? + "\n",
        )?;
        fs::write(output.join(ENVELOPE_FILENAME), envelope)?;
        Ok(bundle.manifest)
    }

    pub fn verify(lock_path: &Path) -> Result<DemoManifest, DemoError> {
        let expected: DemoManifest = serde_json::from_str(&fs::read_to_string(lock_path)?)?;
        let first = build_demo_bundle()?;
        if first.manifest != expected {
            return Err(DemoError::Verification(format!(
                "reviewed lock {} differs from the canonical builder output",
                lock_path.display()
            )));
        }
        let second = build_demo_bundle()?;
        if first.document_json != second.document_json || first.manifest != second.manifest {
            return Err(DemoError::Verification(
                "canonical plaintext or manifest changed between builds".into(),
            ));
        }
        let first_envelope = create_demo_envelope(first.document.clone())?;
        let second_envelope = create_demo_envelope(second.document.clone())?;
        if first_envelope == second_envelope {
            return Err(DemoError::Verification(
                "encrypted demo outputs reused cryptographic randomness".into(),
            ));
        }
        for envelope in [&first_envelope, &second_envelope] {
            let header = inspect(envelope)?;
            if header.id() != DEMO_VAULT_ID || header.title() != DEMO_TITLE {
                return Err(DemoError::Verification(
                    "encrypted demo header identity changed".into(),
                ));
            }
            let opened = open(envelope, DEMO_PASSWORD.as_bytes())?;
            if opened.document() != &first.document {
                return Err(DemoError::Verification(
                    "encrypted demo did not open to canonical plaintext".into(),
                ));
            }
        }
        Ok(first.manifest)
    }

    fn id(value: &'static str) -> Result<StableId, DemoError> {
        Ok(StableId::new("demo.id", value)?)
    }

    #[allow(clippy::too_many_arguments)]
    fn chart(
        id: StableId,
        label: &str,
        role: ChartRole,
        person_id: Option<StableId>,
        date: &str,
        time: &str,
        zone: &str,
        options: &ChartCalculationOptions,
        points: &[oracle_studio_core::ChartPointId],
        default_natal: bool,
    ) -> Result<ChartDefinition, DemoError> {
        Ok(ChartDefinition::new(
            id,
            label,
            role,
            person_id,
            LocalDateTimeInput::new(date, time, zone)?,
            options.clone(),
            points.to_vec(),
            default_natal,
        )?)
    }

    fn comparison(
        id: StableId,
        label: &str,
        inner: StableId,
        outer: StableId,
        points: &[oracle_studio_core::ChartPointId],
        aspects: &[oracle_studio_core::AspectDefinition],
    ) -> Result<ComparisonPreset, DemoError> {
        Ok(ComparisonPreset::new(
            id,
            label,
            inner,
            outer,
            points.to_vec(),
            points.to_vec(),
            aspects.to_vec(),
            WheelOrientation::AscendantLeft,
        )?)
    }

    fn manifest(document: &VaultDocument, document_json: &str) -> Result<DemoManifest, DemoError> {
        let chart_artifact_content_ids = document
            .chart_calculations()
            .iter()
            .map(|calculation| {
                calculation
                    .snapshot()
                    .content_id()
                    .map(|content_id| (calculation.id().as_str().into(), content_id))
                    .map_err(|error| DemoError::Artifact(error.to_string()))
            })
            .collect::<Result<_, _>>()?;
        let comparison_artifact_content_ids = document
            .comparison_calculations()
            .iter()
            .map(|calculation| {
                calculation
                    .snapshot()
                    .content_id()
                    .map(|content_id| (calculation.id().as_str().into(), content_id))
                    .map_err(|error| DemoError::Artifact(error.to_string()))
            })
            .collect::<Result<_, _>>()?;
        Ok(DemoManifest {
            schema_version: 1,
            vault_id: DEMO_VAULT_ID.into(),
            title: DEMO_TITLE.into(),
            vault_document_schema_version: VAULT_DOCUMENT_SCHEMA_VERSION,
            envelope_format_version: ENVELOPE_FORMAT_VERSION,
            astraeus_import_revision: ASTRAEUS_IMPORT_REVISION.into(),
            ephemeris: "moshier".into(),
            aspect_set: "standard".into(),
            document_sha256: format!("sha256:{:x}", Sha256::digest(document_json.as_bytes())),
            people: document.people().len(),
            locations: document.saved_locations().len(),
            charts: document.chart_definitions().len(),
            comparisons: document.comparison_presets().len(),
            chart_artifact_content_ids,
            comparison_artifact_content_ids,
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn canonical_builder_matches_the_reviewed_lock() {
            let expected: DemoManifest = serde_json::from_str(include_str!(
                "../../../fixtures/demo/oracle-studio-demo.lock.json"
            ))
            .unwrap();
            let first = build_demo_bundle().unwrap();
            let second = build_demo_bundle().unwrap();
            assert_eq!(first.document_json, second.document_json);
            assert_eq!(first.manifest, second.manifest);
            assert_eq!(first.manifest, expected);
            assert_eq!((first.manifest.people, first.manifest.locations), (2, 2));
            assert_eq!((first.manifest.charts, first.manifest.comparisons), (4, 3));
        }

        #[test]
        fn encrypted_outputs_are_fresh_and_open_to_the_same_stable_demo() {
            let bundle = build_demo_bundle().unwrap();
            let first = create_demo_envelope(bundle.document.clone()).unwrap();
            let second = create_demo_envelope(bundle.document.clone()).unwrap();
            assert_ne!(first, second);
            for envelope in [first, second] {
                let header = inspect(&envelope).unwrap();
                assert_eq!(header.id(), DEMO_VAULT_ID);
                assert_eq!(header.title(), DEMO_TITLE);
                let opened = open(&envelope, DEMO_PASSWORD.as_bytes()).unwrap();
                assert_eq!(opened.document(), &bundle.document);
            }
        }
    }
}

#[cfg(feature = "builder")]
pub use builder::*;
