# Structured Nutrition Database Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Adopt a structured `nutrition-db.md` convention (one entry per `## Heading`, with a fenced ` ```yaml ` block) so the daylog watcher and `rebuild`/`sync` commands materialize foods into three new core SQLite tables (`foods`, `food_aliases`, `food_ingredients`).

**Architecture:** The existing flat `src/materializer.rs` is split into `src/materializer/{mod.rs, daily.rs, nutrition.rs}` so each parser owns its file. A line-oriented state machine in `nutrition.rs` splits the markdown by `## ` headings, parses each fenced YAML block via `yaml-rust2`, and produces `FoodInsert` values. The watcher's existing per-file dispatch widens to recognize `nutrition-db.md` in addition to `YYYY-MM-DD.md`. Foods data is core (always present), not module-owned, because lookup is independent of which TUI modules are enabled.

**Tech Stack:** Rust 2021, rusqlite (bundled), yaml-rust2, regex, color-eyre, serde_json. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-04-29-structured-nutrition-db-design.md`

---

## File Structure

**New files:**
- `src/materializer/mod.rs` — re-exports the public surface that `crate::materializer::*` callers expect
- `src/materializer/daily.rs` — the existing daily-note parser, lifted from `src/materializer.rs` with no behavior change, plus `FileKind` and `materialized_file_kind`
- `src/materializer/nutrition.rs` — entry splitter, YAML→`FoodInsert` builder, and `materialize_nutrition_db`

**Modified files:**
- `src/materializer.rs` — deleted (replaced by directory)
- `src/db.rs` — three new tables in `CORE_SCHEMA`; new `NutrientPanel`/`TotalPanel`/`FoodIngredient`/`FoodInsert`/`FoodLookup`/`NutritionStatus` types and `insert_food`/`delete_all_foods`/`lookup_food_by_name_or_alias`/`nutrition_status` helpers
- `src/main.rs` — `cmd_status` surfaces `nutrition_db` field
- `README.md` — new "Nutrition database" section
- `CLAUDE.md` — File Map entries for the new files

**Test scope:**
- `src/db.rs` `mod tests` — schema, cascade, roundtrip, lookup, conflict, status helpers
- `src/materializer/daily.rs` `mod tests` — existing tests preserved verbatim, plus `materialized_file_kind`
- `src/materializer/nutrition.rs` `mod tests` — entry splitting, validation, insertion roundtrip, partial-failure
- `tests/integration.rs` — `sync_all`/`rebuild_all` include nutrition-db.md, mtime-gating, missing-file silence

---

## Task 1: Split `src/materializer.rs` into a directory module

Pure mechanical refactor. No behavior change. The existing test suite is the regression guard.

**Files:**
- Create: `src/materializer/mod.rs`
- Create: `src/materializer/daily.rs`
- Delete: `src/materializer.rs`

- [ ] **Step 1: Confirm baseline tests pass**

Run: `cargo test --lib materializer`
Expected: all materializer tests pass. Note the count.

- [ ] **Step 2: Create `src/materializer/daily.rs` with the current contents**

```bash
mkdir -p src/materializer
git mv src/materializer.rs src/materializer/daily.rs
```

- [ ] **Step 3: Create `src/materializer/mod.rs`**

```rust
mod daily;
pub mod nutrition;

pub use daily::{
    materialize_file, preprocess_yaml, rebuild_all, start_watcher, sync_all, yaml_f64_field,
    yaml_i32_field, yaml_str_field, FileKind, materialized_file_kind,
};
pub use nutrition::materialize_nutrition_db;
```

`FileKind` and `materialized_file_kind` and `nutrition` don't exist yet — Step 4 stubs them so the module compiles.

- [ ] **Step 4: Add minimal stubs so the tree compiles**

In `src/materializer/daily.rs`, add near the top (above existing imports):

```rust
/// What kind of file the materializer recognizes. Used by the watcher
/// dispatch and by sync/rebuild to pick the right parser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    DailyNote,
    NutritionDb,
}

/// Classify a path. Returns `None` for hidden, swap, or unrelated files.
pub fn materialized_file_kind(path: &std::path::Path) -> Option<FileKind> {
    if is_note_file(path) {
        return Some(FileKind::DailyNote);
    }
    let filename = path.file_name().and_then(|f| f.to_str())?;
    if filename == "nutrition-db.md" {
        return Some(FileKind::NutritionDb);
    }
    None
}
```

Create `src/materializer/nutrition.rs` with the bare minimum:

```rust
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
```

- [ ] **Step 5: Verify the workspace compiles and tests pass**

Run: `cargo build && cargo test --lib materializer`
Expected: clean build, all materializer tests pass with the same count as Step 1.

- [ ] **Step 6: Run the full suite to catch external import breakage**

Run: `cargo test`
Expected: full suite passes.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "refactor: split materializer.rs into materializer/ module

No behavior change. Lift the existing parser into materializer/daily.rs,
add a materializer/mod.rs that preserves the existing public surface via
re-exports, and create a materializer/nutrition.rs stub that subsequent
tasks fill in. Add FileKind and materialized_file_kind so the watcher
can dispatch by file type."
```

---

## Task 2: Add `foods`, `food_aliases`, `food_ingredients` schema

Schema only. No insert/lookup helpers yet — those come in Task 3 so they get their own focused tests.

**Files:**
- Modify: `src/db.rs`

- [ ] **Step 1: Write failing schema tests**

Append to `mod tests` in `src/db.rs`:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib db::tests::test_core_schema_creates_food_tables db::tests::test_food_aliases_index_exists db::tests::test_food_cascade_delete`
Expected: FAIL — `no such table: foods`.

- [ ] **Step 3: Extend `CORE_SCHEMA` in `src/db.rs`**

Append to the `CORE_SCHEMA` const string (after the `sync_meta` table):

```sql

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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib db::tests::test_core_schema_creates_food_tables db::tests::test_food_aliases_index_exists db::tests::test_food_cascade_delete`
Expected: 3 PASS.

Also run the full db tests to confirm no regression:

Run: `cargo test --lib db::`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add src/db.rs
git commit -m "feat: add foods, food_aliases, food_ingredients schema

Wide column-per-nutrient layout matches the existing days table
pattern. CASCADE on food_id cleans up child rows when a food is
deleted; food_aliases.alias is indexed for case-insensitive lookup."
```

---

## Task 3: Add `FoodInsert`, `FoodLookup` types and `insert_food` / `delete_all_foods` / `lookup_food_by_name_or_alias`

**Files:**
- Modify: `src/db.rs`

- [ ] **Step 1: Write failing tests**

