//! Serialized Swiss Ephemeris adapter with explicit source enforcement.

use std::{
    collections::BTreeMap,
    ffi::{CStr, CString},
    fs::File,
    io::Read,
    os::raw::c_char,
    path::{Path, PathBuf},
    sync::Mutex,
};

use astraeus_core::{
    AngularPosition, Ayanamsa, CalculationError, CalculationProvenance, CalculationRequest,
    CalculationResult, CelestialObject, ChartAngles, EphemerisAdapter, EphemerisSource, HouseCusps,
    HouseSystem, Position, Zodiac,
};
use astraeus_events::{
    EclipseClassification, EclipseKind, EclipseSearchDirection, EventCoordinateFrame, EventError,
    EventPositionProvider, EventPositionRequest, EventPositionSample, GlobalEclipseMaximum,
    GlobalEclipseProvider,
};
use chrono::{Datelike, Timelike};
use sha2::{Digest, Sha256};
use sweph_sys as sys;

static SWISS_LOCK: Mutex<()> = Mutex::new(());

pub const PINNED_SWISS_EPHEMERIS_REVISION: &str = "cae9ecd4b201544d85e411aced17660932514d43";
pub const PINNED_SWISS_EPHEMERIS_FILES: [(&str, &str); 3] = [
    (
        "sepl_18.se1",
        "ca1393ceab3a44fbc895887cf789c68819ae6a1cbc9b22225872dbe4ccd99a66",
    ),
    (
        "semo_18.se1",
        "1ca07bd67c24374d77226180c20a4f9996cba013697894810518e7eb582ca4f7",
    ),
    (
        "seas_18.se1",
        "a2cd8fc33807c78ca9a700c91c2e042258b12fc4796519e00781440b5ad8b2e2",
    ),
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EphemerisMode {
    Moshier,
    SwissFiles(PathBuf),
}

#[derive(Clone, Debug)]
pub struct SwissEphemerisAdapter {
    mode: EphemerisMode,
    data_revision: Option<String>,
}

impl SwissEphemerisAdapter {
    pub fn moshier() -> Self {
        Self {
            mode: EphemerisMode::Moshier,
            data_revision: None,
        }
    }

    pub fn swiss_files(path: impl AsRef<Path>) -> Result<Self, CalculationError> {
        let path = path.as_ref();
        if !path.is_dir() {
            return Err(CalculationError::DataUnavailable(format!(
                "Swiss Ephemeris path is not a directory: {}",
                path.display()
            )));
        }
        path_to_c_string(path)?;
        Ok(Self {
            mode: EphemerisMode::SwissFiles(path.to_owned()),
            data_revision: None,
        })
    }

    pub fn swiss_files_with_revision(
        path: impl AsRef<Path>,
        data_revision: impl Into<String>,
    ) -> Result<Self, CalculationError> {
        let mut adapter = Self::swiss_files(path)?;
        let revision = data_revision.into();
        CalculationProvenance::new(
            "Swiss Ephemeris",
            "2.10.03",
            EphemerisSource::SwissFiles,
            Some(revision.clone()),
        )?;
        adapter.data_revision = Some(revision);
        Ok(adapter)
    }

    /// Use only the declared Swiss data bundle after verifying every pinned file.
    pub fn pinned_swiss_files(path: impl AsRef<Path>) -> Result<Self, CalculationError> {
        verify_pinned_swiss_files(path.as_ref())?;
        Self::swiss_files_with_revision(path, PINNED_SWISS_EPHEMERIS_REVISION)
    }

    pub fn mode(&self) -> &EphemerisMode {
        &self.mode
    }

    fn calculate_locked(
        &self,
        request: &CalculationRequest,
    ) -> Result<CalculationResult, CalculationError> {
        let source_flag = match &self.mode {
            EphemerisMode::Moshier => sys::SEFLG_MOSEPH,
            EphemerisMode::SwissFiles(path) => {
                let path = path_to_c_string(path)?;
                // SAFETY: the CString remains alive for the call; the global adapter lock is held.
                unsafe { sys::swe_set_ephe_path(path.as_ptr()) };
                sys::SEFLG_SWIEPH
            }
        };
        let sidereal_flag = if request.zodiac() == Zodiac::Sidereal {
            let mode = ayanamsa_code(request.ayanamsa().expect("validated sidereal request"));
            // SAFETY: numeric mappings come from the vendored official header; lock is held.
            unsafe { sys::swe_set_sid_mode(mode, 0.0, 0.0) };
            sys::SEFLG_SIDEREAL
        } else {
            0
        };
        let flags = source_flag | sidereal_flag | sys::SEFLG_SPEED;
        let instant = request.instant().as_datetime();
        let hour = f64::from(instant.hour())
            + f64::from(instant.minute()) / 60.0
            + (f64::from(instant.second()) + f64::from(instant.nanosecond()) / 1e9) / 3600.0;
        // SAFETY: scalar inputs are validated and the global adapter lock is held.
        let jd = unsafe {
            sys::swe_julday(
                instant.year(),
                instant.month() as i32,
                instant.day() as i32,
                hour,
                sys::SE_GREG_CAL,
            )
        };

        let mut positions = BTreeMap::new();
        for object in request.objects() {
            if self.mode == EphemerisMode::Moshier && *object == CelestialObject::Chiron {
                return Err(CalculationError::UnsupportedObject(*object));
            }
            let mut output = [0.0; 6];
            let mut error = [0 as c_char; sys::SE_MAX_STNAME];
            // SAFETY: output/error buffers satisfy the C API; lock is held.
            let returned_flags = unsafe {
                sys::swe_calc_ut(
                    jd,
                    object_code(*object),
                    flags,
                    output.as_mut_ptr(),
                    error.as_mut_ptr(),
                )
            };
            if returned_flags < 0 {
                return Err(CalculationError::ObjectCalculation {
                    object: *object,
                    message: error_message(&error),
                });
            }
            let actual_source = returned_flags & sys::SEFLG_EPHMASK;
            if actual_source != source_flag {
                return Err(CalculationError::DataUnavailable(format!(
                    "requested {} for {object:?}, but Swiss Ephemeris returned {}",
                    source_name(source_flag),
                    source_name(actual_source)
                )));
            }
            positions.insert(
                *object,
                Position::new(output[0], output[1], output[2], output[3])?,
            );
        }

        let house_flags = source_flag | sidereal_flag;
        let (cusps, angles) = calculate_houses(jd, house_flags, request)?;
        let (_, previous_angles) = calculate_houses(jd - 30.0 / 86_400.0, house_flags, request)?;
        let (_, next_angles) = calculate_houses(jd + 30.0 / 86_400.0, house_flags, request)?;
        let angle_position = |index| {
            AngularPosition::new(
                angles[index],
                signed_angular_difference(previous_angles[index], next_angles[index]) * 1_440.0,
            )
        };
        let chart_angles =
            ChartAngles::new(angle_position(0)?, angle_position(1)?, angle_position(3)?)?;
        let houses = HouseCusps::new(cusps[1..13].to_vec(), chart_angles)?;
        let source = match self.mode {
            EphemerisMode::Moshier => EphemerisSource::Moshier,
            EphemerisMode::SwissFiles(_) => EphemerisSource::SwissFiles,
        };
        let provenance = CalculationProvenance::new(
            "Swiss Ephemeris",
            native_version(),
            source,
            self.data_revision.clone(),
        )?;
        CalculationResult::new(request, positions, houses, provenance)
    }
}

/// Verify the three declared Swiss Ephemeris files and their SHA-256 digests.
pub fn verify_pinned_swiss_files(path: &Path) -> Result<(), CalculationError> {
    if !path.is_dir() {
        return Err(CalculationError::DataUnavailable(format!(
            "Swiss Ephemeris path is not a directory: {}",
            path.display()
        )));
    }
    path_to_c_string(path)?;
    for (name, expected) in PINNED_SWISS_EPHEMERIS_FILES {
        let file_path = path.join(name);
        let mut file = File::open(&file_path).map_err(|error| {
            CalculationError::DataUnavailable(format!(
                "could not open pinned Swiss Ephemeris file {}: {error}",
                file_path.display()
            ))
        })?;
        let mut digest = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let count = file.read(&mut buffer).map_err(|error| {
                CalculationError::DataUnavailable(format!(
                    "could not read pinned Swiss Ephemeris file {}: {error}",
                    file_path.display()
                ))
            })?;
            if count == 0 {
                break;
            }
            digest.update(&buffer[..count]);
        }
        let actual = format!("{:x}", digest.finalize());
        if actual != expected {
            return Err(CalculationError::DataUnavailable(format!(
                "pinned Swiss Ephemeris file {name} failed SHA-256 verification"
            )));
        }
    }
    Ok(())
}

