use crate::modules::{InsertOp, Module};
use color_eyre::eyre::{Result, WrapErr};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
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

#[cfg(test)]
pub const CORE_SCHEMA_TEST_HOOK: &str = CORE_SCHEMA;

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

// --- Foods (nutrition database) ---

#[derive(Debug, Clone, Default, PartialEq)]
pub struct NutrientPanel {
    pub kcal: Option<f64>,
    pub protein: Option<f64>,
    pub carbs: Option<f64>,
    pub fat: Option<f64>,
    pub sat_fat: Option<f64>,
    pub sugar: Option<f64>,
    pub salt: Option<f64>,
    pub fiber: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TotalPanel {
    pub weight_g: Option<f64>,
    pub kcal: Option<f64>,
    pub protein: Option<f64>,
    pub carbs: Option<f64>,
    pub fat: Option<f64>,
    pub sat_fat: Option<f64>,
    pub sugar: Option<f64>,
    pub salt: Option<f64>,
    pub fiber: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FoodIngredient {
    pub ingredient_name: String,
    pub amount_g: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct FoodInsert {
    pub name: String,
    pub per_100g: Option<NutrientPanel>,
    pub per_100ml: Option<NutrientPanel>,
    pub density_g_per_ml: Option<f64>,
    pub total: Option<TotalPanel>,
    pub gi: Option<f64>,
    pub gl_per_100g: Option<f64>,
    pub gl_per_100ml: Option<f64>,
    pub ii: Option<f64>,
    pub description: Option<String>,
    pub notes: Option<String>,
    pub aliases: Vec<String>,
    pub ingredients: Vec<FoodIngredient>,
}

#[derive(Debug, Clone)]
pub struct FoodLookup {
    pub id: i64,
    pub name: String,
    pub per_100g: Option<NutrientPanel>,
    pub per_100ml: Option<NutrientPanel>,
    pub density_g_per_ml: Option<f64>,
    pub total: Option<TotalPanel>,
    pub gi: Option<f64>,
    pub gl_per_100g: Option<f64>,
    pub gl_per_100ml: Option<f64>,
    pub ii: Option<f64>,
    pub description: Option<String>,
    pub notes: Option<String>,
}

/// Delete every row in `foods`. CASCADEs to `food_aliases` and `food_ingredients`.
pub fn delete_all_foods(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM foods", [])?;
    Ok(())
}

/// Insert one food (plus its aliases and ingredients) and return the new id.
/// Returns Err on a UNIQUE conflict on `name` — caller decides whether to
/// skip-and-warn or abort. Caller is expected to run this inside a
/// transaction; partial writes can otherwise leak on alias/ingredient failure.
pub fn insert_food(conn: &Connection, food: &FoodInsert) -> Result<i64> {
    let default_panel = NutrientPanel::default();
    let default_total = TotalPanel::default();
    let p100g = food.per_100g.as_ref().unwrap_or(&default_panel);
    let p100ml = food.per_100ml.as_ref().unwrap_or(&default_panel);
    let total = food.total.as_ref().unwrap_or(&default_total);
    conn.execute(
        "INSERT INTO foods (
            name,
            kcal_per_100g, protein_per_100g, carbs_per_100g, fat_per_100g,
            sat_fat_per_100g, sugar_per_100g, salt_per_100g, fiber_per_100g,
            kcal_per_100ml, protein_per_100ml, carbs_per_100ml, fat_per_100ml,
            sat_fat_per_100ml, sugar_per_100ml, salt_per_100ml, fiber_per_100ml,
            density_g_per_ml,
            total_weight_g, total_kcal, total_protein, total_carbs, total_fat,
            total_sat_fat, total_sugar, total_salt, total_fiber,
            gi, gl_per_100g, gl_per_100ml, ii,
            description, notes
        ) VALUES (
            ?1,
            ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9,
            ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
            ?18,
            ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27,
            ?28, ?29, ?30, ?31,
            ?32, ?33
        )",
        rusqlite::params![
            food.name,
            p100g.kcal,
            p100g.protein,
            p100g.carbs,
            p100g.fat,
            p100g.sat_fat,
            p100g.sugar,
            p100g.salt,
            p100g.fiber,
            p100ml.kcal,
            p100ml.protein,
            p100ml.carbs,
            p100ml.fat,
            p100ml.sat_fat,
            p100ml.sugar,
            p100ml.salt,
            p100ml.fiber,
            food.density_g_per_ml,
            total.weight_g,
            total.kcal,
            total.protein,
            total.carbs,
            total.fat,
            total.sat_fat,
            total.sugar,
            total.salt,
            total.fiber,
            food.gi,
            food.gl_per_100g,
            food.gl_per_100ml,
            food.ii,
            food.description,
            food.notes,
        ],
    )?;
    let id = conn.last_insert_rowid();

    for alias in &food.aliases {
        conn.execute(
            "INSERT OR IGNORE INTO food_aliases (food_id, alias) VALUES (?1, ?2)",
            rusqlite::params![id, alias],
        )?;
    }
    for (pos, ing) in food.ingredients.iter().enumerate() {
        conn.execute(
            "INSERT INTO food_ingredients (food_id, position, ingredient_name, amount_g)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, pos as i64, ing.ingredient_name, ing.amount_g],
        )?;
    }
    Ok(id)
}

/// Case-insensitive lookup. Lowercases `query` before matching against
/// `food_aliases.alias` (which is stored already lowercased — including
/// the auto-inserted lowercased heading). Returns `None` if no match.
pub fn lookup_food_by_name_or_alias(conn: &Connection, query: &str) -> Result<Option<FoodLookup>> {
    let needle = query.trim().to_lowercase();
    let row = conn.query_row(
        "SELECT
            f.id, f.name,
            f.kcal_per_100g, f.protein_per_100g, f.carbs_per_100g, f.fat_per_100g,
            f.sat_fat_per_100g, f.sugar_per_100g, f.salt_per_100g, f.fiber_per_100g,
            f.kcal_per_100ml, f.protein_per_100ml, f.carbs_per_100ml, f.fat_per_100ml,
            f.sat_fat_per_100ml, f.sugar_per_100ml, f.salt_per_100ml, f.fiber_per_100ml,
            f.density_g_per_ml,
            f.total_weight_g, f.total_kcal, f.total_protein, f.total_carbs, f.total_fat,
            f.total_sat_fat, f.total_sugar, f.total_salt, f.total_fiber,
            f.gi, f.gl_per_100g, f.gl_per_100ml, f.ii,
            f.description, f.notes
         FROM foods f JOIN food_aliases a ON a.food_id = f.id
         WHERE a.alias = ?1 LIMIT 1",
        [&needle],
        |r| {
            let panel_g = NutrientPanel {
                kcal: r.get(2)?,
                protein: r.get(3)?,
                carbs: r.get(4)?,
                fat: r.get(5)?,
                sat_fat: r.get(6)?,
                sugar: r.get(7)?,
                salt: r.get(8)?,
                fiber: r.get(9)?,
            };
            let panel_ml = NutrientPanel {
                kcal: r.get(10)?,
                protein: r.get(11)?,
                carbs: r.get(12)?,
                fat: r.get(13)?,
                sat_fat: r.get(14)?,
                sugar: r.get(15)?,
                salt: r.get(16)?,
                fiber: r.get(17)?,
            };
            let total = TotalPanel {
                weight_g: r.get(19)?,
                kcal: r.get(20)?,
                protein: r.get(21)?,
                carbs: r.get(22)?,
                fat: r.get(23)?,
                sat_fat: r.get(24)?,
                sugar: r.get(25)?,
                salt: r.get(26)?,
                fiber: r.get(27)?,
            };
            Ok(FoodLookup {
                id: r.get(0)?,
                name: r.get(1)?,
                per_100g: nutrient_panel_or_none(&panel_g),
                per_100ml: nutrient_panel_or_none(&panel_ml),
                density_g_per_ml: r.get(18)?,
                total: total_panel_or_none(&total),
                gi: r.get(28)?,
                gl_per_100g: r.get(29)?,
                gl_per_100ml: r.get(30)?,
                ii: r.get(31)?,
                description: r.get(32)?,
                notes: r.get(33)?,
            })
        },
    );
    match row {
        Ok(food) => Ok(Some(food)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn nutrient_panel_or_none(p: &NutrientPanel) -> Option<NutrientPanel> {
    if p.kcal.is_none()
        && p.protein.is_none()
        && p.carbs.is_none()
        && p.fat.is_none()
        && p.sat_fat.is_none()
        && p.sugar.is_none()
        && p.salt.is_none()
        && p.fiber.is_none()
    {
        None
    } else {
        Some(p.clone())
    }
}

fn total_panel_or_none(p: &TotalPanel) -> Option<TotalPanel> {
    if p.weight_g.is_none()
        && p.kcal.is_none()
        && p.protein.is_none()
        && p.carbs.is_none()
        && p.fat.is_none()
        && p.sat_fat.is_none()
        && p.sugar.is_none()
        && p.salt.is_none()
        && p.fiber.is_none()
    {
        None
    } else {
        Some(p.clone())
    }
}

#[derive(Debug, Clone)]
pub struct NutritionStatus {
    pub foods_count: i64,
    pub last_synced: Option<String>,
}

pub fn nutrition_status(conn: &Connection) -> Result<NutritionStatus> {
    let foods_count: i64 = conn.query_row("SELECT COUNT(*) FROM foods", [], |r| r.get(0))?;
    let last_synced: Option<String> = conn
        .query_row(
            "SELECT value FROM sync_meta WHERE key = 'last_nutrition_sync'",
            [],
            |r| r.get(0),
        )
        .optional()?
        .filter(|s: &String| !s.is_empty());
    Ok(NutritionStatus {
        foods_count,
        last_synced,
    })
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

    fn sample_food_insert() -> FoodInsert {
        FoodInsert {
            name: "Kelda Skogssvampsoppa".to_string(),
            per_100g: Some(NutrientPanel {
                kcal: Some(70.0),
                protein: Some(1.4),
                carbs: Some(4.8),
                fat: Some(5.0),
                sat_fat: Some(3.0),
                sugar: Some(1.6),
                salt: Some(0.89),
                fiber: None,
            }),
            per_100ml: None,
            density_g_per_ml: None,
            total: None,
            gi: Some(40.0),
            gl_per_100g: Some(2.0),
            gl_per_100ml: None,
            ii: Some(35.0),
            description: None,
            notes: Some("svamp + grädde".to_string()),
            aliases: vec![
                "kelda skogssvampsoppa".to_string(),
                "skogssvampsoppa".to_string(),
            ],
            ingredients: vec![],
        }
    }

    #[test]
    fn test_insert_food_returns_id() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(CORE_SCHEMA).unwrap();

        let id = insert_food(&conn, &sample_food_insert()).unwrap();
        assert!(id > 0);
    }

    #[test]
    fn test_insert_food_writes_aliases_and_ingredients() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(CORE_SCHEMA).unwrap();

        let mut food = sample_food_insert();
        food.ingredients = vec![
            FoodIngredient {
                ingredient_name: "Whey".to_string(),
                amount_g: Some(62.0),
            },
            FoodIngredient {
                ingredient_name: "Water".to_string(),
                amount_g: None,
            },
        ];
        let id = insert_food(&conn, &food).unwrap();

        let alias_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM food_aliases WHERE food_id = ?1",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(alias_count, 2);

        let ingredients: Vec<(i64, String, Option<f64>)> = conn
            .prepare(
                "SELECT position, ingredient_name, amount_g
                 FROM food_ingredients WHERE food_id = ?1 ORDER BY position",
            )
            .unwrap()
            .query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(ingredients.len(), 2);
        assert_eq!(ingredients[0].0, 0);
        assert_eq!(ingredients[0].1, "Whey");
        assert_eq!(ingredients[0].2, Some(62.0));
        assert_eq!(ingredients[1].0, 1);
        assert_eq!(ingredients[1].2, None);
    }

    #[test]
    fn test_lookup_food_by_name_case_insensitive() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(CORE_SCHEMA).unwrap();
        insert_food(&conn, &sample_food_insert()).unwrap();

        let by_lower = lookup_food_by_name_or_alias(&conn, "kelda skogssvampsoppa")
            .unwrap()
            .unwrap();
        let by_canonical = lookup_food_by_name_or_alias(&conn, "Kelda Skogssvampsoppa")
            .unwrap()
            .unwrap();
        let by_alias = lookup_food_by_name_or_alias(&conn, "Skogssvampsoppa")
            .unwrap()
            .unwrap();

        assert_eq!(by_lower.id, by_canonical.id);
        assert_eq!(by_lower.id, by_alias.id);
        assert_eq!(by_lower.name, "Kelda Skogssvampsoppa");
        assert!(by_lower.per_100g.is_some());
        assert_eq!(by_lower.per_100g.as_ref().unwrap().kcal, Some(70.0));
    }

    #[test]
    fn test_lookup_food_missing_returns_none() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(CORE_SCHEMA).unwrap();
        assert!(lookup_food_by_name_or_alias(&conn, "ghost food")
            .unwrap()
            .is_none());
    }

    #[test]
    fn test_unique_name_conflict() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(CORE_SCHEMA).unwrap();
        insert_food(&conn, &sample_food_insert()).unwrap();
        let err = insert_food(&conn, &sample_food_insert()).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.to_lowercase().contains("unique") || msg.contains("constraint"),
            "expected UNIQUE-style error, got: {msg}"
        );
    }

    #[test]
    fn test_delete_all_foods_clears_children() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(CORE_SCHEMA).unwrap();
        insert_food(&conn, &sample_food_insert()).unwrap();

        delete_all_foods(&conn).unwrap();

        let food_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM foods", [], |r| r.get(0))
            .unwrap();
        let alias_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM food_aliases", [], |r| r.get(0))
            .unwrap();
        assert_eq!(food_count, 0);
        assert_eq!(alias_count, 0);
    }

    #[test]
    fn test_nutrition_status_empty() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(CORE_SCHEMA).unwrap();

        let s = nutrition_status(&conn).unwrap();
        assert_eq!(s.foods_count, 0);
        assert!(s.last_synced.is_none());
    }

    #[test]
    fn test_nutrition_status_after_insert() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(CORE_SCHEMA).unwrap();
        insert_food(&conn, &sample_food_insert()).unwrap();
        conn.execute(
            "INSERT INTO sync_meta (key, value) VALUES ('last_nutrition_sync', '2026-04-29T14:22:11')",
            [],
        )
        .unwrap();

        let s = nutrition_status(&conn).unwrap();
        assert_eq!(s.foods_count, 1);
        assert_eq!(s.last_synced.as_deref(), Some("2026-04-29T14:22:11"));
    }

    #[test]
    fn test_nutrition_status_empty_string_treated_as_none() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(CORE_SCHEMA).unwrap();
        conn.execute(
            "INSERT INTO sync_meta (key, value) VALUES ('last_nutrition_sync', '')",
            [],
        )
        .unwrap();

        let s = nutrition_status(&conn).unwrap();
        assert!(s.last_synced.is_none());
    }
}
