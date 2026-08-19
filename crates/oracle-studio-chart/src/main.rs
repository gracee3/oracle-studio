use std::{fs, path::PathBuf};

use astraeus_comparison::ComparisonArtifact;
use chrono::{DateTime, FixedOffset, Utc};
use clap::{Parser, Subcommand, ValueEnum};
use oracle_studio_chart::{
    ChartInformation, PlayerOptions, render_player_html_with_options, write_private_atomic,
};
use oracle_studio_chart_view::{
    ChartScene, RenderOptions, TransitTimeline, WheelOrientation, render_biwheel_svg,
};
use thiserror::Error;

#[derive(Parser)]
#[command(
    name = "oracle-studio-chart",
    version,
    about = "Render validated Astraeus transit comparisons without recalculation"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Render one exact transit-to-natal comparison as SVG.
    Svg {
        #[arg(long)]
        comparison: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, value_enum, default_value_t = OrientationArg::AscendantLeft)]
        orientation: OrientationArg,
        #[arg(long)]
        overwrite: bool,
    },
    /// Render one or more exact frames as a self-contained HTML player.
    Timeline {
        #[arg(long, required = true, num_args = 1..)]
        comparison: Vec<PathBuf>,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, value_enum, default_value_t = OrientationArg::AscendantLeft)]
        orientation: OrientationArg,
        #[arg(long, default_value = "Oracle Studio transit timeline")]
        title: String,
        /// Display name for the fixed natal / inner chart.
        #[arg(long, default_value = "Natal")]
        natal_name: String,
        /// Natal date and time with numeric offset; must equal the artifact instant.
        #[arg(long, value_name = "RFC3339")]
        natal_datetime: Option<DateTime<FixedOffset>>,
        /// Caller-supplied natal location label (not inferred from coordinates).
        #[arg(long)]
        natal_location: Option<String>,
        /// Display name for the moving transit / outer chart.
        #[arg(long, default_value = "Transits")]
        transit_name: String,
        /// First transit date and time with offset; must equal the first frame instant.
        #[arg(long, value_name = "RFC3339")]
        transit_datetime: Option<DateTime<FixedOffset>>,
        /// Caller-supplied transit location label (not inferred from coordinates).
        #[arg(long)]
        transit_location: Option<String>,
        #[arg(long)]
        overwrite: bool,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum OrientationArg {
    AscendantLeft,
    ZodiacZeroTop,
}

impl From<OrientationArg> for WheelOrientation {
    fn from(value: OrientationArg) -> Self {
        match value {
            OrientationArg::AscendantLeft => Self::AscendantLeft,
            OrientationArg::ZodiacZeroTop => Self::ZodiacZeroTop,
        }
    }
}

#[derive(Debug, Error)]
enum CliError {
    #[error("could not read comparison artifact {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("comparison artifact {path} failed validation: {message}")]
    InvalidArtifact { path: PathBuf, message: String },
    #[error(transparent)]
    Timeline(#[from] oracle_studio_chart_view::TransitTimelineError),
    #[error(transparent)]
    Output(#[from] oracle_studio_chart::ChartOutputError),
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Command::Svg {
            comparison,
            output,
            orientation,
            overwrite,
        } => {
            let artifact = read_comparison(&comparison)?;
            let scene = ChartScene::from_comparison(&artifact)?;
            let svg = render_biwheel_svg(
                &scene,
                &RenderOptions {
                    orientation: orientation.into(),
                },
            );
            write_private_atomic(&output, svg.as_bytes(), overwrite)?;
        }
        Command::Timeline {
            comparison,
            output,
            orientation,
            title,
            natal_name,
            natal_datetime,
            natal_location,
            transit_name,
            transit_datetime,
            transit_location,
            overwrite,
        } => {
            let mut artifacts = comparison
                .iter()
                .map(|path| {
                    let artifact = read_comparison(path)?;
                    let scene = ChartScene::from_comparison(&artifact)?;
                    let timestamp = DateTime::parse_from_rfc3339(&scene.timestamp)
                        .expect("scene validates its timestamp")
                        .with_timezone(&Utc);
                    Ok((timestamp, artifact))
                })
                .collect::<Result<Vec<_>, CliError>>()?;
            artifacts.sort_by_key(|(timestamp, _)| *timestamp);
            let artifacts: Vec<_> = artifacts
                .into_iter()
                .map(|(_, artifact)| artifact)
                .collect();
            let timeline = TransitTimeline::from_comparisons(&artifacts)?;
            let mut options = PlayerOptions::new(title, orientation.into());
            options.natal = ChartInformation {
                name: natal_name,
                location: natal_location,
                local_datetime: natal_datetime,
            };
            options.transit = ChartInformation {
                name: transit_name,
                location: transit_location,
                local_datetime: transit_datetime,
            };
            let html = render_player_html_with_options(&timeline, &options)?;
            write_private_atomic(&output, html.as_bytes(), overwrite)?;
        }
    }
    Ok(())
}

fn read_comparison(path: &PathBuf) -> Result<ComparisonArtifact, CliError> {
    let input = fs::read_to_string(path).map_err(|source| CliError::Read {
        path: path.clone(),
        source,
    })?;
    ComparisonArtifact::from_json(&input).map_err(|error| CliError::InvalidArtifact {
        path: path.clone(),
        message: error.to_string(),
    })
}
