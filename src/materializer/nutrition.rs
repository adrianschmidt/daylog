use crate::config::Config;
use color_eyre::eyre::Result;
use rusqlite::Connection;
use std::path::Path;

/// Parse `nutrition-db.md` and replace the `foods` table contents.
/// Returns the number of foods successfully inserted.
/// Missing or empty file → silent no-op, returns 0.
pub fn materialize_nutrition_db(
    _conn: &Connection,
    _file_path: &Path,
    _config: &Config,
) -> Result<usize> {
    Ok(0)
}
