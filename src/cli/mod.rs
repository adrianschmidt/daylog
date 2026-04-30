pub mod bp_cmd;
pub mod completions;
pub mod food_cmd;
pub mod log_cmd;
pub mod note_cmd;
pub mod sleep_cmd;

use clap::{Parser, Subcommand};
use clap_complete::Shell;

#[derive(Parser)]
#[command(
    name = "daylog",
    version,
    about = "A terminal dashboard that tracks your life from markdown notes"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Set up daylog: create config, generate demo data
    Init {
        /// Notes directory path (skip interactive prompt)
        #[arg(long)]
        notes_dir: Option<String>,
        /// Skip demo data generation
        #[arg(long)]
        no_demo: bool,
    },
    /// Log a value to today's note
    Log {
        /// Field name (weight, sleep, mood, energy, lift, climb, metric)
        field: String,
        /// Value (all args joined — no shell quoting needed)
        #[arg(trailing_var_arg = true)]
        value: Vec<String>,
    },
    /// Print today's data as JSON
    Status,
    /// Sync notes to database (one-shot, no TUI)
    Sync,
    /// Open today's note (or a specific date) in $EDITOR
    Edit {
        /// Date in YYYY-MM-DD format (defaults to today)
        date: Option<String>,
    },
    /// Delete and rebuild the database from all notes
    Rebuild,
    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        shell: Shell,
    },
    /// Record bedtime (uses now, or pass a time)
    ///
    /// Stores the pending bedtime in `.daylog-state.toml` next to the DB.
    /// Run `daylog sleep-end` after waking to finalize the entry.
    ///
    /// Re-running before `sleep-end` replaces the previous pending bedtime
    /// (with a stderr notice). A pending bedtime older than 24h is treated
    /// as stale and discarded by `sleep-end`.
    SleepStart {
        /// Bedtime in HH:MM (24h) or H:MMam/pm (12h)
        time: Option<String>,
    },
    /// Finalize sleep entry on today's note (uses now, or pass a wake time)
    ///
    /// Reads the pending bedtime from `daylog sleep-start` and writes
    /// `sleep: "bedtime-waketime"` to today's note. The wake date is
    /// always calendar today (the date on the wall clock), independent of
    /// `day_start_hour` — bedtimes past midnight land on the wake-day's
    /// note, which is the convention this command exists to enforce.
    ///
    /// The written value is rendered per `time_format` from your config
    /// (`12h` or `24h`); the database always stores canonical 24h.
    SleepEnd {
        /// Wake time in HH:MM (24h) or H:MMam/pm (12h)
        time: Option<String>,
    },
    /// Log a food entry to the day's `## Food` section
    Food {
        /// Name (literal or nutrition-db alias)
        name: String,
        /// Amount with optional unit (e.g., 500g, 250ml). Required for
        /// per_100g/per_100ml entries; optional for total-panel entries.
        amount: Option<String>,
        /// Custom kcal value (skips nutrition-db lookup; requires
        /// --protein, --carbs, --fat to also be set)
        #[arg(long)]
        kcal: Option<f64>,
        #[arg(long)]
        protein: Option<f64>,
        #[arg(long)]
        carbs: Option<f64>,
        #[arg(long)]
        fat: Option<f64>,
        #[arg(long)]
        gi: Option<f64>,
        #[arg(long)]
        gl: Option<f64>,
        #[arg(long)]
        ii: Option<f64>,
        /// Override target date (YYYY-MM-DD). Default: effective_today.
        #[arg(long)]
        date: Option<String>,
        /// Override entry time (HH:MM 24h or H:MMam/pm 12h). Default: now.
        #[arg(long)]
        time: Option<String>,
    },
    /// Log a free-text note to the day's `## Notes` section
    Note {
        #[arg(long)]
        date: Option<String>,
        #[arg(long)]
        time: Option<String>,
        /// Note text or [notes.aliases] key (joined; no shell quoting needed)
        #[arg(trailing_var_arg = true)]
        text: Vec<String>,
    },
    /// Log a blood pressure reading (YAML + `## Vitals` line)
    Bp {
        sys: i32,
        dia: i32,
        pulse: i32,
        #[arg(long, conflicts_with = "evening")]
        morning: bool,
        #[arg(long)]
        evening: bool,
        #[arg(long)]
        date: Option<String>,
        #[arg(long)]
        time: Option<String>,
    },
}

/// Helpers shared by food/note/bp for resolving --date and --time flags
/// and rendering the timestamp prefix per `config.time_format`.
pub mod resolve {
    use chrono::{Local, NaiveDate, NaiveTime};
    use color_eyre::eyre::Result;
    use color_eyre::Help;

    use crate::config::Config;
    use crate::time;

    /// Resolve the target date for a logging command. `--date` overrides;
    /// otherwise `config.effective_today_date()`.
    pub fn target_date(flag: Option<&str>, config: &Config) -> Result<NaiveDate> {
        match flag {
            Some(s) => NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
                .map_err(|_| color_eyre::eyre::eyre!("Invalid --date: '{s}'. Expected YYYY-MM-DD."))
                .suggestion("Use a date in YYYY-MM-DD form, e.g., 2026-04-30."),
            None => Ok(config.effective_today_date()),
        }
    }

    /// Resolve the timestamp for the `**HH:MM**` prefix and BP slot
    /// detection. `--time` overrides; otherwise `Local::now().time()`.
    pub fn target_time(flag: Option<&str>) -> Result<NaiveTime> {
        match flag {
            Some(s) => time::parse_time(s)
                .ok_or_else(|| {
                    color_eyre::eyre::eyre!(
                        "Invalid --time: '{s}'. Expected HH:MM (24h) or H:MMam/pm (12h)."
                    )
                })
                .suggestion("Examples: 22:30, 07:05, 10:30pm, 6:15am."),
            None => Ok(Local::now().time()),
        }
    }
}
