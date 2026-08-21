use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::ExitCode;

use astraeus_artifacts::CalculationArtifact;
use astraeus_core::{CalculationRequest, EphemerisAdapter};
use astraeus_swiss::SwissEphemerisAdapter;
use astraeus_timeseries::{AspectTimelineRequest, calculate_aspect_timeline};
use serde_json::json;

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("astraeus: {error}");
            eprintln!(
                "usage: astraeus chart cast [REQUEST|-] --ephemeris <moshier|swiss-files> [--ephemeris-path PATH] [--pretty]"
            );
            eprintln!(
                "       astraeus timeline aspect [REQUEST|-] --ephemeris <moshier|swiss-files> [--ephemeris-path PATH] [--pretty]"
            );
            eprintln!("       astraeus artifact <canonicalize|inspect> [PATH|-] [--pretty]");
            ExitCode::from(2)
        }
    }
}

fn run(args: Vec<String>) -> Result<String, String> {
    let [group, command, rest @ ..] = args.as_slice() else {
        return Err("a command is required".into());
    };
    match (group.as_str(), command.as_str()) {
        ("chart", "cast") => run_chart(parse_provider_options(rest)?),
        ("timeline", "aspect") => run_timeline(parse_provider_options(rest)?),
        ("artifact", "canonicalize" | "inspect") => {
            run_artifact(command, parse_artifact_options(rest)?)
        }
        ("chart", _) => Err(format!("unknown chart command `{command}`")),
        ("timeline", _) => Err(format!("unknown timeline command `{command}`")),
        ("artifact", _) => Err(format!("unknown artifact command `{command}`")),
        _ => Err(format!("unknown command group `{group}`")),
    }
}

#[derive(Clone, Copy)]
enum ProviderMode {
    Moshier,
    SwissFiles,
}

struct ProviderOptions {
    path: Option<String>,
    pretty: bool,
    mode: ProviderMode,
    ephemeris_path: Option<PathBuf>,
}

struct ArtifactOptions {
    path: Option<String>,
    pretty: bool,
}

fn parse_provider_options(args: &[String]) -> Result<ProviderOptions, String> {
    let mut path = None;
    let mut pretty = false;
    let mut mode = None;
    let mut ephemeris_path = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--pretty" => pretty = true,
            "--ephemeris" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or("--ephemeris requires moshier or swiss-files")?;
                if mode.is_some() {
                    return Err("--ephemeris may be supplied only once".into());
                }
                mode = Some(match value.as_str() {
                    "moshier" => ProviderMode::Moshier,
                    "swiss-files" => ProviderMode::SwissFiles,
                    _ => return Err(format!("unknown ephemeris mode `{value}`")),
                });
            }
            "--ephemeris-path" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or("--ephemeris-path requires a directory")?;
                if ephemeris_path.is_some() {
                    return Err("--ephemeris-path may be supplied only once".into());
                }
                ephemeris_path = Some(PathBuf::from(value));
            }
            value if value.starts_with("--") => return Err(format!("unknown option `{value}`")),
            value => {
                if path.replace(value.to_owned()).is_some() {
                    return Err("only one request path may be supplied".into());
                }
            }
        }
        index += 1;
    }
    Ok(ProviderOptions {
        path,
        pretty,
        mode: mode.ok_or("--ephemeris is required")?,
        ephemeris_path,
    })
}

fn parse_artifact_options(args: &[String]) -> Result<ArtifactOptions, String> {
    let mut path = None;
    let mut pretty = false;
    for arg in args {
        match arg.as_str() {
            "--pretty" => pretty = true,
            value if value.starts_with("--") => return Err(format!("unknown option `{value}`")),
            value => {
                if path.replace(value.to_owned()).is_some() {
                    return Err("only one artifact path may be supplied".into());
                }
            }
        }
    }
    Ok(ArtifactOptions { path, pretty })
}

fn adapter(options: &ProviderOptions) -> Result<SwissEphemerisAdapter, String> {
    match options.mode {
        ProviderMode::Moshier => {
            if options.ephemeris_path.is_some() {
                return Err("--ephemeris-path is valid only with swiss-files".into());
            }
            Ok(SwissEphemerisAdapter::moshier())
        }
        ProviderMode::SwissFiles => {
            let path = options
                .ephemeris_path
                .clone()
                .or_else(|| env::var_os("ASTRAEUS_SWISS_EPHEMERIS_PATH").map(PathBuf::from))
                .ok_or("swiss-files requires --ephemeris-path or ASTRAEUS_SWISS_EPHEMERIS_PATH")?;
            SwissEphemerisAdapter::pinned_swiss_files(path)
                .map_err(|error| format!("invalid pinned Swiss Ephemeris bundle: {error}"))
        }
    }
}

fn run_chart(options: ProviderOptions) -> Result<String, String> {
    let input = read_input(options.path.as_deref())?;
    let request: CalculationRequest = serde_json::from_str(&input)
        .map_err(|error| format!("invalid calculation request: {error}"))?;
    let result = adapter(&options)?
        .calculate(&request)
        .map_err(|error| format!("chart calculation failed: {error}"))?;
    let artifact = CalculationArtifact::new(request, result)
        .map_err(|error| format!("could not build calculation artifact: {error}"))?;
    if options.pretty {
        artifact.to_pretty_json().map_err(|error| error.to_string())
    } else {
        artifact.to_json().map_err(|error| error.to_string())
    }
}

fn run_timeline(options: ProviderOptions) -> Result<String, String> {
    let input = read_input(options.path.as_deref())?;
    let request: AspectTimelineRequest = serde_json::from_str(&input)
        .map_err(|error| format!("invalid aspect timeline request: {error}"))?;
    let artifact = calculate_aspect_timeline(&adapter(&options)?, request)
        .map_err(|error| format!("aspect timeline calculation failed: {error}"))?;
    if options.pretty {
        artifact.to_pretty_json().map_err(|error| error.to_string())
    } else {
        artifact.to_json().map_err(|error| error.to_string())
    }
}

fn run_artifact(command: &str, options: ArtifactOptions) -> Result<String, String> {
    let input = read_input(options.path.as_deref())?;
    let artifact = CalculationArtifact::from_json(&input)
        .map_err(|error| format!("invalid calculation artifact: {error}"))?;
    match command {
        "canonicalize" => {
            if options.pretty {
                artifact.to_pretty_json().map_err(|error| error.to_string())
            } else {
                artifact.to_json().map_err(|error| error.to_string())
            }
        }
        "inspect" => serde_json::to_string_pretty(&json!({
            "kind": "calculation",
            "content_id": artifact.content_id().map_err(|error| error.to_string())?,
            "schema_version": astraeus_artifacts::SCHEMA_VERSION,
            "instant": artifact.request().instant().as_datetime().to_rfc3339(),
            "zodiac": format!("{:?}", artifact.request().zodiac()),
            "objects": artifact.request().objects().len(),
        }))
        .map_err(|error| error.to_string()),
        _ => unreachable!("validated artifact command"),
    }
}

fn read_input(path: Option<&str>) -> Result<String, String> {
    match path.unwrap_or("-") {
        "-" => {
            let mut input = String::new();
            io::stdin()
                .read_to_string(&mut input)
                .map_err(|error| format!("could not read stdin: {error}"))?;
            Ok(input)
        }
        path => fs::read_to_string(path).map_err(|error| format!("could not read {path}: {error}")),
    }
}
