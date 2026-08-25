mod client;
mod completions;
mod daemon;
mod favorites_cache;
mod i18n;
#[cfg(target_os = "linux")]
mod mpris;
mod protocol;
mod theme;
mod ui;
mod web_login;

use std::fs;

use anyhow::Result;
use tracing_subscriber::EnvFilter;

use crate::protocol::{send_line, socket_path, Command};

/// Initialize file-based logging (no-op if RUST_LOG is not set).
fn init_logging(path: &str) {
    if std::env::var("RUST_LOG").is_ok() {
        if let Ok(log_file) = fs::File::create(path) {
            let _ = tracing_subscriber::fmt()
                .with_env_filter(EnvFilter::from_default_env())
                .with_target(false)
                .with_file(true)
                .with_line_number(true)
                .with_writer(log_file)
                .with_ansi(false)
                .try_init();
        }
    }
}

fn print_help() {
    println!("deezer-tui — Terminal-based Deezer player");
    println!();
    println!("Usage: deezer-tui [OPTIONS]");
    println!();
    println!("Options:");
    println!("  -p, --toggle              Toggle play/pause");
    println!("      --play                Resume playback");
    println!("      --pause               Pause playback");
    println!("      --stop                Stop playback");
    println!("  -n, --next                Skip to next track");
    println!("  -b, --prev                Go to previous track");
    println!("  -s, --status              Show current playback status");
    println!("      --json                Format status output as JSON");
    println!("      --volume <0-100>      Set volume percentage");
    println!("      --volume-up [STEP]    Increase volume (default: 5%)");
    println!("      --volume-down [STEP]  Decrease volume (default: 5%)");
    println!("      --seek <SECS>         Seek to absolute position in seconds");
    println!("      --seek-forward [SECS] Seek forward by seconds (default: 5s)");
    println!("      --seek-backward [SECS]Seek backward by seconds (default: 5s)");
    println!("      --shuffle             Toggle shuffle mode");
    println!("      --repeat              Cycle repeat mode (off -> queue -> track)");
    println!("      --like                Add currently playing track to favorites");
    println!("      --dislike             Dislike currently playing track");
    println!("      --completions <SHELL> Generate shell completions (bash, zsh, fish)");
    println!("  -q, --quit                Stop the daemon");
    println!("  -v, --version             Show version info");
    println!("  -h, --help                Show this help message");
}

fn print_version() {
    println!(
        "deezer-tui {} ({}/{})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
    );
    println!("License: WTFPL");
    println!("Author:  Tatayoyoh");
    println!("GitHub:  https://github.com/Tatayoyoh/deezer-tui");
}

