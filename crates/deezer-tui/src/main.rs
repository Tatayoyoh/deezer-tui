mod client;
mod daemon;
mod protocol;
mod theme;
mod ui;
mod web_login;

use std::fs;

use anyhow::Result;
use tracing_subscriber::EnvFilter;

use crate::protocol::{socket_path, Command, send_line};

fn main() -> Result<()> {
    // Initialize logging to file (stdout/stderr conflict with TUI)
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

    // Check for --quit / -q flag
    let args: Vec<String> = std::env::args().collect();
    let quit_mode = args.iter().any(|a| a == "-q" || a == "--quit");

    if quit_mode {
        return handle_quit();
    }

    // Try to connect to an existing daemon
    let sock_path = socket_path();
    if try_connect_sync(&sock_path) {
        // Daemon is running — launch as client
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            let mut client = client::Client::connect().await?;
            client.run().await
        })
    } else {
        // No daemon running — fork: child becomes daemon, parent becomes client
        #[cfg(unix)]
        {
            start_with_fork()
        }
        #[cfg(not(unix))]
        {
            // On non-Unix, just run daemon in-process (no background support)
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async {
                let mut d = daemon::Daemon::new()?;
                d.run().await
            })
        }
    }
}

/// Handle `deezer-tui -q` / `--quit`: connect to daemon and send shutdown.
fn handle_quit() -> Result<()> {
    let sock_path = socket_path();
    if !sock_path.exists() {
        eprintln!("deezer-tui: no daemon running");
        return Ok(());
    }

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        use tokio::io::AsyncReadExt;
        match tokio::net::UnixStream::connect(&sock_path).await {
            Ok(mut stream) => {
                if let Err(e) = send_line(&mut stream, &Command::Shutdown).await {
                    eprintln!("deezer-tui: failed to send shutdown: {e}");
                    return Ok(());
                }
                // Drain all data until EOF (daemon sends snapshots before closing)
                let _ = tokio::time::timeout(
                    std::time::Duration::from_secs(3),
                    async {
                        let mut buf = [0u8; 4096];
                        loop {
                            match stream.read(&mut buf).await {
                                Ok(0) => break, // EOF — daemon closed
                                Ok(_) => continue,
                                Err(_) => break,
                            }
                        }
                    },
                ).await;
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
fn start_with_fork() -> Result<()> {
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

            // Build tokio runtime AFTER fork (no inherited threads)
            let rt = tokio::runtime::Runtime::new()?;
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
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async {
                let mut client = client::Client::connect().await?;
                client.run().await
            })
        }
    }
}
