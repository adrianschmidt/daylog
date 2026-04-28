use chrono::{Local, NaiveTime, Timelike};
use color_eyre::eyre::Result;
use color_eyre::Help;

use crate::config::Config;
use crate::state::{self, PendingSleepStart};
use crate::time;

const MAX_PENDING_AGE_HOURS: i64 = 24;

/// Parse a CLI time argument, attaching a format-specific suggestion on failure.
/// `example` is the time used in the suggestion text (e.g., `"22:30"` or `"06:15"`).
fn parse_time_arg(s: &str, example: &str) -> Result<NaiveTime> {
    time::parse_time(s)
        .ok_or_else(|| {
            color_eyre::eyre::eyre!("Invalid time: '{s}'. Expected HH:MM (24h) or H:MMam/pm (12h).")
        })
        .suggestion(format!(
            "Use 24h form like {example} or 12h form like {}.",
            example_12h(example)
        ))
}

/// Best-effort 12h example for the suggestion text. Falls back to a sensible
/// default if `example` isn't a clean HH:MM.
fn example_12h(example: &str) -> String {
    time::parse_time(example)
        .map(|t| time::format_time(t, crate::config::TimeFormat::TwelveHour))
        .unwrap_or_else(|| "10:30pm".to_string())
}

pub fn cmd_sleep_start(time_arg: Option<&str>, config: &Config) -> Result<()> {
    let now = Local::now();
    let bedtime = match time_arg {
        Some(s) => parse_time_arg(s, "22:30")?,
        None => now.time().with_second(0).expect("0 < 60"),
    };

    let notes_dir = config.notes_dir_path();
    let mut s = state::load(&notes_dir);
    if let Some(prev) = &s.sleep_start {
        eprintln!(
            "Replacing pending sleep-start (was {} from {}).",
            time::format_time(prev.bedtime, config.time_format),
            prev.recorded_at.format("%Y-%m-%d %H:%M")
        );
    }
    s.sleep_start = Some(PendingSleepStart {
        bedtime,
        recorded_at: now,
    });
    state::save(&notes_dir, &s)?;

    eprintln!(
        "Sleep start recorded: {}",
        time::format_time(bedtime, config.time_format)
    );
    Ok(())
}