fn main() -> Result<()> {
    // Initialize i18n: config override > system locale > English
    let config = deezer_core::Config::load();
    let locale = config
        .language
        .as_deref()
        .and_then(i18n::Locale::from_str)
        .unwrap_or_else(i18n::detect_locale);
    i18n::set(locale);

    // Check for flags
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        return Ok(());
    }

    if args.iter().any(|a| a == "-v" || a == "--version") {
        print_version();
        return Ok(());
    }

    // Shell completions generator
    if let Some(pos) = args.iter().position(|a| a == "--completions") {
        if let Some(shell) = args.get(pos + 1) {
            completions::generate_completions(shell);
            return Ok(());
        } else {
            eprintln!("Error: --completions requires a shell argument (bash, zsh, fish)");
            std::process::exit(1);
        }
    }

    if args.iter().any(|a| a == "-q" || a == "--quit") {
        return handle_quit();
    }

    // Status query
    if args
        .iter()
        .any(|a| a == "-s" || a == "--status" || a == "--current")
    {
        let json = args.iter().any(|a| a == "--json" || a == "-j");
        return handle_status(json);
    }

    // Volume controls
    if let Some(pos) = args.iter().position(|a| a == "--volume") {
        if let Some(val_str) = args.get(pos + 1) {
            if let Ok(val) = val_str.parse::<f32>() {
                let volume = (val / 100.0).clamp(0.0, 1.0);
                return send_command_to_daemon(Command::SetVolume { volume });
            } else {
                eprintln!("Error: --volume requires a number between 0 and 100");
                std::process::exit(1);
            }
        } else {
            eprintln!("Error: --volume requires a percentage value (0-100)");
            std::process::exit(1);
        }
    }

    if let Some(pos) = args.iter().position(|a| a == "--volume-up") {
        let step = args
            .get(pos + 1)
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(5.0)
            / 100.0;
        if let Ok(Some(s)) = fetch_daemon_snapshot() {
            let volume = (s.volume + step).clamp(0.0, 1.0);
            return send_command_to_daemon(Command::SetVolume { volume });
        } else {
            eprintln!("deezer-tui: no daemon running");
            return Ok(());
        }
    }

    if let Some(pos) = args.iter().position(|a| a == "--volume-down") {
        let step = args
            .get(pos + 1)
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or(5.0)
            / 100.0;
        if let Ok(Some(s)) = fetch_daemon_snapshot() {
            let volume = (s.volume - step).clamp(0.0, 1.0);
            return send_command_to_daemon(Command::SetVolume { volume });
        } else {
            eprintln!("deezer-tui: no daemon running");
            return Ok(());
        }
    }

    // Seek controls
    if let Some(pos) = args.iter().position(|a| a == "--seek") {
        if let Some(secs_str) = args.get(pos + 1) {
            if let Ok(secs) = secs_str.parse::<u64>() {
                return send_command_to_daemon(Command::SeekAbsolute { secs });
            }
        }
        eprintln!("Error: --seek requires seconds as a positive integer");
        std::process::exit(1);
    }

    if let Some(pos) = args.iter().position(|a| a == "--seek-forward") {
        let secs = args
            .get(pos + 1)
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(5);
        return send_command_to_daemon(Command::SeekForward { secs });
    }

    if let Some(pos) = args.iter().position(|a| a == "--seek-backward") {
        let secs = args
            .get(pos + 1)
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(5);
        return send_command_to_daemon(Command::SeekBackward { secs });
    }

    // Playback controls
    if args.iter().any(|a| a == "--play") {
        if let Ok(Some(s)) = fetch_daemon_snapshot() {
            if s.status != deezer_core::player::state::PlaybackStatus::Playing {
                return send_command_to_daemon(Command::TogglePause);
            }
            return Ok(());
        } else {
            eprintln!("deezer-tui: no daemon running");
            return Ok(());
        }
    }

    if args.iter().any(|a| a == "--pause") {
        if let Ok(Some(s)) = fetch_daemon_snapshot() {
            if s.status == deezer_core::player::state::PlaybackStatus::Playing {
                return send_command_to_daemon(Command::TogglePause);
            }
            return Ok(());
        } else {
            eprintln!("deezer-tui: no daemon running");
            return Ok(());
        }
    }

    if args.iter().any(|a| a == "--stop") {
        return send_command_to_daemon(Command::Stop);
    }

    if args.iter().any(|a| a == "-n" || a == "--next") {
        return send_command_to_daemon(Command::NextTrack);
    }
    if args.iter().any(|a| a == "-b" || a == "--prev") {
        return send_command_to_daemon(Command::PrevTrack);
    }
    if args.iter().any(|a| a == "-p" || a == "--toggle") {
        return send_command_to_daemon(Command::TogglePause);
    }
    if args
        .iter()
        .any(|a| a == "--shuffle" || a == "--toggle-shuffle")
    {
        return send_command_to_daemon(Command::ToggleShuffle);
    }
    if args
        .iter()
        .any(|a| a == "--repeat" || a == "--cycle-repeat")
    {
        return send_command_to_daemon(Command::CycleRepeat);
    }

    if args.iter().any(|a| a == "--like") {
        if let Ok(Some(s)) = fetch_daemon_snapshot() {
            if let Some(track) = s.current_track {
                return send_command_to_daemon(Command::AddFavorite {
                    track_id: track.track_id,
                });
            } else {
                eprintln!("deezer-tui: no track is currently playing");
                return Ok(());
            }
        } else {
            eprintln!("deezer-tui: no daemon running");
            return Ok(());
        }
    }

    if args.iter().any(|a| a == "--dislike") {
        if let Ok(Some(s)) = fetch_daemon_snapshot() {
            if let Some(track) = s.current_track {
                return send_command_to_daemon(Command::DislikeTrack {
                    track_id: track.track_id,
                });
            } else {
                eprintln!("deezer-tui: no track is currently playing");
                return Ok(());
            }
        } else {
            eprintln!("deezer-tui: no daemon running");
            return Ok(());
        }
    }

    let show_updated = args.iter().any(|a| a == "--updated");

    // Try to connect to an existing daemon
    let sock_path = socket_path();
    if try_connect_sync(&sock_path) {
        // Daemon is running — launch as client
        init_logging("/tmp/deezer-tui.log");
        let rt = build_client_runtime()?;
        rt.block_on(async {
            let mut client = client::Client::connect().await?;
            client.run(show_updated).await
        })
    } else {
        // No daemon running — fork: child becomes daemon, parent becomes client
        #[cfg(unix)]
        {
            start_with_fork(show_updated)
        }
        #[cfg(not(unix))]
        {
            // On non-Unix, just run daemon in-process (no background support)
            let rt = build_daemon_runtime()?;
            rt.block_on(async {
                let mut d = daemon::Daemon::new()?;
                d.run().await
            })
        }
    }
}