fn calculate_houses(
    jd: f64,
    flags: i32,
    request: &CalculationRequest,
) -> Result<([f64; 13], [f64; 10]), CalculationError> {
    let mut cusps = [0.0; 13];
    let mut angles = [0.0; 10];
    // SAFETY: buffers have the sizes required for twelve-house systems; lock is held.
    let status = unsafe {
        sys::swe_houses_ex(
            jd,
            flags,
            request.location().latitude_degrees(),
            request.location().longitude_degrees(),
            house_code(request.house_system()),
            cusps.as_mut_ptr(),
            angles.as_mut_ptr(),
        )
    };
    if status < 0 {
        return Err(CalculationError::Provider(format!(
            "{:?} houses could not be calculated at latitude {}",
            request.house_system(),
            request.location().latitude_degrees()
        )));
    }
    Ok((cusps, angles))
}

fn signed_angular_difference(first: f64, second: f64) -> f64 {
    let difference = (second - first).rem_euclid(360.0);
    if difference > 180.0 {
        difference - 360.0
    } else {
        difference
    }
}

impl EphemerisAdapter for SwissEphemerisAdapter {
    fn calculate(
        &self,
        request: &CalculationRequest,
    ) -> Result<CalculationResult, CalculationError> {
        let _guard = SWISS_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.calculate_locked(request)
    }
}

