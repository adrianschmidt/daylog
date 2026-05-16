//! `vitalog bp` — write blood pressure to YAML + append a `## Vitals` line.

use chrono::NaiveTime;
use color_eyre::eyre::{bail, Result};

use crate::body;
use crate::cli::resolve;
use crate::config::Config;
use crate::frontmatter;
use crate::time;

/// Morning/evening cutoff: time-of-measurement in [day_start_hour, 14:00) →
/// morning; otherwise evening. `--morning` and `--evening` flags override.
/// Times before `day_start_hour` are late-night of the *previous* effective
/// day and classify as evening (issue #34).
const MORNING_CUTOFF_HOUR: u32 = 14;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Morning,
    Evening,
}

impl Slot {
    fn yaml_prefix(self) -> &'static str {
        match self {
            Slot::Morning => "bp_morning",
            Slot::Evening => "bp_evening",
        }
    }
}

impl std::fmt::Display for Slot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Slot::Morning => write!(f, "morning"),
            Slot::Evening => write!(f, "evening"),
        }
    }
}

/// Decide the slot from explicit flags or the measurement time.
pub fn pick_slot(morning: bool, evening: bool, when: NaiveTime, day_start_hour: u8) -> Slot {
    if morning {
        return Slot::Morning;
    }
    if evening {
        return Slot::Evening;
    }
    use chrono::Timelike;
    let hour = when.hour();
    if hour < u32::from(day_start_hour) {
        return Slot::Evening;
    }
    if hour < MORNING_CUTOFF_HOUR {
        Slot::Morning
    } else {
        Slot::Evening
    }
}

#[allow(clippy::too_many_arguments)]
pub fn execute(
    sys: i32,
    dia: i32,
    pulse: i32,
    morning: bool,
    evening: bool,
    date_flag: Option<&str>,
    time_flag: Option<&str>,
    config: &Config,
    quiet: bool,
) -> Result<()> {
    if morning && evening {
        // clap's `conflicts_with` should already block this, but keep a
        // defensive bail in case the function is called programmatically.
        bail!("--morning and --evening are mutually exclusive.");
    }

    let date = resolve::target_date(date_flag, config)?;
    let date_str = date.format("%Y-%m-%d").to_string();
    let when = resolve::target_time(time_flag)?;
    let slot = pick_slot(morning, evening, when, config.day_start_hour);

    validate_or_warn(sys, dia, pulse);

    let formatted_time = time::format_time(when, config.time_format);
    let prefix = slot.yaml_prefix();
    let body_line = format!("- **{formatted_time}** BP: {sys}/{dia}, pulse {pulse} bpm");

    let note_path = config.notes_dir_path().join(format!("{date_str}.md"));
    let content = if note_path.exists() {
        std::fs::read_to_string(&note_path)?
    } else {
        crate::template::render_daily_note(&date_str, config)
    };

    let updated = frontmatter::set_scalar(&content, &format!("{prefix}_sys"), &sys.to_string());
    let updated = frontmatter::set_scalar(&updated, &format!("{prefix}_dia"), &dia.to_string());
    let updated = frontmatter::set_scalar(&updated, &format!("{prefix}_pulse"), &pulse.to_string());
    let updated = body::ensure_section(&updated, "Vitals");
    let updated = body::append_line_to_section(&updated, "Vitals", &body_line);

    frontmatter::atomic_write(&note_path, &updated)?;
    if quiet {
        eprintln!("BP logged: {date_str} {formatted_time} {sys}/{dia}, pulse {pulse} bpm ({slot})");
    } else {
        eprintln!("BP logged: {date_str} {formatted_time} ({slot})");
        eprintln!("  {body_line}");
    }
    Ok(())
}