/// Build a runtime for the TUI client and one-shot commands.
/// Needs a real worker thread: the main loop blocks the thread it runs on
/// inside crossterm's synchronous `event::poll`, which would otherwise starve
/// the background daemon-socket reader task on a current-thread runtime.
fn build_client_runtime() -> Result<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()?)
}

/// Build a multi-thread runtime for the daemon with a limited worker pool.
/// 2 workers is enough for concurrent API calls + background tasks.
fn build_daemon_runtime() -> Result<tokio::runtime::Runtime> {
    Ok(tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?)
}

/// Fetch a single snapshot from the daemon (if alive).
fn fetch_daemon_snapshot() -> Result<Option<crate::protocol::DaemonSnapshot>> {
    let sock_path = socket_path();
    if !sock_path.exists() {
        return Ok(None);
    }

    let rt = build_client_runtime()?;
    rt.block_on(async {
        use crate::protocol::read_line;
        match tokio::net::UnixStream::connect(&sock_path).await {
            Ok(stream) => {
                let (read_half, _write_half) = stream.into_split();
                let mut reader = tokio::io::BufReader::new(read_half);
                let snap = tokio::time::timeout(
                    std::time::Duration::from_secs(2),
                    read_line::<crate::protocol::ServerMessage, _>(&mut reader),
                )
                .await;

                match snap {
                    Ok(Ok(Some(crate::protocol::ServerMessage::Snapshot(s)))) => Ok(Some(s)),
                    _ => Ok(None),
                }
            }
            Err(_) => Ok(None),
        }
    })
}

fn format_time(secs: u64) -> String {
    let m = secs / 60;
    let s = secs % 60;
    format!("{m:02}:{s:02}")
}

/// Handle `deezer-tui -s` / `--status`: query daemon and format now-playing status.
fn handle_status(json: bool) -> Result<()> {
    match fetch_daemon_snapshot()? {
        Some(s) => {
            if json {
                let json_val = serde_json::json!({
                    "status": match s.status {
                        deezer_core::player::state::PlaybackStatus::Playing => "playing",
                        deezer_core::player::state::PlaybackStatus::Paused => "paused",
                        deezer_core::player::state::PlaybackStatus::Stopped => "stopped",
                        deezer_core::player::state::PlaybackStatus::Loading => "loading",
                    },
                    "track": s.current_track.as_ref().map(|t| serde_json::json!({
                        "id": t.track_id,
                        "title": t.title,
                        "artist": t.artist,
                        "album": t.album,
                        "duration": t.duration,
                    })),
                    "position_secs": s.position_secs,
                    "duration_secs": s.duration_secs,
                    "volume": s.volume,
                    "volume_percent": (s.volume * 100.0).round() as u32,
                    "shuffle": s.shuffle,
                    "repeat": match s.repeat {
                        deezer_core::player::state::RepeatMode::Off => "off",
                        deezer_core::player::state::RepeatMode::Queue => "queue",
                        deezer_core::player::state::RepeatMode::Track => "track",
                    },
                    "quality": format!("{:?}", s.quality),
                });
                println!("{}", serde_json::to_string_pretty(&json_val)?);
            } else {
                match (&s.status, &s.current_track) {
                    (deezer_core::player::state::PlaybackStatus::Playing, Some(t)) => {
                        let pos = format_time(s.position_secs);
                        let dur = format_time(s.duration_secs);
                        println!("▶ {} — {} [{}/{}]", t.title, t.artist, pos, dur);
                    }
                    (deezer_core::player::state::PlaybackStatus::Paused, Some(t)) => {
                        let pos = format_time(s.position_secs);
                        let dur = format_time(s.duration_secs);
                        println!("⏸ {} — {} [{}/{}]", t.title, t.artist, pos, dur);
                    }
                    (deezer_core::player::state::PlaybackStatus::Loading, Some(t)) => {
                        println!("⏳ {} — {}", t.title, t.artist);
                    }
                    _ => {
                        println!("⏹ Stopped");
                    }
                }
            }
        }
        None => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({ "status": "offline", "error": "no daemon running" })
                );
            } else {
                eprintln!("deezer-tui: no daemon running");
            }
        }
    }
    Ok(())
}