impl EventPositionProvider for SwissEphemerisAdapter {
    fn sample_event_positions(
        &self,
        request: &EventPositionRequest,
    ) -> Result<EventPositionSample, EventError> {
        let _guard = SWISS_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let source_flag = self.source_flag_locked().map_err(event_error)?;
        let jd = julian_day(request.instant());
        let frame_flag = match request.frame() {
            EventCoordinateFrame::Configured { zodiac, ayanamsa } => {
                if zodiac == Zodiac::Sidereal {
                    let mode = ayanamsa_code(ayanamsa.ok_or_else(|| {
                        EventError::Provider("sidereal event frame omitted ayanamsa".into())
                    })?);
                    // SAFETY: the validated mode is set while the global lock is held.
                    unsafe { sys::swe_set_sid_mode(mode, 0.0, 0.0) };
                    sys::SEFLG_SIDEREAL
                } else {
                    0
                }
            }
            EventCoordinateFrame::TropicalOfDate => 0,
            EventCoordinateFrame::BirthEpochEcliptic { epoch } => {
                let epoch_ut = julian_day(epoch);
                // Swiss user mode takes t0 in TT. ECL_T0 fixes the reference
                // plane to the ecliptic at the birth epoch.
                let epoch_tt = epoch_ut + unsafe { sys::swe_deltat(epoch_ut) };
                const SE_SIDM_USER: i32 = 255;
                const SE_SIDBIT_ECL_T0: i32 = 256;
                // SAFETY: numeric constants come from the vendored 2.10.03 header;
                // the global adapter lock is held.
                unsafe { sys::swe_set_sid_mode(SE_SIDM_USER | SE_SIDBIT_ECL_T0, epoch_tt, 0.0) };
                sys::SEFLG_SIDEREAL
            }
        };
        let flags = source_flag | frame_flag | sys::SEFLG_SPEED;
        let mut positions = BTreeMap::new();
        for object in request.objects() {
            if self.mode == EphemerisMode::Moshier && *object == CelestialObject::Chiron {
                return Err(EventError::Provider(
                    CalculationError::UnsupportedObject(*object).to_string(),
                ));
            }
            let mut output = [0.0; 6];
            let mut error = [0 as c_char; sys::SE_MAX_STNAME];
            // SAFETY: buffers satisfy the native API and the global lock is held.
            let returned = unsafe {
                sys::swe_calc_ut(
                    jd,
                    object_code(*object),
                    flags,
                    output.as_mut_ptr(),
                    error.as_mut_ptr(),
                )
            };
            if returned < 0 {
                return Err(EventError::Provider(error_message(&error)));
            }
            let actual_source = returned & sys::SEFLG_EPHMASK;
            if actual_source != source_flag {
                return Err(EventError::Provider(format!(
                    "requested {}, but Swiss Ephemeris returned {} for {object:?}",
                    source_name(source_flag),
                    source_name(actual_source)
                )));
            }
            positions.insert(
                *object,
                AngularPosition::new(output[0], output[3]).map_err(event_error)?,
            );
        }
        EventPositionSample::new(
            request.clone(),
            positions,
            self.provenance_locked().map_err(event_error)?,
        )
    }
}

