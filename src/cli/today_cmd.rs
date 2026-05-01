//! `daylog today [date]` — print a compact daily summary.

use color_eyre::eyre::Result;

use crate::config::Config;

pub fn execute(_date_flag: Option<&str>, _json: bool, _config: &Config) -> Result<()> {
    color_eyre::eyre::bail!("daylog today: not yet implemented")
}
