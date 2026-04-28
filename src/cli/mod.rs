pub mod completions;
pub mod log_cmd;
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
}
