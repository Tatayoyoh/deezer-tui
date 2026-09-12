//! Sanitisation of untrusted metadata before it reaches the terminal.

/// Maximum number of characters kept from a single metadata field.
const MAX_FIELD_LEN: usize = 100;

/// Strip control characters from a string coming from the Deezer API and cap
/// its length.
///
/// Track, artist and album names are attacker-influenced data: they end up in
/// the terminal window title (`ESC ] 0 ; … BEL`) and on stdout for `--status`,
/// which is piped into tmux status lines and status bars. A raw `BEL` closes
/// the OSC string early, so anything after it would be interpreted by the
/// terminal as escape sequences (clipboard writes via OSC 52, mode changes,
/// window ops). `char::is_control` covers C0, DEL and the C1 range.
pub fn sanitize(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .take(MAX_FIELD_LEN)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_bel_and_escape() {
        assert_eq!(
            sanitize("Song\x07\x1b]52;c;cGF5bG9hZA==\x07"),
            "Song]52;c;cGF5bG9hZA=="
        );
        assert_eq!(sanitize("Song\x1b[2J"), "Song[2J");
    }

    #[test]
    fn strips_newlines_and_c1() {
        assert_eq!(sanitize("Line\nBreak\r\t"), "LineBreak");
        assert_eq!(sanitize("A\u{0090}B"), "AB");
    }

    #[test]
    fn keeps_normal_text_and_truncates() {
        assert_eq!(sanitize("Björk — Jóga (Live)"), "Björk — Jóga (Live)");
        assert_eq!(sanitize(&"a".repeat(250)).chars().count(), MAX_FIELD_LEN);
    }
}
