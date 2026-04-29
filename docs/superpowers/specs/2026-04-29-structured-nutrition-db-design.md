# Structured nutrition database

**Issue:** [adrianschmidt/daylog#10](https://github.com/adrianschmidt/daylog/issues/10)
**Date:** 2026-04-29

## Background

The convention in daylog is **markdown is the AI-accessible source format; SQLite is a derived cache**. Daily notes follow that pattern: `YYYY-MM-DD.md` files contain YAML frontmatter that the watcher materializes into core and module-owned tables.

Outside daylog's awareness, a `nutrition-db.md` file has accumulated in the user's notes directory, written by a Claude Code session that needed somewhere to store nutritional values for repeat-eaten items. It is freetext markdown — human-readable, but not machine-readable. A CLI command can't look up "kelda skogssvampsoppa" and get nutrition values.

This spec adopts a structured convention inside `nutrition-db.md` so the materializer can parse it into SQLite tables, mirroring the daily-note pipeline.

## Goals

1. A markdown convention for `nutrition-db.md` entries: one entry per `## Heading`, with a fenced ` ```yaml ` block carrying structured fields. Freeform prose below the block is preserved as `notes`.
2. Three new core tables — `foods`, `food_aliases`, `food_ingredients` — populated by a new parser called from `sync_all`, `rebuild_all`, and the file watcher.
3. Live reload: editing `nutrition-db.md` updates the `foods` table within ~500 ms, the same UX as daily notes.
4. A lookup helper `db::lookup_food_by_name_or_alias` ready for issue #6 (`daylog food` CLI) to consume.

## Non-goals

- Converting existing freetext entries. Adrian's current `nutrition-db.md` lives in a personal install only and is not part of the repo. Conversion happens locally after this PR is installed.
- Adding `daylog food` lookup or write CLI commands. That is issue #6.
- Adding a Nutrition tab to the TUI.
- Auto-resolving composite-recipe totals from ingredient amounts. v1 stores ingredients as a list; computing totals from them is a future concern.
- A `per_portion` shorthand (`{amount_g: 500, kcal: 350}`). Consumers can compute portion values from `per_100g` + amount. Adding it now adds parser/schema complexity for marginal value.
- Multiple density variants per entry (raw vs. cooked). One density per food.
- A `tags` field. Easy to add later if needed.
- An auto-created starter `nutrition-db.md`. Daylog never writes this file; it only reads. The format is documented in `README.md`.

## Architecture

### File map (changes)

```
src/
  db.rs                        +foods, +food_aliases, +food_ingredients tables
                               +FoodInsert, +FoodLookup, +FoodIngredient types
                               +insert_food, +delete_all_foods,
                               +lookup_food_by_name_or_alias, +nutrition_status
  materializer.rs              REMOVED — split into materializer/{mod.rs, daily.rs}
  materializer/
    mod.rs                     re-exports + shared helpers (preprocess_yaml, etc.)
    daily.rs                   existing daily-note parser
                               *materialized_file_kind dispatch fn + FileKind enum
                               *watcher dispatches by FileKind
                               *sync_all / rebuild_all also call nutrition::materialize_nutrition_db
    nutrition.rs               new: split_entries, build_food_insert,
                               materialize_nutrition_db
README.md                      +"Nutrition database" section with format spec + example
CLAUDE.md                      +file-map entries for nutrition.rs and the new db helpers
```

### Data flow

```
Watcher event on {notes_dir}/nutrition-db.md
  → debounced 500 ms (existing logic)
  → materialize_nutrition_db(conn, path, config)
    → DELETE-then-INSERT-all in one transaction (mirrors materialize_file)
    → split_entries: line-oriented state machine, returns Vec<ParsedEntry>
    → for each entry: build_food_insert → db::insert_food
    → entries that fail validation: warn to stderr, skip, keep going

daylog rebuild
  → existing rebuild_all loops daily notes, then once-off calls
    materialize_nutrition_db if file exists.

daylog sync
  → same loop with mtime gate; nutrition-db.md is mtime-gated too.
```

### Key invariants

- DELETE-then-INSERT-all per file change. No incremental per-entry diffing. Cheap because `foods` is small (tens to hundreds of entries) and mirrors the existing daily-note pattern.
- A single bad entry doesn't kill the whole rebuild — warn and continue. File-level failures (file unreadable, no entries in a non-empty file) bubble up as `Err`.
- `nutrition-db.md` lives at `{notes_dir}/nutrition-db.md`. No new config option for path.
- Deletion of `nutrition-db.md` is a no-op. The `foods` table retains its last successful state. Rationale: deleting the file is more likely a fat-finger than an explicit "wipe my food DB" intent.
- Empty / missing file → silent no-op, returns 0.

## Markdown convention

Each entry is a `## Heading` followed by a fenced ` ```yaml ` block. Freeform prose may follow the block and is stored as `notes`. Dividers (`---`) between entries are tolerated but not required. A leading `# H1` title is also tolerated.

````markdown
## Kelda Skogssvampsoppa

```yaml
per_100g:
  kcal: 70
  protein: 1.4
  carbs: 4.8
  fat: 5.0
  sat_fat: 3.0
  sugar: 1.6
  salt: 0.89
gi: 40
gl_per_100g: 2
ii: 35
aliases: [skogssvampsoppa]
```

Innehåller svamp + grädde — IBS-trigger.

## Laktosfri helmjölk 3%

```yaml
per_100ml:
  kcal: 62
  protein: 3.4
  carbs: 4.8
  fat: 3.0
density_g_per_ml: 1.03
gi: 30
ii: 90
aliases: [helmjölk, mjölk]
```

## proteinshake

```yaml
description: 62g pulver + 4 dl vatten
ingredients:
  - food: Body Science Whey 100% Madagascar Vanilla
    amount_g: 62
gi: 5
ii: 85
```
````

### Recognized YAML fields

All optional except: at least one of `per_100g` / `per_100ml` / `total` must be present.

| Field | Type | Notes |
|---|---|---|
| `per_100g` | mapping | Canonical for solids. Subkeys: `kcal`, `protein`, `carbs`, `fat`, `sat_fat`, `sugar`, `salt`, `fiber`. All optional reals. |
| `per_100ml` | mapping | Canonical for liquids. Same subkeys. |
| `density_g_per_ml` | real (>0) | Rejected if ≤ 0. |
| `gi` | real | Glycemic index. Warning logged (still stored) if outside 0..200. |
| `gl_per_100g` | real | Glycemic load per 100g. |
| `ii` | real | Insulin index. Warning logged (still stored) if outside 0..200. |
| `aliases` | list of strings | Trimmed, lowercased, deduped. The heading's lowercased form is added implicitly. |
| `description` | string | Free text — for composites like "62g pulver + 4 dl vatten". |
| `ingredients` | list of `{food: str, amount_g: real}` | Stored as-is. Entries missing `food` are skipped with warning. Missing `amount_g` is allowed (stored NULL). |
| `total` | mapping | For composite recipes. Subkeys: `weight_g`, `kcal`, `protein`, `carbs`, `fat`, `sat_fat`, `sugar`, `salt`, `fiber`. |

Unknown top-level YAML keys → warning to stderr, entry still inserted. Unknown subkeys under `per_100g` / `per_100ml` / `total` → silently ignored. Both rules are forward-compatibility hooks.

The heading text (after `## `, trimmed) is the canonical food name (`foods.name`, case-preserved). It is also auto-inserted as a lowercased alias.

## Database schema

Three new tables added to `CORE_SCHEMA` in `db.rs`:

```sql
CREATE TABLE IF NOT EXISTS foods (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,           -- heading text, case-preserved
    -- Per-100g (canonical for solids). NULL if liquid-only.
    kcal_per_100g       REAL,
    protein_per_100g    REAL,
    carbs_per_100g      REAL,
    fat_per_100g        REAL,
    sat_fat_per_100g    REAL,
    sugar_per_100g      REAL,
    salt_per_100g       REAL,
    fiber_per_100g      REAL,
    -- Per-100ml (canonical for liquids). NULL if solid-only.
    kcal_per_100ml      REAL,
    protein_per_100ml   REAL,
    carbs_per_100ml     REAL,
    fat_per_100ml       REAL,
    sat_fat_per_100ml   REAL,
    sugar_per_100ml     REAL,
    salt_per_100ml      REAL,
    fiber_per_100ml     REAL,
    density_g_per_ml    REAL,
    -- Composite recipe totals (NULL for plain foods).
    total_weight_g      REAL,
    total_kcal          REAL,
    total_protein       REAL,
    total_carbs         REAL,
    total_fat           REAL,
    total_sat_fat       REAL,
    total_sugar         REAL,
    total_salt          REAL,
    total_fiber         REAL,
    -- Indices
    gi                  REAL,
    gl_per_100g         REAL,
    ii                  REAL,
    -- Free text
    description         TEXT,
    notes               TEXT
);

CREATE TABLE IF NOT EXISTS food_aliases (
    food_id INTEGER NOT NULL REFERENCES foods(id) ON DELETE CASCADE,
    alias TEXT NOT NULL,                 -- always lowercased
    PRIMARY KEY (food_id, alias)
);

CREATE INDEX IF NOT EXISTS idx_food_aliases_alias ON food_aliases(alias);

CREATE TABLE IF NOT EXISTS food_ingredients (
    food_id INTEGER NOT NULL REFERENCES foods(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,           -- preserve list order
    ingredient_name TEXT NOT NULL,       -- the food field as written; not FK
    amount_g REAL,
    PRIMARY KEY (food_id, position)
);
```

### Schema notes

- Wide column-per-nutrient layout instead of EAV. The set of fields is small and stable; queries from `daylog food` will want all values at once. Mirrors how `days` is laid out.
- `food_ingredients.ingredient_name` is a string, not an FK to `foods`. The issue's example proteinshake references "Body Science Whey 100% Madagascar Vanilla" by display name; we don't enforce that the referenced food exists in v1.
- `name` is unique. Two entries with the same heading is a parse error (warned + second one skipped).

### Helpers in `db.rs`

```rust
pub fn delete_all_foods(conn: &Connection) -> Result<()>;  // CASCADEs aliases + ingredients

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

pub struct FoodIngredient {
    pub ingredient_name: String,
    pub amount_g: Option<f64>,
}

pub struct FoodInsert {
    pub name: String,
    pub per_100g: Option<NutrientPanel>,
    pub per_100ml: Option<NutrientPanel>,
    pub density_g_per_ml: Option<f64>,
    pub total: Option<TotalPanel>,
    pub gi: Option<f64>,
    pub gl_per_100g: Option<f64>,
    pub ii: Option<f64>,
    pub description: Option<String>,
    pub notes: Option<String>,
    pub aliases: Vec<String>,           // already lowercased + heading included
    pub ingredients: Vec<FoodIngredient>,
}

pub fn insert_food(conn: &Connection, food: &FoodInsert) -> Result<i64>;

pub struct FoodLookup {
    pub id: i64,
    pub name: String,
    pub per_100g: Option<NutrientPanel>,
    pub per_100ml: Option<NutrientPanel>,
    pub density_g_per_ml: Option<f64>,
    pub total: Option<TotalPanel>,
    pub gi: Option<f64>,
    pub gl_per_100g: Option<f64>,
    pub ii: Option<f64>,
    pub description: Option<String>,
    pub notes: Option<String>,
}

/// Case-insensitive lookup. Lowercases `query` before matching against
/// `food_aliases.alias` (which is already stored lowercased, including
/// the auto-inserted lowercased heading).
pub fn lookup_food_by_name_or_alias(
    conn: &Connection,
    query: &str,
) -> Result<Option<FoodLookup>>;

pub struct NutritionStatus {
    pub foods_count: i64,
    pub last_synced: Option<String>,    // ISO8601
}

pub fn nutrition_status(conn: &Connection) -> Result<NutritionStatus>;
```

`lookup_food_by_name_or_alias` is included now — even though the lookup CLI is issue #6 — so the parser tests can verify end-to-end roundtrips. Issue #6 just calls it.

## Parser

### Layout

`src/materializer.rs` becomes a directory:

```
src/materializer/
  mod.rs        pub use daily::{materialize_file, sync_all, rebuild_all,
                                start_watcher, preprocess_yaml,
                                yaml_f64_field, yaml_i32_field, yaml_str_field,
                                materialized_file_kind, FileKind};
                pub use nutrition::materialize_nutrition_db;
  daily.rs      existing daily-note parser, lifted from materializer.rs
  nutrition.rs  new: split_entries, build_food_insert, materialize_nutrition_db
```

External callers (`main.rs`, `app.rs`, `modules/training.rs`, `tests/integration.rs`) keep their existing `crate::materializer::*` import paths unchanged — the re-exports preserve the public surface.

### Public surface (nutrition.rs)

```rust
pub fn materialize_nutrition_db(
    conn: &Connection,
    file_path: &Path,
    config: &Config,    // currently unused — reserved for future field
) -> Result<usize>;     // returns count of foods successfully inserted

struct ParsedEntry {
    name: String,
    yaml: Yaml,           // the parsed fenced block
    notes: Option<String>,
    line_number: usize,    // for error messages
}

fn split_entries(content: &str) -> Vec<ParsedEntry>;
fn build_food_insert(entry: &ParsedEntry) -> Result<FoodInsert>;
```

### Splitting algorithm (line-oriented)

1. Iterate lines with their numbers.
2. State machine: `OutsideEntry` → `InsideEntry { name, lineno, yaml_lines, notes_lines, in_yaml_fence }`.
3. A line `^## (.+)` starts a new entry (after flushing the prior one).
4. Inside an entry: a line matching `^```yaml\s*$` opens the YAML fence. Everything until the closing ` ``` ` accumulates into `yaml_lines`. Everything else becomes `notes_lines`.
5. `---` dividers and the H1 title at the top of the file are tolerated and skipped.
6. At EOF, flush the last entry.

We use a hand-rolled line splitter rather than `pulldown-cmark` to avoid pulling in a markdown CST dependency for this single, simple shape.

The fenced YAML block is parsed by `yaml_rust2::YamlLoader::load_from_str` directly, **without** running `preprocess_yaml`. The daily-note preprocessor exists to repair common frontmatter quirks (missing spaces after colons, unquoted sleep ranges with embedded colons). Those quirks don't apply inside a fenced ` ```yaml ` block, where the user is explicitly writing YAML.

### Validation in `build_food_insert`

- At least one of `per_100g` / `per_100ml` / `total` must be present.
- `density_g_per_ml` rejected if ≤ 0.
- `gi`, `ii` outside 0..200 → warning, still stored.
- Unknown top-level YAML keys → warning, entry still inserted.
- Unknown subkeys under `per_100g` / `per_100ml` / `total` → silently ignored.
- `aliases`: trimmed, lowercased, deduped; heading's lowercased form added implicitly.
- `ingredients` entries missing `food` → skipped with warning. Missing `amount_g` → stored NULL.

### Error policy

- `materialize_nutrition_db` returns `Ok(count)` even when individual entries fail. Per-entry failures print `"Warning: nutrition-db.md entry '{name}' (line {N}): {err}"` to stderr.
- File-level failures (file unreadable; entire file YAML-malformed in a way that prevents splitting) bubble up as `Err`.
- Empty / missing `nutrition-db.md` → silent no-op, returns 0.

### Insertion transaction

```
BEGIN
  delete_all_foods(&tx)                // CASCADEs aliases + ingredients
  for each successfully-built FoodInsert:
    insert_food(&tx, &fi)              // 1 INSERT to foods + N to aliases + M to ingredients
                                        // UNIQUE conflict on `name` → warn + skip; tx not aborted
  set sync_meta last_nutrition_sync = mtime
COMMIT
```

## Watcher integration

The existing watcher in `start_watcher` filters note files via `is_note_file` (regex `^\d{4}-\d{2}-\d{2}\.md$`). Two changes:

1. New dispatch:
   ```rust
   pub enum FileKind { DailyNote, NutritionDb }
   pub fn materialized_file_kind(path: &Path) -> Option<FileKind>;
   ```
   `None` for hidden/swap/non-md/other names.

2. The pending-files loop dispatches:
   ```rust
   for path in pending_files.drain() {
       if !path.exists() {
           match materialized_file_kind(&path) {
               Some(FileKind::DailyNote) => {
                   // existing delete-by-date logic
               }
               Some(FileKind::NutritionDb) => {
                   // no-op: deletion does not wipe the foods table
               }
               None => {}
           }
           continue;
       }
       match materialized_file_kind(&path) {
           Some(FileKind::DailyNote) => materialize_file(...),
           Some(FileKind::NutritionDb) => materialize_nutrition_db(...),
           None => {} // shouldn't reach: filter excluded it
       }
   }
   ```

Connection-loss detection (`disk I/O error` / `database is locked` / `unable to open`) is unchanged.

Deletion of `nutrition-db.md` is a no-op: the missing-path branch dispatches on `FileKind::NutritionDb` and does not touch the `foods` table.

## `sync_all` / `rebuild_all`

Both gain a single call after the daily-note loop completes:

```rust
let nutrition_path = notes_dir.join("nutrition-db.md");
if nutrition_path.exists() {
    let mtime = std::fs::metadata(&nutrition_path)?
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    // sync_all only: skip if mtime < threshold
    // rebuild_all: always parse
    match nutrition::materialize_nutrition_db(conn, &nutrition_path, config) {
        Ok(_n) => synced += 1,
        Err(e) => {
            eprintln!("Error parsing nutrition-db.md: {e}");
            errors += 1;
        }
    }
}
```

## `daylog status --json`

Adds one field to the top-level JSON:

```json
"nutrition_db": {
  "foods_count": 32,
  "last_synced": "2026-04-29T14:22:11"
}
```

Backed by `db::nutrition_status`. `last_synced` is null when the table has never been populated.

## Configuration

No new config options. `nutrition-db.md` is expected at `{notes_dir}/nutrition-db.md`. The path is not configurable in v1.

## Documentation

`README.md` gains a "Nutrition database" section with:
- The file location: `{notes_dir}/nutrition-db.md`.
- The format: heading + ```yaml block, with the recognized fields table.
- Two short example entries (one solid + one composite).
- A note that the file is parsed live by the watcher and rebuilt by `daylog rebuild`.

`CLAUDE.md` gains entries under "File Map" for the new `materializer/nutrition.rs` and the new `db.rs` helpers.

## Error handling matrix

| Situation | Behavior |
|---|---|
| `nutrition-db.md` missing | Silent no-op |
| `nutrition-db.md` present but empty | No-op, returns 0 |
| Heading without YAML block | Skipped, warning to stderr |
| YAML block with syntax error | Entry skipped, warning, other entries still parsed |
| Entry has none of per_100g / per_100ml / total | Skipped, warning naming the entry |
| `density_g_per_ml: 0` (or negative) | Skipped, warning |
| `gi` / `ii` outside 0..200 | Stored, warning |
| Unknown top-level key (e.g., `tags`) | Stored entry, warning |
| Unknown subkey under per_100g | Silently ignored |
| Two entries with the same heading | Second one skipped, warning, first wins |
| `ingredients` entry missing `food` | Item skipped, warning, other items in same entry kept |
| File deleted while watcher running | No-op; foods table unchanged |
| File contains only `---` and prose, no headings | No-op, returns 0 |

## Testing strategy

### `db.rs`

- `test_core_schema_creates_foods` — extends the existing schema test; assert `foods`, `food_aliases`, `food_ingredients` exist plus `idx_food_aliases_alias`.
- `test_food_cascade_delete` — insert food + alias + ingredient, `delete_all_foods`, assert child rows gone.
- `test_insert_food_roundtrip` — full FoodInsert in, `lookup_food_by_name_or_alias` returns equivalent FoodLookup.
- `test_lookup_case_insensitive` — heading `"Kelda Skogssvampsoppa"` → lookup with both `"kelda skogssvampsoppa"` and `"Kelda Skogssvampsoppa"` returns the same row (the helper lowercases the query before matching).
- `test_unique_name_conflict` — second `insert_food` with same name returns the conflict (caller skips).

### `materializer/nutrition.rs` parser

- `test_split_entries_basic` — three entries, one per_100g, one per_100ml, one composite. Verifies count, names, and that yaml block + notes prose are correctly partitioned.
- `test_split_handles_h1_and_dividers` — `# H1` at top + `---` between entries are tolerated and ignored.
- `test_entry_without_yaml_block_skipped` — heading with prose only → warning, not parsed.
- `test_yaml_only_no_per100_anything_errors` — entry with just `gi: 40` and no per_100g/per_100ml/total → error mentioning the entry name.
- `test_aliases_normalized` — `aliases: [Skogssvampsoppa, "te med mjölk"]` → all lowercased, deduped, heading auto-added.
- `test_ingredients_preserve_order` — three ingredients in YAML; `position` reflects list order; entry with missing `food` key skipped with warning.
- `test_unknown_top_level_key_warns_not_errors` — `tags: [foo]` → entry still inserted, warning issued.
- `test_density_validation` — `density_g_per_ml: 0` rejected; `density_g_per_ml: 1.03` accepted.

### Integration (parser → DB)

- `test_materialize_nutrition_db_e2e` — fixture file with three entries (skogssvampsoppa per_100g, mjölk per_100ml + density + alias, proteinshake composite with description + ingredients); call `materialize_nutrition_db`; assert foods count = 3, food_aliases contains expected aliases, food_ingredients has proteinshake's ingredient(s) in order.
- `test_rerun_replaces_all` — run twice; second run with one fewer entry; deleted entry is gone (DELETE-then-INSERT semantics).
- `test_partial_failure_continues` — three entries, middle one has YAML syntax error; assert two entries inserted, function returns `Ok(2)`, stderr contains the entry name.

### `sync_all` / `rebuild_all`

- `test_sync_includes_nutrition_db` — note dir has 2 daily notes + nutrition-db.md; sync_all from clean state → all parsed.
- `test_sync_skips_nutrition_when_unchanged` — set last_sync to now, touch nutrition-db.md backwards, run sync_all → foods count unchanged. Mirrors the daily-note mtime gate.
- `test_rebuild_reparses_nutrition` — same setup; `rebuild_all` re-parses regardless.
- `test_missing_nutrition_db_silent` — no nutrition-db.md present; sync_all/rebuild_all succeed without error.

### Watcher

- `test_materialized_file_kind` — pure unit test of dispatch fn: daily, nutrition, hidden file, swap file, random non-md → expected `FileKind` / `None`.
- The existing codebase has no integration tests of `start_watcher` itself, so we do not add one for the watcher in this PR. Manual end-to-end verification covers the integration. (Adding watcher integration tests is a separate concern.)

### Status JSON

- `test_status_json_includes_nutrition` — after sync, `daylog status --json` contains `nutrition_db.foods_count` matching DB.
- `test_status_json_no_nutrition_when_empty` — fresh DB, `foods_count: 0`, `last_synced: null`.

### Manual verification (in the implementation plan, not the spec)

- Write a hand-crafted nutrition-db.md fixture into a real notes_dir.
- Run `daylog rebuild`; check `daylog status --json | jq .nutrition_db`.
- Inspect: `sqlite3 .daylog.db "SELECT name, kcal_per_100g FROM foods"`.
- Edit a value in the markdown; verify the watcher reparses within ~500 ms.

## Out of scope (explicitly)

- Migrating Adrian's existing freetext entries. Done locally after install.
- `daylog food` CLI commands (lookup, write, alias). Issue #6.
- A Nutrition tab in the TUI.
- Resolving composite-recipe totals from ingredient amounts.
- A `per_portion` schema shorthand.
- Multi-density variants (raw vs. cooked).
- A `tags` field.
- Configuring `nutrition-db.md`'s path.
