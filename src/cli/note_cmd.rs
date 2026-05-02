//! `daylog note` — append a free-text note to the day's `## Notes` section.

use color_eyre::eyre::{bail, Result};

use crate::body;
use crate::cli::resolve;
use crate::config::Config;
use crate::frontmatter;
use crate::time;

pub fn execute(
    text: &[String],
    date_flag: Option<&str>,
    time_flag: Option<&str>,
    config: &Config,
    quiet: bool,
) -> Result<()> {
    if text.is_empty() {
        bail!("Note text required.");
    }
    let joined = text.join(" ");
    let trimmed = joined.trim();
    if trimmed.is_empty() {
        bail!("Note text required.");
    }

    let date = resolve::target_date(date_flag, config)?;
    let date_str = date.format("%Y-%m-%d").to_string();
    let when = resolve::target_time(time_flag)?;

    let body_text = config
        .notes
        .aliases
        .get(trimmed)
        .map(String::as_str)
        .unwrap_or(trimmed);

    let formatted_time = time::format_time(when, config.time_format);
    let line = format!("- **{formatted_time}** {body_text}");

    let note_path = config.notes_dir_path().join(format!("{date_str}.md"));
    let content = if note_path.exists() {
        std::fs::read_to_string(&note_path)?
    } else {
        crate::template::render_daily_note(&date_str, config)
    };

    let updated = body::ensure_section(&content, "Notes");
    let updated = body::append_line_to_section(&updated, "Notes", &line);
    frontmatter::atomic_write(&note_path, &updated)?;

    if quiet {
        eprintln!("Note logged: {date_str} {formatted_time}");
    } else {
        eprintln!("Note logged: {date_str} {formatted_time}");
        eprintln!("  {line}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn config_with_alias(notes_dir: &Path, key: &str, value: &str) -> Config {
        let toml_str = format!(
            r#"
notes_dir = '{}'
time_format = '24h'

[notes.aliases]
{key} = "{value}"
"#,
            notes_dir.display().to_string().replace('\\', "/")
        );
        toml::from_str(&toml_str).expect("config parses")
    }

    fn read_today(notes_dir: &Path, config: &Config) -> String {
        let date = config.effective_today();
        std::fs::read_to_string(notes_dir.join(format!("{date}.md"))).unwrap()
    }

    #[test]
    fn note_literal_appends_with_timestamp() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_with_alias(dir.path(), "ignored", "ignored");
        execute(
            &["Attentin".into(), "10mg".into()],
            None,
            Some("12:30"),
            &config,
            true,
        )
        .unwrap();

        let note = read_today(dir.path(), &config);
        assert!(note.contains("## Notes"), "got:\n{note}");
        assert!(note.contains("- **12:30** Attentin 10mg"), "got:\n{note}");
    }

    #[test]
    fn note_alias_expands() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_with_alias(dir.path(), "med-morning", "Morgonmedicin (Elvanse 70mg)");
        execute(&["med-morning".into()], None, Some("07:55"), &config, true).unwrap();

        let note = read_today(dir.path(), &config);
        assert!(
            note.contains("- **07:55** Morgonmedicin (Elvanse 70mg)"),
            "got:\n{note}"
        );
    }

    #[test]
    fn note_alias_falls_through_when_missing() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_with_alias(dir.path(), "med-morning", "expanded");
        execute(&["unknown-key".into()], None, Some("08:00"), &config, true).unwrap();

        let note = read_today(dir.path(), &config);
        assert!(note.contains("- **08:00** unknown-key"), "got:\n{note}");
        assert!(!note.contains("expanded"));
    }

    #[test]
    fn note_empty_text_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_with_alias(dir.path(), "ignored", "ignored");
        let err = execute(&[], None, Some("08:00"), &config, true).unwrap_err();
        assert!(err.to_string().contains("Note text required"));
    }

    #[test]
    fn note_uses_explicit_date_flag() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_with_alias(dir.path(), "ignored", "ignored");
        execute(
            &["Late entry".into()],
            Some("2026-04-29"),
            Some("23:59"),
            &config,
            true,
        )
        .unwrap();

        let other = std::fs::read_to_string(dir.path().join("2026-04-29.md")).unwrap();
        assert!(other.contains("- **23:59** Late entry"), "got:\n{other}");
    }

    #[test]
    fn note_invalid_date_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_with_alias(dir.path(), "ignored", "ignored");
        let err = execute(
            &["x".into()],
            Some("2026-13-45"),
            Some("08:00"),
            &config,
            true,
        )
        .unwrap_err();
        assert!(err.to_string().contains("Invalid --date"));
    }

    #[test]
    fn note_invalid_time_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_with_alias(dir.path(), "ignored", "ignored");
        let err = execute(&["x".into()], None, Some("25:00"), &config, true).unwrap_err();
        assert!(err.to_string().contains("Invalid --time"));
    }

    #[test]
    fn note_uses_time_format_12h() {
        let dir = tempfile::TempDir::new().unwrap();
        let toml_str = format!(
            r#"
notes_dir = '{}'
time_format = '12h'
"#,
            dir.path().display().to_string().replace('\\', "/")
        );
        let config: Config = toml::from_str(&toml_str).unwrap();

        execute(&["Coffee".into()], None, Some("13:30"), &config, true).unwrap();
        let note = read_today(dir.path(), &config);
        assert!(note.contains("- **1:30pm** Coffee"), "got:\n{note}");
    }
}