fn validate_or_warn(sys: i32, dia: i32, pulse: i32) {
    if !(50..=300).contains(&sys) {
        eprintln!("Warning: sys={sys} outside plausible range 50–300; logging anyway.");
    }
    if !(30..=200).contains(&dia) {
        eprintln!("Warning: dia={dia} outside plausible range 30–200; logging anyway.");
    }
    if !(30..=250).contains(&pulse) {
        eprintln!("Warning: pulse={pulse} outside plausible range 30–250; logging anyway.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveTime;
    use std::path::Path;

    fn t(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    fn config_in(notes_dir: &Path, fmt: &str) -> Config {
        let toml_str = format!(
            "notes_dir = '{}'\ntime_format = '{fmt}'\n",
            notes_dir.display().to_string().replace('\\', "/")
        );
        toml::from_str(&toml_str).unwrap()
    }

    fn config_in_with_day_start(notes_dir: &Path, fmt: &str, day_start_hour: u8) -> Config {
        let toml_str = format!(
            "notes_dir = '{}'\ntime_format = '{fmt}'\nday_start_hour = {day_start_hour}\n",
            notes_dir.display().to_string().replace('\\', "/")
        );
        toml::from_str(&toml_str).unwrap()
    }

    fn read_today(notes_dir: &Path, config: &Config) -> String {
        let date = config.effective_today();
        std::fs::read_to_string(notes_dir.join(format!("{date}.md"))).unwrap()
    }

    // --- pick_slot pure logic ---

    #[test]
    fn slot_auto_morning_before_14() {
        assert_eq!(pick_slot(false, false, t(13, 59), 0), Slot::Morning);
        assert_eq!(pick_slot(false, false, t(7, 30), 0), Slot::Morning);
        assert_eq!(pick_slot(false, false, t(0, 0), 0), Slot::Morning);
    }

    #[test]
    fn slot_auto_evening_at_14_and_after() {
        assert_eq!(pick_slot(false, false, t(14, 0), 0), Slot::Evening);
        assert_eq!(pick_slot(false, false, t(20, 30), 0), Slot::Evening);
        assert_eq!(pick_slot(false, false, t(23, 59), 0), Slot::Evening);
    }

    #[test]
    fn slot_explicit_flags_override_time() {
        assert_eq!(pick_slot(true, false, t(20, 0), 0), Slot::Morning);
        assert_eq!(pick_slot(false, true, t(7, 0), 0), Slot::Evening);
    }

    #[test]
    fn slot_auto_evening_for_late_night_before_day_start() {
        // With day_start_hour=4, [00:00, 04:00) is the late-night window
        // of the *previous* effective day — classify as evening so it
        // doesn't clobber the morning frontmatter (issue #34).
        assert_eq!(pick_slot(false, false, t(0, 0), 4), Slot::Evening);
        assert_eq!(pick_slot(false, false, t(0, 34), 4), Slot::Evening);
        assert_eq!(pick_slot(false, false, t(3, 59), 4), Slot::Evening);
    }

    #[test]
    fn slot_auto_morning_at_and_after_day_start() {
        // With day_start_hour=4, times in [04:00, 14:00) → morning as before.
        assert_eq!(pick_slot(false, false, t(4, 0), 4), Slot::Morning);
        assert_eq!(pick_slot(false, false, t(7, 30), 4), Slot::Morning);
        assert_eq!(pick_slot(false, false, t(13, 59), 4), Slot::Morning);
    }

    // --- end-to-end via execute ---

    #[test]
    fn writes_three_yaml_fields_for_morning() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in(dir.path(), "24h");
        execute(
            141,
            96,
            70,
            false,
            false,
            None,
            Some("07:30"),
            &config,
            true,
        )
        .unwrap();

        let note = read_today(dir.path(), &config);
        assert!(note.contains("bp_morning_sys: 141"), "got:\n{note}");
        assert!(note.contains("bp_morning_dia: 96"));
        assert!(note.contains("bp_morning_pulse: 70"));
    }

    #[test]
    fn writes_three_yaml_fields_for_evening() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in(dir.path(), "24h");
        execute(
            133,
            73,
            62,
            false,
            false,
            None,
            Some("18:00"),
            &config,
            true,
        )
        .unwrap();

        let note = read_today(dir.path(), &config);
        assert!(note.contains("bp_evening_sys: 133"), "got:\n{note}");
        assert!(note.contains("bp_evening_dia: 73"));
        assert!(note.contains("bp_evening_pulse: 62"));
    }

    #[test]
    fn vitals_line_has_no_slot_suffix_and_includes_bpm() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in(dir.path(), "24h");
        execute(
            141,
            96,
            70,
            false,
            false,
            None,
            Some("07:30"),
            &config,
            true,
        )
        .unwrap();

        let note = read_today(dir.path(), &config);
        assert!(
            note.contains("- **07:30** BP: 141/96, pulse 70 bpm"),
            "got:\n{note}"
        );
        assert!(!note.contains("(morning)"));
        assert!(!note.contains("(evening)"));
    }

    #[test]
    fn explicit_evening_overrides_time() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in(dir.path(), "24h");
        execute(133, 73, 62, false, true, None, Some("09:00"), &config, true).unwrap();

        let note = read_today(dir.path(), &config);
        assert!(note.contains("bp_evening_sys: 133"));
        assert!(!note.contains("bp_morning_sys"));
    }

    #[test]
    fn rerun_morning_overwrites_yaml_appends_vitals() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in(dir.path(), "24h");
        execute(
            140,
            95,
            70,
            false,
            false,
            None,
            Some("07:00"),
            &config,
            true,
        )
        .unwrap();
        execute(
            135,
            90,
            65,
            false,
            false,
            None,
            Some("07:30"),
            &config,
            true,
        )
        .unwrap();

        let note = read_today(dir.path(), &config);
        // YAML overwritten in place: only the second value present.
        assert!(note.contains("bp_morning_sys: 135"));
        assert!(!note.contains("bp_morning_sys: 140"));
        // Vitals body keeps both lines chronologically.
        assert!(note.contains("- **07:00** BP: 140/95, pulse 70 bpm"));
        assert!(note.contains("- **07:30** BP: 135/90, pulse 65 bpm"));
    }

    #[test]
    fn creates_vitals_section_if_missing() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in(dir.path(), "24h");
        execute(
            141,
            96,
            70,
            false,
            false,
            None,
            Some("07:30"),
            &config,
            true,
        )
        .unwrap();

        let note = read_today(dir.path(), &config);
        assert!(note.contains("## Vitals"));
    }

    #[test]
    fn date_flag_writes_to_named_day() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in(dir.path(), "24h");
        execute(
            141,
            96,
            70,
            false,
            false,
            Some("2026-04-29"),
            Some("07:30"),
            &config,
            true,
        )
        .unwrap();

        let path = dir.path().join("2026-04-29.md");
        let note = std::fs::read_to_string(&path).unwrap();
        assert!(note.contains("bp_morning_sys: 141"));
        assert!(note.contains("- **07:30** BP: 141/96, pulse 70 bpm"));
    }

    #[test]
    fn late_night_reading_with_day_start_hour_4_does_not_clobber_morning() {
        // Issue #34: with day_start_hour=4, a 00:34 BP entry attributed
        // to the previous effective day must classify as evening, not
        // morning — otherwise it overwrites the actual morning reading.
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in_with_day_start(dir.path(), "24h", 4);

        execute(
            137,
            86,
            57,
            false,
            false,
            Some("2026-05-15"),
            Some("08:28"),
            &config,
            true,
        )
        .unwrap();
        execute(
            132,
            72,
            56,
            false,
            false,
            Some("2026-05-15"),
            Some("00:34"),
            &config,
            true,
        )
        .unwrap();

        let note = std::fs::read_to_string(dir.path().join("2026-05-15.md")).unwrap();
        assert!(
            note.contains("bp_morning_sys: 137"),
            "morning sys was clobbered:\n{note}"
        );
        assert!(
            note.contains("bp_morning_dia: 86"),
            "morning dia was clobbered:\n{note}"
        );
        assert!(
            note.contains("bp_morning_pulse: 57"),
            "morning pulse was clobbered:\n{note}"
        );
        assert!(
            note.contains("bp_evening_sys: 132"),
            "evening sys not written:\n{note}"
        );
        assert!(
            note.contains("bp_evening_dia: 72"),
            "evening dia not written:\n{note}"
        );
        assert!(
            note.contains("bp_evening_pulse: 56"),
            "evening pulse not written:\n{note}"
        );
    }

    #[test]
    fn invalid_date_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in(dir.path(), "24h");
        let err = execute(
            141,
            96,
            70,
            false,
            false,
            Some("2026-13-45"),
            Some("07:30"),
            &config,
            true,
        )
        .unwrap_err();
        assert!(err.to_string().contains("Invalid --date"));
    }

    #[test]
    fn invalid_time_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in(dir.path(), "24h");
        let err = execute(
            141,
            96,
            70,
            false,
            false,
            None,
            Some("25:00"),
            &config,
            true,
        )
        .unwrap_err();
        assert!(err.to_string().contains("Invalid --time"));
    }
}
