use crate::modules::{InsertOp, Module};
use color_eyre::eyre::{Result, WrapErr};
use rusqlite::{Connection, OpenFlags};
use std::path::Path;

const CORE_SCHEMA: &str = "
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;
PRAGMA busy_timeout = 5000;

CREATE TABLE IF NOT EXISTS days (
    date TEXT PRIMARY KEY,
    sleep_start TEXT,
    sleep_end TEXT,
    sleep_hours REAL,
    sleep_quality INTEGER,
    mood INTEGER,
    energy INTEGER,
    weight REAL,
    notes TEXT,
    file_mtime REAL,
    parsed_at TEXT
);

CREATE TABLE IF NOT EXISTS metrics (
    date TEXT NOT NULL REFERENCES days(date) ON DELETE CASCADE,
    name TEXT NOT NULL,
    value REAL NOT NULL,
    PRIMARY KEY (date, name)
);

CREATE TABLE IF NOT EXISTS sync_meta (
    key TEXT PRIMARY KEY,
    value TEXT
);

CREATE TABLE IF NOT EXISTS foods (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    kcal_per_100g       REAL,
    protein_per_100g    REAL,
    carbs_per_100g      REAL,
    fat_per_100g        REAL,
    sat_fat_per_100g    REAL,
    sugar_per_100g      REAL,
    salt_per_100g       REAL,
    fiber_per_100g      REAL,
    kcal_per_100ml      REAL,
    protein_per_100ml   REAL,
    carbs_per_100ml     REAL,
    fat_per_100ml       REAL,
    sat_fat_per_100ml   REAL,
    sugar_per_100ml     REAL,
    salt_per_100ml      REAL,
    fiber_per_100ml     REAL,
    density_g_per_ml    REAL,
    total_weight_g      REAL,
    total_kcal          REAL,
    total_protein       REAL,
    total_carbs         REAL,
    total_fat           REAL,
    total_sat_fat       REAL,
    total_sugar         REAL,
    total_salt          REAL,
    total_fiber         REAL,
    gi                  REAL,
    gl_per_100g         REAL,
    gl_per_100ml        REAL,
    ii                  REAL,
    description         TEXT,
    notes               TEXT
);

CREATE TABLE IF NOT EXISTS food_aliases (
    food_id INTEGER NOT NULL REFERENCES foods(id) ON DELETE CASCADE,
    alias TEXT NOT NULL,
    PRIMARY KEY (food_id, alias)
);

CREATE INDEX IF NOT EXISTS idx_food_aliases_alias ON food_aliases(alias);

CREATE TABLE IF NOT EXISTS food_ingredients (
    food_id INTEGER NOT NULL REFERENCES foods(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    ingredient_name TEXT NOT NULL,
    amount_g REAL,
    PRIMARY KEY (food_id, position)
);
";

/// Open a read-write connection for the watcher thread.
pub fn open_rw(path: &Path) -> Result<Connection> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
    )
    .wrap_err_with(|| format!("Failed to open database at {}", path.display()))?;
    conn.execute_batch(
        "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 5000;",
    )
    .wrap_err("Failed to set database pragmas")?;
    Ok(conn)
}

/// Open a read-only connection for the TUI thread.
pub fn open_ro(path: &Path) -> Result<Connection> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).wrap_err_with(
        || {
            format!(
                "Failed to open database at {}. Is another daylog instance running?",
                path.display()
            )
        },
    )?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .wrap_err("Failed to set database pragmas")?;
    Ok(conn)
}

/// Initialize the database: create core tables + module tables.
pub fn init_db(conn: &Connection, modules: &[Box<dyn Module>]) -> Result<()> {
    conn.execute_batch(CORE_SCHEMA)
        .wrap_err("Failed to create core schema")?;

    for module in modules {
        let schema = module.schema();
        if !schema.is_empty() {
            conn.execute_batch(schema).wrap_err_with(|| {
                format!("Failed to create schema for module '{}'", module.id())
            })?;
        }
    }
    Ok(())
}