pub fn cmd_sleep_end(time_arg: Option<&str>, config: &Config) -> Result<()> {
    let now = Local::now();
    let wake = match time_arg {
        Some(s) => parse_time_arg(s, "06:15")?,
        None => now.time().with_second(0).expect("0 < 60"),
    };

    let notes_dir = config.notes_dir_path();
    let mut state = state::load(&notes_dir);

    let pending = match state.sleep_start.take() {
        Some(p) => p,
        None => {
            return Err(color_eyre::eyre::eyre!("No pending sleep-start.")).suggestion(
                "Run `daylog sleep-start` before bed, or use \
                 `daylog log sleep \"HH:MM-HH:MM\"` for a one-shot entry.",
            );
        }
    };

    let age = now.signed_duration_since(pending.recorded_at);
    if age > chrono::Duration::hours(MAX_PENDING_AGE_HOURS) {
        state::save(&notes_dir, &state)?;
        let stale_bedtime = time::format_time(pending.bedtime, config.time_format);
        let stale_recorded_at = pending.recorded_at.format("%Y-%m-%d %H:%M");
        return Err(color_eyre::eyre::eyre!(
            "No pending sleep-start (discarded stale bedtime {stale_bedtime} from {stale_recorded_at})."
        ))
        .suggestion(format!(
            "If you slept that night, recover it with `daylog log sleep \"{stale_bedtime}-HH:MM\"`. \
             Otherwise run `daylog sleep-start` before bed."
        ));
    }

    let bedtime = pending.bedtime;
    // Use calendar today, not effective_today_date(): if the user wakes at
    // 03:00 with day_start_hour=4, the sleep belongs on the wake-day's note
    // (today on the wall clock), not yesterday's.
    let wake_date = now.date_naive();
    let date_str = wake_date.format("%Y-%m-%d").to_string();

    crate::cli::log_cmd::write_sleep_for_date(&date_str, bedtime, wake, config)?;

    state::save(&notes_dir, &state)?;

    let formatted = time::format_sleep_range(bedtime, wake, config.time_format);
    let hours = time::sleep_hours(bedtime, wake);
    eprintln!("Sleep recorded: {formatted} ({hours:.2}h) on {date_str}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveTime;
    use std::path::Path;

    fn config_in(notes_dir: &std::path::Path, fmt: &str) -> Config {
        let toml_str = format!(
            "notes_dir = '{}'\ntime_format = '{fmt}'\n",
            notes_dir.display().to_string().replace('\\', "/")
        );
        toml::from_str(&toml_str).unwrap()
    }

    #[test]
    fn sleep_start_with_explicit_time_writes_state() {
        let dir = tempfile::TempDir::new().unwrap();
        let cfg = config_in(dir.path(), "24h");
        cmd_sleep_start(Some("22:30"), &cfg).unwrap();

        let s = state::load(dir.path());
        let pending = s.sleep_start.unwrap();
        assert_eq!(pending.bedtime, NaiveTime::from_hms_opt(22, 30, 0).unwrap());
    }

    #[test]
    fn sleep_start_without_time_uses_now() {
        let dir = tempfile::TempDir::new().unwrap();
        let cfg = config_in(dir.path(), "24h");
        let before = Local::now();
        cmd_sleep_start(None, &cfg).unwrap();
        let after = Local::now();

        let s = state::load(dir.path());
        let pending = s.sleep_start.unwrap();
        // recorded_at within the window [before, after]
        assert!(pending.recorded_at >= before);
        assert!(pending.recorded_at <= after);
    }

    #[test]
    fn sleep_start_overwrites_previous() {
        let dir = tempfile::TempDir::new().unwrap();
        let cfg = config_in(dir.path(), "24h");
        cmd_sleep_start(Some("22:30"), &cfg).unwrap();
        cmd_sleep_start(Some("23:45"), &cfg).unwrap();

        let s = state::load(dir.path());
        assert_eq!(
            s.sleep_start.unwrap().bedtime,
            NaiveTime::from_hms_opt(23, 45, 0).unwrap()
        );
    }

    #[test]
    fn sleep_start_rejects_invalid_time() {
        let dir = tempfile::TempDir::new().unwrap();
        let cfg = config_in(dir.path(), "24h");
        let err = cmd_sleep_start(Some("banana"), &cfg).unwrap_err();
        assert!(err.to_string().contains("Invalid time"));
    }

    fn read_today_note(notes_dir: &Path) -> String {
        let today = Local::now().format("%Y-%m-%d").to_string();
        std::fs::read_to_string(notes_dir.join(format!("{today}.md"))).unwrap()
    }

    #[test]
    fn sleep_end_happy_path_writes_today_and_clears_state() {
        let dir = tempfile::TempDir::new().unwrap();
        let cfg = config_in(dir.path(), "24h");
        cmd_sleep_start(Some("22:30"), &cfg).unwrap();
        cmd_sleep_end(Some("06:15"), &cfg).unwrap();

        let note = read_today_note(dir.path());
        assert!(
            note.contains("sleep: \"22:30-06:15\""),
            "expected canonical 24h sleep entry, got: {note}"
        );

        let s = state::load(dir.path());
        assert!(
            s.sleep_start.is_none(),
            "pending state should be cleared after sleep-end"
        );
    }

    #[test]
    fn sleep_end_uses_time_format_12h() {
        let dir = tempfile::TempDir::new().unwrap();
        let cfg = config_in(dir.path(), "12h");
        cmd_sleep_start(Some("22:30"), &cfg).unwrap();
        cmd_sleep_end(Some("06:15"), &cfg).unwrap();

        let note = read_today_note(dir.path());
        assert!(
            note.contains("sleep: \"10:30pm-6:15am\""),
            "expected 12h-formatted sleep entry, got: {note}"
        );
    }

    #[test]
    fn sleep_end_no_pending_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        let cfg = config_in(dir.path(), "24h");
        let err = cmd_sleep_end(Some("06:15"), &cfg).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("No pending sleep-start"), "got: {msg}");
    }

    #[test]
    fn sleep_end_stale_pending_errors_and_clears_state() {
        use crate::state::PendingState;

        let dir = tempfile::TempDir::new().unwrap();
        let cfg = config_in(dir.path(), "24h");

        // Manually save state with a recorded_at >24h ago.
        let stale = Local::now() - chrono::Duration::hours(25);
        let s = PendingState {
            sleep_start: Some(PendingSleepStart {
                bedtime: NaiveTime::from_hms_opt(22, 30, 0).unwrap(),
                recorded_at: stale,
            }),
        };
        state::save(dir.path(), &s).unwrap();

        let err = cmd_sleep_end(Some("06:15"), &cfg).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("No pending sleep-start"), "got: {msg}");
        assert!(msg.contains("stale"), "expected stale suffix, got: {msg}");
        // The discarded bedtime should be surfaced so the user can recover it.
        assert!(
            msg.contains("22:30"),
            "expected discarded bedtime in message, got: {msg}"
        );

        // State should be cleared
        let after = state::load(dir.path());
        assert!(after.sleep_start.is_none());
    }

    #[test]
    fn sleep_end_creates_today_note_from_template() {
        let dir = tempfile::TempDir::new().unwrap();
        let cfg = config_in(dir.path(), "24h");
        cmd_sleep_start(Some("22:30"), &cfg).unwrap();
        cmd_sleep_end(Some("06:15"), &cfg).unwrap();

        let today = Local::now().format("%Y-%m-%d").to_string();
        let path = dir.path().join(format!("{today}.md"));
        assert!(path.exists(), "today's note should be created");
        let note = std::fs::read_to_string(&path).unwrap();
        assert!(note.starts_with("---\n"), "should have frontmatter");
    }

    #[test]
    fn sleep_end_rejects_invalid_time() {
        let dir = tempfile::TempDir::new().unwrap();
        let cfg = config_in(dir.path(), "24h");
        cmd_sleep_start(Some("22:30"), &cfg).unwrap();
        let err = cmd_sleep_end(Some("banana"), &cfg).unwrap_err();
        assert!(err.to_string().contains("Invalid time"));
    }

    // Regression guard: wake date must be calendar today, not effective_today().
    // day_start_hour is set high so that any consultation of effective_today()
    // would write to a different file; the assertion confirms calendar-today
    // is used by the CLI, regardless of how day_start_hour is configured.
    #[test]
    fn sleep_end_uses_calendar_today_not_day_start_hour() {
        let dir = tempfile::TempDir::new().unwrap();
        let toml_str = format!(
            "notes_dir = '{}'\ntime_format = '24h'\nday_start_hour = 4\n",
            dir.path().display().to_string().replace('\\', "/")
        );
        let cfg: Config = toml::from_str(&toml_str).unwrap();
        cmd_sleep_start(Some("22:30"), &cfg).unwrap();
        cmd_sleep_end(Some("06:15"), &cfg).unwrap();

        let calendar_today = Local::now().format("%Y-%m-%d").to_string();
        let calendar_path = dir.path().join(format!("{calendar_today}.md"));
        assert!(
            calendar_path.exists(),
            "expected wake-date file at calendar-today {calendar_today}"
        );
    }
}
