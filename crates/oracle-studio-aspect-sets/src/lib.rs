//! Global, unencrypted, content-addressed aspect-set settings.

use std::collections::BTreeSet;

use astraeus_core::{
    AspectDefinition, AspectDefinitions, ChartPointSelection, PhaseAwareAspectDefinition,
    PhaseAwareAspectDefinitions,
};
pub use astraeus_core::{AspectKind, AspectOrbValues, ChartPointId};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const ASPECT_SET_SCHEMA_VERSION: u32 = 2;
pub const ASPECT_SET_SETTINGS_VERSION: u32 = 1;
pub const MAX_IMPORT_BYTES: usize = 64 * 1024;
pub const STANDARD_ID: &str = "builtin.standard";
const BUILTIN_IDS: [&str; 4] = [
    "builtin.tight",
    STANDARD_ID,
    "builtin.synastry",
    "builtin.synwide",
];

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AspectSetRule {
    kind: AspectKind,
    enabled: bool,
    orbs: AspectOrbValues,
    display_order: u8,
}

impl AspectSetRule {
    pub fn new(
        kind: AspectKind,
        enabled: bool,
        orbs: AspectOrbValues,
        display_order: u8,
    ) -> Result<Self, AspectSetError> {
        for value in [
            orbs.luminary_applying_degrees(),
            orbs.luminary_separating_degrees(),
            orbs.other_applying_degrees(),
            orbs.other_separating_degrees(),
        ] {
            if !(0.0..=30.0).contains(&value) {
                return Err(AspectSetError::InvalidOrb(value.to_string()));
            }
        }
        Ok(Self {
            kind,
            enabled,
            orbs,
            display_order,
        })
    }

