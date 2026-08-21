use std::{env, path::PathBuf, process::ExitCode};

use oracle_studio_demo::{build_demo_bundle, generate, verify};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut arguments = env::args().skip(1);
    match arguments.next().as_deref() {
        Some("generate") => {
            let output = required_path(arguments.next(), "generate requires an output directory")?;
            reject_extra(arguments)?;
            let manifest = generate(&output).map_err(|error| error.to_string())?;
            println!(
                "generated {} with {} charts and {} comparisons",
                output.display(),
                manifest.charts,
                manifest.comparisons
            );
            Ok(())
        }
        Some("verify") => {
            let lock = required_path(arguments.next(), "verify requires a lock path")?;
            reject_extra(arguments)?;
            let manifest = verify(&lock).map_err(|error| error.to_string())?;
            println!("verified {} ({})", manifest.title, manifest.document_sha256);
            Ok(())
        }
        Some("manifest") => {
            reject_extra(arguments)?;
            let bundle = build_demo_bundle().map_err(|error| error.to_string())?;
            println!(
                "{}",
                serde_json::to_string_pretty(&bundle.manifest).map_err(|error| error.to_string())?
            );
            Ok(())
        }
        _ => Err("usage: oracle-studio-demo <generate OUTPUT|verify LOCK|manifest>".into()),
    }
}

fn required_path(value: Option<String>, message: &str) -> Result<PathBuf, String> {
    value.map(PathBuf::from).ok_or_else(|| message.into())
}

fn reject_extra(mut arguments: impl Iterator<Item = String>) -> Result<(), String> {
    if arguments.next().is_some() {
        Err("unexpected extra argument".into())
    } else {
        Ok(())
    }
}
