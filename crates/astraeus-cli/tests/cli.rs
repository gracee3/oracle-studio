use std::io::Write;
use std::process::{Command, Output, Stdio};

const CHART_REQUEST: &str = r#"{
  "instant":"2000-01-01T12:00:00Z",
  "location":{"latitude_degrees":51.4779,"longitude_degrees":0.0,"elevation_meters":46.0},
  "objects":["sun","moon"],
  "zodiac":"tropical",
  "ayanamsa":null,
  "house_system":"placidus"
}"#;

const CHIRON_CHART_REQUEST: &str = r#"{
  "instant":"2000-01-01T12:00:00Z",
  "location":{"latitude_degrees":51.4779,"longitude_degrees":0.0,"elevation_meters":46.0},
  "objects":["sun","chiron"],
  "zodiac":"tropical",
  "ayanamsa":null,
  "house_system":"placidus"
}"#;

const TIMELINE_REQUEST: &str = r#"{
  "subject":{"kind":"moving_moving","first_object":"sun","second_object":"moon","frame":{"kind":"tropical_of_date"}},
  "aspect":{"kind":"conjunction","orb_degrees":10.0},
  "start":"2000-01-01T12:00:00Z",
  "end":"2000-01-01T18:00:00Z",
  "cadence_seconds":3600
}"#;

const CHIRON_TIMELINE_REQUEST: &str = r#"{
  "subject":{"kind":"moving_moving","first_object":"sun","second_object":"chiron","frame":{"kind":"tropical_of_date"}},
  "aspect":{"kind":"trine","orb_degrees":5.0},
  "start":"2000-01-01T12:00:00Z",
  "end":"2000-01-01T18:00:00Z",
  "cadence_seconds":3600
}"#;

fn run_stdin(args: &[&str], input: &str, swiss_path: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_astraeus"));
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(path) = swiss_path {
        command.env("ASTRAEUS_SWISS_EPHEMERIS_PATH", path);
    }
    let mut child = command.spawn().expect("run Astraeus CLI");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn missing_command_reports_usage() {
    let output = Command::new(env!("CARGO_BIN_EXE_astraeus"))
        .output()
        .expect("run Astraeus CLI");
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("usage:"));
}

#[test]
fn chart_and_timeline_require_explicit_ephemeris_mode() {
    for (args, input) in [
        (&["chart", "cast", "-"][..], CHART_REQUEST),
        (&["timeline", "aspect", "-"][..], TIMELINE_REQUEST),
    ] {
        let output = run_stdin(args, input, None);
        assert_eq!(output.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&output.stderr).contains("--ephemeris is required"));
    }
}

#[test]
fn moshier_chart_and_timeline_emit_schema_v1_json_and_ignore_swiss_environment() {
    let chart = run_stdin(
        &["chart", "cast", "-", "--ephemeris", "moshier"],
        CHART_REQUEST,
        Some("/definitely/not/a/swiss/bundle"),
    );
    assert!(
        chart.status.success(),
        "{}",
        String::from_utf8_lossy(&chart.stderr)
    );
    let chart_json: serde_json::Value = serde_json::from_slice(&chart.stdout).unwrap();
    assert_eq!(chart_json["schema_version"], 1);
    assert_eq!(
        chart_json["result"]["provenance"]["ephemeris_source"],
        "moshier"
    );

    let timeline = run_stdin(
        &["timeline", "aspect", "-", "--ephemeris", "moshier"],
        TIMELINE_REQUEST,
        Some("/definitely/not/a/swiss/bundle"),
    );
    assert!(
        timeline.status.success(),
        "{}",
        String::from_utf8_lossy(&timeline.stderr)
    );
    let timeline_json: serde_json::Value = serde_json::from_slice(&timeline.stdout).unwrap();
    assert_eq!(timeline_json["schema_version"], 1);
    assert_eq!(timeline_json["samples"].as_array().unwrap().len(), 7);
    assert_eq!(
        timeline_json["provider_provenance"]["ephemeris_source"],
        "moshier"
    );
}

#[test]
fn moshier_rejects_swiss_only_chiron() {
    let output = run_stdin(
        &["chart", "cast", "-", "--ephemeris", "moshier"],
        CHIRON_CHART_REQUEST,
        None,
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Chiron"));
}

#[test]
#[ignore = "set ASTRAEUS_SWISS_EPHEMERIS_PATH to the pinned .se1 bundle"]
fn swiss_files_cli_verifies_the_bundle_for_chart_and_timeline() {
    let path = std::env::var("ASTRAEUS_SWISS_EPHEMERIS_PATH").unwrap();
    let chart = run_stdin(
        &["chart", "cast", "-", "--ephemeris", "swiss-files"],
        CHIRON_CHART_REQUEST,
        Some(&path),
    );
    assert!(
        chart.status.success(),
        "{}",
        String::from_utf8_lossy(&chart.stderr)
    );
    let chart_json: serde_json::Value = serde_json::from_slice(&chart.stdout).unwrap();
    assert_eq!(
        chart_json["result"]["provenance"]["data_revision"],
        "cae9ecd4b201544d85e411aced17660932514d43"
    );

    let timeline = run_stdin(
        &["timeline", "aspect", "-", "--ephemeris", "swiss-files"],
        CHIRON_TIMELINE_REQUEST,
        Some(&path),
    );
    assert!(
        timeline.status.success(),
        "{}",
        String::from_utf8_lossy(&timeline.stderr)
    );
    let timeline_json: serde_json::Value = serde_json::from_slice(&timeline.stdout).unwrap();
    assert_eq!(
        timeline_json["provider_provenance"]["data_revision"],
        "cae9ecd4b201544d85e411aced17660932514d43"
    );
}