impl GlobalEclipseProvider for SwissEphemerisAdapter {
    fn find_global_eclipse(
        &self,
        kind: EclipseKind,
        reference: astraeus_core::UtcInstant,
        direction: EclipseSearchDirection,
    ) -> Result<GlobalEclipseMaximum, EventError> {
        let _guard = SWISS_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let source_flag = self.source_flag_locked().map_err(event_error)?;
        let start_jd = julian_day(reference);
        self.verify_eclipse_sources_locked(start_jd, source_flag)?;
        let mut times = [0.0; 10];
        let mut error = [0 as c_char; sys::SE_MAX_STNAME];
        let backward = i32::from(direction == EclipseSearchDirection::Backward);
        // SAFETY: buffers have the sizes required by the native API and the
        // process-wide Swiss lock is held.
        let flags = unsafe {
            match kind {
                EclipseKind::Solar => sys::swe_sol_eclipse_when_glob(
                    start_jd,
                    source_flag,
                    0,
                    times.as_mut_ptr(),
                    backward,
                    error.as_mut_ptr(),
                ),
                EclipseKind::Lunar => sys::swe_lun_eclipse_when(
                    start_jd,
                    source_flag,
                    0,
                    times.as_mut_ptr(),
                    backward,
                    error.as_mut_ptr(),
                ),
            }
        };
        if flags <= 0 {
            return Err(EventError::Provider(error_message(&error)));
        }
        let exact = utc_from_julian_day(times[0])?;
        let residual_seconds = (julian_day(exact) - times[0]).abs() * 86_400.0;
        GlobalEclipseMaximum::new(
            kind,
            exact,
            eclipse_classifications(flags),
            flags,
            residual_seconds,
            self.provenance_locked().map_err(event_error)?,
        )
    }
}

impl SwissEphemerisAdapter {
    fn source_flag_locked(&self) -> Result<i32, CalculationError> {
        match &self.mode {
            EphemerisMode::Moshier => Ok(sys::SEFLG_MOSEPH),
            EphemerisMode::SwissFiles(path) => {
                let path = path_to_c_string(path)?;
                // SAFETY: the CString remains alive for the call; lock is held.
                unsafe { sys::swe_set_ephe_path(path.as_ptr()) };
                Ok(sys::SEFLG_SWIEPH)
            }
        }
    }

    fn provenance_locked(&self) -> Result<CalculationProvenance, CalculationError> {
        let source = match self.mode {
            EphemerisMode::Moshier => EphemerisSource::Moshier,
            EphemerisMode::SwissFiles(_) => EphemerisSource::SwissFiles,
        };
        Ok(CalculationProvenance::new(
            "Swiss Ephemeris",
            native_version(),
            source,
            self.data_revision.clone(),
        )?)
    }

    fn verify_eclipse_sources_locked(&self, jd: f64, source_flag: i32) -> Result<(), EventError> {
        for object in [CelestialObject::Sun, CelestialObject::Moon] {
            let mut output = [0.0; 6];
            let mut error = [0 as c_char; sys::SE_MAX_STNAME];
            // SAFETY: buffers satisfy the native API and the lock is held.
            let returned = unsafe {
                sys::swe_calc_ut(
                    jd,
                    object_code(object),
                    source_flag,
                    output.as_mut_ptr(),
                    error.as_mut_ptr(),
                )
            };
            if returned < 0 {
                return Err(EventError::Provider(error_message(&error)));
            }
            if returned & sys::SEFLG_EPHMASK != source_flag {
                return Err(EventError::Provider(format!(
                    "requested {}, but Swiss Ephemeris returned {} for {object:?}",
                    source_name(source_flag),
                    source_name(returned & sys::SEFLG_EPHMASK)
                )));
            }
        }
        Ok(())
    }
}

fn julian_day(instant: astraeus_core::UtcInstant) -> f64 {
    let instant = instant.as_datetime();
    let hour = f64::from(instant.hour())
        + f64::from(instant.minute()) / 60.0
        + (f64::from(instant.second()) + f64::from(instant.nanosecond()) / 1e9) / 3600.0;
    // SAFETY: the normalized UTC components are valid scalar inputs.
    unsafe {
        sys::swe_julday(
            instant.year(),
            instant.month() as i32,
            instant.day() as i32,
            hour,
            sys::SE_GREG_CAL,
        )
    }
}

fn event_error(error: impl ToString) -> EventError {
    EventError::Provider(error.to_string())
}