/// Execute a set of InsertOps within an existing transaction context.
/// The caller is responsible for BEGIN/COMMIT.
pub fn execute_insert_ops(conn: &Connection, date: &str, ops: &[InsertOp]) -> Result<()> {
    for op in ops {
        match op {
            InsertOp::Metric { name, value } => {
                conn.execute(
                    "INSERT OR REPLACE INTO metrics (date, name, value) VALUES (?1, ?2, ?3)",
                    rusqlite::params![date, name, value],
                )?;
            }
            InsertOp::Row { table, columns } => {
                if !crate::modules::is_valid_module_table(table) {
                    color_eyre::eyre::bail!(
                        "Module tried to insert into table '{}' which it didn't create in schema()",
                        table
                    );
                }
                let col_names: Vec<&str> = columns.iter().map(|(name, _)| *name).collect();
                let placeholders: Vec<String> =
                    (1..=columns.len()).map(|i| format!("?{i}")).collect();
                let sql = format!(
                    "INSERT INTO {} ({}) VALUES ({})",
                    table,
                    col_names.join(", "),
                    placeholders.join(", ")
                );
                let params: Vec<Box<dyn rusqlite::types::ToSql>> =
                    columns.iter().map(|(_, v)| v.to_sql()).collect();
                let param_refs: Vec<&dyn rusqlite::types::ToSql> =
                    params.iter().map(|p| p.as_ref()).collect();
                conn.execute(&sql, param_refs.as_slice())?;
            }
        }
    }
    Ok(())
}

/// Delete all data for a given date (CASCADE handles child tables).
pub fn delete_date(conn: &Connection, date: &str) -> Result<()> {
    conn.execute("DELETE FROM days WHERE date = ?1", [date])?;
    Ok(())
}

/// Insert a core days row.
#[allow(clippy::too_many_arguments)]
pub fn insert_day(
    conn: &Connection,
    date: &str,
    sleep_start: Option<&str>,
    sleep_end: Option<&str>,
    sleep_hours: Option<f64>,
    sleep_quality: Option<i32>,
    mood: Option<i32>,
    energy: Option<i32>,
    weight: Option<f64>,
    notes: Option<&str>,
    file_mtime: f64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO days (date, sleep_start, sleep_end, sleep_hours, sleep_quality, mood, energy, weight, notes, file_mtime, parsed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, datetime('now'))",
        rusqlite::params![
            date,
            sleep_start,
            sleep_end,
            sleep_hours,
            sleep_quality,
            mood,
            energy,
            weight,
            notes,
            file_mtime,
        ],
    )?;
    Ok(())
}

/// Get the last sync timestamp from sync_meta.
pub fn get_last_sync(conn: &Connection) -> Result<Option<f64>> {
    let mut stmt = conn.prepare("SELECT value FROM sync_meta WHERE key = 'last_sync'")?;
    let result: Option<String> = stmt.query_row([], |row| row.get(0)).ok();
    match result {
        Some(s) => Ok(s.parse::<f64>().ok()),
        None => Ok(None),
    }
}

/// Update the last sync timestamp.
pub fn set_last_sync(conn: &Connection, timestamp: f64) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO sync_meta (key, value) VALUES ('last_sync', ?1)",
        [timestamp.to_string()],
    )?;
    Ok(())
}

/// Load today's data for status output.
pub fn load_today(conn: &Connection, date: &str) -> Result<Option<serde_json::Value>> {
    let mut stmt = conn.prepare(
        "SELECT date, sleep_start, sleep_end, sleep_hours, sleep_quality, mood, energy, weight, notes
         FROM days WHERE date = ?1",
    )?;
    let result = stmt
        .query_row([date], |row| {
            Ok(serde_json::json!({
                "date": row.get::<_, Option<String>>(0)?,
                "sleep_start": row.get::<_, Option<String>>(1)?,
                "sleep_end": row.get::<_, Option<String>>(2)?,
                "sleep_hours": row.get::<_, Option<f64>>(3)?,
                "sleep_quality": row.get::<_, Option<i32>>(4)?,
                "mood": row.get::<_, Option<i32>>(5)?,
                "energy": row.get::<_, Option<i32>>(6)?,
                "weight": row.get::<_, Option<f64>>(7)?,
                "notes": row.get::<_, Option<String>>(8)?,
            }))
        })
        .ok();
    Ok(result)
}

