//! Browser-compatible Moshier ephemeris calculations for Astraeus.
//!
//! This adapter uses the pure-Rust `swisseph-rs` Moshier backend with all
//! file-backed features disabled. It performs no I/O and never falls back to a
//! different ephemeris source.

use std::collections::BTreeMap;

use astraeus_core::{
    AngularPosition, Ayanamsa, CalculationError, CalculationProvenance, CalculationRequest,
    CalculationResult, CelestialObject, ChartAngles, EphemerisAdapter, EphemerisSource, HouseCusps,
    HouseSystem, Position, Zodiac,
};
use chrono::{Datelike, Timelike};
use swisseph::{
    Body, CalcFlags, Ephemeris, EphemerisConfig, SiderealMode,
    constants::{MOSHLUEPH_END, MOSHLUEPH_START, MOSHPLEPH_END, MOSHPLEPH_START},
    types::{CalendarType, EphemerisSource as NativeSource, HouseSystem as NativeHouseSystem},
};

const PROVIDER_NAME: &str = "swisseph-rs Moshier";
const PROVIDER_VERSION: &str = "0.2.0";
const ANGLE_SPEED_SAMPLE_DAYS: f64 = 30.0 / 86_400.0;

/// Pure-Rust, file-free Moshier provider.
#[derive(Clone, Copy, Debug, Default)]
pub struct MoshierEphemerisAdapter;

impl MoshierEphemerisAdapter {
    /// Construct a pure Moshier adapter.
    pub const fn new() -> Self {
        Self
    }

    fn calculate_complete(
        &self,
        request: &CalculationRequest,
    ) -> Result<CalculationResult, CalculationError> {
        if request.objects().contains(&CelestialObject::Chiron) {
            return Err(CalculationError::UnsupportedObject(CelestialObject::Chiron));
        }

        let jd = julian_day(request);
        if !(supported_start()..=supported_end()).contains(&jd) {
            return Err(CalculationError::DataUnavailable(format!(
                "Moshier supports Julian days {} through {}, got {jd}",
                supported_start(),
                supported_end()
            )));
        }

        let ephemeris = Ephemeris::new(configuration(request)?)
            .map_err(|error| CalculationError::Provider(error.to_string()))?;
        let flags = calculation_flags(request);
        let mut positions = BTreeMap::new();
        for object in request.objects() {
            let result = ephemeris
                .calc_ut(jd, body(*object), flags)
                .map_err(|error| CalculationError::ObjectCalculation {
                    object: *object,
                    message: error.to_string(),
                })?;
            if !result.flags_used.contains(CalcFlags::MOSEPH) {
                return Err(CalculationError::DataUnavailable(format!(
                    "Moshier was requested for {object:?}, but another source was reported"
                )));
            }
            positions.insert(
                *object,
                Position::new(
                    result.data[0],
                    result.data[1],
                    result.data[2],
                    result.data[3],
                )?,
            );
        }

        let houses = houses(&ephemeris, jd, request)?;
        let provenance = CalculationProvenance::new(
            PROVIDER_NAME,
            PROVIDER_VERSION,
            EphemerisSource::Moshier,
            None,
        )?;
        CalculationResult::new(request, positions, houses, provenance)
    }
}

impl EphemerisAdapter for MoshierEphemerisAdapter {
    fn calculate(
        &self,
        request: &CalculationRequest,
    ) -> Result<CalculationResult, CalculationError> {
        self.calculate_complete(request)
    }
}

fn supported_start() -> f64 {
    MOSHPLEPH_START.max(MOSHLUEPH_START)
}

fn supported_end() -> f64 {
    MOSHPLEPH_END.min(MOSHLUEPH_END)
}

fn configuration(request: &CalculationRequest) -> Result<EphemerisConfig, CalculationError> {
    let mut configuration = EphemerisConfig {
        ephemeris_source: NativeSource::Moshier,
        ..EphemerisConfig::default()
    };
    if request.zodiac() == Zodiac::Sidereal {
        let ayanamsa = request.ayanamsa().ok_or_else(|| {
            CalculationError::Provider("validated sidereal request omitted ayanamsa".into())
        })?;
        configuration.set_sidereal_mode(sidereal_mode(ayanamsa) as i32, 0.0, 0.0);
    }
    Ok(configuration)
}

fn calculation_flags(request: &CalculationRequest) -> CalcFlags {
    let mut flags = CalcFlags::MOSEPH | CalcFlags::SPEED;
    if request.zodiac() == Zodiac::Sidereal {
        flags |= CalcFlags::SIDEREAL;
    }
    flags
}

