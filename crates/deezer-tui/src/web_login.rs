//! Browser-based login flow for Deezer.
//!
//! Opens the system browser to Deezer's desktop login callback endpoint.
//! After the user logs in, Deezer presents a `deezer://autolog/<ARL>` link.
//! We register a temporary URI scheme handler to capture the ARL automatically,
//! with a manual paste fallback if the handler doesn't work.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tracing::{debug, info, warn};

const LOGIN_URL: &str = "https://www.deezer.com/desktop/login/electron/callback";
const TIMEOUT: Duration = Duration::from_secs(300); // 5 minutes
const POLL_INTERVAL: Duration = Duration::from_millis(200);
const DESKTOP_FILE_NAME: &str = "deezer-tui-auth.desktop";
const MIME_TYPE: &str = "x-scheme-handler/deezer";

/// Run the browser-based login flow. Returns `Some(arl)` on success, `None` if cancelled.
///
/// This function is blocking and should be called with the terminal suspended
/// (raw mode disabled, alternate screen left).
pub fn login_via_browser() -> Result<Option<String>> {
    let auth_file = temp_auth_file_path();

    // Clean up any stale auth file
    let _ = fs::remove_file(&auth_file);

    // Set up the URI handler
    let old_handler = setup_uri_handler(&auth_file)
        .inspect_err(|e| warn!("Failed to set up URI handler: {e}"))
        .ok()
        .flatten();

    // Open browser
    println!("\n  Opening browser for Deezer login...\n");
    if let Err(e) = open_browser(LOGIN_URL) {
        println!("  Could not open browser automatically: {e}");
        println!("  Please open this URL manually:");
        println!("  {LOGIN_URL}\n");
    }

    println!("  Log in to your Deezer account in the browser.");
    println!("  After login, click the \"Open Deezer\" link.\n");
    println!("  If the link doesn't work, copy the deezer://autolog/... URL");
    println!("  and paste it here:\n");
    print!("  > ");
    io::stdout().flush()?;

    // Wait for ARL from either the handler (file) or stdin (paste)
    let arl = wait_for_arl(&auth_file);

    // Cleanup
    cleanup_uri_handler(old_handler.as_deref());
    let _ = fs::remove_file(&auth_file);

    match arl {
        Some(arl) => {
            info!("ARL obtained via browser login ({} chars)", arl.len());
            println!("\n  Login token received!");
            Ok(Some(arl))
        }
        None => {
            println!("\n  Login cancelled or timed out.");
            Ok(None)
        }
    }
}

/// Parse an ARL from various input formats:
/// - `deezer://autolog/<ARL>`
/// - `autolog/<ARL>`
/// - Raw ARL (192 hex chars)
fn parse_arl(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Try stripping known prefixes
    let arl = trimmed
        .strip_prefix("deezer://autolog/")
        .or_else(|| trimmed.strip_prefix("autolog/"))
        .unwrap_or(trimmed);

    let arl = arl.trim();

    // Validate: should be a hex string, typically 192 chars
    if arl.len() >= 128 && arl.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(arl.to_string())
    } else {
        debug!(
            "Invalid ARL format: len={}, input='{}'",
            arl.len(),
            &trimmed[..trimmed.len().min(40)]
        );
        None
    }
}

/// Generate a unique temp file path for receiving the ARL.
fn temp_auth_file_path() -> PathBuf {
    let pid = std::process::id();
    PathBuf::from(format!("/tmp/deezer-tui-auth-{pid}"))
}

/// Path to the temporary .desktop file.
fn desktop_file_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home)
        .join(".local/share/applications")
        .join(DESKTOP_FILE_NAME)
}

/// Path to the temporary handler script.
fn handler_script_path() -> PathBuf {
    PathBuf::from(format!(
        "/tmp/deezer-tui-auth-handler-{}.sh",
        std::process::id()
    ))
}

/// Path to `~/.config/mimeapps.list`.
fn mimeapps_list_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".config/mimeapps.list")
}

/// Path to our backup of `mimeapps.list`.
fn mimeapps_backup_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".config/mimeapps.list.deezer-tui-bak")
}