/// Load weight trend for the last N days.
pub fn load_weight_trend(conn: &Connection, days: i32) -> Result<Vec<(String, f64)>> {
    let mut stmt = conn.prepare(
        "SELECT date, weight FROM days WHERE weight IS NOT NULL ORDER BY date DESC LIMIT ?1",
    )?;
    let rows = stmt
        .query_map([days], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Load metrics for a date.
pub fn load_metrics(conn: &Connection, date: &str) -> Result<Vec<(String, f64)>> {
    let mut stmt = conn.prepare("SELECT name, value FROM metrics WHERE date = ?1")?;
    let rows = stmt
        .query_map([date], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Load metric trend for the last N days.
pub fn load_metric_trend(
    conn: &Connection,
    metric_name: &str,
    days: i32,
) -> Result<Vec<(String, f64)>> {
    let mut stmt = conn.prepare(
        "SELECT m.date, m.value FROM metrics m
         JOIN days d ON d.date = m.date
         WHERE m.name = ?1 ORDER BY m.date DESC LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![metric_name, days], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_core_schema_creates_tables() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(CORE_SCHEMA).unwrap();

        // Verify tables exist
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();

        assert!(tables.contains(&"days".to_string()));
        assert!(tables.contains(&"metrics".to_string()));
        assert!(tables.contains(&"sync_meta".to_string()));
    }

    #[test]
    fn test_cascade_delete() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(CORE_SCHEMA).unwrap();

        insert_day(
            &conn,
            "2026-03-28",
            None,
            None,
            None,
            None,
            None,
            None,
            Some(173.4),
            None,
            0.0,
        )
        .unwrap();
        conn.execute(
            "INSERT INTO metrics (date, name, value) VALUES ('2026-03-28', 'resting_hr', 52.0)",
            [],
        )
        .unwrap();

        delete_date(&conn, "2026-03-28").unwrap();

        let count: i32 = conn
            .query_row("SELECT COUNT(*) FROM metrics", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_sync_meta() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(CORE_SCHEMA).unwrap();

        assert!(get_last_sync(&conn).unwrap().is_none());
        set_last_sync(&conn, 1234567890.0).unwrap();
        assert_eq!(get_last_sync(&conn).unwrap(), Some(1234567890.0));
    }

    #[test]
    fn test_core_schema_creates_food_tables() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(CORE_SCHEMA).unwrap();

        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();

        assert!(tables.contains(&"foods".to_string()));
        assert!(tables.contains(&"food_aliases".to_string()));
        assert!(tables.contains(&"food_ingredients".to_string()));
    }

    #[test]
    fn test_food_aliases_index_exists() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(CORE_SCHEMA).unwrap();

        let indices: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='index' AND name LIKE 'idx_%'")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();

        assert!(indices.contains(&"idx_food_aliases_alias".to_string()));
    }

    #[test]
    fn test_food_cascade_delete() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(CORE_SCHEMA).unwrap();

        conn.execute(
            "INSERT INTO foods (name, kcal_per_100g) VALUES ('Test Food', 100)",
            [],
        )
        .unwrap();
        let food_id: i64 = conn
            .query_row("SELECT id FROM foods WHERE name = 'Test Food'", [], |r| {
                r.get(0)
            })
            .unwrap();
        conn.execute(
            "INSERT INTO food_aliases (food_id, alias) VALUES (?1, 'test')",
            [food_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO food_ingredients (food_id, position, ingredient_name, amount_g)
             VALUES (?1, 0, 'whey', 50.0)",
            [food_id],
        )
        .unwrap();

        conn.execute("DELETE FROM foods", []).unwrap();

        let alias_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM food_aliases", [], |r| r.get(0))
            .unwrap();
        let ingredient_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM food_ingredients", [], |r| r.get(0))
            .unwrap();
        assert_eq!(alias_count, 0);
        assert_eq!(ingredient_count, 0);
    }
}
