//! `daylog bp` — write blood pressure to YAML + append a `## Vitals` line.

use color_eyre::eyre::Result;

use crate::config::Config;

#[allow(clippy::too_many_arguments)]
pub fn execute(
    _sys: i32,
    _dia: i32,
    _pulse: i32,
    _morning: bool,
    _evening: bool,
    _date: Option<&str>,
    _time: Option<&str>,
    _config: &Config,
) -> Result<()> {
    color_eyre::eyre::bail!("daylog bp: not yet implemented")
}
