# Sleep Commands Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `daylog sleep-start` and `daylog sleep-end` commands that handle past-midnight bedtime date math, plus a configurable `time_format` and DB normalization to a canonical 24h representation.

**Architecture:** A new `time` module owns all parsing/formatting (replacing inline logic in `materializer`). A new `state` module manages a `.daylog-state.toml` sidecar holding the pending bedtime. A new `sleep_cmd` module implements the two commands. Existing call sites (`materializer`, `log_cmd`, `dashboard`) migrate to the `time` module so input parsing stays bilingual (12h + 24h) while DB and TUI display use canonical 24h internally and `config.time_format` controls rendering.

**Tech Stack:** Rust 2021, chrono 0.4 (with serde feature), serde + toml for state, clap for CLI, color-eyre for errors. `parse_sleep`/`parse_time_to_minutes` in `materializer.rs` will be deleted and replaced by the `time` module.

**Spec:** `docs/superpowers/specs/2026-04-26-sleep-commands-design.md`

---

## File Structure

**New files:**
- `src/time.rs` — time parsing, formatting, sleep-range helpers (~200 LOC)
- `src/state.rs` — `.daylog-state.toml` read/write (~80 LOC)
- `src/cli/sleep_cmd.rs` — `cmd_sleep_start`, `cmd_sleep_end` (~150 LOC)

**Modified files:**
- `src/config.rs` — add `TimeFormat` enum + `time_format` field
- `src/lib.rs` — add `pub mod time;` and `pub mod state;`
- `src/cli/mod.rs` — add `SleepStart` and `SleepEnd` subcommands + `pub mod sleep_cmd;`
- `src/main.rs` — dispatch new commands
- `src/materializer.rs` — replace `parse_sleep`/`parse_time_to_minutes` with `time::*`; store canonical 24h in DB
- `src/cli/log_cmd.rs` — validate `sleep` via `time::parse_sleep_range`, format on write per `time_format`
- `src/modules/dashboard.rs` — format DB sleep times per `time_format`
- `presets/default.toml` — document `time_format`

---

## Task 1: Add `TimeFormat` config option

**Files:**
- Modify: `src/config.rs`

- [ ] **Step 1: Write failing tests**

Add at the bottom of `mod tests` in `src/config.rs`:

```rust
    #[test]
    fn test_time_format_defaults_to_12h() {
        let config: Config = toml::from_str("notes_dir = '/tmp/test'\n").unwrap();
        assert_eq!(config.time_format, TimeFormat::TwelveHour);
    }

    #[test]
    fn test_time_format_24h() {
        let config: Config =
            toml::from_str("notes_dir = '/tmp/test'\ntime_format = '24h'\n").unwrap();
        assert_eq!(config.time_format, TimeFormat::TwentyFourHour);
    }

    #[test]
    fn test_time_format_12h_explicit() {
        let config: Config =
            toml::from_str("notes_dir = '/tmp/test'\ntime_format = '12h'\n").unwrap();
        assert_eq!(config.time_format, TimeFormat::TwelveHour);
    }

    #[test]
    fn test_time_format_invalid() {
        let result: std::result::Result<Config, _> =
            toml::from_str("notes_dir = '/tmp/test'\ntime_format = 'military'\n");
        assert!(result.is_err());
    }

    #[test]
    fn test_time_format_display() {
        assert_eq!(TimeFormat::TwelveHour.to_string(), "12h");
        assert_eq!(TimeFormat::TwentyFourHour.to_string(), "24h");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib config::tests::test_time_format`
Expected: compilation error — `TimeFormat` not defined.

- [ ] **Step 3: Add `TimeFormat` enum**

Add after the existing `WeightUnit` block in `src/config.rs` (around line 24):

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub enum TimeFormat {
    #[default]
    #[serde(rename = "12h")]
    TwelveHour,
    #[serde(rename = "24h")]
    TwentyFourHour,
}

impl fmt::Display for TimeFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimeFormat::TwelveHour => write!(f, "12h"),
            TimeFormat::TwentyFourHour => write!(f, "24h"),
        }
    }
}
```

- [ ] **Step 4: Add field to `Config` struct**

In `Config` (around line 39, after `weight_unit`):

```rust
    #[serde(default)]
    pub time_format: TimeFormat,
```

- [ ] **Step 5: Add focused suggestion in `Config::load`**

In `Config::load` map_err (around line 109), extend the suggestion logic:

```rust
        let config: Config = toml::from_str(&contents).map_err(|e| {
            let err = color_eyre::eyre::eyre!("Failed to parse config at {}: {e}", path.display());
            if e.message().contains("weight_unit") {
                err.suggestion("weight_unit must be \"kg\" or \"lbs\" (default: \"lbs\").")
            } else if e.message().contains("time_format") {
                err.suggestion("time_format must be \"12h\" or \"24h\" (default: \"12h\").")
            } else {
                err
            }
        })?;
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --lib config::`
Expected: all pass, including new `test_time_format_*` tests.

- [ ] **Step 7: Commit**

```bash
git add src/config.rs
git commit -m "feat: add time_format config option (12h/24h, default 12h)"
```

---

## Task 2: Create `time` module — parse_time

**Files:**
- Create: `src/time.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Add module declaration**

Edit `src/lib.rs`, add:

```rust
pub mod time;
```

- [ ] **Step 2: Write failing tests for `parse_time`**

Create `src/time.rs`:

