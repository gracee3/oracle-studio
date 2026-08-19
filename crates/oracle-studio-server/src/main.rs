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
    let listener = bind_loopback(0).await?;
    let address = listener.local_addr()?;
    validate_loopback(address)?;
    let origin = format!("http://{address}");
    let token = launch_token()?;
    let state = AppState::new(&origin, token.as_str(), DEFAULT_IDLE_TIMEOUT)?;
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
