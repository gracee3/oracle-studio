use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use oracle_studio_chart_view::{
    RenderOptions, TransitTimeline, WheelOrientation, render_biwheel_svg,
};
use serde::Serialize;
use thiserror::Error;

const PLAYER_CSS: &str = include_str!("player.css");
const PLAYER_JS: &str = include_str!("player.js");
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Error)]
pub enum ChartOutputError {
    #[error("output destination already exists: {0}")]
    DestinationExists(PathBuf),
    #[error("output destination has no file name: {0}")]
    MissingFileName(PathBuf),
    #[error("could not serialize the render timeline: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("could not write chart output: {0}")]
    Io(#[from] io::Error),
}

#[derive(Serialize)]
struct PlayerData<'a> {
    schema_version: u32,
    orientation: WheelOrientation,
    timeline: &'a TransitTimeline,
}

pub fn render_player_html(
    timeline: &TransitTimeline,
    title: &str,
    orientation: WheelOrientation,
) -> Result<String, ChartOutputError> {
    let first_time = chrono::DateTime::parse_from_rfc3339(&timeline.frames[0].timestamp)
        .expect("validated timeline timestamp")
        .with_timezone(&chrono::Utc);
    let first_scene = timeline.scene_at(first_time);
    let svg = render_biwheel_svg(&first_scene, &RenderOptions { orientation });
    let data = serde_json::to_string(&PlayerData {
        schema_version: 1,
        orientation,
        timeline,
    })?;
    let data = script_safe_json(&data);
    let title = escape_html(title);
    Ok(format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; img-src data:; base-uri 'none'; form-action 'none'\"><title>{title}</title><style>{PLAYER_CSS}</style></head><body><main><header><p class=\"eyebrow\">Oracle Studio</p><h1>{title}</h1><p class=\"privacy-warning\" role=\"note\">This self-contained file contains derived chart positions, houses, timestamps, and aspects. Share it as carefully as any chart export.</p></header><section class=\"chart-shell\" aria-label=\"Interactive transit biwheel\"><div id=\"chart-stage\">{svg}</div><div class=\"controls\" aria-label=\"Playback controls\"><div class=\"transport\"><button id=\"previous-frame\" type=\"button\" title=\"Previous exact frame\">Previous</button><button id=\"reverse\" type=\"button\" aria-pressed=\"false\">Reverse</button><button id=\"play-pause\" type=\"button\" aria-pressed=\"false\">Play</button><button id=\"forward\" type=\"button\" aria-pressed=\"true\">Forward</button><button id=\"next-frame\" type=\"button\" title=\"Next exact frame\">Next</button></div><label class=\"scrubber-label\" for=\"scrubber\">Timeline <input id=\"scrubber\" type=\"range\" step=\"1000\"></label><div class=\"readout\"><output id=\"timestamp\" for=\"scrubber\"></output><label for=\"playback-rate\">Speed <select id=\"playback-rate\"><option value=\"900\">15 min/s</option><option value=\"3600\" selected>1 hour/s</option><option value=\"21600\">6 hours/s</option><option value=\"86400\">1 day/s</option></select></label></div><fieldset><legend>Visible layers</legend><label><input id=\"toggle-natal\" type=\"checkbox\" checked> Natal</label><label><input id=\"toggle-transit\" type=\"checkbox\" checked> Transits</label><label><input id=\"toggle-aspects\" type=\"checkbox\" checked> Aspects</label></fieldset></div></section></main><footer><p>Oracle Studio is free software licensed AGPL-3.0-or-later. Source and license: <a href=\"https://github.com/gracee3/oracle-studio\">github.com/gracee3/oracle-studio</a>.</p><p>No astrology is calculated in this player; it displays validated Astraeus comparison artifacts.</p></footer><script id=\"oracle-timeline\" type=\"application/json\">{data}</script><script>{PLAYER_JS}</script></body></html>"
    ))
}

/// Publish an owner-only file atomically. Existing destinations are refused
/// unless the caller explicitly opts into replacement.
pub fn write_private_atomic(
    destination: &Path,
    bytes: &[u8],
    overwrite: bool,
) -> Result<(), ChartOutputError> {
    if !overwrite && destination.exists() {
        return Err(ChartOutputError::DestinationExists(
            destination.to_path_buf(),
        ));
    }
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let file_name = destination
        .file_name()
        .ok_or_else(|| ChartOutputError::MissingFileName(destination.to_path_buf()))?;
    let mut temporary = None;
    let mut file = None;
    for _ in 0..100 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".{}.oracle-studio-chart-{}-{sequence}.tmp",
            file_name.to_string_lossy(),
            std::process::id()
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&candidate) {
            Ok(opened) => {
                temporary = Some(candidate);
                file = Some(opened);
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    let temporary = temporary.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate a unique temporary output file",
        )
    })?;
    let result = (|| -> Result<(), ChartOutputError> {
        let mut file = file.expect("temporary path and file are set together");
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        if overwrite {
            fs::rename(&temporary, destination)?;
        } else if let Err(error) = fs::hard_link(&temporary, destination) {
            if error.kind() == io::ErrorKind::AlreadyExists {
                return Err(ChartOutputError::DestinationExists(
                    destination.to_path_buf(),
                ));
            }
            return Err(error.into());
        } else {
            fs::remove_file(&temporary)?;
        }
        #[cfg(unix)]
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if temporary.exists() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn script_safe_json(value: &str) -> String {
    value
        .replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