```rust
use chrono::NaiveTime;

use crate::config::TimeFormat;

/// Parse a time string in either 12-hour (`10:30pm`, `6am`) or
/// 24-hour (`22:30`, `0:28`, `06:52`) format. Case-insensitive.
/// Whitespace between digits and am/pm is tolerated.
pub fn parse_time(s: &str) -> Option<NaiveTime> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_24h_basic() {
        assert_eq!(parse_time("22:30"), NaiveTime::from_hms_opt(22, 30, 0));
        assert_eq!(parse_time("06:52"), NaiveTime::from_hms_opt(6, 52, 0));
        assert_eq!(parse_time("0:28"), NaiveTime::from_hms_opt(0, 28, 0));
        assert_eq!(parse_time("00:28"), NaiveTime::from_hms_opt(0, 28, 0));
        assert_eq!(parse_time("23:59"), NaiveTime::from_hms_opt(23, 59, 0));
    }

    #[test]
    fn parse_12h_basic() {
        assert_eq!(parse_time("10:30pm"), NaiveTime::from_hms_opt(22, 30, 0));
        assert_eq!(parse_time("6:15am"), NaiveTime::from_hms_opt(6, 15, 0));
        assert_eq!(parse_time("11pm"), NaiveTime::from_hms_opt(23, 0, 0));
        assert_eq!(parse_time("6am"), NaiveTime::from_hms_opt(6, 0, 0));
    }

    #[test]
    fn parse_12h_case_insensitive_and_spaces() {
        assert_eq!(parse_time("10:30PM"), NaiveTime::from_hms_opt(22, 30, 0));
        assert_eq!(parse_time("10:30 pm"), NaiveTime::from_hms_opt(22, 30, 0));
        assert_eq!(parse_time("10:30 PM"), NaiveTime::from_hms_opt(22, 30, 0));
    }

    #[test]
    fn parse_12h_midnight_and_noon() {
        assert_eq!(parse_time("12:00am"), NaiveTime::from_hms_opt(0, 0, 0));
        assert_eq!(parse_time("12:30am"), NaiveTime::from_hms_opt(0, 30, 0));
        assert_eq!(parse_time("12:00pm"), NaiveTime::from_hms_opt(12, 0, 0));
        assert_eq!(parse_time("12:30pm"), NaiveTime::from_hms_opt(12, 30, 0));
    }

    #[test]
    fn parse_invalid_returns_none() {
        assert!(parse_time("").is_none());
        assert!(parse_time("abc").is_none());
        assert!(parse_time("25:00").is_none());
        assert!(parse_time("12:60").is_none());
        assert!(parse_time("13pm").is_none()); // 13 is not a 12h hour
        assert!(parse_time("24:00").is_none());
        assert!(parse_time("-1:00").is_none());
    }

    #[test]
    fn parse_strips_quotes() {
        assert_eq!(parse_time("\"22:30\""), NaiveTime::from_hms_opt(22, 30, 0));
        assert_eq!(parse_time("'10:30pm'"), NaiveTime::from_hms_opt(22, 30, 0));
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --lib time::`
Expected: compilation succeeds, tests panic with `not yet implemented`.

- [ ] **Step 4: Implement `parse_time`**

Replace the `todo!()` with:

