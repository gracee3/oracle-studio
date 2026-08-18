use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let mut random = [0_u8; 16];
        getrandom::fill(&mut random).unwrap();
        let suffix = random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let path = std::env::temp_dir().join(format!("oracle-studio-chart-test-{suffix}"));
        fs::create_dir(&path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        Self(path)
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/comparisons")
        .join(name)
}

fn chart_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_oracle-studio-chart"))
}

fn run(command: &mut Command) -> Output {
    command.output().unwrap()
}

fn success(command: &mut Command) -> Output {
    let output = run(command);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn svg_cli_is_private_atomic_and_requires_explicit_overwrite() {
    let directory = TestDirectory::new();
    let output = directory.0.join("chart.svg");
    success(
        chart_command()
            .arg("svg")
            .arg("--comparison")
            .arg(fixture("frame-01.json"))
            .arg("--output")
            .arg(&output)
            .args(["--orientation", "zodiac-zero-top"]),
    );
    let original = fs::read_to_string(&output).unwrap();
    assert!(original.contains("data-orientation=\"zodiac-zero-top\""));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&output).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    let refused = run(chart_command()
        .arg("svg")
        .arg("--comparison")
        .arg(fixture("frame-02.json"))
        .arg("--output")
        .arg(&output));
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("already exists"));
    assert_eq!(fs::read_to_string(&output).unwrap(), original);

    success(
        chart_command()
            .arg("svg")
            .arg("--comparison")
            .arg(fixture("frame-02.json"))
            .arg("--output")
            .arg(&output)
            .arg("--overwrite"),
    );
    let replacement = fs::read_to_string(&output).unwrap();
    assert_ne!(replacement, original);
    assert!(!replacement.contains("2026-01-01T12:00:00+00:00"));
    assert!(replacement.contains(
        "id=\"transit-point-sun\" class=\"chart-point chart-point--transit\" data-point-id=\"Sun\" data-longitude=\"0.500000000000\""
    ));
    assert!(fs::read_dir(&directory.0).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".tmp")
    }));
}

