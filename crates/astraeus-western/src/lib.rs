//! Explicit, versioned Western astrology policy derived from neutral charts.

use astraeus_core::{CelestialObject, ChartPointId, PointPlacement, SignPlacement, ZodiacSign};
use astraeus_derived::DerivedChartArtifact;
use serde::{Deserialize, Serialize, Serializer};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RulershipPolicy {
    TraditionalV1,
    ModernV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecanPolicy {
    ChaldeanFacesV1,
    TriplicityDecansV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WesternPolicy {
    rulership: RulershipPolicy,
    decans: DecanPolicy,
}

impl WesternPolicy {
    pub const fn new(rulership: RulershipPolicy, decans: DecanPolicy) -> Self {
        Self { rulership, decans }
    }

    pub const fn rulership(self) -> RulershipPolicy {
        self.rulership
    }

    pub const fn decans(self) -> DecanPolicy {
        self.decans
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Element {
    Fire,
    Earth,
    Air,
    Water,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Modality {
    Cardinal,
    Fixed,
    Mutable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Polarity {
    Positive,
    Negative,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DignityKind {
    Domicile,
    Detriment,
    Exaltation,
    Fall,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DignityCondition {
    kind: DignityKind,
    exact_longitude_degrees: Option<f64>,
    distance_from_exact_degrees: Option<f64>,
}

impl DignityCondition {
    pub const fn kind(self) -> DignityKind {
        self.kind
    }

    pub fn exact_longitude_degrees(self) -> Option<f64> {
        self.exact_longitude_degrees
    }

    pub fn distance_from_exact_degrees(self) -> Option<f64> {
        self.distance_from_exact_degrees
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WesternPointAnnotation {
    point: ChartPointId,
    sign: SignPlacement,
    element: Element,
    modality: Modality,
    polarity: Polarity,
    sign_rulers: Vec<CelestialObject>,
    decan_index: u8,
    decan_ruler: CelestialObject,
    dignities: Vec<DignityCondition>,
}

impl WesternPointAnnotation {
    pub const fn point(&self) -> ChartPointId {
        self.point
    }

    pub const fn sign(&self) -> SignPlacement {
        self.sign
    }

    pub fn sign_rulers(&self) -> &[CelestialObject] {
        &self.sign_rulers
    }

    pub const fn decan_index(&self) -> u8 {
        self.decan_index
    }

    pub const fn decan_ruler(&self) -> CelestialObject {
        self.decan_ruler
    }

    pub fn dignities(&self) -> &[DignityCondition] {
        &self.dignities
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WesternChartArtifact {
    chart: DerivedChartArtifact,
    policy: WesternPolicy,
    annotations: Vec<WesternPointAnnotation>,
}

#[derive(Serialize)]
struct ArtifactRef<'a> {
    schema_version: u32,
    chart: &'a DerivedChartArtifact,
    policy: WesternPolicy,
    annotations: &'a [WesternPointAnnotation],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactWire {
    schema_version: u32,
    chart: DerivedChartArtifact,
    policy: WesternPolicy,
    annotations: Vec<WesternPointAnnotation>,
}

#[derive(Debug, Error)]
pub enum WesternArtifactError {
    #[error("invalid Western chart artifact JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported Western chart artifact schema version {0}")]
    UnsupportedSchema(u32),
    #[error("serialized Western annotations do not match the selected policy")]
    AnnotationMismatch,
}

impl WesternChartArtifact {
    pub fn new(chart: DerivedChartArtifact, policy: WesternPolicy) -> Self {
        let annotations = chart
            .placements()
            .iter()
            .map(|placement| annotate(*placement, policy))
            .collect();
        Self {
            chart,
            policy,
            annotations,
        }
    }

    pub fn from_json(input: &str) -> Result<Self, WesternArtifactError> {
        let wire: ArtifactWire = serde_json::from_str(input)?;
        if wire.schema_version != SCHEMA_VERSION {
            return Err(WesternArtifactError::UnsupportedSchema(wire.schema_version));
        }
        let artifact = Self::new(wire.chart, wire.policy);
        if !annotations_match(&artifact.annotations, &wire.annotations) {
            return Err(WesternArtifactError::AnnotationMismatch);
        }
        Ok(artifact)
    }

    pub fn chart(&self) -> &DerivedChartArtifact {
        &self.chart
    }

    pub const fn policy(&self) -> WesternPolicy {
        self.policy
    }

    pub fn annotations(&self) -> &[WesternPointAnnotation] {
        &self.annotations
    }

    pub fn to_json(&self) -> Result<String, WesternArtifactError> {
        Ok(serde_json::to_string(self)?)
    }

    pub fn to_pretty_json(&self) -> Result<String, WesternArtifactError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn content_sha256(&self) -> Result<String, WesternArtifactError> {
        Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(self)?)))
    }

    pub fn content_id(&self) -> Result<String, WesternArtifactError> {
        Ok(format!("sha256:{}", self.content_sha256()?))
    }
}

impl Serialize for WesternChartArtifact {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ArtifactRef {
            schema_version: SCHEMA_VERSION,
            chart: &self.chart,
            policy: self.policy,
            annotations: &self.annotations,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for WesternChartArtifact {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = ArtifactWire::deserialize(deserializer)?;
        if wire.schema_version != SCHEMA_VERSION {
            return Err(serde::de::Error::custom(format!(
                "unsupported Western chart artifact schema version {}",
                wire.schema_version
            )));
        }
        let artifact = Self::new(wire.chart, wire.policy);
        if !annotations_match(&artifact.annotations, &wire.annotations) {
            return Err(serde::de::Error::custom(
                "serialized Western annotations do not match the selected policy",
            ));
        }
        Ok(artifact)
    }
}

fn annotations_match(first: &[WesternPointAnnotation], second: &[WesternPointAnnotation]) -> bool {
    first.len() == second.len()
        && first.iter().zip(second).all(|(first, second)| {
            first.point == second.point
                && first.sign.sign() == second.sign.sign()
                && (first.sign.degrees_within_sign() - second.sign.degrees_within_sign()).abs()
                    <= 1e-12
                && first.element == second.element
                && first.modality == second.modality
                && first.polarity == second.polarity
                && first.sign_rulers == second.sign_rulers
                && first.decan_index == second.decan_index
                && first.decan_ruler == second.decan_ruler
                && dignities_match(&first.dignities, &second.dignities)
        })
}

fn dignities_match(first: &[DignityCondition], second: &[DignityCondition]) -> bool {
    first.len() == second.len()
        && first.iter().zip(second).all(|(first, second)| {
            first.kind == second.kind
                && optional_float_matches(
                    first.exact_longitude_degrees,
                    second.exact_longitude_degrees,
                )
                && optional_float_matches(
                    first.distance_from_exact_degrees,
                    second.distance_from_exact_degrees,
                )
        })
}

fn optional_float_matches(first: Option<f64>, second: Option<f64>) -> bool {
    match (first, second) {
        (None, None) => true,
        (Some(first), Some(second)) => (first - second).abs() <= 1e-12,
        _ => false,
    }
}

pub fn sign_rulers(sign: ZodiacSign, policy: RulershipPolicy) -> Vec<CelestialObject> {
    let traditional = match sign {
        ZodiacSign::Aries | ZodiacSign::Scorpio => CelestialObject::Mars,
        ZodiacSign::Taurus | ZodiacSign::Libra => CelestialObject::Venus,
        ZodiacSign::Gemini | ZodiacSign::Virgo => CelestialObject::Mercury,
        ZodiacSign::Cancer => CelestialObject::Moon,
        ZodiacSign::Leo => CelestialObject::Sun,
        ZodiacSign::Sagittarius | ZodiacSign::Pisces => CelestialObject::Jupiter,
        ZodiacSign::Capricorn | ZodiacSign::Aquarius => CelestialObject::Saturn,
    };
    let mut rulers = vec![traditional];
    if policy == RulershipPolicy::ModernV1 {
        match sign {
            ZodiacSign::Aquarius => rulers.push(CelestialObject::Uranus),
            ZodiacSign::Pisces => rulers.push(CelestialObject::Neptune),
            ZodiacSign::Scorpio => rulers.push(CelestialObject::Pluto),
            _ => {}
        }
    }
    rulers
}

pub fn decan_ruler(
    sign: ZodiacSign,
    degrees_within_sign: f64,
    policy: DecanPolicy,
) -> Option<(u8, CelestialObject)> {
    if !degrees_within_sign.is_finite() || !(0.0..30.0).contains(&degrees_within_sign) {
        return None;
    }
    let decan = (degrees_within_sign / 10.0).floor() as usize;
    let ruler = match policy {
        DecanPolicy::ChaldeanFacesV1 => {
            const ORDER: [CelestialObject; 7] = [
                CelestialObject::Mars,
                CelestialObject::Sun,
                CelestialObject::Venus,
                CelestialObject::Mercury,
                CelestialObject::Moon,
                CelestialObject::Saturn,
                CelestialObject::Jupiter,
            ];
            ORDER[(sign.index() * 3 + decan) % ORDER.len()]
        }
        DecanPolicy::TriplicityDecansV1 => triplicity_rulers(sign)[decan],
    };
    Some((decan as u8 + 1, ruler))
}

pub fn dignities(
    point: ChartPointId,
    sign: SignPlacement,
    policy: RulershipPolicy,
) -> Vec<DignityCondition> {
    let Some(object) = point.celestial_object() else {
        return Vec::new();
    };
    let mut conditions = Vec::new();
    if sign_rulers(sign.sign(), policy).contains(&object) {
        conditions.push(condition(DignityKind::Domicile, None, sign));
    }
    let opposite = ZodiacSign::ALL[(sign.sign().index() + 6) % 12];
    if sign_rulers(opposite, policy).contains(&object) {
        conditions.push(condition(DignityKind::Detriment, None, sign));
    }
    if let Some((exaltation_sign, exact)) = exaltation(object) {
        if sign.sign() == exaltation_sign {
            conditions.push(condition(DignityKind::Exaltation, Some(exact), sign));
        }
        let fall_sign = ZodiacSign::ALL[(exaltation_sign.index() + 6) % 12];
        if sign.sign() == fall_sign {
            conditions.push(condition(
                DignityKind::Fall,
                Some((exact + 180.0).rem_euclid(360.0)),
                sign,
            ));
        }
    }
    conditions
}

fn annotate(placement: PointPlacement, policy: WesternPolicy) -> WesternPointAnnotation {
    let sign = placement.sign();
    let (decan_index, decan_ruler) =
        decan_ruler(sign.sign(), sign.degrees_within_sign(), policy.decans)
            .expect("validated sign placement always has a valid decan");
    WesternPointAnnotation {
        point: placement.point(),
        sign,
        element: element(sign.sign()),
        modality: modality(sign.sign()),
        polarity: polarity(sign.sign()),
        sign_rulers: sign_rulers(sign.sign(), policy.rulership),
        decan_index,
        decan_ruler,
        dignities: dignities(placement.point(), sign, policy.rulership),
    }
}

fn condition(
    kind: DignityKind,
    exact_longitude_degrees: Option<f64>,
    placement: SignPlacement,
) -> DignityCondition {
    DignityCondition {
        kind,
        exact_longitude_degrees,
        distance_from_exact_degrees: exact_longitude_degrees.map(|exact| {
            let difference = (placement.longitude_degrees() - exact).rem_euclid(360.0);
            if difference > 180.0 {
                difference - 360.0
            } else {
                difference
            }
        }),
    }
}

fn exaltation(object: CelestialObject) -> Option<(ZodiacSign, f64)> {
    match object {
        CelestialObject::Sun => Some((ZodiacSign::Aries, 19.0)),
        CelestialObject::Moon => Some((ZodiacSign::Taurus, 33.0)),
        CelestialObject::Mercury => Some((ZodiacSign::Virgo, 165.0)),
        CelestialObject::Venus => Some((ZodiacSign::Pisces, 357.0)),
        CelestialObject::Mars => Some((ZodiacSign::Capricorn, 298.0)),
        CelestialObject::Jupiter => Some((ZodiacSign::Cancer, 105.0)),
        CelestialObject::Saturn => Some((ZodiacSign::Libra, 201.0)),
        _ => None,
    }
}

fn triplicity_rulers(sign: ZodiacSign) -> [CelestialObject; 3] {
    use CelestialObject::{Jupiter, Mars, Mercury, Moon, Saturn, Sun, Venus};
    match sign {
        ZodiacSign::Aries => [Mars, Sun, Jupiter],
        ZodiacSign::Leo => [Sun, Jupiter, Mars],
        ZodiacSign::Sagittarius => [Jupiter, Mars, Sun],
        ZodiacSign::Taurus => [Venus, Mercury, Saturn],
        ZodiacSign::Virgo => [Mercury, Saturn, Venus],
        ZodiacSign::Capricorn => [Saturn, Venus, Mercury],
        ZodiacSign::Gemini => [Mercury, Venus, Saturn],
        ZodiacSign::Libra => [Venus, Saturn, Mercury],
        ZodiacSign::Aquarius => [Saturn, Mercury, Venus],
        ZodiacSign::Cancer => [Moon, Mars, Jupiter],
        ZodiacSign::Scorpio => [Mars, Jupiter, Moon],
        ZodiacSign::Pisces => [Jupiter, Moon, Mars],
    }
}

fn element(sign: ZodiacSign) -> Element {
    match sign.index() % 4 {
        0 => Element::Fire,
        1 => Element::Earth,
        2 => Element::Air,
        _ => Element::Water,
    }
}

fn modality(sign: ZodiacSign) -> Modality {
    match sign.index() % 3 {
        0 => Modality::Cardinal,
        1 => Modality::Fixed,
        _ => Modality::Mutable,
    }
}

fn polarity(sign: ZodiacSign) -> Polarity {
    if sign.index().is_multiple_of(2) {
        Polarity::Positive
    } else {
        Polarity::Negative
    }
}
