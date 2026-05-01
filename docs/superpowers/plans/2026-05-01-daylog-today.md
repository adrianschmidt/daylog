# `daylog today [date]` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `daylog today [date]` — a one-shot CLI command that prints a compact daily summary (food macros, weight, sleep, BP morning, custom metrics) with optional goal comparison driven by suffix-keyed YAML frontmatter in `goals.md`.

**Architecture:** Three new modules — `food_sum.rs` (parses `## Food` markdown back to macro totals; inverse of `cli::food_cmd::format_line`), `goals.rs` (parses suffix-keyed YAML frontmatter from `goals.md`), and `cli/today_cmd.rs` (assembles a `DaySummary` from the food parser + DB + frontmatter, then renders text or JSON). No new external dependencies.

**Tech Stack:** Rust, rusqlite (existing), yaml-rust2 (existing), serde_json (existing), color_eyre (existing), chrono (existing).

**Spec:** `docs/superpowers/specs/2026-05-01-daylog-today-design.md`

---

## File Structure

| File | Status | Responsibility |
|---|---|---|
| `src/food_sum.rs` | new | Parse `## Food` section → `FoodTotals { kcal, protein, carbs, fat, entry_count, skipped_lines }`. Pure, no I/O. |
| `src/goals.rs` | new | Read `{notes_dir}/goals.md`, parse YAML frontmatter, suffix-group keys (`_target`/`_min`/`_max`) into `Goals`. |
| `src/cli/today_cmd.rs` | new | `DaySummary` type, `assemble()`, `render_text()`, `render_json()`, `execute()` entry point. |
| `src/lib.rs` | modify | Add `pub mod food_sum;` and `pub mod goals;`. |
| `src/cli/mod.rs` | modify | Add `pub mod today_cmd;` and `Today { date, json }` variant on `Commands`. |
| `src/main.rs` | modify | Dispatch `Commands::Today` to `cmd_today` helper. |
| `tests/today.rs` | new | End-to-end integration test with temp dir, fixture daily note + goals.md + DB. |
| `README.md` | modify | One-line mention under the existing CLI list. |

---

## Task 1: `food_sum.rs` — parse `## Food` into macro totals

**Files:**
- Create: `src/food_sum.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Create `src/food_sum.rs` with types and a stub returning zeros**

```rust
//! Parse the `## Food` section of a daily note back into aggregate
//! macro totals. Inverse of `cli::food_cmd::format_line`.

#[derive(Debug, Default, Clone, PartialEq)]
pub struct FoodTotals {
    pub kcal: f64,
    pub protein: f64,
    pub carbs: f64,
    pub fat: f64,
    pub entry_count: usize,
    pub skipped_lines: usize,
}

pub fn sum_food_section(_markdown: &str) -> FoodTotals {
    FoodTotals::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_returns_zeros() {
        assert_eq!(sum_food_section(""), FoodTotals::default());
    }
}
```

- [ ] **Step 2: Add `pub mod food_sum;` to `src/lib.rs`**

Insert in alphabetical position (after `db`, before `frontmatter`).

- [ ] **Step 3: Run the stub test**

```bash
cargo test --lib food_sum
```

Expected: 1 test passes.

- [ ] **Step 4: Add a failing test for the happy path**

Append inside `mod tests`:

```rust
    #[test]
    fn sums_single_well_formed_line() {
        let md = "---\ndate: 2026-04-30\n---\n\n## Food\n- **12:42** Soup (500g) (350 kcal, 7.0g protein, 24.0g carbs, 25.0g fat)\n";
        let r = sum_food_section(md);
        assert_eq!(r.kcal, 350.0);
        assert!((r.protein - 7.0).abs() < 1e-6);
        assert!((r.carbs - 24.0).abs() < 1e-6);
        assert!((r.fat - 25.0).abs() < 1e-6);
        assert_eq!(r.entry_count, 1);
        assert_eq!(r.skipped_lines, 0);
    }
```

- [ ] **Step 5: Run to confirm failure**

```bash
cargo test --lib food_sum::tests::sums_single_well_formed_line
```

Expected: FAIL (returns zeros).

- [ ] **Step 6: Implement section walker + literal-token parser**

Replace the stub `sum_food_section` and add helpers:

```rust
pub fn sum_food_section(markdown: &str) -> FoodTotals {
    let mut totals = FoodTotals::default();
    let lines: Vec<&str> = markdown.lines().collect();

    let start = match lines.iter().position(|l| l.trim_end() == "## Food") {
        Some(i) => i + 1,
        None => return totals,
    };
    let end = lines
        .iter()
        .enumerate()
        .skip(start)
        .find_map(|(i, l)| l.starts_with("## ").then_some(i))
        .unwrap_or(lines.len());

    for line in &lines[start..end] {
        if !line.starts_with("- **") {
            continue;
        }
        match parse_food_line(line) {
            Some((kcal, protein, carbs, fat)) => {
                totals.kcal += kcal;
                totals.protein += protein;
                totals.carbs += carbs;
                totals.fat += fat;
                totals.entry_count += 1;
            }
            None => {
                totals.skipped_lines += 1;
            }
        }
    }
    totals
}

fn parse_food_line(line: &str) -> Option<(f64, f64, f64, f64)> {
    let kcal = extract_number_before(line, " kcal")?;
    let protein = extract_number_before(line, "g protein").unwrap_or(0.0);
    let carbs = extract_number_before(line, "g carbs").unwrap_or(0.0);
    let fat = extract_number_before(line, "g fat").unwrap_or(0.0);
    Some((kcal, protein, carbs, fat))
}