#[test]
fn timeline_cli_sorts_frames_embeds_only_render_data_and_escapes_titles() {
    let directory = TestDirectory::new();
    let output = directory.0.join("timeline.html");
    let hostile_title = "Fictional </script><script>alert('x')</script> & chart";
    let hostile_natal_name = "Example <Natal> & inner";
    let hostile_location = "Sample </p><script>alert('place')</script>";
    success(
        chart_command()
            .arg("timeline")
            .arg("--comparison")
            .arg(fixture("frame-03.json"))
            .arg(fixture("frame-01.json"))
            .arg(fixture("frame-02.json"))
            .arg("--output")
            .arg(&output)
            .arg("--title")
            .arg(hostile_title)
            .args(["--natal-name", hostile_natal_name])
            .args(["--natal-datetime", "2000-01-01T05:30:00+05:30"])
            .args(["--natal-location", hostile_location])
            .args(["--transit-name", "Example transits"])
            .args(["--transit-datetime", "2025-12-31T19:00:00-05:00"])
            .args(["--transit-location", "Fictional test location"]),
    );
    let html = fs::read_to_string(&output).unwrap();
    let first = html.find("2026-01-01T00:00:00+00:00").unwrap();
    let second = html.find("2026-01-01T12:00:00+00:00").unwrap();
    let third = html.find("2026-01-03T12:00:00+00:00").unwrap();
    assert!(first < second && second < third);
    assert!(!html.contains("</script><script>alert"));
    assert!(html.contains("Fictional &lt;/script&gt;&lt;script&gt;alert"));
    assert!(html.contains("Example &lt;Natal&gt; &amp; inner"));
    assert!(html.contains("Sample &lt;/p&gt;&lt;script&gt;alert"));
    assert!(html.contains("Natal chart <span aria-hidden=\"true\">·</span> Inner wheel"));
    assert!(html.contains("Transit event <span aria-hidden=\"true\">·</span> Outer wheel"));
    assert!(html.contains("id=\"natal-chart-datetime\""));
    assert!(html.contains("Sat, Jan 01, 2000 · 05:30 +05:30"));
    assert!(html.contains("id=\"transit-chart-datetime\""));
    assert!(html.contains("Wed, Dec 31, 2025 · 19:00 -05:00"));
    assert!(html.contains("Tropical <span aria-hidden=\"true\">·</span> Placidus"));
    assert!(html.contains("\"schema_version\":2"));
    assert!(html.contains("\"transit_offset_seconds\":-18000"));
    assert!(!html.contains("frame-01.json"));
    assert!(!html.contains("fixtures/comparisons"));
    assert!(!html.contains("latitude_degrees"));
    assert!(!html.contains("longitude_degrees\":0.0,\"elevation_meters"));
    assert!(!html.contains("deterministic_mock"));
    assert!(!html.contains("<script src="));
    assert!(!html.contains("<link"));
    assert!(!html.contains("fetch("));
    assert!(!html.contains("XMLHttpRequest"));
    assert!(!html.contains("<table"));
    assert!(html.contains("Content-Security-Policy"));
    assert!(html.contains("id=\"play-pause\""));
    assert!(html.contains("id=\"reverse\""));
    assert!(html.contains("id=\"forward\""));
    assert!(html.contains("id=\"previous-frame\""));
    assert!(html.contains("id=\"next-frame\""));
    assert!(html.contains("id=\"scrubber\""));
    assert!(html.contains("id=\"playback-rate\""));
    assert!(html.contains("id=\"toggle-natal\""));
    assert!(html.contains("id=\"toggle-transit\""));
    assert!(html.contains("id=\"toggle-aspects\""));
    assert!(html.contains("aspects: frames[index].aspects"));
    assert!(html.contains("Number(svg.dataset.transitInnerRadius)"));
    assert!(html.contains("function layoutLabels(points, lane)"));
    assert!(html.contains("precision: 'degree'"));
    assert!(!html.contains("data-role=\"sign\""));
    assert!(html.contains("data-role=\"position\""));
    assert!(html.contains("data-role=\"aspect-glyph\""));
    assert!(html.contains("document.getElementById('natal-structure-layer')"));
    assert!(html.contains("function formatChartDatetime(milliseconds, offsetSeconds)"));
    assert!(html.contains("transitChartDatetime.textContent = formatChartDatetime"));
    assert!(html.contains("AGPL-3.0-or-later"));
    assert!(html.contains("contains derived chart positions"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&output).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn timeline_cli_rejects_display_dates_that_do_not_match_artifact_instants() {
    let directory = TestDirectory::new();
    let output = directory.0.join("natal-mismatch.html");
    let result = run(chart_command()
        .arg("timeline")
        .arg("--comparison")
        .arg(fixture("frame-01.json"))
        .arg("--output")
        .arg(&output)
        .args(["--natal-datetime", "2000-01-02T00:00:00Z"]));
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr)
            .contains("natal display datetime 2000-01-02T00:00:00+00:00")
    );
    assert!(!output.exists());

    let transit_output = directory.0.join("transit-mismatch.html");
    let transit_result = run(chart_command()
        .arg("timeline")
        .arg("--comparison")
        .arg(fixture("frame-01.json"))
        .arg("--output")
        .arg(&transit_output)
        .args(["--transit-datetime", "2026-01-02T00:00:00Z"]));
    assert!(!transit_result.status.success());
    assert!(
        String::from_utf8_lossy(&transit_result.stderr)
            .contains("transit display datetime 2026-01-02T00:00:00+00:00")
    );
    assert!(!transit_output.exists());
}

#[test]
fn malformed_input_fails_without_publishing_an_output() {
    let directory = TestDirectory::new();
    let malformed = directory.0.join("malformed.json");
    let output = directory.0.join("chart.svg");
    fs::write(&malformed, "{not valid JSON}").unwrap();
    let result = run(chart_command()
        .arg("svg")
        .arg("--comparison")
        .arg(&malformed)
        .arg("--output")
        .arg(&output));
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("failed validation"));
    assert!(!output.exists());
}
