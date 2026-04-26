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
}
