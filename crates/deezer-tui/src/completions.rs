//! Shell completion generation for deezer-tui.

use std::io::{self, Write};

/// Generate shell completions for the specified shell.
pub fn generate_completions(shell: &str) {
    match shell.to_lowercase().as_str() {
        "bash" => print_bash_completions(),
        "zsh" => print_zsh_completions(),
        "fish" => print_fish_completions(),
        _ => {
            eprintln!("Unsupported shell: '{shell}'. Supported shells: bash, zsh, fish");
            std::process::exit(1);
        }
    }
}

fn print_bash_completions() {
    let script = r#"_deezer_tui() {
    local cur prev opts
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"
    opts="-p --toggle --play --pause --stop -n --next -b --prev -s --status --json --volume --volume-up --volume-down --seek --seek-forward --seek-backward --shuffle --repeat --like --dislike --completions -q --quit -v --version -h --help"

    case "${prev}" in
        --completions)
            COMPREPLY=( $(compgen -W "bash zsh fish" -- "${cur}") )
            return 0
            ;;
        --volume|--volume-up|--volume-down|--seek|--seek-forward|--seek-backward)
            return 0
            ;;
        *)
            ;;
    esac

    COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
    return 0
}
complete -F _deezer_tui deezer-tui
"#;
    let _ = io::stdout().write_all(script.as_bytes());
}

fn print_zsh_completions() {
    let script = r#"#compdef deezer-tui

_deezer_tui() {
    local -a arguments
    arguments=(
        '(-p --toggle)'{-p,--toggle}'[Toggle play/pause]'
        '--play[Resume playback]'
        '--pause[Pause playback]'
        '--stop[Stop playback]'
        '(-n --next)'{-n,--next}'[Skip to next track]'
        '(-b --prev)'{-b,--prev}'[Go to previous track]'
        '(-s --status)'{-s,--status}'[Show current playback status]'
        '--json[Format status output as JSON]'
        '--volume[Set volume percentage (0-100)]:volume (0-100):'
        '--volume-up[Increase volume by percentage (default 5%)]:step:'
        '--volume-down[Decrease volume by percentage (default 5%)]:step:'
        '--seek[Seek to absolute position in seconds]:seconds:'
        '--seek-forward[Seek forward by seconds]:seconds:'
        '--seek-backward[Seek backward by seconds]:seconds:'
        '--shuffle[Toggle shuffle mode]'
        '--repeat[Cycle repeat mode (off -> queue -> track)]'
        '--like[Add currently playing track to favorites]'
        '--dislike[Dislike currently playing track]'
        '--completions[Generate shell completions]:shell:(bash zsh fish)'
        '(-q --quit)'{-q,--quit}'[Stop the daemon]'
        '(-v --version)'{-v,--version}'[Show version info]'
        '(-h --help)'{-h,--help}'[Show help message]'
    )
    _arguments -s $arguments
}

_deezer_tui "$@"
"#;
    let _ = io::stdout().write_all(script.as_bytes());
}

fn print_fish_completions() {
    let script = r#"complete -c deezer-tui -s p -l toggle -d "Toggle play/pause"
complete -c deezer-tui -l play -d "Resume playback"
complete -c deezer-tui -l pause -d "Pause playback"
complete -c deezer-tui -l stop -d "Stop playback"
complete -c deezer-tui -s n -l next -d "Skip to next track"
complete -c deezer-tui -s b -l prev -d "Go to previous track"
complete -c deezer-tui -s s -l status -d "Show current playback status"
complete -c deezer-tui -l json -d "Format status output as JSON"
complete -c deezer-tui -l volume -d "Set volume percentage (0-100)" -x
complete -c deezer-tui -l volume-up -d "Increase volume by percentage (default 5%)" -x
complete -c deezer-tui -l volume-down -d "Decrease volume by percentage (default 5%)" -x
complete -c deezer-tui -l seek -d "Seek to absolute position in seconds" -x
complete -c deezer-tui -l seek-forward -d "Seek forward by seconds" -x
complete -c deezer-tui -l seek-backward -d "Seek backward by seconds" -x
complete -c deezer-tui -l shuffle -d "Toggle shuffle mode"
complete -c deezer-tui -l repeat -d "Cycle repeat mode"
complete -c deezer-tui -l like -d "Add current track to favorites"
complete -c deezer-tui -l dislike -d "Dislike current track"
complete -c deezer-tui -l completions -d "Generate shell completions" -x -a "bash zsh fish"
complete -c deezer-tui -s q -l quit -d "Stop the daemon"
complete -c deezer-tui -s v -l version -d "Show version info"
complete -c deezer-tui -s h -l help -d "Show help message"
"#;
    let _ = io::stdout().write_all(script.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_completions_dont_panic() {
        generate_completions("bash");
        generate_completions("zsh");
        generate_completions("fish");
    }
}
