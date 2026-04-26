use chrono::{Local, Timelike};
use color_eyre::eyre::Result;

use crate::config::Config;
use crate::state::{self, PendingSleepStart};
use crate::time;

/// Records bedtime as pending state for later finalization by `sleep-end`.
pub fn cmd_sleep_start(time_arg: Option<&str>, config: &Config) -> Result<()> {
    let bedtime = match time_arg {
        Some(s) => time::parse_time(s).ok_or_else(|| {
            color_eyre::eyre::eyre!(
                "Invalid time: '{s}'. Expected HH:MM (24h) or H:MMam/pm (12h)."
            )
        })?,
        None => Local::now()
            .time()
            .with_second(0)
            .unwrap_or_else(|| Local::now().time()),
    };

    let now = Local::now();
    let mut s = state::load(&config.notes_dir_path());
    s.sleep_start = Some(PendingSleepStart {
        bedtime,
        recorded_at: now,
    });
    state::save(&config.notes_dir_path(), &s)?;

    eprintln!(
        "Sleep start recorded: {}",
        time::format_time(bedtime, config.time_format)
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveTime;

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
}
