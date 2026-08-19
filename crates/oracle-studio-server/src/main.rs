use std::{path::PathBuf, time::Duration};

use clap::Parser;
use oracle_studio_server::{
    AppState, DEFAULT_IDLE_TIMEOUT, app, bind_loopback, launch_token, validate_loopback,
};

#[derive(Debug, Parser)]
#[command(about = "Serve Oracle Studio only on the local loopback interface")]
struct Args {
    #[arg(long, default_value = "crates/oracle-studio-ui/dist")]
    dist: PathBuf,
    /// Loopback port to use. Zero asks the operating system for a random port.
    #[arg(long, default_value_t = 0)]
    port: u16,
    /// Store the unencrypted public GeoNames catalog here instead of the XDG data directory.
    #[arg(long)]
    catalog_dir: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if !args.dist.join("index.html").is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "Studio distribution is missing at {}; build the UI with Trunk first",
                args.dist.display()
            ),
        )
        .into());
    }
    let listener = bind_loopback(args.port).await?;
    let address = listener.local_addr()?;
    validate_loopback(address)?;
    let origin = format!("http://{address}");
    let token = launch_token()?;
    let state = match args.catalog_dir {
        Some(catalog_dir) => {
            AppState::with_catalog_root(&origin, token.as_str(), DEFAULT_IDLE_TIMEOUT, catalog_dir)?
        }
        None => AppState::new(&origin, token.as_str(), DEFAULT_IDLE_TIMEOUT)?,
    };
    println!(
        "Oracle Studio is ready at {origin}/#token={}",
        token.as_str()
    );
    println!(
        "The bearer token is valid only for this process; the vault locks after {} minutes of inactivity.",
        Duration::from_secs(DEFAULT_IDLE_TIMEOUT.as_secs()).as_secs() / 60
    );
    axum::serve(listener, app(state, args.dist)).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_an_explicit_loopback_port_for_managed_tunnels() {
        let args = Args::try_parse_from([
            "oracle-studio-host",
            "--port",
            "40369",
            "--dist",
            "studio-dist",
        ])
        .unwrap();

        assert_eq!(args.port, 40_369);
        assert_eq!(args.dist, PathBuf::from("studio-dist"));
    }

    #[test]
    fn keeps_random_port_selection_as_the_default() {
        let args = Args::try_parse_from(["oracle-studio-host"]).unwrap();
        assert_eq!(args.port, 0);
    }
}
