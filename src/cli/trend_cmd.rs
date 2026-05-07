//! `vitalog trend <field> [days]` — print a chart of recent values.

use color_eyre::eyre::Result;

use crate::config::Config;

pub fn execute(_field: &str, _days: u32, _compact: bool, _json: bool, _config: &Config) -> Result<()> {
    color_eyre::eyre::bail!("trend command not yet implemented");
}