fn houses(
    ephemeris: &Ephemeris,
    jd: f64,
    request: &CalculationRequest,
) -> Result<HouseCusps, CalculationError> {
    let flags = if request.zodiac() == Zodiac::Sidereal {
        CalcFlags::SIDEREAL
    } else {
        CalcFlags::empty()
    };
    let calculate = |sample_jd| {
        ephemeris.houses_ex2(
            sample_jd,
            flags,
            request.location().latitude_degrees(),
            request.location().longitude_degrees(),
            house_system(request.house_system()),
        )
    };
    let current = calculate(jd).map_err(|error| {
        CalculationError::Provider(format!(
            "{:?} houses could not be calculated at latitude {}: {error}",
            request.house_system(),
            request.location().latitude_degrees()
        ))
    })?;
    let previous = calculate(jd - ANGLE_SPEED_SAMPLE_DAYS)
        .map_err(|error| CalculationError::Provider(error.to_string()))?;
    let next = calculate(jd + ANGLE_SPEED_SAMPLE_DAYS)
        .map_err(|error| CalculationError::Provider(error.to_string()))?;
    let angle = |value: f64, before: f64, after: f64| {
        AngularPosition::new(
            value,
            signed_angular_difference(before, after) / (2.0 * ANGLE_SPEED_SAMPLE_DAYS),
        )
    };
    let chart_angles = ChartAngles::new(
        angle(
            current.ascmc.ascendant,
            previous.ascmc.ascendant,
            next.ascmc.ascendant,
        )?,
        angle(current.ascmc.mc, previous.ascmc.mc, next.ascmc.mc)?,
        angle(
            current.ascmc.vertex,
            previous.ascmc.vertex,
            next.ascmc.vertex,
        )?,
    )?;
    HouseCusps::new(current.cusps[1..=12].to_vec(), chart_angles).map_err(Into::into)
}

fn signed_angular_difference(first: f64, second: f64) -> f64 {
    let difference = (second - first).rem_euclid(360.0);
    if difference > 180.0 {
        difference - 360.0
    } else {
        difference
    }
}

fn julian_day(request: &CalculationRequest) -> f64 {
    let instant = request.instant().as_datetime();
    let hour = f64::from(instant.hour())
        + f64::from(instant.minute()) / 60.0
        + (f64::from(instant.second()) + f64::from(instant.nanosecond()) / 1e9) / 3600.0;
    swisseph::date::julday(
        instant.year(),
        instant.month() as i32,
        instant.day() as i32,
        hour,
        CalendarType::Gregorian,
    )
}

fn body(object: CelestialObject) -> Body {
    match object {
        CelestialObject::Sun => Body::Sun,
        CelestialObject::Moon => Body::Moon,
        CelestialObject::Mercury => Body::Mercury,
        CelestialObject::Venus => Body::Venus,
        CelestialObject::Mars => Body::Mars,
        CelestialObject::Jupiter => Body::Jupiter,
        CelestialObject::Saturn => Body::Saturn,
        CelestialObject::Uranus => Body::Uranus,
        CelestialObject::Neptune => Body::Neptune,
        CelestialObject::Pluto => Body::Pluto,
        CelestialObject::MeanNode => Body::MeanNode,
        CelestialObject::TrueNode => Body::TrueNode,
        CelestialObject::Chiron => Body::Chiron,
    }
}

fn house_system(system: HouseSystem) -> NativeHouseSystem {
    match system {
        HouseSystem::Placidus => NativeHouseSystem::Placidus,
        HouseSystem::Koch => NativeHouseSystem::Koch,
        HouseSystem::Porphyry => NativeHouseSystem::Porphyry,
        HouseSystem::Regiomontanus => NativeHouseSystem::Regiomontanus,
        HouseSystem::Campanus => NativeHouseSystem::Campanus,
        HouseSystem::Equal => NativeHouseSystem::Equal,
        HouseSystem::WholeSign => NativeHouseSystem::WholeSign,
    }
}

fn sidereal_mode(ayanamsa: Ayanamsa) -> SiderealMode {
    match ayanamsa {
        Ayanamsa::FaganBradley => SiderealMode::FaganBradley,
        Ayanamsa::Lahiri => SiderealMode::Lahiri,
        Ayanamsa::DeLuce => SiderealMode::DeLuce,
        Ayanamsa::Raman => SiderealMode::Raman,
        Ayanamsa::Krishnamurti => SiderealMode::Krishnamurti,
        Ayanamsa::Yukteshwar => SiderealMode::Yukteshwar,
        Ayanamsa::JnBhasin => SiderealMode::JnBhasin,
    }
}