/// Find the rightmost occurrence of `suffix` in `s`, then walk backwards
/// past whitespace to capture a number (digits + optional decimal point).
fn extract_number_before(s: &str, suffix: &str) -> Option<f64> {
    let pos = s.rfind(suffix)?;
    let before = &s.as_bytes()[..pos];
    let mut end = before.len();
    while end > 0 && before[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    let mut start = end;
    while start > 0 {
        let c = before[start - 1];
        if c.is_ascii_digit() || c == b'.' {
            start -= 1;
        } else {
            break;
        }
    }
    if start == end {
        return None;
    }
    std::str::from_utf8(&before[start..end]).ok()?.parse().ok()
}
```

- [ ] **Step 7: Run to confirm passes**

```bash
cargo test --lib food_sum
```

Expected: 2 tests pass.

- [ ] **Step 8: Add edge-case tests**

Append inside `mod tests`:

```rust
    #[test]
    fn sums_multiple_lines() {
        let md = "## Food\n- **08:00** A (100 kcal, 1.0g protein, 10.0g carbs, 2.0g fat)\n- **12:00** B (200 kcal, 5.0g protein, 20.0g carbs, 8.0g fat)\n";
        let r = sum_food_section(md);
        assert_eq!(r.kcal, 300.0);
        assert_eq!(r.entry_count, 2);
    }

    #[test]
    fn line_missing_kcal_token_is_skipped() {
        let md = "## Food\n- **12:00** Hand-edited line with no nutrients\n";
        let r = sum_food_section(md);
        assert_eq!(r.entry_count, 0);
        assert_eq!(r.skipped_lines, 1);
    }

    #[test]
    fn line_with_only_kcal_treats_missing_macros_as_zero() {
        let md = "## Food\n- **08:00** Coffee (5 kcal)\n";
        let r = sum_food_section(md);
        assert_eq!(r.kcal, 5.0);
        assert_eq!(r.protein, 0.0);
        assert_eq!(r.entry_count, 1);
        assert_eq!(r.skipped_lines, 0);
    }

    #[test]
    fn prose_lines_under_food_section_ignored() {
        let md = "## Food\nHad a great breakfast today.\n- **08:00** Eggs (200 kcal, 12.0g protein, 1.0g carbs, 15.0g fat)\n";
        let r = sum_food_section(md);
        assert_eq!(r.entry_count, 1);
        assert_eq!(r.skipped_lines, 0);
    }

    #[test]
    fn no_food_section_returns_zeros() {
        let md = "---\ndate: 2026-04-30\n---\n\n## Notes\n- Nothing\n";
        assert_eq!(sum_food_section(md), FoodTotals::default());
    }

    #[test]
    fn stops_at_next_section_heading() {
        let md = "## Food\n- **08:00** A (100 kcal, 1.0g protein, 10.0g carbs, 2.0g fat)\n## Notes\n- **09:00** B (999 kcal, 99.0g protein, 99.0g carbs, 99.0g fat)\n";
        let r = sum_food_section(md);
        assert_eq!(r.kcal, 100.0);
        assert_eq!(r.entry_count, 1);
    }

    #[test]
    fn round_trip_with_format_line() {
        use crate::cli::food_cmd::{format_line, RenderedEntry};
        let entry = RenderedEntry {
            display_name: "Test".into(),
            amount_segment: Some((500.0, "g")),
            kcal: Some(350.0),
            protein: Some(7.0),
            carbs: Some(24.0),
            fat: Some(25.0),
            gi: Some(40.0),
            gl: Some(10.0),
            ii: Some(35.0),
        };
        let line = format_line(&entry, "12:42");
        let md = format!("## Food\n{line}\n");
        let r = sum_food_section(&md);
        assert_eq!(r.kcal, 350.0);
        assert!((r.protein - 7.0).abs() < 1e-6);
        assert!((r.carbs - 24.0).abs() < 1e-6);
        assert!((r.fat - 25.0).abs() < 1e-6);
    }
```

- [ ] **Step 9: Run all tests**

```bash
cargo test --lib food_sum
```

Expected: 8 tests pass.

- [ ] **Step 10: Commit**

```bash
git add src/food_sum.rs src/lib.rs
git commit -m "feat: parse \`## Food\` section into macro totals (food_sum)"
```

---

## Task 2: `goals.rs` — parse goals.md frontmatter into a Goals map

**Files:**
- Create: `src/goals.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Create `src/goals.rs`**

```rust
//! Parse `{notes_dir}/goals.md` YAML frontmatter into a suffix-keyed
//! threshold map. Recognized suffixes: `_target`, `_min`, `_max`.
//! Non-matching keys are silently ignored. Body of goals.md is not
//! read (free-form prose for the user / LLMs).

use color_eyre::eyre::Result;
use color_eyre::Help;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use yaml_rust2::{Yaml, YamlLoader};

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Threshold {
    pub target: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct Goals {
    pub thresholds: HashMap<String, Threshold>,
    pub source_path: PathBuf,
    pub present: bool,
}

pub fn load_goals(notes_dir: &Path) -> Result<Goals> {
    let path = notes_dir.join("goals.md");
    let empty = Goals {
        thresholds: HashMap::new(),
        source_path: path.clone(),
        present: false,
    };
    if !path.exists() {
        return Ok(empty);
    }
    let content = std::fs::read_to_string(&path)?;
    let yaml_str = match extract_frontmatter(&content) {
        Some(s) => s,
        None => return Ok(empty),
    };
    let docs = YamlLoader::load_from_str(yaml_str)
        .map_err(|e| color_eyre::eyre::eyre!("Failed to parse goals.md frontmatter: {e}"))
        .suggestion("Goals frontmatter must be valid YAML between `---` markers.")?;
    let doc = match docs.into_iter().next() {
        Some(d) => d,
        None => return Ok(empty),
    };
    let map = match doc {
        Yaml::Hash(h) => h,
        _ => return Ok(empty),
    };

    let mut thresholds: HashMap<String, Threshold> = HashMap::new();
    for (k, v) in map {
        let key = match k.as_str() {
            Some(s) => s.to_string(),
            None => continue,
        };
        let (name, slot) = match split_suffix(&key) {
            Some(p) => p,
            None => continue,
        };
        let value = yaml_to_f64(&v).ok_or_else(|| {
            color_eyre::eyre::eyre!(
                "goals.md `{key}` must be a number, got: {}",
                yaml_to_display(&v)
            )
        })
        .suggestion("Set numeric values like `kcal_min: 1900`.")?;
        let entry = thresholds.entry(name.to_string()).or_default();
        match slot {
            "target" => entry.target = Some(value),
            "min" => entry.min = Some(value),
            "max" => entry.max = Some(value),
            _ => unreachable!("split_suffix only returns target/min/max"),
        }
    }

    Ok(Goals {
        present: !thresholds.is_empty(),
        thresholds,
        source_path: path,
    })
}

/// Return the YAML between leading `---\n` and the next `---\n` line, or
/// `None` if no frontmatter block exists.
fn extract_frontmatter(content: &str) -> Option<&str> {
    let body = content.strip_prefix("---\n")?;
    let close = body.find("\n---\n").or_else(|| {
        // Allow trailing close marker without final newline.
        if body.ends_with("\n---") {
            Some(body.len() - 4)
        } else {
            None
        }
    })?;
    Some(&body[..close])
}

/// Split `<name>_<slot>` where slot is one of `target`/`min`/`max`.
/// Tries longer suffixes first (none of them shadow another, but order
/// is fixed for clarity).
fn split_suffix(key: &str) -> Option<(&str, &str)> {
    for (suffix, slot) in [("_target", "target"), ("_min", "min"), ("_max", "max")] {
        if let Some(name) = key.strip_suffix(suffix) {
            if !name.is_empty() {
                return Some((name, slot));
            }
        }
    }
    None
}

fn yaml_to_f64(y: &Yaml) -> Option<f64> {
    match y {
        Yaml::Integer(i) => Some(*i as f64),
        Yaml::Real(s) => s.parse().ok(),
        _ => None,
    }
}

fn yaml_to_display(y: &Yaml) -> String {
    match y {
        Yaml::String(s) => format!("`{s}`"),
        Yaml::Boolean(b) => format!("`{b}`"),
        Yaml::Null => "`null`".into(),
        _ => "non-numeric value".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_goals(dir: &Path, body: &str) {
        std::fs::write(dir.join("goals.md"), body).unwrap();
    }

    #[test]
    fn missing_file_returns_empty_not_present() {
        let dir = tempfile::TempDir::new().unwrap();
        let g = load_goals(dir.path()).unwrap();
        assert!(!g.present);
        assert!(g.thresholds.is_empty());
    }

    #[test]
    fn parses_simple_kcal_min() {
        let dir = tempfile::TempDir::new().unwrap();
        write_goals(dir.path(), "---\nkcal_min: 1900\n---\n\n# notes\n");
        let g = load_goals(dir.path()).unwrap();
        assert!(g.present);
        let t = g.thresholds.get("kcal").unwrap();
        assert_eq!(t.min, Some(1900.0));
        assert_eq!(t.max, None);
        assert_eq!(t.target, None);
    }

    #[test]
    fn groups_three_suffixes_for_one_metric() {
        let dir = tempfile::TempDir::new().unwrap();
        write_goals(
            dir.path(),
            "---\nkcal_min: 1900\nkcal_max: 2200\nkcal_target: 2050\n---\n",
        );
        let g = load_goals(dir.path()).unwrap();
        let t = g.thresholds.get("kcal").unwrap();
        assert_eq!(t.min, Some(1900.0));
        assert_eq!(t.max, Some(2200.0));
        assert_eq!(t.target, Some(2050.0));
    }

    #[test]
    fn non_suffix_keys_ignored() {
        let dir = tempfile::TempDir::new().unwrap();
        write_goals(
            dir.path(),
            "---\nkcal_min: 1900\nnotes: \"hi\"\nfavourite_color: blue\n---\n",
        );
        let g = load_goals(dir.path()).unwrap();
        assert_eq!(g.thresholds.len(), 1);
        assert!(g.thresholds.contains_key("kcal"));
    }

    #[test]
    fn no_frontmatter_returns_not_present() {
        let dir = tempfile::TempDir::new().unwrap();
        write_goals(dir.path(), "# Just a heading, no YAML\n");
        let g = load_goals(dir.path()).unwrap();
        assert!(!g.present);
    }

    #[test]
    fn empty_thresholds_not_present() {
        let dir = tempfile::TempDir::new().unwrap();
        write_goals(dir.path(), "---\nnotes: \"only commentary\"\n---\n");
        let g = load_goals(dir.path()).unwrap();
        assert!(!g.present);
    }

    #[test]
    fn non_numeric_value_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        write_goals(dir.path(), "---\nkcal_min: \"foo\"\n---\n");
        let err = load_goals(dir.path()).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("kcal_min"), "got: {msg}");
        assert!(msg.contains("must be a number"), "got: {msg}");
    }

    #[test]
    fn float_values_parse() {
        let dir = tempfile::TempDir::new().unwrap();
        write_goals(dir.path(), "---\nweight_target: 110.5\n---\n");
        let g = load_goals(dir.path()).unwrap();
        let t = g.thresholds.get("weight").unwrap();
        assert_eq!(t.target, Some(110.5));
    }

    #[test]
    fn multi_word_metric_name_uses_full_prefix() {
        let dir = tempfile::TempDir::new().unwrap();
        write_goals(dir.path(), "---\nresting_hr_max: 65\n---\n");
        let g = load_goals(dir.path()).unwrap();
        let t = g.thresholds.get("resting_hr").unwrap();
        assert_eq!(t.max, Some(65.0));
    }
}
```

- [ ] **Step 2: Add `pub mod goals;` to `src/lib.rs`**

Insert in alphabetical position (after `frontmatter`, before `materializer`).

- [ ] **Step 3: Run tests**

```bash
cargo test --lib goals
```

Expected: 9 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/goals.rs src/lib.rs
git commit -m "feat: parse goals.md frontmatter into suffix-keyed thresholds"
```

---

## Task 3: CLI variant + main wiring + today_cmd skeleton

**Files:**
- Create: `src/cli/today_cmd.rs`
- Modify: `src/cli/mod.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Create `src/cli/today_cmd.rs` skeleton**

```rust
//! `daylog today [date]` — print a compact daily summary.

use color_eyre::eyre::Result;

use crate::config::Config;

pub fn execute(_date_flag: Option<&str>, _json: bool, _config: &Config) -> Result<()> {
    color_eyre::eyre::bail!("daylog today: not yet implemented")
}
```

- [ ] **Step 2: Add `pub mod today_cmd;` to `src/cli/mod.rs`**

Insert after `pub mod sleep_cmd;` (currently line 7).

- [ ] **Step 3: Add `Today` variant to `Commands` enum in `src/cli/mod.rs`**

Append inside the `Commands` enum, after the existing `Bp { .. }` variant (around line 148):

```rust
    /// Print a compact daily summary (food totals, weight, sleep, BP morning,
    /// custom metrics) with optional goal comparison from goals.md.
    Today {
        /// Date in YYYY-MM-DD format (defaults to effective today)
        date: Option<String>,
        /// Print JSON instead of formatted text
        #[arg(long)]
        json: bool,
    },
```

- [ ] **Step 4: Wire dispatch in `src/main.rs`**

Add a match arm in the main `match cli.command` block (alongside the other arms):

```rust
        Some(Commands::Today { date, json }) => cmd_today(date, json),
```

Append at the bottom of `src/main.rs`:

```rust
fn cmd_today(date: Option<String>, json: bool) -> Result<()> {
    let config = Config::load()?;
    daylog::cli::today_cmd::execute(date.as_deref(), json, &config)
}
```

- [ ] **Step 5: Build to confirm it compiles**

```bash
cargo build
```

Expected: clean build, no warnings about unused variants.

- [ ] **Step 6: Smoke-test the stub**

```bash
cargo run -- today 2>&1 | head -5
```

Expected: error message ending with `daylog today: not yet implemented`.

- [ ] **Step 7: Commit**

```bash
git add src/cli/today_cmd.rs src/cli/mod.rs src/main.rs
git commit -m "feat: add \`daylog today [date]\` CLI skeleton"
```

---

## Task 4: `today_cmd.rs` — `DaySummary` types + `render_text`

This task implements rendering against fixture `DaySummary` structs. No DB or filesystem reads yet — those land in Task 6. Pure functions are easier to test exhaustively, and the tests here pin the entire output format.

**Files:**
- Modify: `src/cli/today_cmd.rs`

- [ ] **Step 1: Replace `src/cli/today_cmd.rs` with types and a render skeleton**

```rust
//! `daylog today [date]` — print a compact daily summary.

use chrono::NaiveDate;
use color_eyre::eyre::Result;

use crate::config::{Config, WeightUnit};
use crate::food_sum::FoodTotals;
use crate::goals::{Goals, Threshold};

#[derive(Debug, Clone, Default)]
pub struct DayFields {
    pub weight: Option<f64>,
    pub sleep_hours: Option<f64>,
    pub sleep_start: Option<String>,
    pub sleep_end: Option<String>,
    pub mood: Option<i32>,
    pub energy: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct BpReading {
    pub sys: i32,
    pub dia: i32,
    pub pulse: i32,
}

/// One row in the `[metrics]` config-driven custom-metrics list.
#[derive(Debug, Clone)]
pub struct CustomMetric {
    pub id: String,
    pub display: String,
    pub value: Option<f64>,
    pub unit: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DaySummary {
    pub date: NaiveDate,
    pub food: FoodTotals,
    pub day: DayFields,
    /// `(delta, previous_logged_date)` if today has a weight and a prior
    /// day with a weight exists.
    pub weight_delta: Option<(f64, NaiveDate)>,
    pub bp_morning: Option<BpReading>,
    pub custom_metrics: Vec<CustomMetric>,
    pub food_skipped: usize,
    pub goals_warnings: Vec<String>,
    pub weight_unit: WeightUnit,
}

pub fn execute(_date_flag: Option<&str>, _json: bool, _config: &Config) -> Result<()> {
    color_eyre::eyre::bail!("daylog today: not yet implemented")
}

/// Render the summary as a human-readable terminal block.
/// `color = true` enables ANSI escape codes for accent colors.
pub fn render_text(_summary: &DaySummary, _goals: &Goals, _color: bool) -> String {
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
}
```

- [ ] **Step 2: Confirm it compiles**

```bash
cargo build
```

Expected: clean build.

- [ ] **Step 3: Add a fixture helper + a failing test for the food block**

Inside `mod tests`, append:

```rust
    use crate::config::WeightUnit;
    use std::collections::HashMap;

    fn fixture_summary() -> DaySummary {
        DaySummary {
            date: NaiveDate::from_ymd_opt(2026, 4, 30).unwrap(),
            food: FoodTotals {
                kcal: 1513.0,
                protein: 147.0,
                carbs: 77.0,
                fat: 59.0,
                entry_count: 4,
                skipped_lines: 0,
            },
            day: DayFields {
                weight: Some(121.5),
                sleep_hours: Some(6.4),
                sleep_start: Some("23:00".into()),
                sleep_end: Some("05:24".into()),
                mood: None,
                energy: None,
            },
            weight_delta: Some((1.3, NaiveDate::from_ymd_opt(2026, 4, 29).unwrap())),
            bp_morning: None,
            custom_metrics: vec![],
            food_skipped: 0,
            goals_warnings: vec![],
            weight_unit: WeightUnit::Kg,
        }
    }

    fn fixture_goals() -> Goals {
        let mut thresholds = HashMap::new();
        thresholds.insert(
            "kcal".into(),
            Threshold {
                min: Some(1900.0),
                max: Some(2200.0),
                target: None,
            },
        );
        thresholds.insert(
            "protein".into(),
            Threshold {
                min: Some(140.0),
                max: None,
                target: None,
            },
        );
        thresholds.insert(
            "weight".into(),
            Threshold {
                target: Some(110.0),
                min: None,
                max: None,
            },
        );
        Goals {
            thresholds,
            source_path: std::path::PathBuf::from("/tmp/goals.md"),
            present: true,
        }
    }

    #[test]
    fn render_text_food_block_with_goals() {
        let s = fixture_summary();
        let g = fixture_goals();
        let out = render_text(&s, &g, false);
        assert!(out.contains("2026-04-30 — Daily summary"), "got:\n{out}");
        assert!(out.contains("Calories:"), "got:\n{out}");
        assert!(out.contains("1513"), "got:\n{out}");
        assert!(out.contains("1900–2200 kcal"), "got:\n{out}");
        assert!(out.contains("387 below min"), "got:\n{out}");
        assert!(out.contains("Protein:"), "got:\n{out}");
        assert!(out.contains("147"), "got:\n{out}");
        assert!(out.contains("≥140 g"), "got:\n{out}");
        assert!(out.contains("over minimum"), "got:\n{out}");
        assert!(out.contains("Carbs:"), "got:\n{out}");
        assert!(out.contains("77 g"), "got:\n{out}");
        assert!(out.contains("Fat:"), "got:\n{out}");
        assert!(out.contains("59 g"), "got:\n{out}");
    }
```

- [ ] **Step 4: Run to confirm failure**

```bash
cargo test --lib today_cmd::tests::render_text_food_block_with_goals
```

Expected: FAIL (render_text returns empty string).

- [ ] **Step 5: Implement `render_text` body block + helpers**

Replace the stub `render_text` and add helpers:

```rust
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";

fn paint(color: bool, code: &str, body: &str) -> String {
    if color {
        format!("{code}{body}{RESET}")
    } else {
        body.to_string()
    }
}

pub fn render_text(summary: &DaySummary, goals: &Goals, color: bool) -> String {
    let mut out = String::new();
    out.push_str(&format!("{} — Daily summary\n\n", summary.date));

    // --- Food block ---
    let kcal_t = goals.thresholds.get("kcal");
    out.push_str(&render_food_row(
        "Calories",
        summary.food.kcal,
        "kcal",
        kcal_t,
        color,
    ));
    let protein_t = goals.thresholds.get("protein");
    out.push_str(&render_food_row(
        "Protein",
        summary.food.protein,
        "g",
        protein_t,
        color,
    ));
    let carbs_t = goals.thresholds.get("carbs");
    out.push_str(&render_food_row(
        "Carbs",
        summary.food.carbs,
        "g",
        carbs_t,
        color,
    ));
    let fat_t = goals.thresholds.get("fat");
    out.push_str(&render_food_row("Fat", summary.food.fat, "g", fat_t, color));

    out.push('\n');

    // --- Weight / Sleep / BP ---
    out.push_str(&render_weight_row(summary, goals.thresholds.get("weight"), color));
    out.push_str(&render_sleep_row(summary, color));
    out.push_str(&render_bp_row(summary, color));

    // --- Custom metrics ---
    for m in &summary.custom_metrics {
        out.push_str(&render_custom_row(m, goals.thresholds.get(&m.id), color));
    }

    // --- Hint lines ---
    let mut hints: Vec<String> = Vec::new();
    if !goals.present {
        hints.push(format!(
            "(No goals defined — add `<metric>_min/_max/_target` keys to {}.)",
            goals.source_path.display()
        ));
    }
    if summary.food_skipped > 0 {
        let plural = if summary.food_skipped == 1 { "" } else { "s" };
        hints.push(format!(
            "({} food line{plural} couldn't be parsed)",
            summary.food_skipped
        ));
    }
    for w in &summary.goals_warnings {
        hints.push(format!("({w})"));
    }
    if !hints.is_empty() {
        out.push('\n');
        for h in hints {
            out.push_str(&paint(color, DIM, &h));
            out.push('\n');
        }
    }

    out
}

fn render_food_row(
    label: &str,
    value: f64,
    unit: &str,
    threshold: Option<&Threshold>,
    color: bool,
) -> String {
    let value_int = value.round() as i64;
    let goal_part = match threshold {
        Some(t) => format_threshold_inline(t, unit),
        None => String::new(),
    };
    let annotation = match threshold {
        Some(t) => annotate_value(value, t, color),
        None => String::new(),
    };
    let body = if goal_part.is_empty() {
        format!("{label}: {value_int} {unit}")
    } else {
        format!("{label}: {value_int} / {goal_part}")
    };
    if annotation.is_empty() {
        format!("{body}\n")
    } else {
        format!("{body}     {annotation}\n")
    }
}

/// Format a threshold inline: "1900–2200 kcal", "≥140 g", "≤65 bpm",
/// "→ 110 kg", or combinations.
fn format_threshold_inline(t: &Threshold, unit: &str) -> String {
    match (t.min, t.max, t.target) {
        (Some(min), Some(max), _) => format!("{}–{} {unit}", trim_num(min), trim_num(max)),
        (Some(min), None, _) => format!("≥{} {unit}", trim_num(min)),
        (None, Some(max), _) => format!("≤{} {unit}", trim_num(max)),
        (None, None, Some(tgt)) => format!("→ {} {unit}", trim_num(tgt)),
        (None, None, None) => String::new(),
    }
}

fn trim_num(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v}")
    }
}

/// Build the trailing `(387 below min)` / `✓ over minimum` / `✓ within range`
/// annotation for a value vs threshold.
fn annotate_value(value: f64, t: &Threshold, color: bool) -> String {
    if let Some(min) = t.min {
        if value < min {
            let delta = (min - value).round() as i64;
            return paint(color, RED, &format!("({delta} below min)"));
        }
    }
    if let Some(max) = t.max {
        if value > max {
            let delta = (value - max).round() as i64;
            return paint(color, RED, &format!("({delta} above max)"));
        }
    }
    if t.min.is_some() && t.max.is_none() {
        return paint(color, GREEN, "✓ over minimum");
    }
    if t.min.is_none() && t.max.is_some() {
        return paint(color, GREEN, "✓ under maximum");
    }
    if t.min.is_some() && t.max.is_some() {
        return paint(color, GREEN, "✓ within range");
    }
    // Target-only: don't annotate (just show the target inline).
    String::new()
}

fn render_weight_row(summary: &DaySummary, threshold: Option<&Threshold>, color: bool) -> String {
    let unit = summary.weight_unit.to_string();
    let value = match summary.day.weight {
        Some(v) => v,
        None => {
            return format!("Weight:    {}\n", paint(color, DIM, "not logged"));
        }
    };
    let goal_part = match threshold {
        Some(t) => format_threshold_inline(t, &unit),
        None => String::new(),
    };
    let mut line = if goal_part.is_empty() {
        format!("Weight:    {} {unit}", trim_num(value))
    } else {
        format!("Weight:    {} {unit} / {goal_part}", trim_num(value))
    };
    if let Some((delta, prev_date)) = summary.weight_delta {
        let label = format_delta_label(summary.date, prev_date);
        let sign = if delta >= 0.0 { "+" } else { "" };
        line.push_str(&format!("  (Δ {sign}{} vs {label})", trim_num(delta)));
    }
    line.push('\n');
    line
}

fn format_delta_label(today: NaiveDate, prev: NaiveDate) -> String {
    let diff = today.signed_duration_since(prev).num_days();
    if diff == 1 {
        "yesterday".into()
    } else {
        prev.format("%Y-%m-%d").to_string()
    }
}

fn render_sleep_row(summary: &DaySummary, color: bool) -> String {
    match summary.day.sleep_hours {
        Some(h) => {
            let hours = h.floor() as i64;
            let mins = ((h - h.floor()) * 60.0).round() as i64;
            format!("Sleep:     {hours}h {mins:02}min\n")
        }
        None => format!("Sleep:     {}\n", paint(color, DIM, "not logged")),
    }
}

fn render_bp_row(summary: &DaySummary, color: bool) -> String {
    match &summary.bp_morning {
        Some(b) => format!("BP morning:   {}/{} (pulse {})\n", b.sys, b.dia, b.pulse),
        None => format!("BP morning:   {}\n", paint(color, DIM, "not logged")),
    }
}

fn render_custom_row(metric: &CustomMetric, threshold: Option<&Threshold>, color: bool) -> String {
    let unit_str = metric.unit.as_deref().unwrap_or("");
    let value_str = match metric.value {
        Some(v) => trim_num(v),
        None => return format!("{}: {}\n", metric.display, paint(color, DIM, "not logged")),
    };
    let goal_part = match threshold {
        Some(t) => format_threshold_inline(t, unit_str),
        None => String::new(),
    };
    let annotation = match (metric.value, threshold) {
        (Some(v), Some(t)) => annotate_value(v, t, color),
        _ => String::new(),
    };
    let body = if goal_part.is_empty() {
        if unit_str.is_empty() {
            format!("{}: {value_str}", metric.display)
        } else {
            format!("{}: {value_str} {unit_str}", metric.display)
        }
    } else {
        format!("{}: {value_str} / {goal_part}", metric.display)
    };
    if annotation.is_empty() {
        format!("{body}\n")
    } else {
        format!("{body}     {annotation}\n")
    }
}
```

- [ ] **Step 6: Run the food-block test**

```bash
cargo test --lib today_cmd::tests::render_text_food_block_with_goals
```

Expected: PASS.

- [ ] **Step 7: Add the remaining render_text tests**

Append inside `mod tests`:

```rust
    #[test]
    fn render_text_weight_sleep_bp_block() {
        let s = fixture_summary();
        let g = fixture_goals();
        let out = render_text(&s, &g, false);
        assert!(out.contains("Weight:    121.5 kg"), "got:\n{out}");
        assert!(out.contains("→ 110 kg"), "got:\n{out}");
        assert!(out.contains("Δ +1.3 vs yesterday"), "got:\n{out}");
        assert!(out.contains("Sleep:     6h 24min"), "got:\n{out}");
        assert!(out.contains("BP morning:"), "got:\n{out}");
        assert!(out.contains("not logged"), "got:\n{out}");
    }

    #[test]
    fn render_text_no_goals_emits_hint() {
        let s = fixture_summary();
        let g = Goals {
            thresholds: HashMap::new(),
            source_path: std::path::PathBuf::from("/notes/goals.md"),
            present: false,
        };
        let out = render_text(&s, &g, false);
        assert!(out.contains("No goals defined"), "got:\n{out}");
        assert!(out.contains("/notes/goals.md"), "got:\n{out}");
        // No goal annotations on rows.
        assert!(!out.contains("below min"));
        assert!(!out.contains("over minimum"));
    }

    #[test]
    fn render_text_skipped_food_lines_emits_hint() {
        let mut s = fixture_summary();
        s.food_skipped = 2;
        let g = fixture_goals();
        let out = render_text(&s, &g, false);
        assert!(
            out.contains("2 food lines couldn't be parsed"),
            "got:\n{out}"
        );
    }

    #[test]
    fn render_text_unknown_metric_warning() {
        let mut s = fixture_summary();
        s.goals_warnings.push("unknown metric `mystery` in goals.md".into());
        let g = fixture_goals();
        let out = render_text(&s, &g, false);
        assert!(out.contains("unknown metric `mystery`"), "got:\n{out}");
    }

    #[test]
    fn render_text_weight_delta_non_yesterday_uses_actual_date() {
        let mut s = fixture_summary();
        s.date = NaiveDate::from_ymd_opt(2026, 4, 30).unwrap();
        s.weight_delta = Some((0.4, NaiveDate::from_ymd_opt(2026, 4, 25).unwrap()));
        let g = fixture_goals();
        let out = render_text(&s, &g, false);
        assert!(out.contains("Δ +0.4 vs 2026-04-25"), "got:\n{out}");
        assert!(!out.contains("vs yesterday"));
    }

    #[test]
    fn render_text_color_off_strips_escapes() {
        let mut s = fixture_summary();
        s.day.weight = None; // forces a "not logged" row
        let g = fixture_goals();
        let out = render_text(&s, &g, false);
        assert!(!out.contains("\x1b["), "got:\n{out:?}");
    }

    #[test]
    fn render_text_color_on_includes_escapes_for_below_min() {
        let s = fixture_summary();
        let g = fixture_goals();
        let out = render_text(&s, &g, true);
        assert!(out.contains("\x1b[31m"), "got:\n{out:?}");
    }

    #[test]
    fn render_text_custom_metric_with_max_above_max() {
        let mut s = fixture_summary();
        s.custom_metrics.push(CustomMetric {
            id: "resting_hr".into(),
            display: "Resting HR".into(),
            value: Some(72.0),
            unit: Some("bpm".into()),
        });
        let mut g = fixture_goals();
        g.thresholds.insert(
            "resting_hr".into(),
            Threshold {
                max: Some(65.0),
                min: None,
                target: None,
            },
        );
        let out = render_text(&s, &g, false);
        assert!(out.contains("Resting HR: 72 / ≤65 bpm"), "got:\n{out}");
        assert!(out.contains("7 above max"), "got:\n{out}");
    }
```

- [ ] **Step 8: Run all today_cmd tests**

```bash
cargo test --lib today_cmd
```

Expected: 8 tests pass.

- [ ] **Step 9: Commit**

```bash
git add src/cli/today_cmd.rs
git commit -m "feat: render daily summary text block (today_cmd::render_text)"
```

---

## Task 5: `today_cmd.rs` — `render_json`

**Files:**
- Modify: `src/cli/today_cmd.rs`

- [ ] **Step 1: Add a failing test for the JSON shape**

Append inside `mod tests`:

```rust
    #[test]
    fn render_json_shape() {
        let s = fixture_summary();
        let g = fixture_goals();
        let v = render_json(&s, &g);
        assert_eq!(v["date"], "2026-04-30");
        let kcal = &v["metrics"]["kcal"];
        assert_eq!(kcal["value"], 1513.0);
        assert_eq!(kcal["min"], 1900.0);
        assert_eq!(kcal["max"], 2200.0);
        assert!(kcal["target"].is_null());
        assert_eq!(v["metrics"]["weight"]["value"], 121.5);
        assert_eq!(v["metrics"]["weight"]["target"], 110.0);
        assert_eq!(v["metrics"]["weight"]["delta"], 1.3);
        assert_eq!(v["metrics"]["weight"]["delta_vs_date"], "2026-04-29");
        assert!(v["bp_morning"].is_null());
        assert_eq!(v["sleep"]["hours"], 6.4);
        assert_eq!(v["sleep"]["start"], "23:00");
        assert_eq!(v["sleep"]["end"], "05:24");
        assert_eq!(v["goals_present"], true);
        assert!(v["warnings"].as_array().unwrap().is_empty());
    }

    #[test]
    fn render_json_includes_warnings_and_skipped() {
        let mut s = fixture_summary();
        s.food_skipped = 1;
        s.goals_warnings.push("unknown metric `mystery` in goals.md".into());
        let g = fixture_goals();
        let v = render_json(&s, &g);
        let warnings = v["warnings"].as_array().unwrap();
        assert!(warnings.iter().any(|w| w.as_str().unwrap().contains("mystery")));
        assert!(warnings.iter().any(|w| w.as_str().unwrap().contains("food line")));
    }
```

- [ ] **Step 2: Run to confirm failure**

```bash
cargo test --lib today_cmd::tests::render_json_shape
```

Expected: FAIL (`render_json` doesn't exist).

- [ ] **Step 3: Implement `render_json`**

Append to `src/cli/today_cmd.rs`:

```rust
pub fn render_json(summary: &DaySummary, goals: &Goals) -> serde_json::Value {
    let mut metrics = serde_json::Map::new();

    // Food macros — always present (zeros if no entries).
    metrics.insert("kcal".into(), metric_obj(summary.food.kcal, goals.thresholds.get("kcal"), None));
    metrics.insert("protein".into(), metric_obj(summary.food.protein, goals.thresholds.get("protein"), None));
    metrics.insert("carbs".into(), metric_obj(summary.food.carbs, goals.thresholds.get("carbs"), None));
    metrics.insert("fat".into(), metric_obj(summary.food.fat, goals.thresholds.get("fat"), None));

    // Optional days-table metrics.
    if let Some(w) = summary.day.weight {
        let mut o = metric_obj(w, goals.thresholds.get("weight"), None);
        if let Some((delta, prev)) = summary.weight_delta {
            o["delta"] = delta.into();
            o["delta_vs_date"] = prev.format("%Y-%m-%d").to_string().into();
        }
        metrics.insert("weight".into(), o);
    }
    if let Some(h) = summary.day.sleep_hours {
        metrics.insert("sleep_hours".into(), metric_obj(h, goals.thresholds.get("sleep_hours"), None));
    }
    if let Some(m) = summary.day.mood {
        metrics.insert("mood".into(), metric_obj(m as f64, goals.thresholds.get("mood"), None));
    }
    if let Some(e) = summary.day.energy {
        metrics.insert("energy".into(), metric_obj(e as f64, goals.thresholds.get("energy"), None));
    }

    // Custom metrics.
    for m in &summary.custom_metrics {
        if let Some(v) = m.value {
            metrics.insert(m.id.clone(), metric_obj(v, goals.thresholds.get(&m.id), m.unit.clone()));
        }
    }

    // Sleep object (richer view).
    let sleep = match (summary.day.sleep_hours, &summary.day.sleep_start, &summary.day.sleep_end) {
        (Some(h), Some(s), Some(e)) => serde_json::json!({
            "hours": h,
            "start": s,
            "end": e,
        }),
        (Some(h), _, _) => serde_json::json!({ "hours": h }),
        _ => serde_json::Value::Null,
    };

    // BP morning.
    let bp = match &summary.bp_morning {
        Some(b) => serde_json::json!({ "sys": b.sys, "dia": b.dia, "pulse": b.pulse }),
        None => serde_json::Value::Null,
    };

    // Warnings: collected from food_skipped + goals_warnings.
    let mut warnings: Vec<serde_json::Value> = summary
        .goals_warnings
        .iter()
        .map(|s| serde_json::Value::String(s.clone()))
        .collect();
    if summary.food_skipped > 0 {
        let plural = if summary.food_skipped == 1 { "" } else { "s" };
        warnings.push(serde_json::Value::String(format!(
            "{} food line{plural} couldn't be parsed",
            summary.food_skipped
        )));
    }

    serde_json::json!({
        "date": summary.date.format("%Y-%m-%d").to_string(),
        "metrics": serde_json::Value::Object(metrics),
        "sleep": sleep,
        "bp_morning": bp,
        "goals_present": goals.present,
        "warnings": warnings,
    })
}

fn metric_obj(value: f64, threshold: Option<&Threshold>, unit: Option<String>) -> serde_json::Value {
    let mut o = serde_json::Map::new();
    o.insert("value".into(), value.into());
    let (min, max, target) = match threshold {
        Some(t) => (t.min, t.max, t.target),
        None => (None, None, None),
    };
    o.insert("min".into(), min.map(serde_json::Value::from).unwrap_or(serde_json::Value::Null));
    o.insert("max".into(), max.map(serde_json::Value::from).unwrap_or(serde_json::Value::Null));
    o.insert("target".into(), target.map(serde_json::Value::from).unwrap_or(serde_json::Value::Null));
    if let Some(u) = unit {
        o.insert("unit".into(), serde_json::Value::String(u));
    }
    serde_json::Value::Object(o)
}
```

- [ ] **Step 4: Run JSON tests**

```bash
cargo test --lib today_cmd
```

Expected: all tests pass (10 total in this module).

- [ ] **Step 5: Commit**

```bash
git add src/cli/today_cmd.rs
git commit -m "feat: render daily summary as JSON (today_cmd::render_json)"
```

---

## Task 6: `today_cmd.rs` — `assemble` + `execute`

This task wires `render_text` / `render_json` to real data: parses today's note, queries the DB, computes the weight delta, extracts BP morning from frontmatter.

**Files:**
- Modify: `src/cli/today_cmd.rs`

- [ ] **Step 1: Add a failing test for assemble**

Append inside `mod tests`:

```rust
    use crate::db;
    use crate::db::NutrientPanel;

    fn config_in(notes_dir: &std::path::Path) -> Config {
        let toml_str = format!(
            "notes_dir = '{}'\ntime_format = '24h'\nweight_unit = 'kg'\n",
            notes_dir.display().to_string().replace('\\', "/")
        );
        toml::from_str(&toml_str).unwrap()
    }

    #[test]
    fn assemble_reads_food_weight_sleep_bp() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in(dir.path());

        // Write a daily note with food + BP morning frontmatter.
        let date = "2026-04-30";
        let note = format!(
            "---\n\
             date: {date}\n\
             weight: 121.5\n\
             sleep: \"23:00-05:24\"\n\
             bp_morning_sys: 138\n\
             bp_morning_dia: 88\n\
             bp_morning_pulse: 70\n\
             ---\n\n\
             ## Food\n\
             - **08:00** Eggs (200 kcal, 12.0g protein, 1.0g carbs, 15.0g fat)\n\
             - **12:00** Pasta (500 kcal, 18.0g protein, 80.0g carbs, 10.0g fat)\n"
        );
        std::fs::write(dir.path().join(format!("{date}.md")), note).unwrap();

        // Set up DB and sync the note (so days table gets weight/sleep).
        let registry = crate::modules::build_registry(&config);
        let conn = db::open_rw(&config.db_path()).unwrap();
        db::init_db(&conn, &registry).unwrap();
        crate::modules::validate_module_tables(&registry).unwrap();
        crate::materializer::sync_all(&conn, &config.notes_dir_path(), &config, &registry).unwrap();

        let target = NaiveDate::from_ymd_opt(2026, 4, 30).unwrap();
        let summary = assemble(target, &config, &conn).unwrap();

        assert_eq!(summary.food.kcal, 700.0);
        assert_eq!(summary.food.entry_count, 2);
        assert_eq!(summary.day.weight, Some(121.5));
        assert!((summary.day.sleep_hours.unwrap() - 6.4).abs() < 0.05);
        let bp = summary.bp_morning.unwrap();
        assert_eq!(bp.sys, 138);
        assert_eq!(bp.dia, 88);
        assert_eq!(bp.pulse, 70);
    }

    #[test]
    fn assemble_weight_delta_uses_previous_logged_day() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in(dir.path());

        for (d, w) in [("2026-04-25", 120.0), ("2026-04-30", 121.3)] {
            let note = format!("---\ndate: {d}\nweight: {w}\n---\n\n## Food\n");
            std::fs::write(dir.path().join(format!("{d}.md")), note).unwrap();
        }

        let registry = crate::modules::build_registry(&config);
        let conn = db::open_rw(&config.db_path()).unwrap();
        db::init_db(&conn, &registry).unwrap();
        crate::modules::validate_module_tables(&registry).unwrap();
        crate::materializer::sync_all(&conn, &config.notes_dir_path(), &config, &registry).unwrap();

        let target = NaiveDate::from_ymd_opt(2026, 4, 30).unwrap();
        let summary = assemble(target, &config, &conn).unwrap();
        let (delta, prev) = summary.weight_delta.unwrap();
        assert!((delta - 1.3).abs() < 1e-6);
        assert_eq!(prev, NaiveDate::from_ymd_opt(2026, 4, 25).unwrap());
    }

    #[test]
    fn assemble_missing_note_yields_zero_food() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in(dir.path());

        let registry = crate::modules::build_registry(&config);
        let conn = db::open_rw(&config.db_path()).unwrap();
        db::init_db(&conn, &registry).unwrap();
        crate::modules::validate_module_tables(&registry).unwrap();

        let target = NaiveDate::from_ymd_opt(2026, 4, 30).unwrap();
        let summary = assemble(target, &config, &conn).unwrap();
        assert_eq!(summary.food, FoodTotals::default());
        assert!(summary.day.weight.is_none());
        assert!(summary.bp_morning.is_none());
    }
```

- [ ] **Step 2: Run to confirm failure**

```bash
cargo test --lib today_cmd::tests::assemble_reads_food_weight_sleep_bp
```

Expected: FAIL (`assemble` doesn't exist).

- [ ] **Step 3: Implement `assemble`**

Append to `src/cli/today_cmd.rs`:

```rust
use rusqlite::Connection;
use yaml_rust2::{Yaml, YamlLoader};

pub fn assemble(date: NaiveDate, config: &Config, conn: &Connection) -> Result<DaySummary> {
    let date_str = date.format("%Y-%m-%d").to_string();

    // 1. Parse food from {date}.md (if it exists).
    let note_path = config.notes_dir_path().join(format!("{date_str}.md"));
    let note_content = std::fs::read_to_string(&note_path).unwrap_or_default();
    let food = crate::food_sum::sum_food_section(&note_content);

    // 2. days-table fields.
    let day = load_day_fields(conn, &date_str)?;

    // 3. Weight delta vs previous logged day (look back 60 days).
    let weight_delta = compute_weight_delta(conn, date, &day);

    // 4. BP morning — extract from YAML frontmatter (not in DB).
    let bp_morning = parse_bp_morning(&note_content);

    // 5. Custom metrics from [metrics] config.
    let custom_metrics = load_custom_metrics(conn, &date_str, config)?;

    let food_skipped = food.skipped_lines;
    Ok(DaySummary {
        date,
        food,
        day,
        weight_delta,
        bp_morning,
        custom_metrics,
        food_skipped,
        goals_warnings: vec![], // populated by execute() after loading goals
        weight_unit: config.weight_unit,
    })
}

fn load_day_fields(conn: &Connection, date_str: &str) -> Result<DayFields> {
    let mut stmt = conn.prepare(
        "SELECT sleep_start, sleep_end, sleep_hours, mood, energy, weight
         FROM days WHERE date = ?1",
    )?;
    let row = stmt
        .query_row([date_str], |r| {
            Ok(DayFields {
                sleep_start: r.get(0)?,
                sleep_end: r.get(1)?,
                sleep_hours: r.get(2)?,
                mood: r.get(3)?,
                energy: r.get(4)?,
                weight: r.get(5)?,
            })
        })
        .ok();
    Ok(row.unwrap_or_default())
}

fn compute_weight_delta(conn: &Connection, date: NaiveDate, day: &DayFields) -> Option<(f64, NaiveDate)> {
    let today_weight = day.weight?;
    // Pull recent weights; pick the most recent strictly before `date`.
    let trend = crate::db::load_weight_trend(conn, 60).ok()?;
    for (d_str, w) in trend {
        let d = NaiveDate::parse_from_str(&d_str, "%Y-%m-%d").ok()?;
        if d < date {
            return Some((today_weight - w, d));
        }
    }
    None
}

fn parse_bp_morning(content: &str) -> Option<BpReading> {
    let yaml_str = extract_frontmatter_str(content)?;
    let docs = YamlLoader::load_from_str(yaml_str).ok()?;
    let doc = docs.into_iter().next()?;
    let map = match doc {
        Yaml::Hash(h) => h,
        _ => return None,
    };
    let get_int = |key: &str| -> Option<i32> {
        map.iter()
            .find(|(k, _)| k.as_str() == Some(key))
            .and_then(|(_, v)| v.as_i64())
            .map(|i| i as i32)
    };
    Some(BpReading {
        sys: get_int("bp_morning_sys")?,
        dia: get_int("bp_morning_dia")?,
        pulse: get_int("bp_morning_pulse")?,
    })
}

fn extract_frontmatter_str(content: &str) -> Option<&str> {
    let body = content.strip_prefix("---\n")?;
    let close = body.find("\n---\n").or_else(|| {
        if body.ends_with("\n---") {
            Some(body.len() - 4)
        } else {
            None
        }
    })?;
    Some(&body[..close])
}

fn load_custom_metrics(conn: &Connection, date_str: &str, config: &Config) -> Result<Vec<CustomMetric>> {
    if config.metrics.is_empty() {
        return Ok(vec![]);
    }
    let logged: std::collections::HashMap<String, f64> = crate::db::load_metrics(conn, date_str)?
        .into_iter()
        .collect();
    let mut out: Vec<CustomMetric> = config
        .metrics
        .iter()
        .map(|(id, cfg)| CustomMetric {
            id: id.clone(),
            display: cfg.display.clone(),
            unit: cfg.unit.clone(),
            value: logged.get(id).copied(),
        })
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}
```

- [ ] **Step 4: Run assemble tests**

```bash
cargo test --lib today_cmd::tests::assemble
```

Expected: 3 assemble tests pass.

- [ ] **Step 5: Wire `execute()` to use everything**

Replace the placeholder `execute` body:

```rust
pub fn execute(date_flag: Option<&str>, json: bool, config: &Config) -> Result<()> {
    let date = match date_flag {
        Some(s) => NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
            .map_err(|_| color_eyre::eyre::eyre!("Invalid date: '{s}'. Expected YYYY-MM-DD."))?,
        None => config.effective_today_date(),
    };

    let db_path = config.db_path();
    if !db_path.exists() {
        color_eyre::eyre::bail!(
            "Database not found at {}. Run `daylog init` or `daylog sync` first.",
            db_path.display()
        );
    }
    let conn = crate::db::open_ro(&db_path)?;
    let mut summary = assemble(date, config, &conn)?;

    let goals = crate::goals::load_goals(&config.notes_dir_path())?;

    // Detect goal keys with no known data source → warnings.
    let known: std::collections::HashSet<&str> = ["kcal", "protein", "carbs", "fat", "weight",
        "sleep_hours", "mood", "energy"]
        .into_iter()
        .collect();
    let custom_ids: std::collections::HashSet<String> =
        config.metrics.keys().cloned().collect();
    for name in goals.thresholds.keys() {
        if !known.contains(name.as_str()) && !custom_ids.contains(name) {
            summary
                .goals_warnings
                .push(format!("unknown metric `{name}` in goals.md"));
        }
    }

    if json {
        let v = render_json(&summary, &goals);
        println!("{}", serde_json::to_string_pretty(&v)?);
    } else {
        let color = std::io::stdout().is_terminal()
            && std::env::var_os("NO_COLOR").is_none();
        print!("{}", render_text(&summary, &goals, color));
    }
    Ok(())
}
```

Add the `IsTerminal` import at the top of the file:

```rust
use std::io::IsTerminal;
```

- [ ] **Step 6: Build + smoke-test against the user's notes**

```bash
cargo build
```

Expected: clean build.

If your notes dir is reachable, manually exercise the command:

```bash
cargo run --quiet -- today | head -20
cargo run --quiet -- today --json | head -40
```

Expected: a formatted summary block / JSON object. (No assertion here — this is a manual sanity check; the integration test in Task 7 covers it programmatically.)

- [ ] **Step 7: Commit**

```bash
git add src/cli/today_cmd.rs
git commit -m "feat: assemble DaySummary from notes + DB and wire execute()"
```

---

## Task 7: Integration test

**Files:**
- Create: `tests/today.rs`

- [ ] **Step 1: Create `tests/today.rs`**

```rust
//! End-to-end test for `daylog today`.

use chrono::NaiveDate;
use daylog::cli::today_cmd::{assemble, render_json, render_text};
use daylog::config::Config;
use daylog::db;
use daylog::goals::load_goals;
use daylog::modules;

fn setup(notes_dir_str: &str) -> (tempfile::TempDir, Config) {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().display().to_string().replace('\\', "/");
    let toml_str = format!(
        r#"
notes_dir = "{path}"
time_format = "24h"
weight_unit = "kg"

[modules]
dashboard = true
training = true
trends = true
climbing = false

[metrics]
resting_hr = {{ display = "Resting HR", color = "red", unit = "bpm" }}

[exercises]
squat = {{ display = "Squat", color = "cyan" }}
"#
    );
    let _ = notes_dir_str;
    let config: Config = toml::from_str(&toml_str).unwrap();
    (dir, config)
}

fn write_note(notes_dir: &std::path::Path, date: &str, body: &str) {
    std::fs::write(notes_dir.join(format!("{date}.md")), body).unwrap();
}

fn write_goals(notes_dir: &std::path::Path, body: &str) {
    std::fs::write(notes_dir.join("goals.md"), body).unwrap();
}

#[test]
fn end_to_end_today_text_and_json() {
    let (dir, config) = setup("");

    // --- Fixture data ---
    write_note(
        dir.path(),
        "2026-04-29",
        "---\ndate: 2026-04-29\nweight: 120.2\n---\n\n## Food\n",
    );
    write_note(
        dir.path(),
        "2026-04-30",
        "---\n\
         date: 2026-04-30\n\
         weight: 121.5\n\
         sleep: \"23:00-05:24\"\n\
         bp_morning_sys: 138\n\
         bp_morning_dia: 88\n\
         bp_morning_pulse: 70\n\
         resting_hr: 58\n\
         ---\n\n\
         ## Food\n\
         - **08:00** Eggs (200 kcal, 12.0g protein, 1.0g carbs, 15.0g fat)\n\
         - **12:00** Pasta (500 kcal, 18.0g protein, 80.0g carbs, 10.0g fat)\n\
         - **18:00** Soup (813 kcal, 117.0g protein, -4.0g carbs, 34.0g fat)\n",
    );
    write_goals(
        dir.path(),
        "---\nkcal_min: 1900\nkcal_max: 2200\nprotein_min: 140\nweight_target: 110\n---\n\n# notes\n",
    );

    // Sync notes to DB.
    let registry = modules::build_registry(&config);
    let conn = db::open_rw(&config.db_path()).unwrap();
    db::init_db(&conn, &registry).unwrap();
    modules::validate_module_tables(&registry).unwrap();
    daylog::materializer::sync_all(&conn, &config.notes_dir_path(), &config, &registry).unwrap();

    // --- Assert assemble + render_text ---
    let date = NaiveDate::from_ymd_opt(2026, 4, 30).unwrap();
    let summary = assemble(date, &config, &conn).unwrap();
    let goals = load_goals(&config.notes_dir_path()).unwrap();

    let text = render_text(&summary, &goals, false);
    assert!(text.contains("2026-04-30 — Daily summary"), "got:\n{text}");
    assert!(text.contains("Calories:"));
    assert!(text.contains("1513"));
    assert!(text.contains("1900–2200 kcal"));
    assert!(text.contains("Protein:"));
    assert!(text.contains("147"));
    assert!(text.contains("≥140 g"));
    assert!(text.contains("Weight:    121.5 kg"));
    assert!(text.contains("Δ +1.3 vs 2026-04-29"));
    assert!(text.contains("Sleep:     6h 24min"));
    assert!(text.contains("BP morning:   138/88 (pulse 70)"));
    assert!(text.contains("Resting HR: 58"));

    // --- Assert render_json ---
    let v = render_json(&summary, &goals);
    assert_eq!(v["date"], "2026-04-30");
    assert_eq!(v["metrics"]["kcal"]["min"], 1900.0);
    assert_eq!(v["metrics"]["kcal"]["max"], 2200.0);
    assert_eq!(v["metrics"]["protein"]["min"], 140.0);
    assert_eq!(v["metrics"]["weight"]["delta_vs_date"], "2026-04-29");
    assert_eq!(v["bp_morning"]["sys"], 138);
    assert_eq!(v["sleep"]["hours"], 6.4);
    assert_eq!(v["goals_present"], true);
}

#[test]
fn end_to_end_today_no_goals_emits_hint() {
    let (dir, config) = setup("");

    write_note(
        dir.path(),
        "2026-04-30",
        "---\ndate: 2026-04-30\n---\n\n## Food\n- **08:00** Eggs (200 kcal, 12.0g protein, 1.0g carbs, 15.0g fat)\n",
    );

    let registry = modules::build_registry(&config);
    let conn = db::open_rw(&config.db_path()).unwrap();
    db::init_db(&conn, &registry).unwrap();
    modules::validate_module_tables(&registry).unwrap();
    daylog::materializer::sync_all(&conn, &config.notes_dir_path(), &config, &registry).unwrap();

    let date = NaiveDate::from_ymd_opt(2026, 4, 30).unwrap();
    let summary = assemble(date, &config, &conn).unwrap();
    let goals = load_goals(&config.notes_dir_path()).unwrap();
    assert!(!goals.present);

    let text = render_text(&summary, &goals, false);
    assert!(text.contains("No goals defined"), "got:\n{text}");
}
```

- [ ] **Step 2: Run integration test**

```bash
cargo test --test today
```

Expected: 2 tests pass.

- [ ] **Step 3: Run the full test suite to catch regressions**

```bash
cargo test
```

Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add tests/today.rs
git commit -m "test: end-to-end integration test for \`daylog today\`"
```

---

## Task 8: README + final polish

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Find the existing CLI command list in `README.md`**

```bash
grep -n "daylog log\|daylog status\|daylog edit\|daylog sync\|daylog rebuild" README.md
```

- [ ] **Step 2: Add a `daylog today` entry to the CLI list**

Insert near the existing `daylog status` and `daylog log` entries, briefly describing the command:

```markdown
- `daylog today [date]` — print a compact daily summary (food totals, weight, sleep, BP morning, custom metrics) with optional goal comparison from `goals.md`. Add `--json` for machine-readable output.
```

If `README.md` has a goals.md section already, add a one-line example of the frontmatter format:

```markdown
Goals are read from `goals.md` in your notes dir. Add a YAML frontmatter block with any `<metric>_min`, `<metric>_max`, or `<metric>_target` keys:

```yaml
---
kcal_min: 1900
kcal_max: 2200
protein_min: 140
weight_target: 110
---
```

Otherwise add this snippet under a new "Goals" subsection.

- [ ] **Step 3: Run lint + format**

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
```

Expected: no formatting changes, no clippy warnings.

- [ ] **Step 4: Final test sweep**

```bash
cargo test
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add README.md
git commit -m "docs: document \`daylog today\` in README"
```

- [ ] **Step 6: Push and update the PR**

```bash
git push
```

Expected: branch `feat/daylog-today` updates the existing PR with all implementation commits.

---

## Self-Review Checklist (already applied)

- [x] **Spec coverage:** Every spec section has a corresponding task — food_sum (Task 1), goals (Task 2), CLI skeleton (Task 3), DaySummary + render_text (Task 4), render_json (Task 5), assemble + execute (Task 6), integration test (Task 7), docs (Task 8).
- [x] **Placeholder scan:** All steps contain executable code or specific commands. No "TBD" / "implement later" / "handle errors" placeholders.
- [x] **Type consistency:** `FoodTotals` field is `skipped_lines` everywhere; `DaySummary.food_skipped` mirrors it. `Threshold` fields are `target/min/max`. `BpReading` fields are `sys/dia/pulse`. `CustomMetric` fields are `id/display/value/unit`.
- [x] **Spec edge cases covered:**
  - Missing daily note → food zeros, day fields None, BP None (Task 6 test `assemble_missing_note_yields_zero_food`).
  - Missing goals.md → hint line (Task 4 test `render_text_no_goals_emits_hint` + Task 7 test `end_to_end_today_no_goals_emits_hint`).
  - Non-numeric value in goals → hard error (Task 2 test `non_numeric_value_errors`).
  - Skipped food lines → hint (Task 4 test `render_text_skipped_food_lines_emits_hint`).
  - Unknown metric warning (Task 4 test `render_text_unknown_metric_warning` + Task 6 execute logic).
  - Weight delta non-yesterday → actual date (Task 4 test `render_text_weight_delta_non_yesterday_uses_actual_date`).
  - NO_COLOR / TTY detection (Task 4 test `render_text_color_off_strips_escapes` + Task 6 execute logic; the env-var branch itself is not unit-tested but is a one-line check).
  - Round-trip with `format_line` (Task 1 test `round_trip_with_format_line`).