/// Set up a temporary URI scheme handler for `deezer://`.
/// Returns the name of the previous handler (if any) for restoration.
///
/// To prevent other apps (e.g. deezer-linux) from also opening, we:
/// 1. Register our handler as default via `xdg-mime`
/// 2. Add competing handlers to `[Removed Associations]` in `mimeapps.list`
/// 3. Backup the original `mimeapps.list` for restoration on cleanup
fn setup_uri_handler(auth_file: &Path) -> Result<Option<String>> {
    // Query current handler
    let old_handler = Command::new("xdg-mime")
        .args(["query", "default", MIME_TYPE])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                if s.is_empty() {
                    None
                } else {
                    Some(s)
                }
            } else {
                None
            }
        });

    debug!("Previous deezer:// handler: {:?}", old_handler);

    // Create handler script
    let script_path = handler_script_path();
    let script_content = format!(
        "#!/bin/sh\nARL=\"${{1#deezer://autolog/}}\"\necho \"$ARL\" > '{}'\n",
        auth_file.display()
    );
    fs::write(&script_path, &script_content).context("Failed to write handler script")?;

    // Make executable
    Command::new("chmod")
        .args(["+x", &script_path.to_string_lossy()])
        .output()
        .context("Failed to chmod handler script")?;

    // Create .desktop file
    let desktop_path = desktop_file_path();
    if let Some(parent) = desktop_path.parent() {
        fs::create_dir_all(parent).context("Failed to create applications dir")?;
    }

    let desktop_content = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Deezer TUI Auth\n\
         Exec={} %u\n\
         MimeType=x-scheme-handler/deezer;\n\
         NoDisplay=true\n\
         Terminal=false\n",
        script_path.display()
    );
    fs::write(&desktop_path, &desktop_content).context("Failed to write .desktop file")?;

    // Update desktop database
    if let Some(parent) = desktop_path.parent() {
        let _ = Command::new("update-desktop-database").arg(parent).output();
    }

    // Register as default handler
    Command::new("xdg-mime")
        .args(["default", DESKTOP_FILE_NAME, MIME_TYPE])
        .output()
        .context("Failed to register URI handler")?;

    // Block competing handlers via [Removed Associations] in mimeapps.list.
    // This prevents deezer-linux (or any other app) from also opening.
    if let Some(ref old) = old_handler {
        block_competing_handler(old);
    }

    debug!("Registered deezer:// URI handler");
    Ok(old_handler)
}

/// Add a handler to `[Removed Associations]` in `~/.config/mimeapps.list`
/// so the desktop environment won't use it as a fallback.
/// Backs up the original file first.
fn block_competing_handler(handler_name: &str) {
    let mimeapps = mimeapps_list_path();
    let backup = mimeapps_backup_path();

    // Backup original
    if mimeapps.exists() {
        if let Err(e) = fs::copy(&mimeapps, &backup) {
            warn!("Failed to backup mimeapps.list: {e}");
            return;
        }
    }

    // Read current content
    let content = fs::read_to_string(&mimeapps).unwrap_or_default();

    // Build the removal entry
    let removal_entry = format!("{MIME_TYPE}={handler_name}");

    // Check if [Removed Associations] section exists
    if let Some(pos) = content.find("[Removed Associations]") {
        // Find the end of the section header line
        let after_header = pos + "[Removed Associations]".len();
        let insert_pos = content[after_header..]
            .find('\n')
            .map(|p| after_header + p + 1)
            .unwrap_or(content.len());

        // Check if entry already present
        if !content.contains(&removal_entry) {
            let mut new_content = String::with_capacity(content.len() + removal_entry.len() + 1);
            new_content.push_str(&content[..insert_pos]);
            new_content.push_str(&removal_entry);
            new_content.push('\n');
            new_content.push_str(&content[insert_pos..]);
            let _ = fs::write(&mimeapps, new_content);
        }
    } else {
        // Append new section
        let mut new_content = content;
        if !new_content.ends_with('\n') && !new_content.is_empty() {
            new_content.push('\n');
        }
        new_content.push_str("\n[Removed Associations]\n");
        new_content.push_str(&removal_entry);
        new_content.push('\n');
        let _ = fs::write(&mimeapps, new_content);
    }

    debug!("Blocked competing handler {handler_name} in mimeapps.list");
}

/// Remove the temporary handler and restore the previous state.
fn cleanup_uri_handler(old_handler: Option<&str>) {
    // Remove temp files
    let _ = fs::remove_file(handler_script_path());
    let _ = fs::remove_file(desktop_file_path());

    // Restore original mimeapps.list (undoes both our default + removed associations)
    let mimeapps = mimeapps_list_path();
    let backup = mimeapps_backup_path();
    if backup.exists() {
        let _ = fs::copy(&backup, &mimeapps);
        let _ = fs::remove_file(&backup);
        debug!("Restored mimeapps.list from backup");
    } else if let Some(handler) = old_handler {
        // No backup — just restore the old default
        let _ = Command::new("xdg-mime")
            .args(["default", handler, MIME_TYPE])
            .output();
        debug!("Restored previous deezer:// handler: {handler}");
    }

    // Update desktop database
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let apps_dir = PathBuf::from(home).join(".local/share/applications");
    let _ = Command::new("update-desktop-database")
        .arg(&apps_dir)
        .output();
}

/// Open a URL in the system browser.
fn open_browser(url: &str) -> Result<()> {
    // Try xdg-open first (standard on Linux desktops)
    let browsers = [
        "xdg-open",
        "firefox",
        "chromium",
        "chromium-browser",
        "google-chrome",
    ];

    for browser in &browsers {
        match Command::new(browser).arg(url).spawn() {
            Ok(_) => {
                debug!("Opened browser with {browser}");
                return Ok(());
            }
            Err(_) => continue,
        }
    }

    anyhow::bail!("No browser found")
}

