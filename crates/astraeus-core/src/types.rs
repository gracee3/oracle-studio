use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};

use crate::ValidationError;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CelestialObject {
    Sun,
    Moon,
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

/// Every physical or derived point supported by chart derivation and aspects.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChartPointId {
    Sun,
    Moon,
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

impl From<CelestialObject> for ChartPointId {
    fn from(value: CelestialObject) -> Self {
        match value {
            CelestialObject::Sun => Self::Sun,
            CelestialObject::Moon => Self::Moon,
            CelestialObject::Mercury => Self::Mercury,
            CelestialObject::Venus => Self::Venus,
            CelestialObject::Mars => Self::Mars,
            CelestialObject::Jupiter => Self::Jupiter,
            CelestialObject::Saturn => Self::Saturn,
            CelestialObject::Uranus => Self::Uranus,
            CelestialObject::Neptune => Self::Neptune,
            CelestialObject::Pluto => Self::Pluto,
            CelestialObject::MeanNode => Self::MeanNode,
            CelestialObject::TrueNode => Self::TrueNode,
            CelestialObject::Chiron => Self::Chiron,
        }
    }
}

impl ChartPointId {
    pub const fn celestial_object(self) -> Option<CelestialObject> {
        match self {
            Self::Sun => Some(CelestialObject::Sun),
            Self::Moon => Some(CelestialObject::Moon),
            Self::Mercury => Some(CelestialObject::Mercury),
            Self::Venus => Some(CelestialObject::Venus),
            Self::Mars => Some(CelestialObject::Mars),
            Self::Jupiter => Some(CelestialObject::Jupiter),
            Self::Saturn => Some(CelestialObject::Saturn),
            Self::Uranus => Some(CelestialObject::Uranus),
            Self::Neptune => Some(CelestialObject::Neptune),
            Self::Pluto => Some(CelestialObject::Pluto),
            Self::MeanNode => Some(CelestialObject::MeanNode),
            Self::TrueNode => Some(CelestialObject::TrueNode),
            Self::Chiron => Some(CelestialObject::Chiron),
            Self::MeanSouthNode
            | Self::TrueSouthNode
            | Self::Ascendant
            | Self::Midheaven
            | Self::Descendant
            | Self::ImumCoeli
            | Self::Vertex => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChartAngle {
    Ascendant,
    Midheaven,
    Descendant,
    ImumCoeli,
    Vertex,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Zodiac {
    Tropical,
    Sidereal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ayanamsa {
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
pub enum HouseSystem {
    Placidus,
    Koch,
    Porphyry,
    Regiomontanus,
    Campanus,
    Equal,
    WholeSign,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EphemerisSource {
    Synthetic,
    Moshier,
    SwissFiles,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CalculationProvenance {
    provider: String,
    provider_version: String,
    ephemeris_source: EphemerisSource,
    data_revision: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CalculationProvenanceWire {
    provider: String,
    provider_version: String,
    ephemeris_source: EphemerisSource,
    data_revision: Option<String>,
}

impl<'de> Deserialize<'de> for CalculationProvenance {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = CalculationProvenanceWire::deserialize(deserializer)?;
        Self::new(
            wire.provider,
            wire.provider_version,
            wire.ephemeris_source,
            wire.data_revision,
        )
        .map_err(serde::de::Error::custom)
    }
}

impl CalculationProvenance {
    pub fn new(
        provider: impl Into<String>,
        provider_version: impl Into<String>,
        ephemeris_source: EphemerisSource,
        data_revision: Option<String>,
    ) -> Result<Self, ValidationError> {
        let provider = provider.into();
        let provider_version = provider_version.into();
        validate_text("provider", &provider)?;
        validate_text("provider_version", &provider_version)?;
        if let Some(revision) = &data_revision {
            validate_text("data_revision", revision)?;
        }
        Ok(Self {
            provider,
            provider_version,
            ephemeris_source,
            data_revision,
        })
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }
    pub fn provider_version(&self) -> &str {
        &self.provider_version
    }
    pub fn ephemeris_source(&self) -> EphemerisSource {
        self.ephemeris_source
    }
    pub fn data_revision(&self) -> Option<&str> {
        self.data_revision.as_deref()
    }
}

/// A timestamp normalized to UTC on construction.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct UtcInstant(DateTime<Utc>);

impl UtcInstant {
    pub fn from_datetime(value: DateTime<Utc>) -> Self {
        Self(value)
    }

    pub fn parse_rfc3339(value: &str) -> Result<Self, ValidationError> {
        DateTime::parse_from_rfc3339(value)
            .map(|instant| Self(instant.with_timezone(&Utc)))
            .map_err(|_| ValidationError::InvalidUtcInstant(value.to_owned()))
    }

    pub fn as_datetime(&self) -> DateTime<Utc> {
        self.0
    }
}

impl<'de> Deserialize<'de> for UtcInstant {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse_rfc3339(&value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct GeographicLocation {
    latitude_degrees: f64,
    longitude_degrees: f64,
    elevation_meters: f64,
}

impl GeographicLocation {
    pub fn new(
        latitude_degrees: f64,
        longitude_degrees: f64,
        elevation_meters: f64,
    ) -> Result<Self, ValidationError> {
        validate_range("latitude_degrees", latitude_degrees, -90, 90)?;
        validate_range("longitude_degrees", longitude_degrees, -180, 180)?;
        validate_range("elevation_meters", elevation_meters, -500, 10_000)?;
        Ok(Self {
            latitude_degrees,
            longitude_degrees,
            elevation_meters,
        })
    }

    pub fn latitude_degrees(&self) -> f64 {
        self.latitude_degrees
    }
    pub fn longitude_degrees(&self) -> f64 {
        self.longitude_degrees
    }
    pub fn elevation_meters(&self) -> f64 {
        self.elevation_meters
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GeographicLocationWire {
    latitude_degrees: f64,
    longitude_degrees: f64,
    elevation_meters: f64,
}

impl<'de> Deserialize<'de> for GeographicLocation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = GeographicLocationWire::deserialize(deserializer)?;
        Self::new(
            wire.latitude_degrees,
            wire.longitude_degrees,
            wire.elevation_meters,
        )
        .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Position {
    longitude_degrees: f64,
    latitude_degrees: f64,
    distance_au: f64,
    longitude_speed_degrees_per_day: f64,
}

/// Longitude and instantaneous motion for an angle or another longitude-only point.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct AngularPosition {
    longitude_degrees: f64,
    longitude_speed_degrees_per_day: f64,
}

impl AngularPosition {
    pub fn new(
        longitude_degrees: f64,
        longitude_speed_degrees_per_day: f64,
    ) -> Result<Self, ValidationError> {
        validate_longitude(longitude_degrees)?;
        validate_finite(
            "longitude_speed_degrees_per_day",
            longitude_speed_degrees_per_day,
        )?;
        Ok(Self {
            longitude_degrees,
            longitude_speed_degrees_per_day,
        })
    }

    pub fn longitude_degrees(self) -> f64 {
        self.longitude_degrees
    }

    pub fn longitude_speed_degrees_per_day(self) -> f64 {
        self.longitude_speed_degrees_per_day
    }

    pub fn opposite(self) -> Result<Self, ValidationError> {
        Self::new(
            (self.longitude_degrees + 180.0).rem_euclid(360.0),
            self.longitude_speed_degrees_per_day,
        )
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AngularPositionWire {
    longitude_degrees: f64,
    longitude_speed_degrees_per_day: f64,
}

impl<'de> Deserialize<'de> for AngularPosition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = AngularPositionWire::deserialize(deserializer)?;
        Self::new(wire.longitude_degrees, wire.longitude_speed_degrees_per_day)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ChartAngles {
    ascendant: AngularPosition,
    midheaven: AngularPosition,
    descendant: AngularPosition,
    imum_coeli: AngularPosition,
    vertex: AngularPosition,
}

impl ChartAngles {
    pub fn new(
        ascendant: AngularPosition,
        midheaven: AngularPosition,
        vertex: AngularPosition,
    ) -> Result<Self, ValidationError> {
        Ok(Self {
            ascendant,
            midheaven,
            descendant: ascendant.opposite()?,
            imum_coeli: midheaven.opposite()?,
            vertex,
        })
    }

    pub fn get(&self, angle: ChartAngle) -> AngularPosition {
        match angle {
            ChartAngle::Ascendant => self.ascendant,
            ChartAngle::Midheaven => self.midheaven,
            ChartAngle::Descendant => self.descendant,
            ChartAngle::ImumCoeli => self.imum_coeli,
            ChartAngle::Vertex => self.vertex,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChartAnglesWire {
    ascendant: AngularPosition,
    midheaven: AngularPosition,
    descendant: AngularPosition,
    imum_coeli: AngularPosition,
    vertex: AngularPosition,
}

impl<'de> Deserialize<'de> for ChartAngles {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ChartAnglesWire::deserialize(deserializer)?;
        let angles = Self::new(wire.ascendant, wire.midheaven, wire.vertex)
            .map_err(serde::de::Error::custom)?;
        if !angular_positions_match(angles.descendant, wire.descendant)
            || !angular_positions_match(angles.imum_coeli, wire.imum_coeli)
        {
            return Err(serde::de::Error::custom(
                "descendant/imum_coeli do not oppose ascendant/midheaven",
            ));
        }
        Ok(angles)
    }
}

fn angular_positions_match(first: AngularPosition, second: AngularPosition) -> bool {
    let longitude_difference = (first.longitude_degrees() - second.longitude_degrees())
        .abs()
        .min(360.0 - (first.longitude_degrees() - second.longitude_degrees()).abs());
    longitude_difference <= 1e-9
        && (first.longitude_speed_degrees_per_day() - second.longitude_speed_degrees_per_day())
            .abs()
            <= 1e-9
}

impl Position {
    pub fn new(
        longitude_degrees: f64,
        latitude_degrees: f64,
        distance_au: f64,
        longitude_speed_degrees_per_day: f64,
    ) -> Result<Self, ValidationError> {
        validate_range("longitude_degrees", longitude_degrees, 0, 360)?;
        if longitude_degrees == 360.0 {
            return Err(out_of_range("longitude_degrees", 0, 359, longitude_degrees));
        }
        validate_range("latitude_degrees", latitude_degrees, -90, 90)?;
        validate_range("distance_au", distance_au, 0, i32::MAX)?;
        validate_finite(
            "longitude_speed_degrees_per_day",
            longitude_speed_degrees_per_day,
        )?;
        Ok(Self {
            longitude_degrees,
            latitude_degrees,
            distance_au,
            longitude_speed_degrees_per_day,
        })
    }

    pub fn longitude_degrees(&self) -> f64 {
        self.longitude_degrees
    }
    pub fn latitude_degrees(&self) -> f64 {
        self.latitude_degrees
    }
    pub fn distance_au(&self) -> f64 {
        self.distance_au
    }
    pub fn longitude_speed_degrees_per_day(&self) -> f64 {
        self.longitude_speed_degrees_per_day
    }
    pub fn is_retrograde(&self) -> bool {
        self.longitude_speed_degrees_per_day < 0.0
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PositionWire {
    longitude_degrees: f64,
    latitude_degrees: f64,
    distance_au: f64,
    longitude_speed_degrees_per_day: f64,
}

impl<'de> Deserialize<'de> for Position {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = PositionWire::deserialize(deserializer)?;
        Self::new(
            wire.longitude_degrees,
            wire.latitude_degrees,
            wire.distance_au,
            wire.longitude_speed_degrees_per_day,
        )
        .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct HouseCusps {
    cusps_degrees: [f64; 12],
    angles: ChartAngles,
}

impl HouseCusps {
    pub fn new(cusps_degrees: Vec<f64>, angles: ChartAngles) -> Result<Self, ValidationError> {
        let count = cusps_degrees.len();
        let cusps_degrees: [f64; 12] = cusps_degrees
            .try_into()
            .map_err(|_| ValidationError::InvalidHouseCount(count))?;
        for value in cusps_degrees {
            validate_longitude(value)?;
        }
        let total_arc: f64 = (0..12)
            .map(|index| {
                let next = cusps_degrees[(index + 1) % 12];
                (next - cusps_degrees[index]).rem_euclid(360.0)
            })
            .sum();
        if cusps_degrees.windows(2).any(|pair| pair[0] == pair[1])
            || cusps_degrees[11] == cusps_degrees[0]
            || (total_arc - 360.0).abs() > 1e-8
        {
            return Err(ValidationError::InvalidHouseTopology);
        }
        Ok(Self {
            cusps_degrees,
            angles,
        })
    }

    pub fn cusps_degrees(&self) -> &[f64; 12] {
        &self.cusps_degrees
    }
    pub fn ascendant_degrees(&self) -> f64 {
        self.angles.get(ChartAngle::Ascendant).longitude_degrees()
    }
    pub fn midheaven_degrees(&self) -> f64 {
        self.angles.get(ChartAngle::Midheaven).longitude_degrees()
    }
    pub fn angles(&self) -> &ChartAngles {
        &self.angles
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HouseCuspsWire {
    cusps_degrees: Vec<f64>,
    angles: ChartAngles,
}

impl<'de> Deserialize<'de> for HouseCusps {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = HouseCuspsWire::deserialize(deserializer)?;
        Self::new(wire.cusps_degrees, wire.angles).map_err(serde::de::Error::custom)
    }
}

/// Reusable validated choices that determine a calculation's domain behavior.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CalculationOptions {
    objects: Vec<CelestialObject>,
    zodiac: Zodiac,
    ayanamsa: Option<Ayanamsa>,
    house_system: HouseSystem,
}

impl CalculationOptions {
    pub fn new(
        objects: Vec<CelestialObject>,
        zodiac: Zodiac,
        ayanamsa: Option<Ayanamsa>,
        house_system: HouseSystem,
    ) -> Result<Self, ValidationError> {
        if objects.is_empty() {
            return Err(ValidationError::EmptyObjectSet);
        }
        let mut seen = BTreeSet::new();
        for object in &objects {
            if !seen.insert(*object) {
                return Err(ValidationError::DuplicateObject(*object));
            }
        }
        match (zodiac, ayanamsa) {
            (Zodiac::Sidereal, None) => return Err(ValidationError::MissingAyanamsa),
            (Zodiac::Tropical, Some(_)) => return Err(ValidationError::UnexpectedAyanamsa),
            _ => {}
        }
        Ok(Self {
            objects,
            zodiac,
            ayanamsa,
            house_system,
        })
    }

    pub fn objects(&self) -> &[CelestialObject] {
        &self.objects
    }
    pub fn zodiac(&self) -> Zodiac {
        self.zodiac
    }
    pub fn ayanamsa(&self) -> Option<Ayanamsa> {
        self.ayanamsa
    }
    pub fn house_system(&self) -> HouseSystem {
        self.house_system
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CalculationOptionsWire {
    objects: Vec<CelestialObject>,
    zodiac: Zodiac,
    ayanamsa: Option<Ayanamsa>,
    house_system: HouseSystem,
}

impl<'de> Deserialize<'de> for CalculationOptions {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = CalculationOptionsWire::deserialize(deserializer)?;
        Self::new(wire.objects, wire.zodiac, wire.ayanamsa, wire.house_system)
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CalculationRequest {
    instant: UtcInstant,
    location: GeographicLocation,
    objects: Vec<CelestialObject>,
    zodiac: Zodiac,
    ayanamsa: Option<Ayanamsa>,
    house_system: HouseSystem,
}

impl CalculationRequest {
    pub fn new(
        instant: UtcInstant,
        location: GeographicLocation,
        objects: Vec<CelestialObject>,
        zodiac: Zodiac,
        ayanamsa: Option<Ayanamsa>,
        house_system: HouseSystem,
    ) -> Result<Self, ValidationError> {
        Ok(Self::from_options(
            instant,
            location,
            CalculationOptions::new(objects, zodiac, ayanamsa, house_system)?,
        ))
    }

    pub fn from_options(
        instant: UtcInstant,
        location: GeographicLocation,
        options: CalculationOptions,
    ) -> Self {
        Self {
            instant,
            location,
            objects: options.objects,
            zodiac: options.zodiac,
            ayanamsa: options.ayanamsa,
            house_system: options.house_system,
        }
    }

    pub fn instant(&self) -> UtcInstant {
        self.instant
    }
    pub fn location(&self) -> GeographicLocation {
        self.location
    }
    pub fn objects(&self) -> &[CelestialObject] {
        &self.objects
    }
    pub fn zodiac(&self) -> Zodiac {
        self.zodiac
    }
    pub fn ayanamsa(&self) -> Option<Ayanamsa> {
        self.ayanamsa
    }
    pub fn house_system(&self) -> HouseSystem {
        self.house_system
    }
    pub fn options(&self) -> CalculationOptions {
        CalculationOptions {
            objects: self.objects.clone(),
            zodiac: self.zodiac,
            ayanamsa: self.ayanamsa,
            house_system: self.house_system,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CalculationRequestWire {
    instant: UtcInstant,
    location: GeographicLocation,
    objects: Vec<CelestialObject>,
    zodiac: Zodiac,
    ayanamsa: Option<Ayanamsa>,
    house_system: HouseSystem,
}

impl<'de> Deserialize<'de> for CalculationRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = CalculationRequestWire::deserialize(deserializer)?;
        Self::new(
            wire.instant,
            wire.location,
            wire.objects,
            wire.zodiac,
            wire.ayanamsa,
            wire.house_system,
        )
        .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CalculationResult {
    positions: std::collections::BTreeMap<CelestialObject, Position>,
    houses: HouseCusps,
    provenance: CalculationProvenance,
}

impl CalculationResult {
    pub fn new(
        request: &CalculationRequest,
        positions: std::collections::BTreeMap<CelestialObject, Position>,
        houses: HouseCusps,
        provenance: CalculationProvenance,
    ) -> Result<Self, crate::CalculationError> {
        for object in request.objects() {
            if !positions.contains_key(object) {
                return Err(crate::CalculationError::MissingObject(*object));
            }
        }
        for object in positions.keys() {
            if !request.objects().contains(object) {
                return Err(crate::CalculationError::UnexpectedObject(*object));
            }
        }
        Ok(Self {
            positions,
            houses,
            provenance,
        })
    }

    pub fn positions(&self) -> &std::collections::BTreeMap<CelestialObject, Position> {
        &self.positions
    }

    pub fn houses(&self) -> &HouseCusps {
        &self.houses
    }

    pub fn provenance(&self) -> &CalculationProvenance {
        &self.provenance
    }
}

fn validate_longitude(value: f64) -> Result<(), ValidationError> {
    validate_range("longitude_degrees", value, 0, 360)?;
    if value == 360.0 {
        return Err(out_of_range("longitude_degrees", 0, 359, value));
    }
    Ok(())
}

fn validate_finite(field: &'static str, value: f64) -> Result<(), ValidationError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(ValidationError::NonFinite { field })
    }
}

fn validate_text(field: &'static str, value: &str) -> Result<(), ValidationError> {
    if value.trim().is_empty() {
        Err(ValidationError::EmptyText { field })
    } else {
        Ok(())
    }
}

fn validate_range(
    field: &'static str,
    value: f64,
    minimum: i32,
    maximum: i32,
) -> Result<(), ValidationError> {
    validate_finite(field, value)?;
    if value < f64::from(minimum) || value > f64::from(maximum) {
        Err(out_of_range(field, minimum, maximum, value))
    } else {
        Ok(())
    }
}

fn out_of_range(field: &'static str, minimum: i32, maximum: i32, value: f64) -> ValidationError {
    ValidationError::OutOfRange {
        field,
        minimum,
        maximum,
        value: value.to_string(),
    }
}