Append to `mod tests` in `src/db.rs`:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib db::tests::test_insert_food`
Expected: compilation error — `FoodInsert`, `NutrientPanel`, `insert_food`, etc. not defined.

- [ ] **Step 3: Add the public types and helpers to `src/db.rs`**

Append after the existing `load_metric_trend` function:

```rust
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
/// skip-and-warn or abort.
pub fn insert_food(conn: &Connection, food: &FoodInsert) -> Result<i64> {
    let p100g = food.per_100g.clone().unwrap_or_default();
    let p100ml = food.per_100ml.clone().unwrap_or_default();
    let total = food.total.clone().unwrap_or_default();
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
            p100g.kcal, p100g.protein, p100g.carbs, p100g.fat,
            p100g.sat_fat, p100g.sugar, p100g.salt, p100g.fiber,
            p100ml.kcal, p100ml.protein, p100ml.carbs, p100ml.fat,
            p100ml.sat_fat, p100ml.sugar, p100ml.salt, p100ml.fiber,
            food.density_g_per_ml,
            total.weight_g, total.kcal, total.protein, total.carbs, total.fat,
            total.sat_fat, total.sugar, total.salt, total.fiber,
            food.gi, food.gl_per_100g, food.gl_per_100ml, food.ii,
            food.description, food.notes,
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
pub fn lookup_food_by_name_or_alias(
    conn: &Connection,
    query: &str,
) -> Result<Option<FoodLookup>> {
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib db::tests`
Expected: all PASS, including the new food tests.

- [ ] **Step 5: Commit**

```bash
git add src/db.rs
git commit -m "feat: add FoodInsert/FoodLookup types and CRUD helpers

insert_food writes one INSERT to foods plus N INSERTs to food_aliases
and food_ingredients. lookup_food_by_name_or_alias lowercases the
query and JOINs through food_aliases (case-insensitive lookup)."
```

---

## Task 4: Add `nutrition_status` DB helper

**Files:**
- Modify: `src/db.rs`

- [ ] **Step 1: Write failing tests**

Append to `mod tests` in `src/db.rs`:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib db::tests::test_nutrition_status`
Expected: compilation error — `nutrition_status` and `NutritionStatus` not defined.

- [ ] **Step 3: Add the type and helper**

Append to `src/db.rs`:

```rust
#[derive(Debug, Clone)]
pub struct NutritionStatus {
    pub foods_count: i64,
    pub last_synced: Option<String>,
}

pub fn nutrition_status(conn: &Connection) -> Result<NutritionStatus> {
    let foods_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM foods", [], |r| r.get(0))
        .unwrap_or(0);
    let last_synced: Option<String> = conn
        .query_row(
            "SELECT value FROM sync_meta WHERE key = 'last_nutrition_sync'",
            [],
            |r| r.get(0),
        )
        .ok();
    Ok(NutritionStatus {
        foods_count,
        last_synced,
    })
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib db::tests::test_nutrition_status`
Expected: 2 PASS.

- [ ] **Step 5: Commit**

```bash
git add src/db.rs
git commit -m "feat: add nutrition_status helper for status JSON

Reads foods row count and last_nutrition_sync key from sync_meta.
Returns last_synced=None when the table has never been populated."
```

---

## Task 5: Test and tighten `materialized_file_kind`

The dispatch fn was stubbed in Task 1. This task pins down its behavior with tests.

**Files:**
- Modify: `src/materializer/daily.rs` (test additions)

- [ ] **Step 1: Write failing tests**

Append to `mod tests` in `src/materializer/daily.rs`:

```rust
    use std::path::PathBuf;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn file_kind_classifies_daily_note() {
        assert_eq!(
            materialized_file_kind(&p("2026-04-29.md")),
            Some(FileKind::DailyNote)
        );
        assert_eq!(
            materialized_file_kind(&p("/tmp/notes/2026-04-29.md")),
            Some(FileKind::DailyNote)
        );
    }

    #[test]
    fn file_kind_classifies_nutrition_db() {
        assert_eq!(
            materialized_file_kind(&p("nutrition-db.md")),
            Some(FileKind::NutritionDb)
        );
        assert_eq!(
            materialized_file_kind(&p("/tmp/notes/nutrition-db.md")),
            Some(FileKind::NutritionDb)
        );
    }

    #[test]
    fn file_kind_rejects_hidden_and_swap() {
        assert_eq!(materialized_file_kind(&p(".2026-04-29.md")), None);
        assert_eq!(materialized_file_kind(&p("~nutrition-db.md")), None);
        assert_eq!(materialized_file_kind(&p("nutrition-db.md~")), None);
    }

    #[test]
    fn file_kind_rejects_unrelated() {
        assert_eq!(materialized_file_kind(&p("README.md")), None);
        assert_eq!(materialized_file_kind(&p("notes.txt")), None);
        assert_eq!(materialized_file_kind(&p("food.md")), None);
        assert_eq!(materialized_file_kind(&p("2026-13-99.md")), None);
    }
```

- [ ] **Step 2: Run tests to verify they pass or fail per current stub**

Run: `cargo test --lib materializer::daily::tests::file_kind_`
Expected: `file_kind_rejects_hidden_and_swap` FAILS for `~nutrition-db.md` (the stub only checks `is_note_file` for hidden/swap; nutrition-db.md branch doesn't filter swap variants).

- [ ] **Step 3: Tighten `materialized_file_kind`**

Replace the stub in `src/materializer/daily.rs` with:

```rust
pub fn materialized_file_kind(path: &std::path::Path) -> Option<FileKind> {
    let filename = path.file_name().and_then(|f| f.to_str())?;
    if filename.starts_with('.') || filename.starts_with('~') || filename.ends_with('~') {
        return None;
    }
    if RE_NOTE_FILE.is_match(filename) {
        return Some(FileKind::DailyNote);
    }
    if filename == "nutrition-db.md" {
        return Some(FileKind::NutritionDb);
    }
    None
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib materializer::daily::tests::file_kind_`
Expected: 4 PASS.

- [ ] **Step 5: Commit**

```bash
git add src/materializer/daily.rs
git commit -m "feat: tighten materialized_file_kind with hidden/swap filter

Centralises the hidden/~swap exclusion that previously lived only in
is_note_file. Nutrition-db.md is now rejected when prefixed with ~ or
suffixed with ~ (editor swap files)."
```

---

## Task 6: Implement `nutrition::split_entries` (entry-splitting state machine)

**Files:**
- Modify: `src/materializer/nutrition.rs`

- [ ] **Step 1: Write failing tests**

Replace the contents of `src/materializer/nutrition.rs` with the stub plus a `mod tests` module:

```rust
use crate::config::Config;
use color_eyre::eyre::Result;
use rusqlite::Connection;
use std::path::Path;
use yaml_rust2::Yaml;

#[derive(Debug, Clone)]
pub(crate) struct ParsedEntry {
    pub name: String,
    pub yaml: Yaml,
    pub notes: Option<String>,
    pub line_number: usize,
}

pub fn materialize_nutrition_db(
    _conn: &Connection,
    _file_path: &Path,
    _config: &Config,
) -> Result<usize> {
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use yaml_rust2::Yaml;

    #[test]
    fn split_basic_three_entries() {
        let content = r#"# Nutrition

## Kelda Skogssvampsoppa

```yaml
per_100g:
  kcal: 70
```

Some prose here.

---

## Laktosfri helmjölk 3%

```yaml
per_100ml:
  kcal: 62
```

## proteinshake

```yaml
description: shake
total:
  weight_g: 462
  kcal: 234
```
"#;
        let entries = split_entries(content);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].name, "Kelda Skogssvampsoppa");
        assert_eq!(entries[1].name, "Laktosfri helmjölk 3%");
        assert_eq!(entries[2].name, "proteinshake");
    }

    #[test]
    fn split_attaches_notes_below_yaml() {
        let content = r#"## Foo

```yaml
per_100g:
  kcal: 100
```

Free prose under the block.

More prose.
"#;
        let entries = split_entries(content);
        assert_eq!(entries.len(), 1);
        let notes = entries[0].notes.as_ref().unwrap();
        assert!(notes.contains("Free prose"));
        assert!(notes.contains("More prose"));
    }

    #[test]
    fn split_skips_heading_without_yaml_block() {
        let content = r#"## NoYaml

just some words.

## HasYaml

```yaml
per_100g:
  kcal: 50
```
"#;
        let entries = split_entries(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "HasYaml");
    }

    #[test]
    fn split_tolerates_h1_and_dividers() {
        let content = r#"# Top Title

---

## Foo

```yaml
per_100g:
  kcal: 1
```

---
"#;
        let entries = split_entries(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "Foo");
    }

    #[test]
    fn split_records_line_numbers() {
        let content = r#"## First

```yaml
per_100g:
  kcal: 1
```

## Second

```yaml
per_100ml:
  kcal: 1
```
"#;
        let entries = split_entries(content);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].line_number, 1);
        assert!(entries[1].line_number > entries[0].line_number);
    }

    #[test]
    fn split_empty_file_returns_empty() {
        assert!(split_entries("").is_empty());
        assert!(split_entries("# Just a title\n").is_empty());
    }

    #[test]
    fn split_yaml_with_syntax_error_still_returns_entry() {
        // The splitter shouldn't decide validity — that's build_food_insert's job.
        // Bad YAML → entry.yaml is Yaml::BadValue, but the entry is still returned.
        let content = r#"## Broken

```yaml
per_100g:
  kcal: : :
```
"#;
        let entries = split_entries(content);
        assert_eq!(entries.len(), 1);
        assert!(matches!(entries[0].yaml, Yaml::BadValue));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib materializer::nutrition::tests`
Expected: compilation error — `split_entries` not defined.

- [ ] **Step 3: Implement `split_entries`**

Replace the stub in `src/materializer/nutrition.rs` with the full implementation. Keep the existing `materialize_nutrition_db` stub for now — Task 9 fills it in.

```rust
use crate::config::Config;
use color_eyre::eyre::Result;
use rusqlite::Connection;
use std::path::Path;
use std::sync::LazyLock;
use yaml_rust2::{Yaml, YamlLoader};

#[derive(Debug, Clone)]
pub(crate) struct ParsedEntry {
    pub name: String,
    pub yaml: Yaml,
    pub notes: Option<String>,
    pub line_number: usize,
}

static RE_HEADING: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^##\s+(.+?)\s*$").unwrap());
static RE_YAML_FENCE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^```yaml\s*$").unwrap());
static RE_FENCE_CLOSE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^```\s*$").unwrap());

#[derive(Default)]
struct PendingEntry {
    name: String,
    line_number: usize,
    yaml_lines: Vec<String>,
    notes_lines: Vec<String>,
    in_yaml_fence: bool,
    yaml_seen: bool,
}

pub(crate) fn split_entries(content: &str) -> Vec<ParsedEntry> {
    let mut entries = Vec::new();
    let mut current: Option<PendingEntry> = None;

    for (idx, line) in content.lines().enumerate() {
        let lineno = idx + 1;

        if let Some(caps) = RE_HEADING.captures(line) {
            // Flush previous entry before starting a new one.
            if let Some(prev) = current.take() {
                if let Some(entry) = finalize(prev) {
                    entries.push(entry);
                }
            }
            current = Some(PendingEntry {
                name: caps.get(1).unwrap().as_str().to_string(),
                line_number: lineno,
                ..Default::default()
            });
            continue;
        }

        let Some(entry) = current.as_mut() else {
            continue; // before first ## heading: ignore
        };

        if entry.in_yaml_fence {
            if RE_FENCE_CLOSE.is_match(line) {
                entry.in_yaml_fence = false;
            } else {
                entry.yaml_lines.push(line.to_string());
            }
            continue;
        }

        if !entry.yaml_seen && RE_YAML_FENCE.is_match(line) {
            entry.in_yaml_fence = true;
            entry.yaml_seen = true;
            continue;
        }

        // Anything else (prose, dividers) goes to notes.
        entry.notes_lines.push(line.to_string());
    }

    if let Some(prev) = current.take() {
        if let Some(entry) = finalize(prev) {
            entries.push(entry);
        }
    }
    entries
}

fn finalize(pending: PendingEntry) -> Option<ParsedEntry> {
    if !pending.yaml_seen {
        eprintln!(
            "Warning: nutrition-db.md entry '{}' (line {}) has no fenced YAML block — skipped",
            pending.name, pending.line_number
        );
        return None;
    }
    let yaml_str = pending.yaml_lines.join("\n");
    let yaml = match YamlLoader::load_from_str(&yaml_str) {
        Ok(mut docs) if !docs.is_empty() => docs.remove(0),
        Ok(_) => Yaml::Null,
        Err(_) => Yaml::BadValue,
    };
    let notes_joined = pending
        .notes_lines
        .join("\n")
        .trim_matches(|c: char| c == '\n' || c.is_whitespace())
        .to_string();
    let notes = if notes_joined.is_empty() {
        None
    } else {
        Some(notes_joined)
    };
    Some(ParsedEntry {
        name: pending.name,
        yaml,
        notes,
        line_number: pending.line_number,
    })
}

pub fn materialize_nutrition_db(
    _conn: &Connection,
    _file_path: &Path,
    _config: &Config,
) -> Result<usize> {
    Ok(0)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib materializer::nutrition::tests`
Expected: 7 PASS.

- [ ] **Step 5: Commit**

```bash
git add src/materializer/nutrition.rs
git commit -m "feat: split nutrition-db.md by ## headings + yaml blocks

Line-oriented state machine produces ParsedEntry values. H1 titles and
--- dividers are tolerated. Headings without a fenced ```yaml block are
skipped with a stderr warning. Bad YAML inside the fence yields
Yaml::BadValue; build_food_insert (next task) decides validity."
```

---

## Task 7: Implement `build_food_insert` (YAML → `FoodInsert` with validation)

**Files:**
- Modify: `src/materializer/nutrition.rs`

- [ ] **Step 1: Write failing tests**

Append to `mod tests` in `src/materializer/nutrition.rs`:

```rust
    use yaml_rust2::YamlLoader;

    fn parse(name: &str, yaml_str: &str) -> ParsedEntry {
        let yaml = YamlLoader::load_from_str(yaml_str)
            .ok()
            .and_then(|mut d| if d.is_empty() { None } else { Some(d.remove(0)) })
            .unwrap_or(Yaml::Null);
        ParsedEntry {
            name: name.to_string(),
            yaml,
            notes: None,
            line_number: 1,
        }
    }

    #[test]
    fn build_basic_per_100g() {
        let entry = parse(
            "Kelda Skogssvampsoppa",
            "per_100g:\n  kcal: 70\n  protein: 1.4\ngi: 40\n",
        );
        let fi = build_food_insert(&entry).unwrap();
        assert_eq!(fi.name, "Kelda Skogssvampsoppa");
        assert_eq!(fi.per_100g.as_ref().unwrap().kcal, Some(70.0));
        assert_eq!(fi.per_100g.as_ref().unwrap().protein, Some(1.4));
        assert_eq!(fi.gi, Some(40.0));
        // Heading is auto-added as a lowercased alias.
        assert!(fi.aliases.contains(&"kelda skogssvampsoppa".to_string()));
    }

    #[test]
    fn build_basic_per_100ml_with_density() {
        let entry = parse(
            "Helmjölk",
            "per_100ml:\n  kcal: 62\ndensity_g_per_ml: 1.03\n",
        );
        let fi = build_food_insert(&entry).unwrap();
        assert_eq!(fi.per_100ml.as_ref().unwrap().kcal, Some(62.0));
        assert_eq!(fi.density_g_per_ml, Some(1.03));
    }

    #[test]
    fn build_total_only_is_valid() {
        let entry = parse(
            "proteinshake",
            "description: 62g pulver + 4 dl vatten\n\
             total:\n  weight_g: 462\n  kcal: 234\n  protein: 48\n",
        );
        let fi = build_food_insert(&entry).unwrap();
        assert!(fi.total.is_some());
        assert_eq!(fi.total.as_ref().unwrap().kcal, Some(234.0));
        assert_eq!(fi.description.as_deref(), Some("62g pulver + 4 dl vatten"));
    }

    #[test]
    fn build_rejects_no_panel() {
        let entry = parse("Empty", "gi: 40\n");
        let err = build_food_insert(&entry).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("Empty") && msg.contains("per_100g"),
            "expected error to mention entry name and missing panels: {msg}"
        );
    }

    #[test]
    fn build_rejects_zero_density() {
        let entry = parse(
            "Bad",
            "per_100g:\n  kcal: 50\ndensity_g_per_ml: 0\n",
        );
        let err = build_food_insert(&entry).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("density"), "expected density error: {msg}");
    }

    #[test]
    fn build_aliases_normalized_and_deduped() {
        let entry = parse(
            "Foo Bar",
            "per_100g:\n  kcal: 1\naliases: [Foo, \"FOO\", \"Bar Baz\", \"foo bar\"]\n",
        );
        let fi = build_food_insert(&entry).unwrap();
        // heading auto-added, all lowercased, deduped
        let mut aliases = fi.aliases.clone();
        aliases.sort();
        assert!(aliases.contains(&"foo".to_string()));
        assert!(aliases.contains(&"bar baz".to_string()));
        assert!(aliases.contains(&"foo bar".to_string()));
        // dedup
        let unique_count = {
            let mut a = aliases.clone();
            a.dedup();
            a.len()
        };
        assert_eq!(unique_count, aliases.len());
    }

    #[test]
    fn build_ingredients_preserve_order() {
        let entry = parse(
            "Composite",
            "total:\n  kcal: 100\n\
             ingredients:\n  - food: Whey\n    amount_g: 62\n  - food: Water\n  - food: Sugar\n    amount_g: 5\n",
        );
        let fi = build_food_insert(&entry).unwrap();
        assert_eq!(fi.ingredients.len(), 3);
        assert_eq!(fi.ingredients[0].ingredient_name, "Whey");
        assert_eq!(fi.ingredients[0].amount_g, Some(62.0));
        assert_eq!(fi.ingredients[1].ingredient_name, "Water");
        assert_eq!(fi.ingredients[1].amount_g, None);
        assert_eq!(fi.ingredients[2].ingredient_name, "Sugar");
    }

    #[test]
    fn build_ingredient_missing_food_skipped() {
        let entry = parse(
            "Composite",
            "total:\n  kcal: 100\n\
             ingredients:\n  - amount_g: 50\n  - food: Whey\n    amount_g: 62\n",
        );
        let fi = build_food_insert(&entry).unwrap();
        assert_eq!(fi.ingredients.len(), 1);
        assert_eq!(fi.ingredients[0].ingredient_name, "Whey");
    }

    #[test]
    fn build_unknown_top_level_key_warns_not_errors() {
        let entry = parse(
            "Foo",
            "per_100g:\n  kcal: 1\ntags: [foo, bar]\n",
        );
        // Should not panic / error; entry built normally.
        let fi = build_food_insert(&entry).unwrap();
        assert!(fi.per_100g.is_some());
    }

    #[test]
    fn build_uses_notes_when_present() {
        let mut entry = parse("Foo", "per_100g:\n  kcal: 1\n");
        entry.notes = Some("Some prose.".to_string());
        let fi = build_food_insert(&entry).unwrap();
        assert_eq!(fi.notes.as_deref(), Some("Some prose."));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib materializer::nutrition::tests::build_`
Expected: compilation error — `build_food_insert` not defined.

- [ ] **Step 3: Implement `build_food_insert`**

In `src/materializer/nutrition.rs`, add after the `split_entries`/`finalize` block:

```rust
use crate::db::{FoodIngredient, FoodInsert, NutrientPanel, TotalPanel};
use color_eyre::eyre::eyre;

const KNOWN_TOP_LEVEL_KEYS: &[&str] = &[
    "per_100g",
    "per_100ml",
    "density_g_per_ml",
    "gi",
    "gl_per_100g",
    "gl_per_100ml",
    "ii",
    "aliases",
    "description",
    "ingredients",
    "total",
];

pub(crate) fn build_food_insert(entry: &ParsedEntry) -> Result<FoodInsert> {
    let yaml = &entry.yaml;
    if matches!(yaml, Yaml::BadValue | Yaml::Null) {
        return Err(eyre!(
            "entry '{}' (line {}): YAML block is empty or malformed",
            entry.name,
            entry.line_number
        ));
    }

    let per_100g = read_nutrient_panel(&yaml["per_100g"]);
    let per_100ml = read_nutrient_panel(&yaml["per_100ml"]);
    let total = read_total_panel(&yaml["total"]);
    if per_100g.is_none() && per_100ml.is_none() && total.is_none() {
        return Err(eyre!(
            "entry '{}' (line {}): must include per_100g, per_100ml, or total",
            entry.name,
            entry.line_number
        ));
    }

    let density_g_per_ml = read_real(&yaml["density_g_per_ml"]);
    if let Some(d) = density_g_per_ml {
        if d <= 0.0 {
            return Err(eyre!(
                "entry '{}' (line {}): density_g_per_ml must be > 0 (got {})",
                entry.name,
                entry.line_number,
                d
            ));
        }
    }

    let gi = read_real(&yaml["gi"]);
    let gl_per_100g = read_real(&yaml["gl_per_100g"]);
    let gl_per_100ml = read_real(&yaml["gl_per_100ml"]);
    let ii = read_real(&yaml["ii"]);
    for (label, val) in [("gi", gi), ("ii", ii)] {
        if let Some(v) = val {
            if !(0.0..=200.0).contains(&v) {
                eprintln!(
                    "Warning: entry '{}' (line {}): {label}={v} outside 0..200 (still stored)",
                    entry.name, entry.line_number
                );
            }
        }
    }

    let description = read_string(&yaml["description"]);
    let notes = entry.notes.clone();

    let mut aliases: Vec<String> = read_string_list(&yaml["aliases"])
        .into_iter()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    aliases.push(entry.name.trim().to_lowercase());
    aliases.sort();
    aliases.dedup();

    let ingredients = read_ingredients(&yaml["ingredients"], &entry.name, entry.line_number);

    if let Yaml::Hash(hash) = yaml {
        for (k, _) in hash.iter() {
            if let Yaml::String(key) = k {
                if !KNOWN_TOP_LEVEL_KEYS.contains(&key.as_str()) {
                    eprintln!(
                        "Warning: entry '{}' (line {}): unknown key '{key}' ignored",
                        entry.name, entry.line_number
                    );
                }
            }
        }
    }

    Ok(FoodInsert {
        name: entry.name.trim().to_string(),
        per_100g,
        per_100ml,
        density_g_per_ml,
        total,
        gi,
        gl_per_100g,
        gl_per_100ml,
        ii,
        description,
        notes,
        aliases,
        ingredients,
    })
}

fn read_nutrient_panel(node: &Yaml) -> Option<NutrientPanel> {
    if matches!(node, Yaml::BadValue | Yaml::Null) {
        return None;
    }
    let panel = NutrientPanel {
        kcal: read_real(&node["kcal"]),
        protein: read_real(&node["protein"]),
        carbs: read_real(&node["carbs"]),
        fat: read_real(&node["fat"]),
        sat_fat: read_real(&node["sat_fat"]),
        sugar: read_real(&node["sugar"]),
        salt: read_real(&node["salt"]),
        fiber: read_real(&node["fiber"]),
    };
    if panel == NutrientPanel::default() {
        None
    } else {
        Some(panel)
    }
}

fn read_total_panel(node: &Yaml) -> Option<TotalPanel> {
    if matches!(node, Yaml::BadValue | Yaml::Null) {
        return None;
    }
    let panel = TotalPanel {
        weight_g: read_real(&node["weight_g"]),
        kcal: read_real(&node["kcal"]),
        protein: read_real(&node["protein"]),
        carbs: read_real(&node["carbs"]),
        fat: read_real(&node["fat"]),
        sat_fat: read_real(&node["sat_fat"]),
        sugar: read_real(&node["sugar"]),
        salt: read_real(&node["salt"]),
        fiber: read_real(&node["fiber"]),
    };
    if panel == TotalPanel::default() {
        None
    } else {
        Some(panel)
    }
}

fn read_real(node: &Yaml) -> Option<f64> {
    match node {
        Yaml::Real(s) => s.parse().ok(),
        Yaml::Integer(i) => Some(*i as f64),
        Yaml::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn read_string(node: &Yaml) -> Option<String> {
    match node {
        Yaml::String(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        _ => None,
    }
}

fn read_string_list(node: &Yaml) -> Vec<String> {
    match node {
        Yaml::Array(arr) => arr
            .iter()
            .filter_map(|item| match item {
                Yaml::String(s) => Some(s.clone()),
                Yaml::Integer(i) => Some(i.to_string()),
                _ => None,
            })
            .collect(),
        _ => vec![],
    }
}

fn read_ingredients(node: &Yaml, entry_name: &str, lineno: usize) -> Vec<FoodIngredient> {
    let Yaml::Array(arr) = node else {
        return vec![];
    };
    let mut out = Vec::new();
    for item in arr {
        let Yaml::Hash(_) = item else {
            eprintln!(
                "Warning: entry '{entry_name}' (line {lineno}): ingredient is not a mapping — skipped"
            );
            continue;
        };
        let food = read_string(&item["food"]);
        let Some(food_name) = food else {
            eprintln!(
                "Warning: entry '{entry_name}' (line {lineno}): ingredient missing 'food' — skipped"
            );
            continue;
        };
        out.push(FoodIngredient {
            ingredient_name: food_name,
            amount_g: read_real(&item["amount_g"]),
        });
    }
    out
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib materializer::nutrition::tests::build_`
Expected: 10 PASS.

- [ ] **Step 5: Commit**

```bash
git add src/materializer/nutrition.rs
git commit -m "feat: build FoodInsert from a parsed entry with validation

Requires at least one of per_100g/per_100ml/total. Rejects
density_g_per_ml<=0. Warns (still stores) when gi/ii are outside
0..200, when an unknown top-level key is present, or when an
ingredient lacks a 'food' field. Heading is auto-inserted as a
lowercased alias and the alias list is deduped."
```

---

## Task 8: Implement `materialize_nutrition_db` end-to-end

**Files:**
- Modify: `src/materializer/nutrition.rs`

- [ ] **Step 1: Write failing integration tests**

Append to `mod tests` in `src/materializer/nutrition.rs`:

```rust
    use crate::db::CORE_SCHEMA_TEST_HOOK;
    use std::io::Write;
    use tempfile::TempDir;

    fn open_inmem_with_schema() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(CORE_SCHEMA_TEST_HOOK).unwrap();
        conn
    }

    fn write_fixture(dir: &TempDir, content: &str) -> std::path::PathBuf {
        let path = dir.path().join("nutrition-db.md");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path
    }

    fn empty_config() -> Config {
        toml::from_str("notes_dir = '/tmp'\n").unwrap()
    }

    #[test]
    fn materialize_three_entries_e2e() {
        let conn = open_inmem_with_schema();
        let dir = TempDir::new().unwrap();
        let content = r#"# Nutrition

## Kelda Skogssvampsoppa

```yaml
per_100g:
  kcal: 70
  protein: 1.4
gi: 40
aliases: [skogssvampsoppa]
```

## Helmjölk

```yaml
per_100ml:
  kcal: 62
density_g_per_ml: 1.03
aliases: [mjölk]
```

## proteinshake

```yaml
description: 62g pulver + 4 dl vatten
total:
  weight_g: 462
  kcal: 234
ingredients:
  - food: Whey
    amount_g: 62
  - food: Water
    amount_g: 400
```
"#;
        let path = write_fixture(&dir, content);
        let n = materialize_nutrition_db(&conn, &path, &empty_config()).unwrap();
        assert_eq!(n, 3);

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM foods", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 3);

        let alias_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM food_aliases", [], |r| r.get(0))
            .unwrap();
        // 3 headings + 1 explicit alias each (skogssvampsoppa, mjölk) +
        // proteinshake heading already auto-added → 5
        assert!(alias_count >= 5);

        let ingredient_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM food_ingredients", [], |r| r.get(0))
            .unwrap();
        assert_eq!(ingredient_count, 2);
    }

    #[test]
    fn materialize_replaces_on_rerun() {
        let conn = open_inmem_with_schema();
        let dir = TempDir::new().unwrap();

        let v1 = "## A\n\n```yaml\nper_100g:\n  kcal: 1\n```\n\n## B\n\n```yaml\nper_100g:\n  kcal: 2\n```\n";
        let v2 = "## A\n\n```yaml\nper_100g:\n  kcal: 1\n```\n";

        let path = write_fixture(&dir, v1);
        materialize_nutrition_db(&conn, &path, &empty_config()).unwrap();
        std::fs::write(&path, v2).unwrap();
        materialize_nutrition_db(&conn, &path, &empty_config()).unwrap();

        let names: Vec<String> = conn
            .prepare("SELECT name FROM foods ORDER BY name")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(names, vec!["A".to_string()]);
    }

    #[test]
    fn materialize_partial_failure_continues() {
        let conn = open_inmem_with_schema();
        let dir = TempDir::new().unwrap();
        let content = r#"## Good1

```yaml
per_100g:
  kcal: 1
```

## Bad

```yaml
gi: 40
```

## Good2

```yaml
per_100g:
  kcal: 2
```
"#;
        let path = write_fixture(&dir, content);
        let n = materialize_nutrition_db(&conn, &path, &empty_config()).unwrap();
        assert_eq!(n, 2);

        let names: Vec<String> = conn
            .prepare("SELECT name FROM foods ORDER BY name")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(names, vec!["Good1".to_string(), "Good2".to_string()]);
    }

    #[test]
    fn materialize_missing_file_is_silent() {
        let conn = open_inmem_with_schema();
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nutrition-db.md");
        // file does not exist
        let n = materialize_nutrition_db(&conn, &path, &empty_config()).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn materialize_records_last_synced() {
        let conn = open_inmem_with_schema();
        let dir = TempDir::new().unwrap();
        let path = write_fixture(
            &dir,
            "## A\n\n```yaml\nper_100g:\n  kcal: 1\n```\n",
        );
        materialize_nutrition_db(&conn, &path, &empty_config()).unwrap();

        let last: Option<String> = conn
            .query_row(
                "SELECT value FROM sync_meta WHERE key = 'last_nutrition_sync'",
                [],
                |r| r.get(0),
            )
            .ok();
        assert!(last.is_some());
    }

    #[test]
    fn materialize_duplicate_heading_keeps_first() {
        let conn = open_inmem_with_schema();
        let dir = TempDir::new().unwrap();
        let content = "## Foo\n\n```yaml\nper_100g:\n  kcal: 1\n```\n\n## Foo\n\n```yaml\nper_100g:\n  kcal: 999\n```\n";
        let path = write_fixture(&dir, content);
        let n = materialize_nutrition_db(&conn, &path, &empty_config()).unwrap();
        assert_eq!(n, 1);

        let kcal: f64 = conn
            .query_row(
                "SELECT kcal_per_100g FROM foods WHERE name = 'Foo'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(kcal, 1.0);
    }
```

The test refers to `CORE_SCHEMA_TEST_HOOK`. Expose `CORE_SCHEMA` to the nutrition tests by adding (in `src/db.rs`, just after the `const CORE_SCHEMA` line):

```rust
#[cfg(test)]
pub const CORE_SCHEMA_TEST_HOOK: &str = CORE_SCHEMA;
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib materializer::nutrition::tests::materialize_`
Expected: compilation/runtime failures — current `materialize_nutrition_db` is the stub returning 0.

- [ ] **Step 3: Implement `materialize_nutrition_db`**

In `src/materializer/nutrition.rs`, replace the stub with:

```rust
pub fn materialize_nutrition_db(
    conn: &Connection,
    file_path: &Path,
    _config: &Config,
) -> Result<usize> {
    if !file_path.exists() {
        return Ok(0);
    }
    let content = std::fs::read_to_string(file_path)
        .map_err(|e| eyre!("failed to read {}: {e}", file_path.display()))?;

    let entries = split_entries(&content);
    if entries.is_empty() {
        return Ok(0);
    }

    let mut food_inserts: Vec<FoodInsert> = Vec::new();
    for entry in &entries {
        match build_food_insert(entry) {
            Ok(fi) => food_inserts.push(fi),
            Err(e) => eprintln!(
                "Warning: nutrition-db.md entry '{}' (line {}): {e}",
                entry.name, entry.line_number
            ),
        }
    }

    let tx = conn.unchecked_transaction()?;
    crate::db::delete_all_foods(&tx)?;

    let mut inserted = 0usize;
    let mut seen_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    for fi in &food_inserts {
        if !seen_names.insert(fi.name.clone()) {
            eprintln!(
                "Warning: nutrition-db.md duplicate heading '{}' — first occurrence kept",
                fi.name
            );
            continue;
        }
        match crate::db::insert_food(&tx, fi) {
            Ok(_id) => inserted += 1,
            Err(e) => eprintln!(
                "Warning: nutrition-db.md insert failed for '{}': {e}",
                fi.name
            ),
        }
    }

    let mtime = std::fs::metadata(file_path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| {
            let secs = d.as_secs() as i64;
            chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_default()
        })
        .unwrap_or_default();
    tx.execute(
        "INSERT OR REPLACE INTO sync_meta (key, value) VALUES ('last_nutrition_sync', ?1)",
        [mtime],
    )?;
    tx.commit()?;

    Ok(inserted)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib materializer::nutrition`
Expected: all PASS.

Run: `cargo test --lib`
Expected: full library suite PASS (no regressions).

- [ ] **Step 5: Commit**

```bash
git add src/db.rs src/materializer/nutrition.rs
git commit -m "feat: materialize_nutrition_db reads file and replaces foods table

DELETE-then-INSERT-all in one transaction. Per-entry parse failures
warn to stderr and continue. Duplicate headings keep the first
occurrence. Missing/empty file is a silent no-op. Records the file
mtime as last_nutrition_sync in sync_meta for status output."
```

---

## Task 9: Wire nutrition-db into `sync_all` and `rebuild_all`

**Files:**
- Modify: `src/materializer/daily.rs`
- Modify: `tests/integration.rs`

- [ ] **Step 1: Write failing integration tests**

Append to `tests/integration.rs`:

```rust
#[test]
fn sync_all_includes_nutrition_db() {
    let dir = tempfile::TempDir::new().unwrap();
    let notes_dir = dir.path();
    std::fs::write(
        notes_dir.join("2026-04-29.md"),
        "---\ndate: 2026-04-29\nweight: 173.4\n---\n",
    )
    .unwrap();
    std::fs::write(
        notes_dir.join("nutrition-db.md"),
        "## Apple\n\n```yaml\nper_100g:\n  kcal: 52\n```\n",
    )
    .unwrap();

    let db_path = notes_dir.join(".daylog.db");
    let config: daylog::config::Config = toml::from_str(&format!(
        "notes_dir = '{}'\n",
        notes_dir.display().to_string().replace('\\', "/")
    ))
    .unwrap();
    let registry = daylog::modules::build_registry(&config);
    let conn = daylog::db::open_rw(&db_path).unwrap();
    daylog::db::init_db(&conn, &registry).unwrap();
    daylog::modules::validate_module_tables(&registry).unwrap();

    let (synced, errors) =
        daylog::materializer::sync_all(&conn, notes_dir, &config, &registry).unwrap();
    assert_eq!(errors, 0);
    assert!(synced >= 2, "expected at least 2 synced (1 note + 1 db)");

    let foods_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM foods", [], |r| r.get(0))
        .unwrap();
    assert_eq!(foods_count, 1);
}

#[test]
fn rebuild_reparses_nutrition_unconditionally() {
    let dir = tempfile::TempDir::new().unwrap();
    let notes_dir = dir.path();
    std::fs::write(
        notes_dir.join("nutrition-db.md"),
        "## Apple\n\n```yaml\nper_100g:\n  kcal: 52\n```\n",
    )
    .unwrap();

    let db_path = notes_dir.join(".daylog.db");
    let config: daylog::config::Config = toml::from_str(&format!(
        "notes_dir = '{}'\n",
        notes_dir.display().to_string().replace('\\', "/")
    ))
    .unwrap();
    let registry = daylog::modules::build_registry(&config);
    let conn = daylog::db::open_rw(&db_path).unwrap();
    daylog::db::init_db(&conn, &registry).unwrap();
    daylog::modules::validate_module_tables(&registry).unwrap();

    daylog::materializer::sync_all(&conn, notes_dir, &config, &registry).unwrap();
    // Mark sync time in the future so a normal sync_all would skip the file.
    let future = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
        + 86_400.0;
    daylog::db::set_last_sync(&conn, future).unwrap();
    // Tweak the file to simulate an updated value but with stale mtime
    // is impossible portably; rebuild should run regardless of mtime.
    std::fs::write(
        notes_dir.join("nutrition-db.md"),
        "## Apple\n\n```yaml\nper_100g:\n  kcal: 99\n```\n",
    )
    .unwrap();

    daylog::materializer::rebuild_all(&conn, notes_dir, &config, &registry).unwrap();

    let kcal: f64 = conn
        .query_row(
            "SELECT kcal_per_100g FROM foods WHERE name = 'Apple'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(kcal, 99.0);
}

#[test]
fn sync_all_silent_when_nutrition_db_missing() {
    let dir = tempfile::TempDir::new().unwrap();
    let notes_dir = dir.path();
    std::fs::write(
        notes_dir.join("2026-04-29.md"),
        "---\ndate: 2026-04-29\n---\n",
    )
    .unwrap();
    // No nutrition-db.md.

    let db_path = notes_dir.join(".daylog.db");
    let config: daylog::config::Config = toml::from_str(&format!(
        "notes_dir = '{}'\n",
        notes_dir.display().to_string().replace('\\', "/")
    ))
    .unwrap();
    let registry = daylog::modules::build_registry(&config);
    let conn = daylog::db::open_rw(&db_path).unwrap();
    daylog::db::init_db(&conn, &registry).unwrap();
    daylog::modules::validate_module_tables(&registry).unwrap();

    let (_synced, errors) =
        daylog::materializer::sync_all(&conn, notes_dir, &config, &registry).unwrap();
    assert_eq!(errors, 0);

    let foods_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM foods", [], |r| r.get(0))
        .unwrap();
    assert_eq!(foods_count, 0);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test integration sync_all_includes_nutrition_db rebuild_reparses_nutrition_unconditionally sync_all_silent_when_nutrition_db_missing`
Expected: foods_count = 0 in the first two tests (because sync/rebuild don't yet call nutrition).

- [ ] **Step 3: Wire nutrition into `sync_all` and `rebuild_all`**

In `src/materializer/daily.rs`, locate `sync_all` and `rebuild_all`. Add at the end of each function (just before `Ok((synced, errors))`):

```rust
    let nutrition_path = notes_dir.join("nutrition-db.md");
    if nutrition_path.exists() {
        let parse = || crate::materializer::nutrition::materialize_nutrition_db(conn, &nutrition_path, config);
        match parse() {
            Ok(_n) => synced += 1,
            Err(e) => {
                eprintln!("Error parsing nutrition-db.md: {e}");
                errors += 1;
            }
        }
    }
```

For `sync_all` only, add an mtime gate above that block (mirroring the daily-note gate). Replace the snippet above with:

```rust
    let nutrition_path = notes_dir.join("nutrition-db.md");
    if nutrition_path.exists() {
        let mtime = std::fs::metadata(&nutrition_path)
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        if mtime >= threshold {
            match crate::materializer::nutrition::materialize_nutrition_db(
                conn,
                &nutrition_path,
                config,
            ) {
                Ok(_n) => synced += 1,
                Err(e) => {
                    eprintln!("Error parsing nutrition-db.md: {e}");
                    errors += 1;
                }
            }
        }
    }
```

For `rebuild_all`, keep the unconditional version (no mtime gate).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test integration sync_all_includes_nutrition_db rebuild_reparses_nutrition_unconditionally sync_all_silent_when_nutrition_db_missing`
Expected: 3 PASS.

Run: `cargo test`
Expected: full suite PASS.

- [ ] **Step 5: Commit**

```bash
git add src/materializer/daily.rs tests/integration.rs
git commit -m "feat: include nutrition-db.md in sync_all and rebuild_all

sync_all is mtime-gated like daily notes; rebuild_all always reparses.
Missing nutrition-db.md is a silent no-op."
```

---

## Task 10: Wire nutrition-db into the watcher

**Files:**
- Modify: `src/materializer/daily.rs`

- [ ] **Step 1: Write failing tests**

The watcher has no existing integration tests in this codebase. Per the spec, we cover the dispatch logic at the unit level and rely on manual end-to-end verification (Task 14). Add one focused unit test confirming that the pending-files loop's file-kind classification routes by name:

Append to `mod tests` in `src/materializer/daily.rs`:

```rust
    #[test]
    fn watcher_dispatch_recognizes_both_kinds() {
        let daily = std::path::Path::new("/notes/2026-04-29.md");
        let nutrition = std::path::Path::new("/notes/nutrition-db.md");
        let other = std::path::Path::new("/notes/scratch.md");
        assert_eq!(materialized_file_kind(daily), Some(FileKind::DailyNote));
        assert_eq!(
            materialized_file_kind(nutrition),
            Some(FileKind::NutritionDb)
        );
        assert!(materialized_file_kind(other).is_none());
    }
```

- [ ] **Step 2: Run test to verify it passes already**

This test follows from Task 5; running it confirms the dispatch fn is in place:

Run: `cargo test --lib materializer::daily::tests::watcher_dispatch_recognizes_both_kinds`
Expected: PASS.

- [ ] **Step 3: Update `start_watcher` to dispatch by `FileKind`**

In `src/materializer/daily.rs`, find `start_watcher`. Two changes:

(a) Update the in-event filter:

```rust
                            if materialized_file_kind(path).is_some() {
                                pending_files.insert(path.clone());
                            }
```

(replacing the `is_note_file(path)` call.)

(b) Update the pending-files processing loop. The current loop body looks like:

```rust
                for path in pending_files.drain() {
                    if path.exists() {
                        if let Err(e) = materialize_file(conn, &path, &current_config, &modules) {
                            // ... error handling ...
                        }
                    } else if let Some(date) = path.file_stem().and_then(|s| s.to_str()) {
                        if is_valid_date(date) {
                            let _ = db::delete_date(conn, date);
                        }
                    }
                }
```

Replace it with:

```rust
                for path in pending_files.drain() {
                    if !path.exists() {
                        match materialized_file_kind(&path) {
                            Some(FileKind::DailyNote) => {
                                if let Some(date) = path.file_stem().and_then(|s| s.to_str()) {
                                    if is_valid_date(date) {
                                        let _ = db::delete_date(conn, date);
                                    }
                                }
                            }
                            Some(FileKind::NutritionDb) => {
                                // Spec: deletion is a no-op; foods table retained.
                            }
                            None => {}
                        }
                        continue;
                    }
                    let result = match materialized_file_kind(&path) {
                        Some(FileKind::DailyNote) => {
                            materialize_file(conn, &path, &current_config, &modules)
                        }
                        Some(FileKind::NutritionDb) => {
                            crate::materializer::nutrition::materialize_nutrition_db(
                                conn,
                                &path,
                                &current_config,
                            )
                            .map(|_| ())
                        }
                        None => continue,
                    };
                    if let Err(e) = result {
                        let err_str = e.to_string();
                        eprintln!("Warning: failed to parse {}: {e}", path.display());
                        if err_str.contains("disk I/O error")
                            || err_str.contains("database is locked")
                            || err_str.contains("unable to open")
                        {
                            conn_failed = true;
                            break;
                        }
                        let _ = conn.execute(
                            "INSERT OR REPLACE INTO sync_meta (key, value) VALUES ('last_error', ?1)",
                            [format!("{}: {e}", path.display())],
                        );
                    }
                }
```

This preserves connection-loss detection. The `materialize_file` call's `Result<()>` and `materialize_nutrition_db`'s `Result<usize>` are unified to `Result<()>` via `.map(|_| ())`.

- [ ] **Step 4: Run the full suite to confirm no regression**

Run: `cargo build && cargo test`
Expected: clean build, full suite PASS.

- [ ] **Step 5: Commit**

```bash
git add src/materializer/daily.rs
git commit -m "feat: watcher dispatches daily notes and nutrition-db.md

Routes pending file events to materialize_file or
materialize_nutrition_db based on FileKind. Missing-file events for
nutrition-db.md are explicit no-ops (foods table retained)."
```

---

## Task 11: Surface `nutrition_db` in `daylog status --json`

**Files:**
- Modify: `src/main.rs`
- Add: integration test in `tests/integration.rs`

- [ ] **Step 1: Write failing test**

Append to `tests/integration.rs`:

```rust
#[test]
fn status_json_includes_nutrition_db() {
    let dir = tempfile::TempDir::new().unwrap();
    let notes_dir = dir.path();
    std::fs::write(
        notes_dir.join("nutrition-db.md"),
        "## Apple\n\n```yaml\nper_100g:\n  kcal: 52\n```\n",
    )
    .unwrap();

    let db_path = notes_dir.join(".daylog.db");
    let config: daylog::config::Config = toml::from_str(&format!(
        "notes_dir = '{}'\n",
        notes_dir.display().to_string().replace('\\', "/")
    ))
    .unwrap();
    let registry = daylog::modules::build_registry(&config);
    let conn = daylog::db::open_rw(&db_path).unwrap();
    daylog::db::init_db(&conn, &registry).unwrap();
    daylog::modules::validate_module_tables(&registry).unwrap();

    daylog::materializer::sync_all(&conn, notes_dir, &config, &registry).unwrap();

    let status = daylog::db::nutrition_status(&conn).unwrap();
    assert_eq!(status.foods_count, 1);
    assert!(status.last_synced.is_some());
}
```

This test verifies the helper directly. The full `cmd_status` runs through `println!` and is harder to capture; we cover it via manual verification in Task 14.

- [ ] **Step 2: Run to verify it passes**

Since Tasks 4 and 9 already implemented `nutrition_status` and the sync wiring, this test should pass already:

Run: `cargo test --test integration status_json_includes_nutrition_db`
Expected: PASS.

- [ ] **Step 3: Add `nutrition_db` field to `cmd_status`**

In `src/main.rs`, find the section in `cmd_status` that surfaces `pending` (around line 174). Add after the `pending` block:

```rust
    let nutrition = db::nutrition_status(&conn)?;
    output["nutrition_db"] = serde_json::json!({
        "foods_count": nutrition.foods_count,
        "last_synced": nutrition.last_synced,
    });
```

- [ ] **Step 4: Verify build + tests**

Run: `cargo build && cargo test`
Expected: clean build, full suite PASS.

- [ ] **Step 5: Commit**

```bash
git add src/main.rs tests/integration.rs
git commit -m "feat: include nutrition_db in daylog status --json

Surfaces foods_count and last_synced from db::nutrition_status."
```

---

## Task 12: Document the format in `README.md`

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Inspect current README sections**

Run: `grep -n '^##' README.md`
Pick a position consistent with the existing structure — typically after the "Configuration" or "Extension recipes" section.

- [ ] **Step 2: Add the "Nutrition database" section**

Append (or insert at the appropriate spot per Step 1) the following Markdown:

````markdown
## Nutrition database

Daylog reads `{notes_dir}/nutrition-db.md` (if present) and materializes it into a `foods` table that other tooling can query. The file is the source of truth — SQLite is a derived cache.

Each entry is one `## Heading` followed by a fenced ` ```yaml ` block. Freeform prose under the block is preserved as `notes`.

```markdown
## Kelda Skogssvampsoppa

```yaml
per_100g:
  kcal: 70
  protein: 1.4
  carbs: 4.8
  fat: 5.0
gi: 40
gl_per_100g: 2
ii: 35
aliases: [skogssvampsoppa]
```

Innehåller svamp + grädde — IBS-trigger.

## proteinshake

```yaml
description: 62g pulver + 4 dl vatten
total:
  weight_g: 462
  kcal: 234
  protein: 48
ingredients:
  - food: Whey
    amount_g: 62
gi: 5
ii: 85
```
```

### Recognized fields

At least one of `per_100g`, `per_100ml`, or `total` must be present. Everything else is optional.

| Field | Meaning |
|---|---|
| `per_100g` / `per_100ml` | Nutrient panel: `kcal`, `protein`, `carbs`, `fat`, `sat_fat`, `sugar`, `salt`, `fiber` |
| `density_g_per_ml` | Conversion between weight and volume |
| `gi` | Glycemic index |
| `gl_per_100g` / `gl_per_100ml` | Glycemic load |
| `ii` | Insulin index |
| `aliases` | Lowercased lookup names. The heading is auto-added. |
| `description` | Free-text composition (e.g. "62g pulver + 4 dl vatten") |
| `ingredients` | List of `{food, amount_g}` for composite recipes |
| `total` | Composite recipe totals (`weight_g`, `kcal`, ... ) |

### Convention: raw vs. cooked

When a food has materially different nutritional values raw vs. cooked (chicken, lentils, ground meat), record one entry per state, named distinctly: `Kycklingbiffar (rå)` and `Kycklingbiffar (stekt)`. The schema stores one panel per row; multi-state foods are split.

### Watcher and rebuild

The file is parsed live by the watcher on every save, and re-parsed from scratch by `daylog rebuild`. Per-entry parse failures warn to stderr; other entries still get loaded. Deleting the file is a no-op — the `foods` table retains its last successful state. `daylog status --json` reports `nutrition_db.foods_count` and `nutrition_db.last_synced`.

````

(The fenced-block-inside-fenced-block above uses ` ``` ` at the inner level; if your README uses a different fencing convention, adjust to match.)

- [ ] **Step 3: Verify the README still renders**

Run: `cat README.md | head -1`
Inspect the new section visually. Run `cargo build` to ensure nothing else regressed (the README change shouldn't affect compilation).

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "docs: add Nutrition database section to README

Documents the heading + fenced-yaml convention, recognized fields,
the raw-vs-cooked two-entry convention, and watcher/rebuild
behavior."
```

---

## Task 13: Update `CLAUDE.md` File Map

**Files:**
- Modify: `CLAUDE.md`

- [ ] **Step 1: Update the File Map block**

In `CLAUDE.md`, find the "File Map" section. Replace the `materializer.rs` line and add nutrition entries:

```
  materializer/
    mod.rs               Re-exports for daily and nutrition parsers
    daily.rs             YAML preprocessor, daily-note parser, watcher dispatch
    nutrition.rs         nutrition-db.md parser (## headings + fenced YAML)
```

In the `db.rs` description line, append a mention of foods:

```
  db.rs                Core tables (days, metrics, sync_meta, foods, food_aliases,
                       food_ingredients), migrations, queries
```

- [ ] **Step 2: Verify**

Run: `cargo build`
Expected: clean (CLAUDE.md changes don't affect compilation; the build serves as a smoke test).

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md File Map for materializer split + foods"
```

---

## Task 14: Final lint, format, and manual verification

**Files:**
- (none — verification step)

- [ ] **Step 1: Run formatter and clippy**

Run: `just lint`
Expected: PASS (no clippy warnings, no formatting diffs).

If clippy raises warnings, fix them and amend the most recent commit they relate to (or, if the warning is in pre-existing code outside this PR's scope, leave it).

- [ ] **Step 2: Run the full test suite**

Run: `just test`
Expected: PASS.

- [ ] **Step 3: Manual end-to-end verification**

- [ ] Write a fixture `nutrition-db.md` into a temp notes_dir:
  ```bash
  mkdir -p /tmp/daylog-mvp
  cat > /tmp/daylog-mvp/nutrition-db.md <<'EOF'
  ## Apple

  ```yaml
  per_100g:
    kcal: 52
    protein: 0.3
    carbs: 14
    fat: 0.2
  gi: 38
  ```
  EOF
  ```
- [ ] Initialize a daylog config pointing at this dir or run `cargo run --bin daylog -- --config <path> rebuild`. (Use whatever invocation path matches the existing CLI entry point — `cargo run -- rebuild` after pointing config at `/tmp/daylog-mvp`.)
- [ ] Verify: `cargo run -- status --json | jq .nutrition_db`
  Expected: `{ "foods_count": 1, "last_synced": "..." }`.
- [ ] Verify: `sqlite3 /tmp/daylog-mvp/.daylog.db "SELECT name, kcal_per_100g FROM foods"`
  Expected: `Apple|52.0`.
- [ ] Edit the fixture (change kcal to 99), wait ~1 s, re-run the sqlite3 query. Note: this requires the watcher to be running (start `daylog` in another terminal). If you don't want to run the TUI, run `cargo run -- sync` after the edit and re-check.

- [ ] **Step 4: Push the branch and open PR**

Per `CLAUDE.md` in the workspace, the PR must target `adrianschmidt/daylog`, NOT upstream:

```bash
git push -u origin feat/structured-nutrition-db
gh pr create -R adrianschmidt/daylog \
  --base main --head adrianschmidt:feat/structured-nutrition-db \
  --title "feat: structured nutrition database (closes #10)" \
  --body "$(cat <<'EOF'
## Summary

Implements #10. Adopts a structured ` ```yaml ` block per `## Heading`
convention inside `nutrition-db.md` and materializes it into three new
core SQLite tables (`foods`, `food_aliases`, `food_ingredients`).

This PR is groundwork for issue #6 (`daylog food` CLI). On its own it
provides the schema, parser, watcher integration, and a
`db::lookup_food_by_name_or_alias` helper. No new user-facing CLI.

Adrian's existing freetext `nutrition-db.md` lives in his personal
install only; conversion to the new format is out of scope for this
PR and will be done locally after install.

## Design

Spec: `docs/superpowers/specs/2026-04-29-structured-nutrition-db-design.md`

## Test plan

- [ ] `just lint` clean
- [ ] `just test` passes
- [ ] Manual: `daylog rebuild` against a hand-written nutrition-db.md fixture; `daylog status --json | jq .nutrition_db` shows expected count
- [ ] Manual: edit the fixture, watcher reparses within ~500 ms

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

---

## Spec coverage check

After Task 14, verify each spec section maps to a task:

- Background / Goals / Non-goals → satisfied by overall plan scope
- Architecture (file map, data flow, invariants) → Task 1 + Task 8
- Markdown convention → Task 6 + Task 7
- Database schema → Task 2 + Task 3 + Task 4
- Parser (split/build/materialize) → Task 6 + Task 7 + Task 8
- Watcher integration → Task 5 + Task 10
- `sync_all` / `rebuild_all` → Task 9
- `daylog status --json` → Task 4 + Task 11
- Documentation → Task 12 + Task 13
- Error handling matrix → covered across Tasks 6/7/8/10
- Testing strategy → tests appear in Tasks 2/3/4/5/6/7/8/9/10/11
- Out of scope items → respected throughout