/// Check if stdin has data available (non-blocking).
fn stdin_has_data() -> bool {
    use std::os::unix::io::AsRawFd;
    let fd = io::stdin().as_raw_fd();
    let mut pollfd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    unsafe { libc::poll(&mut pollfd, 1, 0) > 0 }
}

/// Read available bytes from stdin without blocking.
/// Uses raw `libc::read` to avoid interfering with Rust's internal stdin buffer,
/// which would conflict with crossterm's event reader after we resume the TUI.
fn read_stdin_nonblocking(buf: &mut Vec<u8>) -> usize {
    use std::os::unix::io::AsRawFd;
    let fd = io::stdin().as_raw_fd();
    let mut tmp = [0u8; 4096];
    let n = unsafe { libc::read(fd, tmp.as_mut_ptr() as *mut libc::c_void, tmp.len()) };
    if n > 0 {
        buf.extend_from_slice(&tmp[..n as usize]);
        n as usize
    } else {
        0
    }
}

/// Wait for ARL from either the auth file (URI handler) or stdin (manual paste).
/// Returns `None` on timeout or cancellation (empty input / Ctrl+D).
///
/// Uses non-blocking stdin reads via `libc::poll`/`libc::read` to avoid spawning
/// threads that would linger and steal input from crossterm after the TUI resumes.
fn wait_for_arl(auth_file: &Path) -> Option<String> {
    let start = Instant::now();
    let mut stdin_buf = Vec::new();

    loop {
        // Check timeout
        if start.elapsed() > TIMEOUT {
            warn!("Browser login timed out after {:?}", TIMEOUT);
            return None;
        }

        // Check auth file (written by URI handler)
        if auth_file.exists() {
            if let Ok(content) = fs::read_to_string(auth_file) {
                if let Some(arl) = parse_arl(&content) {
                    debug!("ARL received via URI handler");
                    return Some(arl);
                }
            }
        }

        // Check stdin non-blockingly (manual paste fallback)
        if stdin_has_data() {
            read_stdin_nonblocking(&mut stdin_buf);

            // Process complete lines
            while let Some(newline_pos) = stdin_buf.iter().position(|&b| b == b'\n') {
                let line = String::from_utf8_lossy(&stdin_buf[..newline_pos]).to_string();
                stdin_buf.drain(..=newline_pos);

                if line.trim().is_empty() {
                    return None; // Empty line = cancel
                }
                if let Some(arl) = parse_arl(&line) {
                    debug!("ARL received via stdin paste");
                    return Some(arl);
                }
                print!("  Invalid format. Paste the deezer://autolog/... URL: ");
                let _ = io::stdout().flush();
            }
        }

        std::thread::sleep(POLL_INTERVAL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_arl_full_url() {
        let input = "deezer://autolog/eec5474088bcafa3e16be4492de9abf53472bda2d15527838cd685384c7c82b7daa0f5d745b9cd6756dfe83d65aaa8d6fd91aba6798c714c16a7dfbbe1235730ca72e7f3db0f58c8d9d1f84c29c2236a8ceaae6e639b275ac438379891e998f4";
        let arl = parse_arl(input).unwrap();
        assert_eq!(arl.len(), 192);
        assert!(arl.starts_with("eec547"));
    }

    #[test]
    fn test_parse_arl_raw() {
        let input = "eec5474088bcafa3e16be4492de9abf53472bda2d15527838cd685384c7c82b7daa0f5d745b9cd6756dfe83d65aaa8d6fd91aba6798c714c16a7dfbbe1235730ca72e7f3db0f58c8d9d1f84c29c2236a8ceaae6e639b275ac438379891e998f4";
        let arl = parse_arl(input).unwrap();
        assert_eq!(arl.len(), 192);
    }

    #[test]
    fn test_parse_arl_with_newline() {
        let input = "eec5474088bcafa3e16be4492de9abf53472bda2d15527838cd685384c7c82b7daa0f5d745b9cd6756dfe83d65aaa8d6fd91aba6798c714c16a7dfbbe1235730ca72e7f3db0f58c8d9d1f84c29c2236a8ceaae6e639b275ac438379891e998f4\n";
        let arl = parse_arl(input).unwrap();
        assert_eq!(arl.len(), 192);
    }

    #[test]
    fn test_parse_arl_autolog_prefix() {
        let input = "autolog/eec5474088bcafa3e16be4492de9abf53472bda2d15527838cd685384c7c82b7daa0f5d745b9cd6756dfe83d65aaa8d6fd91aba6798c714c16a7dfbbe1235730ca72e7f3db0f58c8d9d1f84c29c2236a8ceaae6e639b275ac438379891e998f4";
        let arl = parse_arl(input).unwrap();
        assert_eq!(arl.len(), 192);
    }

    #[test]
    fn test_parse_arl_invalid() {
        assert!(parse_arl("").is_none());
        assert!(parse_arl("too-short").is_none());
        assert!(parse_arl("not_hex_at_all_zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz").is_none());
    }
}
