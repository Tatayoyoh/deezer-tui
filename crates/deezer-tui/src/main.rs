mod app;
mod event;
mod theme;
mod ui;

use std::fs;

use anyhow::Result;
use tracing_subscriber::EnvFilter;

use app::App;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging to file (stdout/stderr conflict with TUI)
    // Set RUST_LOG=debug to enable traces, logs go to /tmp/deezer-tui.log
    if std::env::var("RUST_LOG").is_ok() {
        let log_file = fs::File::create("/tmp/deezer-tui.log")?;
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .with_target(false)
            .with_file(true)
            .with_line_number(true)
            .with_writer(log_file)
            .with_ansi(false)
            .init();
    }

    let mut app = App::new()?;
    app.run().await?;

    Ok(())
}