    pub const fn kind(self) -> AspectKind {
        self.kind
    }
    pub const fn enabled(self) -> bool {
        self.enabled
    }
    pub const fn orbs(self) -> AspectOrbValues {
        self.orbs
    }
    pub const fn display_order(self) -> u8 {
        self.display_order
    }
    pub fn with_enabled(self, enabled: bool) -> Self {
        Self { enabled, ..self }
    }
    pub fn with_orbs(self, orbs: AspectOrbValues) -> Result<Self, AspectSetError> {
        Self::new(self.kind, self.enabled, orbs, self.display_order)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AspectSet {
    schema_version: u32,
    id: String,
    revision: u32,
    name: String,
    description: String,
    built_in: bool,
    rules: Vec<AspectSetRule>,
    displayed_points: Vec<ChartPointId>,
    aspected_points: Vec<ChartPointId>,
    content_id: String,
}

#[derive(Serialize)]
struct CanonicalAspectSetRef<'a> {
    schema_version: u32,
    id: &'a str,
    revision: u32,
    name: &'a str,
    description: &'a str,
    built_in: bool,
    rules: &'a [AspectSetRule],
    displayed_points: &'a [ChartPointId],
    aspected_points: &'a [ChartPointId],
}

#[derive(Serialize)]
struct CanonicalAspectSetV1Ref<'a> {
    schema_version: u32,
    id: &'a str,
    revision: u32,
    name: &'a str,
    description: &'a str,
    built_in: bool,
    rules: &'a [AspectSetRule],
    points: &'a [ChartPointId],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AspectSetWireV2 {
    schema_version: u32,
    id: String,
    revision: u32,
    name: String,
    description: String,
    built_in: bool,
    rules: Vec<AspectSetRule>,
    displayed_points: Vec<ChartPointId>,
    aspected_points: Vec<ChartPointId>,
    content_id: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AspectSetWireV1 {
    schema_version: u32,
    id: String,
    revision: u32,
    name: String,
    description: String,
    built_in: bool,
    rules: Vec<AspectSetRule>,
    points: Vec<ChartPointId>,
    content_id: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum AspectSetWire {
    V2(AspectSetWireV2),
    V1(AspectSetWireV1),
}

struct AspectPointSelections {
    displayed: Vec<ChartPointId>,
    aspected: Vec<ChartPointId>,
}

impl AspectSet {
    pub fn new_user(
        id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        rules: Vec<AspectSetRule>,
        points: Vec<ChartPointId>,
    ) -> Result<Self, AspectSetError> {
        Self::new_user_with_points(id, name, description, rules, points.clone(), points)
    }

    pub fn new_user_with_points(
        id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        rules: Vec<AspectSetRule>,
        displayed_points: Vec<ChartPointId>,
        aspected_points: Vec<ChartPointId>,
    ) -> Result<Self, AspectSetError> {
        Self::new_internal(
            id,
            1,
            name,
            description,
            false,
            rules,
            AspectPointSelections {
                displayed: displayed_points,
                aspected: aspected_points,
            },
        )
    }

    fn new_internal(
        id: impl Into<String>,
        revision: u32,
        name: impl Into<String>,
        description: impl Into<String>,
        built_in: bool,
        rules: Vec<AspectSetRule>,
        points: AspectPointSelections,
    ) -> Result<Self, AspectSetError> {
        let mut set = Self {
            schema_version: ASPECT_SET_SCHEMA_VERSION,
            id: id.into(),
            revision,
            name: name.into(),
            description: description.into(),
            built_in,
            rules,
            displayed_points: points.displayed,
            aspected_points: points.aspected,
            content_id: String::new(),
        };
        set.validate_without_identity()?;
        set.content_id = set.computed_content_id()?;
        Ok(set)
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self, AspectSetError> {
        if bytes.len() > MAX_IMPORT_BYTES {
            return Err(AspectSetError::ImportTooLarge(bytes.len()));
        }
        let set: Self = serde_json::from_slice(bytes)?;
        set.validate()?;
        if set.built_in || is_reserved_id(&set.id) {
            return Err(AspectSetError::ReservedId(set.id));
        }
        Ok(set)
    }

    pub fn to_pretty_json(&self) -> Result<Vec<u8>, AspectSetError> {
        self.validate()?;
        Ok(serde_json::to_vec_pretty(self)?)
    }

    pub fn id(&self) -> &str {
        &self.id
    }
    pub const fn revision(&self) -> u32 {
        self.revision
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn description(&self) -> &str {
        &self.description
    }
    pub const fn built_in(&self) -> bool {
        self.built_in
    }
    pub fn rules(&self) -> &[AspectSetRule] {
        &self.rules
    }
    pub fn displayed_points(&self) -> &[ChartPointId] {
        &self.displayed_points
    }
    pub fn aspected_points(&self) -> &[ChartPointId] {
        &self.aspected_points
    }
    /// Compatibility alias for the calculation-participating selection.
    pub fn points(&self) -> &[ChartPointId] {
        self.aspected_points()
    }
    pub fn content_id(&self) -> &str {
        &self.content_id
    }

    pub fn duplicate(
        &self,
        id: impl Into<String>,
        name: impl Into<String>,
    ) -> Result<Self, AspectSetError> {
        Self::new_user_with_points(
            id,
            name,
            format!("Copy of {}. {}", self.name, self.description),
            self.rules.clone(),
            self.displayed_points.clone(),
            self.aspected_points.clone(),
        )
    }

    pub fn renamed(&self, name: impl Into<String>) -> Result<Self, AspectSetError> {
        self.revised_with_points(
            name,
            self.description.clone(),
            self.rules.clone(),
            self.displayed_points.clone(),
            self.aspected_points.clone(),
        )
    }

    pub fn revised(
        &self,
        name: impl Into<String>,
        description: impl Into<String>,
        rules: Vec<AspectSetRule>,
        points: Vec<ChartPointId>,
    ) -> Result<Self, AspectSetError> {
        self.revised_with_points(name, description, rules, points.clone(), points)
    }

    pub fn revised_with_points(
        &self,
        name: impl Into<String>,
        description: impl Into<String>,
        rules: Vec<AspectSetRule>,
        displayed_points: Vec<ChartPointId>,
        aspected_points: Vec<ChartPointId>,
    ) -> Result<Self, AspectSetError> {
        if self.built_in {
            return Err(AspectSetError::ImmutableBuiltin(self.id.clone()));
        }
        Self::new_internal(
            self.id.clone(),
            self.revision
                .checked_add(1)
                .ok_or(AspectSetError::RevisionOverflow)?,
            name,
            description,
            false,
            rules,
            AspectPointSelections {
                displayed: displayed_points,
                aspected: aspected_points,
            },
        )
    }

    pub fn snapshot(&self) -> AspectSetSnapshot {
        AspectSetSnapshot {
            aspect_set_id: self.id.clone(),
            revision: self.revision,
            content_id: self.content_id.clone(),
            rules: self.rules.clone(),
            points: self.aspected_points.clone(),
        }
    }

    pub fn validate(&self) -> Result<(), AspectSetError> {
        self.validate_without_identity()?;
        let actual = self.computed_content_id()?;
        if self.content_id != actual {
            return Err(AspectSetError::ContentIdMismatch {
                expected: self.content_id.clone(),
                actual,
            });
        }
        Ok(())
    }

    fn validate_without_identity(&self) -> Result<(), AspectSetError> {
        if self.schema_version != ASPECT_SET_SCHEMA_VERSION {
            return Err(AspectSetError::UnsupportedSchema(self.schema_version));
        }
        validate_id(&self.id, self.built_in)?;
        if self.revision == 0 {
            return Err(AspectSetError::InvalidRevision);
        }
        validate_text("name", &self.name, 256)?;
        validate_text("description", &self.description, 4096)?;
        validate_rules(&self.rules)?;
        validate_points(&self.displayed_points)?;
        validate_points(&self.aspected_points)
    }

    fn computed_content_id(&self) -> Result<String, AspectSetError> {
        let canonical = CanonicalAspectSetRef {
            schema_version: self.schema_version,
            id: &self.id,
            revision: self.revision,
            name: &self.name,
            description: &self.description,
            built_in: self.built_in,
            rules: &self.rules,
            displayed_points: &self.displayed_points,
            aspected_points: &self.aspected_points,
        };
        Ok(format!(
            "sha256:{:x}",
            Sha256::digest(serde_json::to_vec(&canonical)?)
        ))
    }
}

impl<'de> Deserialize<'de> for AspectSet {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let set = match AspectSetWire::deserialize(deserializer)? {
            AspectSetWire::V2(wire) => {
                let set = Self {
                    schema_version: wire.schema_version,
                    id: wire.id,
                    revision: wire.revision,
                    name: wire.name,
                    description: wire.description,
                    built_in: wire.built_in,
                    rules: wire.rules,
                    displayed_points: wire.displayed_points,
                    aspected_points: wire.aspected_points,
                    content_id: wire.content_id,
                };
                set.validate().map_err(serde::de::Error::custom)?;
                set
            }
            AspectSetWire::V1(wire) => {
                if wire.schema_version != 1 {
                    return Err(serde::de::Error::custom(AspectSetError::UnsupportedSchema(
                        wire.schema_version,
                    )));
                }
                validate_id(&wire.id, wire.built_in).map_err(serde::de::Error::custom)?;
                if wire.revision == 0 {
                    return Err(serde::de::Error::custom(AspectSetError::InvalidRevision));
                }
                validate_text("name", &wire.name, 256).map_err(serde::de::Error::custom)?;
                validate_text("description", &wire.description, 4096)
                    .map_err(serde::de::Error::custom)?;
                validate_rules(&wire.rules).map_err(serde::de::Error::custom)?;
                validate_points(&wire.points).map_err(serde::de::Error::custom)?;
                let canonical = CanonicalAspectSetV1Ref {
                    schema_version: 1,
                    id: &wire.id,
                    revision: wire.revision,
                    name: &wire.name,
                    description: &wire.description,
                    built_in: wire.built_in,
                    rules: &wire.rules,
                    points: &wire.points,
                };
                let actual = format!(
                    "sha256:{:x}",
                    Sha256::digest(
                        serde_json::to_vec(&canonical).map_err(serde::de::Error::custom)?
                    )
                );
                if wire.content_id != actual {
                    return Err(serde::de::Error::custom(
                        AspectSetError::ContentIdMismatch {
                            expected: wire.content_id,
                            actual,
                        },
                    ));
                }
                Self::new_internal(
                    wire.id,
                    wire.revision,
                    wire.name,
                    wire.description,
                    wire.built_in,
                    wire.rules,
                    AspectPointSelections {
                        displayed: wire.points.clone(),
                        aspected: wire.points,
                    },
                )
                .map_err(serde::de::Error::custom)?
            }
        };
        Ok(set)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AspectSetSnapshot {
    aspect_set_id: String,
    revision: u32,
    content_id: String,
    rules: Vec<AspectSetRule>,
    points: Vec<ChartPointId>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AspectSetSnapshotWire {
    aspect_set_id: String,
    revision: u32,
    content_id: String,
    rules: Vec<AspectSetRule>,
    points: Vec<ChartPointId>,
}

impl AspectSetSnapshot {
    /// Build a v5 snapshot for callers of the pre-aspect-set uniform-orb API.
    /// Disabled zero-orb rules fill any absent Ptolemaic kinds, while the point
    /// list is the stable union of the two legacy selections.
    pub fn legacy_uniform(
        definitions: &AspectDefinitions,
        first_points: &ChartPointSelection,
        second_points: &ChartPointSelection,
    ) -> Result<Self, AspectSetError> {
        let rules = [
            AspectKind::Conjunction,
            AspectKind::Opposition,
            AspectKind::Square,
            AspectKind::Trine,
            AspectKind::Sextile,
        ]
        .into_iter()
        .enumerate()
        .map(|(display_order, kind)| {
            let definition = definitions
                .as_slice()
                .iter()
                .find(|definition| definition.kind() == kind);
            let orb = definition.map_or(0.0, |definition| definition.orb_degrees());
            AspectSetRule::new(
                kind,
                definition.is_some(),
                AspectOrbValues::uniform(orb)
                    .map_err(|error| AspectSetError::Astraeus(error.to_string()))?,
                display_order as u8,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
        let mut points = first_points.as_slice().to_vec();
        for point in second_points.as_slice() {
            if !points.contains(point) {
                points.push(*point);
            }
        }
        Ok(AspectSet::new_user(
            "user.legacy-comparison-preset",
            "Legacy comparison preset",
            "Compatibility snapshot resolved from the pre-aspect-set uniform-orb API.",
            rules,
            points,
        )?
        .snapshot())
    }

    pub fn aspect_set_id(&self) -> &str {
        &self.aspect_set_id
    }
    pub const fn revision(&self) -> u32 {
        self.revision
    }
    pub fn content_id(&self) -> &str {
        &self.content_id
    }
    pub fn rules(&self) -> &[AspectSetRule] {
        &self.rules
    }
    pub fn points(&self) -> &[ChartPointId] {
        &self.points
    }

    pub fn phase_aware_definitions(&self) -> Result<PhaseAwareAspectDefinitions, AspectSetError> {
        PhaseAwareAspectDefinitions::new(
            self.rules
                .iter()
                .filter(|rule| rule.enabled)
                .map(|rule| PhaseAwareAspectDefinition::new(rule.kind, rule.orbs))
                .collect(),
        )
        .map_err(|error| AspectSetError::Astraeus(error.to_string()))
    }

    /// Uniform widest rules used only to preserve legacy schema-v1 artifact
    /// envelopes while vault v5 stores the phase-aware result separately.
    pub fn legacy_uniform_definitions(&self) -> Result<AspectDefinitions, AspectSetError> {
        let definitions = self
            .rules
            .iter()
            .filter(|rule| rule.enabled)
            .map(|rule| {
                let orbs = rule.orbs;
                let widest = [
                    orbs.luminary_applying_degrees(),
                    orbs.luminary_separating_degrees(),
                    orbs.other_applying_degrees(),
                    orbs.other_separating_degrees(),
                ]
                .into_iter()
                .max_by(f64::total_cmp)
                .expect("four orb values exist");
                AspectDefinition::new(rule.kind, widest)
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| AspectSetError::Astraeus(error.to_string()))?;
        AspectDefinitions::new(definitions)
            .map_err(|error| AspectSetError::Astraeus(error.to_string()))
    }

    pub fn point_selection(&self) -> Result<ChartPointSelection, AspectSetError> {
        ChartPointSelection::new(self.points.clone())
            .map_err(|error| AspectSetError::Astraeus(error.to_string()))
    }

    fn validate(&self) -> Result<(), AspectSetError> {
        validate_id(&self.aspect_set_id, is_reserved_id(&self.aspect_set_id))?;
        if self.revision == 0 {
            return Err(AspectSetError::InvalidRevision);
        }
        validate_content_id(&self.content_id)?;
        validate_rules(&self.rules)?;
        validate_points(&self.points)
    }
}

impl<'de> Deserialize<'de> for AspectSetSnapshot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = AspectSetSnapshotWire::deserialize(deserializer)?;
        let snapshot = Self {
            aspect_set_id: wire.aspect_set_id,
            revision: wire.revision,
            content_id: wire.content_id,
            rules: wire.rules,
            points: wire.points,
        };
        snapshot.validate().map_err(serde::de::Error::custom)?;
        Ok(snapshot)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AspectSetSettings {
    schema_version: u32,
    sets: Vec<AspectSet>,
    selected_aspect_set_id: String,
}

impl Default for AspectSetSettings {
    fn default() -> Self {
        Self {
            schema_version: ASPECT_SET_SETTINGS_VERSION,
            sets: builtins(),
            selected_aspect_set_id: STANDARD_ID.into(),
        }
    }
}

impl AspectSetSettings {
    pub fn sets(&self) -> &[AspectSet] {
        &self.sets
    }
    pub fn selected_aspect_set_id(&self) -> &str {
        &self.selected_aspect_set_id
    }
    pub fn selected(&self) -> &AspectSet {
        self.sets
            .iter()
            .find(|set| set.id == self.selected_aspect_set_id)
            .expect("validated settings retain selected set")
    }

    pub fn validate(&self) -> Result<(), AspectSetError> {
        if self.schema_version != ASPECT_SET_SETTINGS_VERSION {
            return Err(AspectSetError::UnsupportedSettingsSchema(
                self.schema_version,
            ));
        }
        let defaults = builtins();
        let mut ids = BTreeSet::new();
        for set in &self.sets {
            set.validate()?;
            if !ids.insert(set.id.as_str()) {
                return Err(AspectSetError::DuplicateId(set.id.clone()));
            }
            if set.built_in
                && defaults
                    .iter()
                    .find(|default| default.id == set.id)
                    .is_none_or(|default| default != set)
            {
                return Err(AspectSetError::ImmutableBuiltin(set.id.clone()));
            }
        }
        if !BUILTIN_IDS.iter().all(|id| ids.contains(id)) {
            return Err(AspectSetError::MissingBuiltins);
        }
        if !ids.contains(self.selected_aspect_set_id.as_str()) {
            return Err(AspectSetError::MissingSelection(
                self.selected_aspect_set_id.clone(),
            ));
        }
        Ok(())
    }

    pub fn select(&mut self, id: &str) -> Result<(), AspectSetError> {
        if !self.sets.iter().any(|set| set.id == id) {
            return Err(AspectSetError::MissingSelection(id.into()));
        }
        self.selected_aspect_set_id = id.into();
        Ok(())
    }

    pub fn save_user(&mut self, set: AspectSet) -> Result<(), AspectSetError> {
        set.validate()?;
        if set.built_in || is_reserved_id(&set.id) {
            return Err(AspectSetError::ImmutableBuiltin(set.id));
        }
        if let Some(existing) = self.sets.iter_mut().find(|existing| existing.id == set.id) {
            if existing.built_in {
                return Err(AspectSetError::ImmutableBuiltin(existing.id.clone()));
            }
            *existing = set;
        } else {
            self.sets.push(set);
        }
        self.validate()
    }

    pub fn import(&mut self, bytes: &[u8]) -> Result<&AspectSet, AspectSetError> {
        let set = AspectSet::from_json(bytes)?;
        if self.sets.iter().any(|existing| existing.id == set.id) {
            return Err(AspectSetError::DuplicateId(set.id));
        }
        let id = set.id.clone();
        self.sets.push(set);
        self.selected_aspect_set_id = id;
        self.validate()?;
        Ok(self.selected())
    }

    pub fn duplicate_selected(
        &mut self,
        id: impl Into<String>,
        name: impl Into<String>,
    ) -> Result<(), AspectSetError> {
        let duplicate = self.selected().duplicate(id, name)?;
        if self.sets.iter().any(|existing| existing.id == duplicate.id) {
            return Err(AspectSetError::DuplicateId(duplicate.id));
        }
        self.selected_aspect_set_id = duplicate.id.clone();
        self.sets.push(duplicate);
        self.validate()
    }

    pub fn rename_selected(&mut self, name: impl Into<String>) -> Result<(), AspectSetError> {
        let selected = self.selected().renamed(name)?;
        self.save_user(selected)
    }

    pub fn delete_selected(&mut self) -> Result<(), AspectSetError> {
        if self.selected().built_in {
            return Err(AspectSetError::ImmutableBuiltin(
                self.selected_aspect_set_id.clone(),
            ));
        }
        let id = self.selected_aspect_set_id.clone();
        self.sets.retain(|set| set.id != id);
        self.selected_aspect_set_id = STANDARD_ID.into();
        self.validate()
    }

    pub fn reset_builtins(&mut self) -> Result<(), AspectSetError> {
        let selected_was_builtin = is_reserved_id(&self.selected_aspect_set_id);
        self.sets.retain(|set| !set.built_in);
        let mut restored = builtins();
        restored.append(&mut self.sets);
        self.sets = restored;
        if selected_was_builtin {
            self.selected_aspect_set_id = STANDARD_ID.into();
        }
        self.validate()
    }
}

#[derive(Debug, Error)]
pub enum AspectSetError {
    #[error("invalid aspect-set JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported aspect-set schema version {0}")]
    UnsupportedSchema(u32),
    #[error("unsupported aspect-set settings schema version {0}")]
    UnsupportedSettingsSchema(u32),
    #[error("aspect-set import is {0} bytes; maximum is 65536")]
    ImportTooLarge(usize),
    #[error("invalid aspect-set ID {0:?}")]
    InvalidId(String),
    #[error("reserved aspect-set ID {0}")]
    ReservedId(String),
    #[error("built-in aspect set {0} is immutable")]
    ImmutableBuiltin(String),
    #[error("duplicate aspect-set ID {0}")]
    DuplicateId(String),
    #[error("built-in aspect sets are missing")]
    MissingBuiltins,
    #[error("selected aspect set {0} is missing")]
    MissingSelection(String),
    #[error("aspect-set revision must be positive")]
    InvalidRevision,
    #[error("aspect-set revision overflow")]
    RevisionOverflow,
    #[error("aspect-set field {0} is blank or exceeds its bound")]
    InvalidText(&'static str),
    #[error("aspect-set rules must contain all five unique aspects and display orders 0 through 4")]
    InvalidRules,
    #[error("aspect-set points must be unique, non-empty, browser-supported, and exclude Chiron")]
    InvalidPoints,
    #[error("aspect-set orb must be finite and in 0..=30 degrees, got {0}")]
    InvalidOrb(String),
    #[error("invalid aspect-set content ID")]
    InvalidContentId,
    #[error("aspect-set content ID mismatch: expected {expected}, got {actual}")]
    ContentIdMismatch { expected: String, actual: String },
    #[error("invalid Astraeus aspect policy: {0}")]
    Astraeus(String),
}

pub fn builtins() -> Vec<AspectSet> {
    vec![
        preset(
            "builtin.tight",
            "Tight",
            "Oracle-defined close-orb working set; not a universal standard.",
            [
                (2.0, 2.0, 1.0, 1.0),
                (2.0, 2.0, 1.0, 1.0),
                (1.5, 1.5, 1.0, 1.0),
            ],
            default_points(),
        ),
        preset(
            STANDARD_ID,
            "Standard",
            "Oracle's compatibility default matching the original uniform 8/6/4-degree behavior.",
            [
                (8.0, 8.0, 8.0, 8.0),
                (6.0, 6.0, 6.0, 6.0),
                (4.0, 4.0, 4.0, 4.0),
            ],
            default_points(),
        ),
        preset(
            "builtin.synastry",
            "Synastry",
            "Oracle-defined relationship comparison set; not a universal standard.",
            [
                (10.0, 8.0, 8.0, 6.0),
                (8.0, 6.0, 6.0, 5.0),
                (6.0, 5.0, 4.0, 3.0),
            ],
            all_browser_points(),
        ),
        preset(
            "builtin.synwide",
            "Synwide",
            "Oracle-defined wider relationship comparison set; not a universal standard.",
            [
                (12.0, 10.0, 10.0, 8.0),
                (10.0, 8.0, 8.0, 6.0),
                (8.0, 6.0, 6.0, 4.0),
            ],
            all_browser_points(),
        ),
    ]
}

fn preset(
    id: &str,
    name: &str,
    description: &str,
    groups: [(f64, f64, f64, f64); 3],
    points: Vec<ChartPointId>,
) -> AspectSet {
    let [conjunction_opposition, square_trine, sextile] = groups;
    let rule = |kind, values: (f64, f64, f64, f64), display_order| {
        AspectSetRule::new(
            kind,
            true,
            AspectOrbValues::new(values.0, values.1, values.2, values.3)
                .expect("preset orbs are valid"),
            display_order,
        )
        .expect("preset rule is valid")
    };
    AspectSet::new_internal(
        id,
        1,
        name,
        description,
        true,
        vec![
            rule(AspectKind::Conjunction, conjunction_opposition, 0),
            rule(AspectKind::Opposition, conjunction_opposition, 1),
            rule(AspectKind::Square, square_trine, 2),
            rule(AspectKind::Trine, square_trine, 3),
            rule(AspectKind::Sextile, sextile, 4),
        ],
        AspectPointSelections {
            displayed: points.clone(),
            aspected: points,
        },
    )
    .expect("built-in aspect set is valid")
}

fn default_points() -> Vec<ChartPointId> {
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

fn all_browser_points() -> Vec<ChartPointId> {
    vec![
        ChartPointId::Sun,
        ChartPointId::Moon,
        ChartPointId::Mercury,
        ChartPointId::Venus,
        ChartPointId::Mars,
        ChartPointId::Jupiter,
        ChartPointId::Saturn,
        ChartPointId::Uranus,
        ChartPointId::Neptune,
        ChartPointId::Pluto,
        ChartPointId::MeanNode,
        ChartPointId::TrueNode,
        ChartPointId::MeanSouthNode,
        ChartPointId::TrueSouthNode,
        ChartPointId::Ascendant,
        ChartPointId::Midheaven,
        ChartPointId::Descendant,
        ChartPointId::ImumCoeli,
        ChartPointId::Vertex,
    ]
}

fn validate_rules(rules: &[AspectSetRule]) -> Result<(), AspectSetError> {
    let expected = BTreeSet::from([
        AspectKind::Conjunction,
        AspectKind::Opposition,
        AspectKind::Square,
        AspectKind::Trine,
        AspectKind::Sextile,
    ]);
    let kinds = rules.iter().map(|rule| rule.kind).collect::<BTreeSet<_>>();
    let orders = rules
        .iter()
        .map(|rule| rule.display_order)
        .collect::<BTreeSet<_>>();
    if rules.len() != 5 || kinds != expected || orders != BTreeSet::from([0, 1, 2, 3, 4]) {
        return Err(AspectSetError::InvalidRules);
    }
    for rule in rules {
        AspectSetRule::new(rule.kind, rule.enabled, rule.orbs, rule.display_order)?;
    }
    Ok(())
}

fn validate_points(points: &[ChartPointId]) -> Result<(), AspectSetError> {
    let unique = points.iter().copied().collect::<BTreeSet<_>>();
    if points.is_empty() || unique.len() != points.len() || unique.contains(&ChartPointId::Chiron) {
        Err(AspectSetError::InvalidPoints)
    } else {
        Ok(())
    }
}

fn validate_id(id: &str, built_in: bool) -> Result<(), AspectSetError> {
    let prefix = if built_in { "builtin." } else { "user." };
    if !id.starts_with(prefix)
        || id.len() <= prefix.len()
        || id.len() > 128
        || !id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
        })
    {
        Err(AspectSetError::InvalidId(id.into()))
    } else {
        Ok(())
    }
}

fn is_reserved_id(id: &str) -> bool {
    id.starts_with("builtin.")
}

fn validate_text(field: &'static str, value: &str, maximum: usize) -> Result<(), AspectSetError> {
    if value.trim().is_empty() || value.trim() != value || value.len() > maximum {
        Err(AspectSetError::InvalidText(field))
    } else {
        Ok(())
    }
}

fn validate_content_id(value: &str) -> Result<(), AspectSetError> {
    if value.len() == 71
        && value.starts_with("sha256:")
        && value[7..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(AspectSetError::InvalidContentId)
    }
}
