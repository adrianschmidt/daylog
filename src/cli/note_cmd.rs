//! `daylog note` — append a free-text note to the day's `## Notes` section.

use color_eyre::eyre::Result;

use crate::config::Config;

pub fn execute(
    _text: &[String],
    _date: Option<&str>,
    _time: Option<&str>,
    _config: &Config,
) -> Result<()> {
    color_eyre::eyre::bail!("daylog note: not yet implemented")
}
