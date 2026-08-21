use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    AngularPosition, CalculationResult, CelestialObject, ChartAngle, ChartPointId, HouseCusps,
    ValidationError,
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZodiacSign {
    Aries,
    Taurus,
    Gemini,
    Cancer,
    Leo,
    Virgo,
    Libra,
    Scorpio,
    Sagittarius,
    Capricorn,
    Aquarius,
    Pisces,
}

impl ZodiacSign {
    pub const ALL: [Self; 12] = [
        Self::Aries,
        Self::Taurus,
        Self::Gemini,
        Self::Cancer,
        Self::Leo,
        Self::Virgo,
        Self::Libra,
        Self::Scorpio,
        Self::Sagittarius,
        Self::Capricorn,
        Self::Aquarius,
        Self::Pisces,
    ];

    pub fn from_longitude(longitude_degrees: f64) -> Result<Self, ValidationError> {
        validate_longitude(longitude_degrees)?;
        Ok(Self::ALL[(longitude_degrees / 30.0).floor() as usize])
    }

    pub fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|candidate| *candidate == self)
            .expect("all zodiac signs are present in ZodiacSign::ALL")
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct HouseNumber(u8);

impl HouseNumber {
    pub fn new(value: u8) -> Result<Self, ValidationError> {
        if (1..=12).contains(&value) {
            Ok(Self(value))
        } else {
            Err(ValidationError::InvalidHouseNumber(value))
        }
    }

    pub const fn get(self) -> u8 {
        self.0
    }
}

impl<'de> Deserialize<'de> for HouseNumber {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(u8::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct SignPlacement {
    sign: ZodiacSign,
    degrees_within_sign: f64,
}

impl SignPlacement {
    pub fn from_longitude(longitude_degrees: f64) -> Result<Self, ValidationError> {
        let sign = ZodiacSign::from_longitude(longitude_degrees)?;
        Ok(Self {
            sign,
            degrees_within_sign: longitude_degrees.rem_euclid(30.0),
        })
    }

    pub const fn sign(self) -> ZodiacSign {
        self.sign
    }

    pub fn degrees_within_sign(self) -> f64 {
        self.degrees_within_sign
    }

    pub fn longitude_degrees(self) -> f64 {
        self.sign.index() as f64 * 30.0 + self.degrees_within_sign
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SignPlacementWire {
    sign: ZodiacSign,
    degrees_within_sign: f64,
}

impl<'de> Deserialize<'de> for SignPlacement {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = SignPlacementWire::deserialize(deserializer)?;
        if !wire.degrees_within_sign.is_finite() || !(0.0..30.0).contains(&wire.degrees_within_sign)
        {
            return Err(serde::de::Error::custom(
                "degrees_within_sign must be finite and in [0, 30)",
            ));
        }
        Ok(Self {
            sign: wire.sign,
            degrees_within_sign: wire.degrees_within_sign,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointPlacement {
    point: ChartPointId,
    sign: SignPlacement,
    house: HouseNumber,
}

/// The explicit ordered point population used for aspect calculation.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ChartPointSelection(Vec<ChartPointId>);

impl ChartPointSelection {
    pub fn new(points: Vec<ChartPointId>) -> Result<Self, ValidationError> {
        let mut seen = BTreeSet::new();
        for point in &points {
            if !seen.insert(*point) {
                return Err(ValidationError::DuplicateChartPoint(*point));
            }
        }
        Ok(Self(points))
    }

    pub fn standard(objects: &[CelestialObject]) -> Self {
        let mut points: Vec<_> = objects.iter().copied().map(ChartPointId::from).collect();
        if objects.contains(&CelestialObject::MeanNode) {
            points.push(ChartPointId::MeanSouthNode);
        }
        if objects.contains(&CelestialObject::TrueNode) {
            points.push(ChartPointId::TrueSouthNode);
        }
        points.extend([
            ChartPointId::Ascendant,
            ChartPointId::Midheaven,
            ChartPointId::Descendant,
            ChartPointId::ImumCoeli,
            ChartPointId::Vertex,
        ]);
        Self(points)
    }

    pub fn as_slice(&self) -> &[ChartPointId] {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ChartPointSelection {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(Vec::<ChartPointId>::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl PointPlacement {
    pub const fn point(self) -> ChartPointId {
        self.point
    }

    pub const fn sign(self) -> SignPlacement {
        self.sign
    }

    pub const fn house(self) -> HouseNumber {
        self.house
    }
}

pub fn chart_point_positions(
    result: &CalculationResult,
) -> Result<BTreeMap<ChartPointId, AngularPosition>, ValidationError> {
    let mut points = BTreeMap::new();
    for (object, position) in result.positions() {
        points.insert(
            ChartPointId::from(*object),
            AngularPosition::new(
                position.longitude_degrees(),
                position.longitude_speed_degrees_per_day(),
            )?,
        );
    }
    add_south_node(
        &mut points,
        CelestialObject::MeanNode,
        ChartPointId::MeanSouthNode,
    )?;
    add_south_node(
        &mut points,
        CelestialObject::TrueNode,
        ChartPointId::TrueSouthNode,
    )?;
    for (angle, point) in [
        (ChartAngle::Ascendant, ChartPointId::Ascendant),
        (ChartAngle::Midheaven, ChartPointId::Midheaven),
        (ChartAngle::Descendant, ChartPointId::Descendant),
        (ChartAngle::ImumCoeli, ChartPointId::ImumCoeli),
        (ChartAngle::Vertex, ChartPointId::Vertex),
    ] {
        points.insert(point, result.houses().angles().get(angle));
    }
    Ok(points)
}

pub fn calculate_placements(
    result: &CalculationResult,
) -> Result<Vec<PointPlacement>, ValidationError> {
    chart_point_positions(result)?
        .into_iter()
        .map(|(point, position)| {
            Ok(PointPlacement {
                point,
                sign: SignPlacement::from_longitude(position.longitude_degrees())?,
                house: house_for_longitude(position.longitude_degrees(), result.houses())?,
            })
        })
        .collect()
}

pub fn house_for_longitude(
    longitude_degrees: f64,
    houses: &HouseCusps,
) -> Result<HouseNumber, ValidationError> {
    validate_longitude(longitude_degrees)?;
    for (index, start) in houses.cusps_degrees().iter().copied().enumerate() {
        let end = houses.cusps_degrees()[(index + 1) % 12];
        let arc = (end - start).rem_euclid(360.0);
        let offset = (longitude_degrees - start).rem_euclid(360.0);
        if offset < arc {
            return HouseNumber::new(index as u8 + 1);
        }
    }
    Err(ValidationError::InvalidHouseTopology)
}

fn add_south_node(
    points: &mut BTreeMap<ChartPointId, AngularPosition>,
    north: CelestialObject,
    south: ChartPointId,
) -> Result<(), ValidationError> {
    if let Some(position) = points.get(&ChartPointId::from(north)).copied() {
        points.insert(south, position.opposite()?);
    }
    Ok(())
}

fn validate_longitude(value: f64) -> Result<(), ValidationError> {
    if !value.is_finite() {
        return Err(ValidationError::NonFinite {
            field: "longitude_degrees",
        });
    }
    if !(0.0..360.0).contains(&value) {
        return Err(ValidationError::OutOfRange {
            field: "longitude_degrees",
            minimum: 0,
            maximum: 359,
            value: value.to_string(),
        });
    }
    Ok(())
}