/// Send a single command to the daemon and exit.
fn send_command_to_daemon(cmd: Command) -> Result<()> {
    let sock_path = socket_path();
    if !sock_path.exists() {
        eprintln!("deezer-tui: no daemon running");
        return Ok(());
    }

    let rt = build_client_runtime()?;
    rt.block_on(async {
        match tokio::net::UnixStream::connect(&sock_path).await {
            Ok(mut stream) => {
                if let Err(e) = send_line(&mut stream, &cmd).await {
                    eprintln!("deezer-tui: failed to send command: {e}");
                }
            }
            Err(_) => {
                eprintln!("deezer-tui: no daemon running");
            }
        }
        Ok(())
    })
}

/// Handle `deezer-tui -q` / `--quit`: connect to daemon and send shutdown.
fn handle_quit() -> Result<()> {
    let sock_path = socket_path();
    if !sock_path.exists() {
        eprintln!("deezer-tui: no daemon running");
        return Ok(());
    }

    let rt = build_client_runtime()?;
    rt.block_on(async {
        use tokio::io::AsyncReadExt;
        match tokio::net::UnixStream::connect(&sock_path).await {
            Ok(mut stream) => {
                if let Err(e) = send_line(&mut stream, &Command::Shutdown).await {
                    eprintln!("deezer-tui: failed to send shutdown: {e}");
                    return Ok(());
                }
                // Drain all data until EOF (daemon sends snapshots before closing)
                let _ = tokio::time::timeout(std::time::Duration::from_secs(3), async {
                    let mut buf = [0u8; 4096];
                    loop {
                        match stream.read(&mut buf).await {
                            Ok(0) => break, // EOF — daemon closed
                            Ok(_) => continue,
                            Err(_) => break,
                        }
                    }
                })
                .await;
                eprintln!("deezer-tui: daemon stopped");
            }
            Err(_) => {
                eprintln!("deezer-tui: no daemon running");
            }
        }
        Ok(())
    })
}

/// Check if we can connect to the daemon socket (synchronous).
fn try_connect_sync(sock_path: &std::path::Path) -> bool {
    if !sock_path.exists() {
        return false;
    }
    // Try a synchronous connect to check if daemon is alive
    match std::os::unix::net::UnixStream::connect(sock_path) {
        Ok(_stream) => {
            // Connected — daemon is alive.
            // Drop the stream immediately (we'll reconnect async).
            true
        }
        Err(_) => {
            // Stale socket file — clean up
            let _ = std::fs::remove_file(sock_path);
            false
        }
    }
}

/// Fork: child becomes daemon, parent waits then launches as client.
#[cfg(unix)]
fn start_with_fork(show_updated: bool) -> Result<()> {
    let sock_path = socket_path();

    match unsafe { libc::fork() } {
        -1 => {
            anyhow::bail!("fork() failed");
        }
        0 => {
            // === CHILD: become daemon ===
            unsafe { libc::setsid() };

            // Redirect stdin/stdout/stderr to /dev/null
            let devnull = std::fs::File::open("/dev/null")?;
            use std::os::unix::io::AsRawFd;
            unsafe {
                libc::dup2(devnull.as_raw_fd(), 0); // stdin
                libc::dup2(devnull.as_raw_fd(), 1); // stdout
                libc::dup2(devnull.as_raw_fd(), 2); // stderr
            }

            // Initialize daemon logging to its own file (after fork)
            init_logging("/tmp/deezer-daemon.log");

            // Build tokio runtime AFTER fork (no inherited threads)
            let rt = build_daemon_runtime()?;
            rt.block_on(async {
                match daemon::Daemon::new() {
                    Ok(mut d) => {
                        if let Err(e) = d.run().await {
                            // Can't print, we redirected stderr — just exit
                            let _ = e;
                        }
                    }
                    Err(_) => {}
                }
            });

            // Clean exit
            std::process::exit(0);
        }
        _child_pid => {
            // === PARENT: wait for daemon socket, then run as client ===
            init_logging("/tmp/deezer-tui.log");

            // Wait for the daemon to start listening (up to 3 seconds)
            for _ in 0..60 {
                std::thread::sleep(std::time::Duration::from_millis(50));
                if sock_path.exists() && try_connect_sync(&sock_path) {
                    break;
                }
            }

            if !try_connect_sync(&sock_path) {
                anyhow::bail!("Daemon failed to start (socket not available)");
            }

            // Run as client
            let rt = build_client_runtime()?;
            rt.block_on(async {
                let mut client = client::Client::connect().await?;
                client.run(show_updated).await
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_time() {
        assert_eq!(format_time(0), "00:00");
        assert_eq!(format_time(9), "00:09");
        assert_eq!(format_time(59), "00:59");
        assert_eq!(format_time(60), "01:00");
        assert_eq!(format_time(65), "01:05");
        assert_eq!(format_time(3600), "60:00");
    }
}
