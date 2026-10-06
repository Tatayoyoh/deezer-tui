//! "What's new": the CHANGELOG sections a user has not seen yet, shown in the
//! info modal on the first launch of a new version.
//!
//! The changelog is embedded at build time, so the notes always match the
//! running binary. The last version seen is kept in its own file next to
//! `config.json` rather than in it: the daemon rewrites `config.json` from its
//! in-memory copy, which would drop a value written by the client.

use std::fs;
use std::path::PathBuf;

use deezer_core::config::Config;

const CHANGELOG: &str = include_str!("../../../CHANGELOG.md");

/// One display line of the release notes, before wrapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteLine {
    /// `## [1.19.0] - 2026-10-06` → version and date.
    Version { version: String, date: String },
    /// `### Added`
    Section(String),
    /// `- item` (depth 0) or `    * sub-item` (depth 1), continuation lines
    /// folded in.
    Item { depth: u8, text: String },
    /// Gap between two versions.
    Blank,
}

/// Notes of the running version and of every version released since
/// `last_seen` (exclusive). Without `last_seen`, the running version only.
pub fn notes_since(last_seen: Option<&str>) -> Vec<NoteLine> {
    parse(CHANGELOG, last_seen, env!("CARGO_PKG_VERSION"))
}

fn parse(changelog: &str, last_seen: Option<&str>, current: &str) -> Vec<NoteLine> {
    let current = semver(current);
    let last_seen = last_seen.and_then(semver);
    let wanted = |v: Option<(u32, u32, u32)>| match (v, current) {
        (Some(v), Some(cur)) => v <= cur && last_seen.map_or(v == cur, |seen| v > seen),
        _ => false,
    };

    let mut out = Vec::new();
    let mut keep = false;
    for line in changelog.lines() {
        if let Some(header) = line.strip_prefix("## ") {
            // `[1.19.0] - 2026-10-06`; `[Unreleased]` has no version and is skipped.
            let (version, date) = header.split_once(" - ").unwrap_or((header, ""));
            let version = version.trim_matches(|c| c == '[' || c == ']');
            keep = wanted(semver(version));
            if keep {
                if !out.is_empty() {
                    out.push(NoteLine::Blank);
                }
                out.push(NoteLine::Version {
                    version: version.to_string(),
                    date: date.trim().to_string(),
                });
            }
            continue;
        }
        if !keep || line.trim().is_empty() {
            continue;
        }
        if let Some(section) = line.strip_prefix("### ") {
            out.push(NoteLine::Section(section.trim().to_string()));
        } else if let Some(text) = line.strip_prefix("- ") {
            out.push(NoteLine::Item {
                depth: 0,
                text: text.trim().to_string(),
            });
        } else if let Some(text) = line.trim_start().strip_prefix("* ") {
            out.push(NoteLine::Item {
                depth: 1,
                text: text.trim().to_string(),
            });
        } else if let Some(NoteLine::Item { text, .. }) = out.last_mut() {
            // Indented continuation of the previous item.
            text.push(' ');
            text.push_str(line.trim());
        }
    }
    out
}

/// `1.19.0` → `(1, 19, 0)`.
fn semver(version: &str) -> Option<(u32, u32, u32)> {
    let mut parts = version.trim().trim_start_matches('v').split('.');
    let mut next = || parts.next()?.parse().ok();
    Some((next()?, next()?, next()?))
}

fn seen_path() -> Option<PathBuf> {
    Config::dir().map(|d| d.join("last_version"))
}

/// Version recorded by the last launch, if any.
pub fn last_seen_version() -> Option<String> {
    let seen = fs::read_to_string(seen_path()?).ok()?;
    Some(seen.trim().to_string()).filter(|v| !v.is_empty())
}

/// Record the running version as seen.
pub fn mark_seen() {
    if let Some(path) = seen_path() {
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(path, env!("CARGO_PKG_VERSION"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# Changelog

## [Unreleased]

### Added
- Not released yet

## [1.3.0] - 2026-03-01

### Added
- Feature C:
    * detail one
    * detail two
- Feature D spans
  two lines

## [1.2.0] - 2026-02-01

### Fixed
- Bug B

## [1.1.0] - 2026-01-01

### Added
- Feature A
";

    fn versions(notes: &[NoteLine]) -> Vec<&str> {
        notes
            .iter()
            .filter_map(|n| match n {
                NoteLine::Version { version, .. } => Some(version.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn notes_cover_every_version_since_the_last_seen_one() {
        let notes = parse(SAMPLE, Some("1.1.0"), "1.3.0");
        assert_eq!(versions(&notes), ["1.3.0", "1.2.0"]);
        assert!(!notes.contains(&NoteLine::Item {
            depth: 0,
            text: "Not released yet".into()
        }));
    }

    #[test]
    fn notes_without_a_seen_version_show_the_running_one() {
        assert_eq!(versions(&parse(SAMPLE, None, "1.2.0")), ["1.2.0"]);
        // Unparsable record: same as none.
        assert_eq!(
            versions(&parse(SAMPLE, Some("garbage"), "1.2.0")),
            ["1.2.0"]
        );
    }

    #[test]
    fn notes_skip_versions_newer_than_the_running_one() {
        assert_eq!(versions(&parse(SAMPLE, Some("1.1.0"), "1.2.0")), ["1.2.0"]);
        assert!(parse(SAMPLE, Some("1.3.0"), "1.3.0").is_empty());
    }

    #[test]
    fn notes_parse_sections_items_and_continuations() {
        let notes = parse(SAMPLE, Some("1.2.0"), "1.3.0");
        assert_eq!(
            notes,
            [
                NoteLine::Version {
                    version: "1.3.0".into(),
                    date: "2026-03-01".into()
                },
                NoteLine::Section("Added".into()),
                NoteLine::Item {
                    depth: 0,
                    text: "Feature C:".into()
                },
                NoteLine::Item {
                    depth: 1,
                    text: "detail one".into()
                },
                NoteLine::Item {
                    depth: 1,
                    text: "detail two".into()
                },
                NoteLine::Item {
                    depth: 0,
                    text: "Feature D spans two lines".into()
                },
            ]
        );
    }

    #[test]
    fn embedded_changelog_has_notes_for_the_running_version() {
        let notes = notes_since(None);
        assert_eq!(versions(&notes), [env!("CARGO_PKG_VERSION")]);
        assert!(notes.iter().any(|n| matches!(n, NoteLine::Item { .. })));
    }
}