```rust
pub fn parse_time(s: &str) -> Option<NaiveTime> {
    let s = s.trim().trim_matches('"').trim_matches('\'').trim();
    if s.is_empty() {
        return None;
    }
    let lower = s.to_lowercase();

    // Detect am/pm suffix
    let (clock_part, suffix) = if let Some(rest) = lower.strip_suffix("am") {
        (rest.trim_end(), Some(false))
    } else if let Some(rest) = lower.strip_suffix("pm") {
        (rest.trim_end(), Some(true))
    } else {
        (lower.as_str(), None)
    };

    if clock_part.is_empty() {
        return None;
    }

    // Parse "H" or "H:M"
    let (hour_str, minute_str) = match clock_part.split_once(':') {
        Some((h, m)) => (h, m),
        None => (clock_part, "0"),
    };
    let hour: u32 = hour_str.trim().parse().ok()?;
    let minute: u32 = minute_str.trim().parse().ok()?;

    let hour_24 = match suffix {
        Some(is_pm) => {
            // 12h: hour must be 1..=12
            if !(1..=12).contains(&hour) {
                return None;
            }
            match (is_pm, hour) {
                (false, 12) => 0,        // 12am = 00:00
                (false, h) => h,         // 1am..11am
                (true, 12) => 12,        // 12pm = 12:00
                (true, h) => h + 12,     // 1pm..11pm
            }
        }
        None => {
            if hour > 23 {
                return None;
            }
            hour
        }
    };

    NaiveTime::from_hms_opt(hour_24, minute, 0)
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib time::`
Expected: all `parse_time` tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/lib.rs src/time.rs
git commit -m "feat: add time module with parse_time (12h + 24h)"
```

---

## Task 3: `time` module — format_time

**Files:**
- Modify: `src/time.rs`

- [ ] **Step 1: Write failing tests**

Add to `mod tests` in `src/time.rs`:

```rust
    #[test]
    fn format_24h_zero_padded() {
        let t = NaiveTime::from_hms_opt(22, 30, 0).unwrap();
        assert_eq!(format_time(t, TimeFormat::TwentyFourHour), "22:30");
        let t = NaiveTime::from_hms_opt(6, 5, 0).unwrap();
        assert_eq!(format_time(t, TimeFormat::TwentyFourHour), "06:05");
        let t = NaiveTime::from_hms_opt(0, 0, 0).unwrap();
        assert_eq!(format_time(t, TimeFormat::TwentyFourHour), "00:00");
    }

    #[test]
    fn format_12h_lowercase_no_hour_pad() {
        let t = NaiveTime::from_hms_opt(22, 30, 0).unwrap();
        assert_eq!(format_time(t, TimeFormat::TwelveHour), "10:30pm");
        let t = NaiveTime::from_hms_opt(6, 15, 0).unwrap();
        assert_eq!(format_time(t, TimeFormat::TwelveHour), "6:15am");
        let t = NaiveTime::from_hms_opt(0, 30, 0).unwrap();
        assert_eq!(format_time(t, TimeFormat::TwelveHour), "12:30am");
        let t = NaiveTime::from_hms_opt(12, 0, 0).unwrap();
        assert_eq!(format_time(t, TimeFormat::TwelveHour), "12:00pm");
    }

    #[test]
    fn format_then_parse_roundtrip() {
        for hour in 0..24 {
            for min in [0, 1, 30, 59] {
                let t = NaiveTime::from_hms_opt(hour, min, 0).unwrap();
                for fmt in [TimeFormat::TwelveHour, TimeFormat::TwentyFourHour] {
                    let s = format_time(t, fmt);
                    assert_eq!(parse_time(&s), Some(t), "roundtrip failed for {s}");
                }
            }
        }
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib time::tests::format`
Expected: compile errors — `format_time` not defined.

- [ ] **Step 3: Implement `format_time`**

Add to `src/time.rs`:

```rust
/// Format a `NaiveTime` per the given config format.
///
/// 24h: zero-padded `HH:MM` (`"06:05"`, `"22:30"`).
/// 12h: lowercase suffix, no zero-padding on hour (`"6:05am"`, `"10:30pm"`,
///      `"12:30am"` for midnight, `"12:00pm"` for noon). Always includes
///      minutes for clarity in stored entries.
pub fn format_time(t: NaiveTime, fmt: TimeFormat) -> String {
    use chrono::Timelike;
    let h = t.hour();
    let m = t.minute();
    match fmt {
        TimeFormat::TwentyFourHour => format!("{h:02}:{m:02}"),
        TimeFormat::TwelveHour => {
            let (display_h, suffix) = match h {
                0 => (12, "am"),
                1..=11 => (h, "am"),
                12 => (12, "pm"),
                _ => (h - 12, "pm"),
            };
            format!("{display_h}:{m:02}{suffix}")
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib time::`
Expected: all tests pass (parse + format + roundtrip).

- [ ] **Step 5: Commit**

```bash
git add src/time.rs
git commit -m "feat: add format_time helper to time module"
```

---

## Task 4: `time` module — parse_sleep_range, format_sleep_range, sleep_hours

**Files:**
- Modify: `src/time.rs`

- [ ] **Step 1: Write failing tests**

Add to `mod tests` in `src/time.rs`:

```rust
    #[test]
    fn parse_sleep_range_12h() {
        let (s, e) = parse_sleep_range("10:30pm-6:15am").unwrap();
        assert_eq!(s, NaiveTime::from_hms_opt(22, 30, 0).unwrap());
        assert_eq!(e, NaiveTime::from_hms_opt(6, 15, 0).unwrap());
    }

    #[test]
    fn parse_sleep_range_24h() {
        let (s, e) = parse_sleep_range("22:30-06:15").unwrap();
        assert_eq!(s, NaiveTime::from_hms_opt(22, 30, 0).unwrap());
        assert_eq!(e, NaiveTime::from_hms_opt(6, 15, 0).unwrap());
    }

    #[test]
    fn parse_sleep_range_quoted_and_spaces() {
        let (s, e) = parse_sleep_range("\"10:30pm - 6:15am\"").unwrap();
        assert_eq!(s, NaiveTime::from_hms_opt(22, 30, 0).unwrap());
        assert_eq!(e, NaiveTime::from_hms_opt(6, 15, 0).unwrap());
    }

    #[test]
    fn parse_sleep_range_no_dash() {
        assert!(parse_sleep_range("22:30").is_none());
    }

    #[test]
    fn parse_sleep_range_garbage() {
        assert!(parse_sleep_range("foo-bar").is_none());
        assert!(parse_sleep_range("").is_none());
    }

    #[test]
    fn format_sleep_range_uses_format() {
        let s = NaiveTime::from_hms_opt(22, 30, 0).unwrap();
        let e = NaiveTime::from_hms_opt(6, 15, 0).unwrap();
        assert_eq!(
            format_sleep_range(s, e, TimeFormat::TwelveHour),
            "10:30pm-6:15am"
        );
        assert_eq!(
            format_sleep_range(s, e, TimeFormat::TwentyFourHour),
            "22:30-06:15"
        );
    }

    #[test]
    fn sleep_hours_overnight() {
        let s = NaiveTime::from_hms_opt(22, 30, 0).unwrap();
        let e = NaiveTime::from_hms_opt(6, 15, 0).unwrap();
        assert!((sleep_hours(s, e) - 7.75).abs() < 0.01);
    }

    #[test]
    fn sleep_hours_same_day() {
        let s = NaiveTime::from_hms_opt(0, 28, 0).unwrap();
        let e = NaiveTime::from_hms_opt(6, 52, 0).unwrap();
        assert!((sleep_hours(s, e) - 6.4).abs() < 0.01);
    }

    #[test]
    fn sleep_hours_equal_returns_zero() {
        let s = NaiveTime::from_hms_opt(7, 0, 0).unwrap();
        assert_eq!(sleep_hours(s, s), 0.0);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib time::`
Expected: compile errors — functions not defined.

- [ ] **Step 3: Implement the three helpers**

Add to `src/time.rs`:

```rust
/// Parse a `start-end` sleep range. Surrounding quotes and whitespace
/// around the dash are tolerated.
pub fn parse_sleep_range(s: &str) -> Option<(NaiveTime, NaiveTime)> {
    let s = s.trim().trim_matches('"').trim_matches('\'');
    let (start, end) = s.split_once('-')?;
    let start = parse_time(start)?;
    let end = parse_time(end)?;
    Some((start, end))
}

/// Format a sleep range as `"start-end"` per the given config format.
pub fn format_sleep_range(start: NaiveTime, end: NaiveTime, fmt: TimeFormat) -> String {
    format!("{}-{}", format_time(start, fmt), format_time(end, fmt))
}

/// Compute hours between two times. If end <= start, treats it as
/// crossing midnight (adds 24h). Equal times return 0.0.
/// Result is rounded to 2 decimal places.
pub fn sleep_hours(start: NaiveTime, end: NaiveTime) -> f64 {
    use chrono::Timelike;
    let start_min = (start.hour() * 60 + start.minute()) as i32;
    let end_min = (end.hour() * 60 + end.minute()) as i32;
    let duration = if end_min == start_min {
        0
    } else if end_min < start_min {
        (1440 - start_min) + end_min
    } else {
        end_min - start_min
    };
    let hours = duration as f64 / 60.0;
    (hours * 100.0).round() / 100.0
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib time::`
Expected: all tests pass.

- [ ] **Step 5: Run full test suite to ensure nothing else broke**

Run: `cargo test --lib`
Expected: all pre-existing tests still pass. (Materializer's `parse_sleep` tests are independent — they still use the old function.)

- [ ] **Step 6: Commit**

```bash
git add src/time.rs
git commit -m "feat: add sleep range parsing/formatting and hours math"
```

---

## Task 5: Migrate materializer to use `time` module (DB normalization)

**Files:**
- Modify: `src/materializer.rs`

This task removes `parse_sleep` and `parse_time_to_minutes` from `materializer.rs` and routes through `crate::time`. The DB column contents change from raw input strings to canonical 24h `"HH:MM"`.

- [ ] **Step 1: Write failing test for canonicalization**

In `src/materializer.rs` `mod tests`, add:

```rust
    #[test]
    fn materialize_normalizes_12h_sleep_to_24h() {
        let dir = tempfile::TempDir::new().unwrap();
        let notes_dir = dir.path();
        let db_path = notes_dir.join(".daylog.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE days (
                 date TEXT PRIMARY KEY, sleep_start TEXT, sleep_end TEXT,
                 sleep_hours REAL, sleep_quality INTEGER, mood INTEGER,
                 energy INTEGER, weight REAL, notes TEXT,
                 file_mtime REAL, parsed_at TEXT);
             CREATE TABLE metrics (
                 date TEXT, name TEXT, value REAL,
                 PRIMARY KEY (date, name));",
        )
        .unwrap();

        let file = notes_dir.join("2026-04-26.md");
        std::fs::write(
            &file,
            "---\ndate: 2026-04-26\nsleep: \"10:30pm-6:15am\"\n---\n",
        )
        .unwrap();

        let cfg: Config = toml::from_str(&format!(
            "notes_dir = '{}'\n",
            notes_dir.display().to_string().replace('\\', "/")
        ))
        .unwrap();
        materialize_file(&conn, &file, &cfg, &[]).unwrap();

        let (start, end, hours): (String, String, f64) = conn
            .query_row(
                "SELECT sleep_start, sleep_end, sleep_hours FROM days WHERE date = ?1",
                ["2026-04-26"],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(start, "22:30");
        assert_eq!(end, "06:15");
        assert!((hours - 7.75).abs() < 0.01);
    }

    #[test]
    fn materialize_normalizes_24h_sleep_to_24h() {
        let dir = tempfile::TempDir::new().unwrap();
        let notes_dir = dir.path();
        let db_path = notes_dir.join(".daylog.db");
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE days (
                 date TEXT PRIMARY KEY, sleep_start TEXT, sleep_end TEXT,
                 sleep_hours REAL, sleep_quality INTEGER, mood INTEGER,
                 energy INTEGER, weight REAL, notes TEXT,
                 file_mtime REAL, parsed_at TEXT);
             CREATE TABLE metrics (
                 date TEXT, name TEXT, value REAL,
                 PRIMARY KEY (date, name));",
        )
        .unwrap();

        let file = notes_dir.join("2026-04-26.md");
        std::fs::write(
            &file,
            "---\ndate: 2026-04-26\nsleep: \"22:30-06:15\"\n---\n",
        )
        .unwrap();

        let cfg: Config = toml::from_str(&format!(
            "notes_dir = '{}'\n",
            notes_dir.display().to_string().replace('\\', "/")
        ))
        .unwrap();
        materialize_file(&conn, &file, &cfg, &[]).unwrap();

        let (start, end): (String, String) = conn
            .query_row(
                "SELECT sleep_start, sleep_end FROM days WHERE date = ?1",
                ["2026-04-26"],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(start, "22:30");
        assert_eq!(end, "06:15");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib materializer::tests::materialize_normalizes`
Expected: existing implementation stores raw `"10:30pm"`/`"6:15am"`, so `assert_eq!(start, "22:30")` fails.

- [ ] **Step 3: Replace `parse_sleep` call site**

In `src/materializer.rs`, find `let sleep_data = yaml_str_field(yaml, "sleep").and_then(|s| parse_sleep(&s));` (around line 240) and replace with:

```rust
    let sleep_data = yaml_str_field(yaml, "sleep")
        .as_deref()
        .and_then(crate::time::parse_sleep_range)
        .map(|(start, end)| {
            let hours = crate::time::sleep_hours(start, end);
            (
                crate::time::format_time(start, crate::config::TimeFormat::TwentyFourHour),
                crate::time::format_time(end, crate::config::TimeFormat::TwentyFourHour),
                hours,
            )
        });
```

(`parse_sleep_range` takes `&str`, so the `.as_deref()` adapts the `Option<String>`.)

- [ ] **Step 4: Delete the old `parse_sleep` and `parse_time_to_minutes`**

Remove lines 124–199 in `src/materializer.rs` (the `// --- Sleep Time Parsing ---` block through the end of `parse_time_to_minutes`).

- [ ] **Step 5: Delete or migrate the old `parse_sleep` tests**

In `mod tests` of `src/materializer.rs`, find tests calling `parse_sleep(...)` directly (around line 671 onwards in the original file — `parse_sleep("10:30pm-6:15am")`, `"\"10:55pm-6:40am\""`, `"11pm-7am"`, `"22:30-6:15"`, `"11:45pm-5:45am"`). Delete them — equivalent coverage now lives in `src/time.rs::tests`.

- [ ] **Step 6: Update existing materializer roundtrip test expectations**

Find the existing test (around line 786) that asserts `today["sleep_start"] == "10:30pm"` and update to `"22:30"`, and `today["sleep_end"]` from `"6:15am"` to `"06:15"`.

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test --lib materializer::`
Expected: all tests pass, including new normalization tests and updated roundtrip.

Run: `cargo test --lib`
Expected: full library test suite passes.

- [ ] **Step 8: Commit**

```bash
git add src/materializer.rs
git commit -m "feat: normalize sleep times to canonical 24h in DB"
```

---

## Task 6: Update `log sleep` to use `time` module

**Files:**
- Modify: `src/cli/log_cmd.rs`

- [ ] **Step 1: Write failing tests**

Add to `mod tests` in `src/cli/log_cmd.rs`:

```rust
    #[test]
    fn route_sleep_normalizes_12h_input_with_24h_config() {
        let mut cfg = default_config();
        cfg.time_format = crate::config::TimeFormat::TwentyFourHour;
        let value = vec!["10:30pm-6:15am".to_string()];
        let result =
            route_field("sleep", &value, "10:30pm-6:15am", SAMPLE, &cfg, &empty_modules())
                .unwrap();
        assert!(
            result.contains("sleep: \"22:30-06:15\""),
            "expected 24h normalized, got: {result}"
        );
    }

    #[test]
    fn route_sleep_keeps_12h_with_12h_config() {
        let cfg = default_config(); // default is 12h
        let value = vec!["22:30-06:15".to_string()];
        let result =
            route_field("sleep", &value, "22:30-06:15", SAMPLE, &cfg, &empty_modules()).unwrap();
        assert!(
            result.contains("sleep: \"10:30pm-6:15am\""),
            "expected 12h normalized, got: {result}"
        );
    }

    #[test]
    fn route_sleep_rejects_unparseable() {
        let cfg = default_config();
        let result = route_field(
            "sleep",
            &["banana-foo".into()],
            "banana-foo",
            SAMPLE,
            &cfg,
            &empty_modules(),
        );
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid sleep"));
    }
```

The existing `test_route_sleep` and `test_reject_sleep_no_dash` and `test_accept_valid_inputs` (`"10pm-6am"` for sleep) will need updating because their expectations change. Update them:

Replace `test_route_sleep`:

```rust
    #[test]
    fn test_route_sleep() {
        let cfg = default_config();
        let value = vec!["11pm-7am".to_string()];
        let result =
            route_field("sleep", &value, "11pm-7am", SAMPLE, &cfg, &empty_modules()).unwrap();
        // 12h config preserves the 12h form (canonicalized to "11:00pm-7:00am")
        assert!(
            result.contains("sleep: \"11:00pm-7:00am\""),
            "got: {result}"
        );
    }
```

Replace `test_reject_sleep_no_dash`:

```rust
    #[test]
    fn test_reject_sleep_no_dash() {
        let cfg = default_config();
        let result = route_field(
            "sleep",
            &["10pm".into()],
            "10pm",
            SAMPLE,
            &cfg,
            &empty_modules(),
        );
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid sleep"));
    }
```

In `test_accept_valid_inputs`, the line `route_field("sleep", &["10pm-6am".into()], "10pm-6am", ...)` continues to succeed — leave it.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib cli::log_cmd::`
Expected: new tests fail (sleep value still passed verbatim, not normalized).

- [ ] **Step 3: Update `validate_core_field` for `sleep`**

In `src/cli/log_cmd.rs`, replace the `"sleep"` arm of `validate_core_field` (around line 63):

```rust
        "sleep" => {
            if crate::time::parse_sleep_range(value).is_none() {
                bail!("Invalid sleep: '{value}'. Expected start-end (e.g., 10:30pm-6:15am or 22:30-06:15)");
            }
        }
```

- [ ] **Step 4: Update `route_field` `sleep` arm to format per config**

Replace the `"sleep" => {...}` block in `route_field` (around line 89):

```rust
        "sleep" => {
            validate_core_field("sleep", joined, config)?;
            let (start, end) = crate::time::parse_sleep_range(joined)
                .expect("validated above");
            let formatted = crate::time::format_sleep_range(start, end, config.time_format);
            return Ok(frontmatter::set_scalar(
                content,
                "sleep",
                &format!("\"{}\"", formatted),
            ));
        }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib cli::log_cmd::`
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add src/cli/log_cmd.rs
git commit -m "feat: log sleep validates and normalizes per time_format"
```

---

## Task 7: Dashboard formats sleep per `time_format`

**Files:**
- Modify: `src/modules/dashboard.rs`

- [ ] **Step 1: Write failing test**

Find `mod tests` in `src/modules/dashboard.rs` if it exists; otherwise add one. Add a unit test on a small helper. We'll factor the sleep-line formatting into a private helper `fn sleep_line(start: Option<&str>, end: Option<&str>, hours: Option<f64>, quality: Option<i32>, fmt: TimeFormat) -> String` for testability.

If `mod tests` does not exist, scaffold:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sleep_line_12h_from_canonical_db() {
        let s = format_sleep_line(
            Some("22:30"),
            Some("06:15"),
            Some(7.75),
            None,
            crate::config::TimeFormat::TwelveHour,
        );
        assert!(s.contains("10:30pm-6:15am"), "got: {s}");
        assert!(s.contains("(7.8h)"), "got: {s}");
    }

    #[test]
    fn sleep_line_24h_from_canonical_db() {
        let s = format_sleep_line(
            Some("22:30"),
            Some("06:15"),
            Some(7.75),
            None,
            crate::config::TimeFormat::TwentyFourHour,
        );
        assert!(s.contains("22:30-06:15"), "got: {s}");
    }

    #[test]
    fn sleep_line_with_quality() {
        let s = format_sleep_line(
            Some("22:30"),
            Some("06:15"),
            Some(7.75),
            Some(4),
            crate::config::TimeFormat::TwelveHour,
        );
        assert!(s.contains("quality: 4/5"), "got: {s}");
    }

    #[test]
    fn sleep_line_missing_returns_dashes() {
        let s = format_sleep_line(None, None, None, None, crate::config::TimeFormat::TwelveHour);
        assert!(s.contains("--"), "got: {s}");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib modules::dashboard::tests`
Expected: compile error — `format_sleep_line` not defined.

- [ ] **Step 3: Add the helper**

Near the top of `src/modules/dashboard.rs`, after the `rating_color` helper:

```rust
/// Format the sleep-line text for the dashboard.
/// Reads canonical 24h `HH:MM` from the DB and renders per `fmt`.
pub(crate) fn format_sleep_line(
    sleep_start: Option<&str>,
    sleep_end: Option<&str>,
    sleep_hours: Option<f64>,
    sleep_quality: Option<i32>,
    fmt: crate::config::TimeFormat,
) -> String {
    match (sleep_start, sleep_end, sleep_hours) {
        (Some(start), Some(end), Some(hours)) => {
            let start_t = crate::time::parse_time(start);
            let end_t = crate::time::parse_time(end);
            let range = match (start_t, end_t) {
                (Some(s), Some(e)) => crate::time::format_sleep_range(s, e, fmt),
                _ => format!("{start}-{end}"),
            };
            let quality_str = sleep_quality
                .map(|q| format!("  quality: {q}/5"))
                .unwrap_or_default();
            format!("{range}  ({hours:.1}h){quality_str}")
        }
        _ => "--".to_string(),
    }
}
```

- [ ] **Step 4: Wire the helper into `draw`**

In `draw`, replace the inline `match (&sleep_start, &sleep_end, sleep_hours) { ... }` block (around line 99) with:

```rust
                let sleep_text = format_sleep_line(
                    sleep_start.as_deref(),
                    sleep_end.as_deref(),
                    sleep_hours,
                    sleep_quality,
                    config.time_format,
                );
                let sleep_line = if sleep_text == "--" {
                    vec![
                        Span::styled("Sleep: ", Style::default().fg(Color::Blue)),
                        Span::styled("--", Style::default().fg(Color::DarkGray)),
                    ]
                } else {
                    vec![
                        Span::styled("Sleep: ", Style::default().fg(Color::Blue)),
                        Span::raw(sleep_text),
                    ]
                };
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib modules::dashboard::`
Expected: all tests pass.

Run: `cargo build`
Expected: clean build.

- [ ] **Step 6: Commit**

```bash
git add src/modules/dashboard.rs
git commit -m "feat: dashboard formats sleep per time_format config"
```

---

## Task 8: Create `state` module

**Files:**
- Create: `src/state.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Add module declaration**

Edit `src/lib.rs`:

```rust
pub mod state;
```

- [ ] **Step 2: Write failing tests**

Create `src/state.rs`:

```rust
use chrono::{DateTime, Local, NaiveTime};
use color_eyre::eyre::{Result, WrapErr};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const STATE_FILENAME: &str = ".daylog-state.toml";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PendingState {
    #[serde(default)]
    pub sleep_start: Option<PendingSleepStart>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PendingSleepStart {
    pub bedtime: NaiveTime,
    pub recorded_at: DateTime<Local>,
}

pub fn state_path(notes_dir: &Path) -> PathBuf {
    notes_dir.join(STATE_FILENAME)
}

/// Load pending state from `{notes_dir}/.daylog-state.toml`.
/// Returns empty state if the file is missing OR cannot be parsed
/// (warns on stderr in the latter case). Sleep state is recoverable —
/// failing here would block the user from logging.
pub fn load(notes_dir: &Path) -> PendingState {
    todo!()
}

/// Save pending state atomically.
pub fn save(notes_dir: &Path, state: &PendingState) -> Result<()> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_state() -> PendingState {
        PendingState {
            sleep_start: Some(PendingSleepStart {
                bedtime: NaiveTime::from_hms_opt(22, 30, 0).unwrap(),
                recorded_at: Local::now(),
            }),
        }
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = load(dir.path());
        assert_eq!(s, PendingState::default());
    }

    #[test]
    fn save_then_load_roundtrip() {
        let dir = tempfile::TempDir::new().unwrap();
        let s = sample_state();
        save(dir.path(), &s).unwrap();
        let loaded = load(dir.path());
        assert_eq!(loaded.sleep_start.as_ref().unwrap().bedtime, s.sleep_start.as_ref().unwrap().bedtime);
    }

    #[test]
    fn load_corrupt_file_returns_empty() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(state_path(dir.path()), "this is not toml{{{").unwrap();
        let s = load(dir.path());
        assert_eq!(s, PendingState::default());
    }

    #[test]
    fn save_clears_sleep_start_when_none() {
        let dir = tempfile::TempDir::new().unwrap();
        save(dir.path(), &sample_state()).unwrap();
        save(dir.path(), &PendingState::default()).unwrap();
        let loaded = load(dir.path());
        assert!(loaded.sleep_start.is_none());
    }

    #[test]
    fn save_does_not_leave_temp_file() {
        let dir = tempfile::TempDir::new().unwrap();
        save(dir.path(), &sample_state()).unwrap();
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().into_string().unwrap())
            .collect();
        for name in &entries {
            assert!(
                !name.contains("tmp"),
                "leftover temp file: {name} (entries: {entries:?})"
            );
        }
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --lib state::`
Expected: panic at `todo!()`.

- [ ] **Step 4: Implement `load` and `save`**

Replace the `todo!()` bodies:

```rust
pub fn load(notes_dir: &Path) -> PendingState {
    let path = state_path(notes_dir);
    let contents = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return PendingState::default(),
        Err(e) => {
            eprintln!("Warning: could not read {}: {e}", path.display());
            return PendingState::default();
        }
    };
    match toml::from_str(&contents) {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "Warning: {} is malformed ({e}), treating as empty.",
                path.display()
            );
            PendingState::default()
        }
    }
}

pub fn save(notes_dir: &Path, state: &PendingState) -> Result<()> {
    let path = state_path(notes_dir);
    let contents =
        toml::to_string(state).wrap_err("Failed to serialize pending state to TOML")?;
    let dir = path
        .parent()
        .ok_or_else(|| color_eyre::eyre::eyre!("Invalid state path: {}", path.display()))?;
    let temp = dir.join(format!(".daylog-state.tmp-{}", std::process::id()));
    std::fs::write(&temp, contents)
        .wrap_err_with(|| format!("Failed to write {}", temp.display()))?;
    std::fs::rename(&temp, &path)
        .wrap_err_with(|| format!("Failed to rename to {}", path.display()))?;
    Ok(())
}
```

- [ ] **Step 5: Add `toml` reverse dep check**

`toml` is already in `Cargo.toml` for config parsing — no change needed, but verify by running `cargo build`.

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --lib state::`
Expected: all five tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/lib.rs src/state.rs
git commit -m "feat: add state module for pending sleep-start sidecar"
```

---

## Task 9: `sleep_cmd` — `cmd_sleep_start`

**Files:**
- Create: `src/cli/sleep_cmd.rs`
- Modify: `src/cli/mod.rs`

- [ ] **Step 1: Add module declaration**

Edit `src/cli/mod.rs`, add:

```rust
pub mod sleep_cmd;
```

- [ ] **Step 2: Write failing tests**

Create `src/cli/sleep_cmd.rs`:

```rust
use chrono::{Local, NaiveTime};
use color_eyre::eyre::{bail, Result};

use crate::config::Config;
use crate::state::{self, PendingSleepStart, PendingState};
use crate::time;

/// Records bedtime as pending state for later finalization by `sleep-end`.
pub fn cmd_sleep_start(time_arg: Option<&str>, config: &Config) -> Result<()> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_in(notes_dir: &std::path::Path, fmt: &str) -> Config {
        let toml_str = format!(
            "notes_dir = '{}'\ntime_format = '{fmt}'\n",
            notes_dir.display().to_string().replace('\\', "/")
        );
        toml::from_str(&toml_str).unwrap()
    }

    #[test]
    fn sleep_start_with_explicit_time_writes_state() {
        let dir = tempfile::TempDir::new().unwrap();
        let cfg = config_in(dir.path(), "24h");
        cmd_sleep_start(Some("22:30"), &cfg).unwrap();

        let s = state::load(dir.path());
        let pending = s.sleep_start.unwrap();
        assert_eq!(pending.bedtime, NaiveTime::from_hms_opt(22, 30, 0).unwrap());
    }

    #[test]
    fn sleep_start_without_time_uses_now() {
        let dir = tempfile::TempDir::new().unwrap();
        let cfg = config_in(dir.path(), "24h");
        let before = Local::now();
        cmd_sleep_start(None, &cfg).unwrap();
        let after = Local::now();

        let s = state::load(dir.path());
        let pending = s.sleep_start.unwrap();
        let now_t = before.time();
        // Bedtime should be between `before.time()` and `after.time()`,
        // accounting for second-rounding.
        let _ = (now_t, after.time(), pending.bedtime); // sanity
        // Recorded_at within the window [before, after]
        assert!(pending.recorded_at >= before);
        assert!(pending.recorded_at <= after);
    }

    #[test]
    fn sleep_start_overwrites_previous() {
        let dir = tempfile::TempDir::new().unwrap();
        let cfg = config_in(dir.path(), "24h");
        cmd_sleep_start(Some("22:30"), &cfg).unwrap();
        cmd_sleep_start(Some("23:45"), &cfg).unwrap();

        let s = state::load(dir.path());
        assert_eq!(
            s.sleep_start.unwrap().bedtime,
            NaiveTime::from_hms_opt(23, 45, 0).unwrap()
        );
    }

    #[test]
    fn sleep_start_rejects_invalid_time() {
        let dir = tempfile::TempDir::new().unwrap();
        let cfg = config_in(dir.path(), "24h");
        let err = cmd_sleep_start(Some("banana"), &cfg).unwrap_err();
        assert!(err.to_string().contains("Invalid time"));
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --lib cli::sleep_cmd::tests::sleep_start`
Expected: panic at `todo!()`.

- [ ] **Step 4: Implement `cmd_sleep_start`**

Replace the `todo!()`:

```rust
pub fn cmd_sleep_start(time_arg: Option<&str>, config: &Config) -> Result<()> {
    let bedtime = match time_arg {
        Some(s) => time::parse_time(s).ok_or_else(|| {
            color_eyre::eyre::eyre!(
                "Invalid time: '{s}'. Expected HH:MM (24h) or H:MMam/pm (12h)."
            )
        })?,
        None => Local::now().time().with_second(0).unwrap_or_else(|| Local::now().time()),
    };

    let now = Local::now();
    let mut s = state::load(&config.notes_dir_path());
    s.sleep_start = Some(PendingSleepStart {
        bedtime,
        recorded_at: now,
    });
    state::save(&config.notes_dir_path(), &s)?;

    eprintln!(
        "Sleep start recorded: {}",
        time::format_time(bedtime, config.time_format)
    );
    Ok(())
}
```

The `with_second(0)` import comes from `chrono::Timelike`; add at the top of the file:

```rust
use chrono::Timelike;
```

(Truncating to whole minutes keeps the recorded bedtime aligned with what gets written to the markdown — markdown only stores `HH:MM`.)

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib cli::sleep_cmd::tests::sleep_start`
Expected: all four pass.

- [ ] **Step 6: Commit**

```bash
git add src/cli/mod.rs src/cli/sleep_cmd.rs
git commit -m "feat: add cmd_sleep_start"
```

---

## Task 10: `sleep_cmd` — `cmd_sleep_end`

**Files:**
- Modify: `src/cli/sleep_cmd.rs`

- [ ] **Step 1: Write failing tests**

Add to `mod tests` in `src/cli/sleep_cmd.rs`:

```rust
    use std::path::Path;

    fn read_today_note(notes_dir: &Path) -> String {
        let today = Local::now().format("%Y-%m-%d").to_string();
        std::fs::read_to_string(notes_dir.join(format!("{today}.md"))).unwrap()
    }

    #[test]
    fn sleep_end_happy_path_writes_today_and_clears_state() {
        let dir = tempfile::TempDir::new().unwrap();
        let cfg = config_in(dir.path(), "24h");
        cmd_sleep_start(Some("22:30"), &cfg).unwrap();
        cmd_sleep_end(Some("06:15"), &cfg).unwrap();

        let note = read_today_note(dir.path());
        assert!(
            note.contains("sleep: \"22:30-06:15\""),
            "expected canonical 24h sleep entry, got: {note}"
        );

        let s = state::load(dir.path());
        assert!(
            s.sleep_start.is_none(),
            "pending state should be cleared after sleep-end"
        );
    }

    #[test]
    fn sleep_end_uses_time_format_12h() {
        let dir = tempfile::TempDir::new().unwrap();
        let cfg = config_in(dir.path(), "12h");
        cmd_sleep_start(Some("22:30"), &cfg).unwrap();
        cmd_sleep_end(Some("06:15"), &cfg).unwrap();

        let note = read_today_note(dir.path());
        assert!(
            note.contains("sleep: \"10:30pm-6:15am\""),
            "expected 12h-formatted sleep entry, got: {note}"
        );
    }

    #[test]
    fn sleep_end_no_pending_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        let cfg = config_in(dir.path(), "24h");
        let err = cmd_sleep_end(Some("06:15"), &cfg).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("No pending sleep-start"), "got: {msg}");
    }

    #[test]
    fn sleep_end_stale_pending_errors_and_clears_state() {
        let dir = tempfile::TempDir::new().unwrap();
        let cfg = config_in(dir.path(), "24h");

        // Manually save state with a recorded_at >24h ago.
        let stale = Local::now() - chrono::Duration::hours(25);
        let s = PendingState {
            sleep_start: Some(PendingSleepStart {
                bedtime: NaiveTime::from_hms_opt(22, 30, 0).unwrap(),
                recorded_at: stale,
            }),
        };
        state::save(dir.path(), &s).unwrap();

        let err = cmd_sleep_end(Some("06:15"), &cfg).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("No pending sleep-start"), "got: {msg}");
        assert!(msg.contains("stale"), "expected stale suffix, got: {msg}");

        // State should be cleared
        let after = state::load(dir.path());
        assert!(after.sleep_start.is_none());
    }

    #[test]
    fn sleep_end_creates_today_note_from_template() {
        let dir = tempfile::TempDir::new().unwrap();
        let cfg = config_in(dir.path(), "24h");
        cmd_sleep_start(Some("22:30"), &cfg).unwrap();
        cmd_sleep_end(Some("06:15"), &cfg).unwrap();

        let today = Local::now().format("%Y-%m-%d").to_string();
        let path = dir.path().join(format!("{today}.md"));
        assert!(path.exists(), "today's note should be created");
        let note = std::fs::read_to_string(&path).unwrap();
        assert!(note.starts_with("---\n"), "should have frontmatter");
    }

    #[test]
    fn sleep_end_rejects_invalid_time() {
        let dir = tempfile::TempDir::new().unwrap();
        let cfg = config_in(dir.path(), "24h");
        cmd_sleep_start(Some("22:30"), &cfg).unwrap();
        let err = cmd_sleep_end(Some("banana"), &cfg).unwrap_err();
        assert!(err.to_string().contains("Invalid time"));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib cli::sleep_cmd::tests::sleep_end`
Expected: compile errors — `cmd_sleep_end` not defined.

- [ ] **Step 3: Implement `cmd_sleep_end`**

Add to `src/cli/sleep_cmd.rs`:

```rust
const MAX_PENDING_AGE_HOURS: i64 = 24;

/// Reads pending bedtime, finalizes the sleep entry on today's calendar-day file.
/// Calendar today is used (not `effective_today()`): wake-up time itself
/// defines the new day, so `day_start_hour` doesn't apply.
pub fn cmd_sleep_end(time_arg: Option<&str>, config: &Config) -> Result<()> {
    let wake = match time_arg {
        Some(s) => time::parse_time(s).ok_or_else(|| {
            color_eyre::eyre::eyre!(
                "Invalid time: '{s}'. Expected HH:MM (24h) or H:MMam/pm (12h)."
            )
        })?,
        None => Local::now().time().with_second(0).unwrap_or_else(|| Local::now().time()),
    };

    let notes_dir = config.notes_dir_path();
    let mut state = state::load(&notes_dir);

    let pending = match state.sleep_start.take() {
        Some(p) => p,
        None => {
            bail!(
                "No pending sleep-start. Run `daylog sleep-start` before bed, or use \
                 `daylog log sleep \"HH:MM-HH:MM\"` for a one-shot entry."
            );
        }
    };

    let age = Local::now().signed_duration_since(pending.recorded_at);
    if age.num_hours() > MAX_PENDING_AGE_HOURS {
        // Stale: clear the state and error.
        state::save(&notes_dir, &state)?;
        bail!(
            "No pending sleep-start (ignored stale sleep-start from {}). \
             Run `daylog sleep-start` before bed, or use `daylog log sleep \
             \"HH:MM-HH:MM\"` for a one-shot entry.",
            pending.recorded_at.format("%Y-%m-%d %H:%M")
        );
    }

    let bedtime = pending.bedtime;
    let wake_date = Local::now().date_naive();
    let formatted = time::format_sleep_range(bedtime, wake, config.time_format);

    let note_path = notes_dir.join(format!("{}.md", wake_date.format("%Y-%m-%d")));
    let content = if note_path.exists() {
        std::fs::read_to_string(&note_path)?
    } else {
        crate::template::render_daily_note(&wake_date.format("%Y-%m-%d").to_string(), config)
    };
    let updated = crate::frontmatter::set_scalar(
        &content,
        "sleep",
        &format!("\"{}\"", formatted),
    );
    crate::frontmatter::atomic_write(&note_path, &updated)?;

    // Clear pending and persist.
    state::save(&notes_dir, &state)?;

    let hours = time::sleep_hours(bedtime, wake);
    eprintln!(
        "Sleep recorded: {formatted} ({hours:.2}h) on {}",
        wake_date.format("%Y-%m-%d")
    );
    Ok(())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib cli::sleep_cmd::`
Expected: all sleep_start AND sleep_end tests pass.

- [ ] **Step 5: Run full library test suite**

Run: `cargo test --lib`
Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/cli/sleep_cmd.rs
git commit -m "feat: add cmd_sleep_end with stale-pending guard"
```

---

## Task 11: CLI wiring — subcommands and dispatch

**Files:**
- Modify: `src/cli/mod.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Add subcommands to clap**

In `src/cli/mod.rs`, extend the `Commands` enum:

```rust
    /// Record bedtime (use now, or pass a time)
    SleepStart {
        /// Bedtime in HH:MM (24h) or H:MMam/pm (12h)
        time: Option<String>,
    },
    /// Finalize sleep entry on today's note (use now, or pass a wake time)
    SleepEnd {
        /// Wake time in HH:MM (24h) or H:MMam/pm (12h)
        time: Option<String>,
    },
```

- [ ] **Step 2: Dispatch in `main.rs`**

In `src/main.rs`, extend the `match cli.command` block:

```rust
        Some(Commands::SleepStart { time }) => cmd_sleep_start(time.as_deref()),
        Some(Commands::SleepEnd { time }) => cmd_sleep_end(time.as_deref()),
```

Add the helper functions at the bottom of `src/main.rs`:

```rust
fn cmd_sleep_start(time: Option<&str>) -> Result<()> {
    let config = Config::load()?;
    daylog::cli::sleep_cmd::cmd_sleep_start(time, &config)
}

fn cmd_sleep_end(time: Option<&str>) -> Result<()> {
    let config = Config::load()?;
    daylog::cli::sleep_cmd::cmd_sleep_end(time, &config)
}
```

- [ ] **Step 3: Run smoke build**

Run: `cargo build`
Expected: clean build, no warnings.

- [ ] **Step 4: Run full test suite**

Run: `cargo test`
Expected: all pass.

- [ ] **Step 5: Manual smoke test**

```bash
cargo run -- sleep-start --help
cargo run -- sleep-end --help
```

Expected: clap renders help for both, accepting an optional `[TIME]` positional.

- [ ] **Step 6: Commit**

```bash
git add src/cli/mod.rs src/main.rs
git commit -m "feat: wire sleep-start and sleep-end into CLI"
```

---

## Task 12: Document `time_format` in the default preset

**Files:**
- Modify: `presets/default.toml`

- [ ] **Step 1: Add documentation comment**

In `presets/default.toml`, add after the `weight_unit` block (around line 6):

```toml
# time_format = "12h"  # or "24h". Controls how times are written to markdown
                       # files and shown in the dashboard. The DB always
                       # stores canonical 24h. Default: "12h".
```

- [ ] **Step 2: Verify default config still parses**

Run: `cargo test --lib config::tests::test_parse_default_config`
Expected: pass.

- [ ] **Step 3: Final integration check**

Run: `cargo test`
Expected: all pass.

Run: `just lint` (or `cargo fmt --check && cargo clippy --all-targets -- -D warnings`)
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add presets/default.toml
git commit -m "docs: document time_format in default preset"
```

---

## Self-Review

**Spec coverage:**
- Goal 1 (commands): Tasks 9, 10, 11 ✓
- Goal 2 (canonical 24h DB): Task 5 ✓
- Goal 3 (`time_format` config): Tasks 1, 12 ✓
- Goal 4 (consolidated time module): Tasks 2, 3, 4 ✓ — used by Tasks 5, 6, 7, 9, 10
- `day_start_hour` not consulted: documented in Task 10 implementation comment ✓
- `.daylog-state.toml` sidecar in `notes_dir`: Task 8 ✓
- Stale pending >24h: Task 10 ✓
- Multiple sleep-start: Task 9 (overwrites_previous test) ✓
- Dashboard formats per config: Task 7 ✓
- `log sleep` validation hardened + writes per config: Task 6 ✓
- Error messages match spec wording: Tasks 9, 10 ✓

**Type consistency:** `TimeFormat`, `parse_time`, `parse_sleep_range`, `format_time`, `format_sleep_range`, `sleep_hours`, `PendingState`, `PendingSleepStart`, `cmd_sleep_start`, `cmd_sleep_end` — names consistent across all tasks.

**Migration:** Existing users run `daylog rebuild` once after upgrade to refresh the DB. Documented in spec; no in-code migration logic needed because the watcher re-materializes any changed file automatically.