fn utc_from_julian_day(jd: f64) -> Result<astraeus_core::UtcInstant, EventError> {
    const UNIX_EPOCH_JD: f64 = 2_440_587.5;
    let milliseconds = ((jd - UNIX_EPOCH_JD) * 86_400_000.0).round();
    if !milliseconds.is_finite() || milliseconds < i64::MIN as f64 || milliseconds > i64::MAX as f64
    {
        return Err(EventError::Time("Julian day is outside UTC range".into()));
    }
    let datetime = chrono::DateTime::from_timestamp_millis(milliseconds as i64)
        .ok_or_else(|| EventError::Time("Julian day is outside UTC range".into()))?;
    astraeus_core::UtcInstant::parse_rfc3339(
        &datetime.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    )
    .map_err(event_error)
}

fn eclipse_classifications(flags: i32) -> Vec<EclipseClassification> {
    let mut values = Vec::new();
    for (flag, classification) in [
        (sys::SE_ECL_CENTRAL, EclipseClassification::Central),
        (sys::SE_ECL_NONCENTRAL, EclipseClassification::Noncentral),
        (sys::SE_ECL_TOTAL, EclipseClassification::Total),
        (sys::SE_ECL_ANNULAR, EclipseClassification::Annular),
        (sys::SE_ECL_PARTIAL, EclipseClassification::Partial),
        (sys::SE_ECL_ANNULAR_TOTAL, EclipseClassification::Hybrid),
        (sys::SE_ECL_PENUMBRAL, EclipseClassification::Penumbral),
    ] {
        if flags & flag != 0 {
            values.push(classification);
        }
    }
    values
}

fn path_to_c_string(path: &Path) -> Result<CString, CalculationError> {
    let path = path.to_str().ok_or_else(|| {
        CalculationError::DataUnavailable("Swiss Ephemeris path is not valid UTF-8".into())
    })?;
    CString::new(path.as_bytes()).map_err(|_| {
        CalculationError::DataUnavailable("Swiss Ephemeris path contains a NUL byte".into())
    })
}

fn error_message(buffer: &[c_char]) -> String {
    // SAFETY: the zero-initialized fixed buffer is always NUL terminated.
    unsafe { CStr::from_ptr(buffer.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

fn native_version() -> String {
    let mut buffer = [0 as c_char; sys::SE_MAX_STNAME];
    // SAFETY: the output buffer is valid and the global adapter lock is held.
    unsafe { sys::swe_version(buffer.as_mut_ptr()) };
    error_message(&buffer)
}

fn source_name(flag: i32) -> &'static str {
    match flag {
        sys::SEFLG_SWIEPH => "Swiss files",
        sys::SEFLG_MOSEPH => "Moshier",
        sys::SEFLG_JPLEPH => "JPL",
        _ => "an unknown source",
    }
}

fn object_code(object: CelestialObject) -> i32 {
    match object {
        CelestialObject::Sun => sys::SE_SUN,
        CelestialObject::Moon => sys::SE_MOON,
        CelestialObject::Mercury => sys::SE_MERCURY,
        CelestialObject::Venus => sys::SE_VENUS,
        CelestialObject::Mars => sys::SE_MARS,
        CelestialObject::Jupiter => sys::SE_JUPITER,
        CelestialObject::Saturn => sys::SE_SATURN,
        CelestialObject::Uranus => sys::SE_URANUS,
        CelestialObject::Neptune => sys::SE_NEPTUNE,
        CelestialObject::Pluto => sys::SE_PLUTO,
        CelestialObject::MeanNode => sys::SE_MEAN_NODE,
        CelestialObject::TrueNode => sys::SE_TRUE_NODE,
        CelestialObject::Chiron => sys::SE_CHIRON,
    }
}

fn house_code(system: HouseSystem) -> i32 {
    match system {
        HouseSystem::Placidus => b'P' as i32,
        HouseSystem::Koch => b'K' as i32,
        HouseSystem::Porphyry => b'O' as i32,
        HouseSystem::Regiomontanus => b'R' as i32,
        HouseSystem::Campanus => b'C' as i32,
        HouseSystem::Equal => b'A' as i32,
        HouseSystem::WholeSign => b'W' as i32,
    }
}

fn ayanamsa_code(ayanamsa: Ayanamsa) -> i32 {
    match ayanamsa {
        Ayanamsa::FaganBradley => sys::SE_SIDM_FAGAN_BRADLEY,
        Ayanamsa::Lahiri => sys::SE_SIDM_LAHIRI,
        Ayanamsa::DeLuce => sys::SE_SIDM_DELUCE,
        Ayanamsa::Raman => sys::SE_SIDM_RAMAN,
        Ayanamsa::Krishnamurti => sys::SE_SIDM_KRISHNAMURTI,
        Ayanamsa::Yukteshwar => 7,
        Ayanamsa::JnBhasin => 8,
    }
}
