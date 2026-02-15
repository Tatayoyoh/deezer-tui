mod app;
mod event;
mod theme;
mod ui;

use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use tracing_subscriber::EnvFilter;

use app::App;
use deezer_core::Config;

/// Path to the PID file: `~/.config/deezer-tui/deezer-tui.pid`
fn pid_file_path() -> Option<PathBuf> {
    Config::dir().map(|d| d.join("deezer-tui.pid"))
}

/// Write current process PID to the PID file.
fn write_pid_file() {
    if let Some(path) = pid_file_path() {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&path, std::process::id().to_string());
    }
}

/// Remove the PID file on clean exit.
fn remove_pid_file() {
    if let Some(path) = pid_file_path() {
        let _ = fs::remove_file(path);
    }
}

/// Check if another deezer-tui instance is running.
/// If so, send it SIGUSR1 to bring it back to foreground and return true.
#[cfg(unix)]
fn check_existing_instance() -> bool {
    let Some(path) = pid_file_path() else {
        return false;
    };

    let Ok(content) = fs::read_to_string(&path) else {
        return false;
    };

    let Ok(pid) = content.trim().parse::<i32>() else {
        return false;
    };

    // Check if process is alive (signal 0 = test only)
    if unsafe { libc::kill(pid, 0) } != 0 {
        // Process doesn't exist — stale PID file, remove it
        let _ = fs::remove_file(&path);
        return false;
    }

    // Process is alive — send SIGUSR1 to bring it to foreground
    eprintln!("deezer-tui: signaling existing instance (PID {pid}) to restore TUI...");
    unsafe {
        libc::kill(pid, libc::SIGUSR1);
    }
    true
}

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

    // If another instance is running in background, signal it and exit
    #[cfg(unix)]
    if check_existing_instance() {
        return Ok(());
    }

    write_pid_file();

    let mut app = App::new()?;
    let result = app.run().await;

    remove_pid_file();

    result
}
