# Reminders Time Gates Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `not_before` / `not_after` per-reminder time gates so reminders bound to a time of day stay silent outside their window.

**Architecture:** Two optional `NaiveTime` fields on `Reminder` and `EvaluatedReminder`. A private `within_time_window` helper does offset-math against `day_start_hour` so the gate attaches to the right effective day. `evaluate` gains a `now: NaiveTime` parameter (callers pass `chrono::Local::now().time()`). `due = data_overdue && in_window`. Validation in `load_reminders` rejects wrap-around configs. JSON shape gets two new fields.

**Tech Stack:** Rust, chrono, rusqlite, serde, color_eyre. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-05-12-reminders-time-gates-design.md`

---

## File map

- **Modify** `src/reminders.rs` — add `not_before`/`not_after` fields to `Reminder` and `EvaluatedReminder`; add `within_time_window` and `offset_minutes` helpers; rewire `evaluate` (new `now` param, new due logic, propagate fields into `EvaluatedReminder`); extend `load_reminders` to parse + validate the time strings; extend `to_json`; extend tests.
- **Modify** `src/config.rs` — add `not_before` / `not_after: Option<String>` to `ReminderConfig`.
- **Modify** `src/cli/today_cmd.rs` — pass `chrono::Local::now().time()` into `evaluate`; update the in-file test that calls `evaluate`; extend the JSON-shape tests to assert the new fields.
- **Modify** `src/cli/status_cmd.rs` — pass `chrono::Local::now().time()` into `evaluate`; extend the JSON-shape tests to assert the new fields.
- **Modify** `tests/reminders.rs` — update existing 4 tests to pass a `now` time; add 1 new integration test for the gate path.
- **Modify** `presets/default.toml` — extend the commented `[reminders]` example with `not_before` / `not_after` lines on the brush-style entries.
- **Modify** `README.md` — append a short paragraph to the existing Reminders section.

No new files. No DB schema change. No migration.

---

## Task 1: Add `not_before`/`not_after` fields to types

**Files:**
- Modify: `src/reminders.rs`
- Modify: `src/config.rs`

Goal: add the fields to all three relevant types (`Reminder`, `EvaluatedReminder`, `ReminderConfig`), defaulted to `None`. Make the codebase compile and all existing tests pass. No new behavior yet — `evaluate` continues to ignore the fields, `load_reminders` always populates them as `None`.

- [ ] **Step 1: Add the fields to `ReminderConfig` in `src/config.rs`**

Locate the existing `ReminderConfig` struct (around line 160):

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct ReminderConfig {
    pub display: String,
    pub interval_days: u32,
    pub watch: String,
    pub target: toml::Value,
    #[serde(default)]
    pub count_zero_as_logged: bool,
}
```

Add two new fields after `count_zero_as_logged`:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct ReminderConfig {
    pub display: String,
    pub interval_days: u32,
    pub watch: String,
    pub target: toml::Value,
    #[serde(default)]
    pub count_zero_as_logged: bool,
    #[serde(default)]
    pub not_before: Option<String>,
    #[serde(default)]
    pub not_after: Option<String>,
}
```

- [ ] **Step 2: Add the fields to `Reminder` in `src/reminders.rs`**

Locate the existing `Reminder` struct (top of file, around line 19). The current shape:

```rust
pub struct Reminder {
    pub id: String,
    pub display: String,
    pub interval_days: u32,
    pub watch: WatchSource,
}
```

Add the `NaiveTime` import at the top of the file. Change:

```rust
use chrono::NaiveDate;
```

to:

```rust
use chrono::{NaiveDate, NaiveTime};
```

Then expand `Reminder`:

```rust
pub struct Reminder {
    pub id: String,
    pub display: String,
    pub interval_days: u32,
    pub watch: WatchSource,
    pub not_before: Option<NaiveTime>,
    pub not_after: Option<NaiveTime>,
}
```

- [ ] **Step 3: Add the fields to `EvaluatedReminder` in `src/reminders.rs`**

Locate `EvaluatedReminder` (around line 128). Current:

```rust
pub struct EvaluatedReminder {
    pub id: String,
    pub display: String,
    pub interval_days: u32,
    pub last_done: Option<NaiveDate>,
    pub days_since: Option<i64>,
    pub due: bool,
}
```

Add the two new fields at the end:

```rust
pub struct EvaluatedReminder {
    pub id: String,
    pub display: String,
    pub interval_days: u32,
    pub last_done: Option<NaiveDate>,
    pub days_since: Option<i64>,
    pub due: bool,
    pub not_before: Option<NaiveTime>,
    pub not_after: Option<NaiveTime>,
}
```

- [ ] **Step 4: Update `load_reminders` to populate the new `Reminder` fields as `None`**

In `src/reminders.rs::load_reminders`, locate the `out.push(Reminder { ... })` call. It currently constructs:

```rust
out.push(Reminder {
    id: id.clone(),
    display: cfg.display.clone(),
    interval_days: cfg.interval_days,
    watch,
});
```

Add `not_before: None, not_after: None` (real parsing comes in Task 2):

```rust
out.push(Reminder {
    id: id.clone(),
    display: cfg.display.clone(),
    interval_days: cfg.interval_days,
    watch,
    not_before: None,
    not_after: None,
});
```

- [ ] **Step 5: Update `evaluate` to populate `EvaluatedReminder` fields as `None`**

In `src/reminders.rs::evaluate`, locate the `out.push(EvaluatedReminder { ... })` block. Add `not_before: None, not_after: None` at the end of the struct literal:

```rust
out.push(EvaluatedReminder {
    id: r.id.clone(),
    display: r.display.clone(),
    interval_days: r.interval_days,
    last_done,
    days_since,
    due,
    not_before: None,
    not_after: None,
});
```

(Real propagation from `r.not_before` / `r.not_after` comes in Task 4 along with the time-gate logic.)

- [ ] **Step 6: Fix any test helpers that build `Reminder` literals directly**

The unit tests in `src/reminders.rs::tests` construct `Reminder` values via helper functions like `metric_reminder`, `session_text_reminder`, `session_num_reminder`, `lift_reminder`, `day_field_reminder`. Each helper builds a struct literal of `Reminder`. Add `not_before: None, not_after: None` to each.

Search for `Reminder {` inside `mod tests` (the test module starts around line 700ish — `grep -n "Reminder {" src/reminders.rs` will list them). The helpers are roughly:

```rust
fn metric_reminder(id: &str, interval_days: u32, metric: &str) -> Reminder {
    Reminder {
        id: id.into(),
        display: id.into(),
        interval_days,
        watch: WatchSource::Metric {
            id: metric.into(),
            count_zero_as_logged: false,
        },
        not_before: None,   // ← add
        not_after: None,    // ← add
    }
}
```

Apply the same `not_before: None, not_after: None` addition to:
- `metric_reminder`
- `session_text_reminder`
- `session_num_reminder`
- `lift_reminder`
- `day_field_reminder`

Also check for ad-hoc `Reminder { ... }` literals inside test bodies — there's at least one in `evaluate_metric_zero_value_counts_when_opted_in`. Apply the same fix.

- [ ] **Step 7: Verify build + tests are clean**

Run: `cargo build`
Expected: clean build, no warnings.

Run: `cargo test`
Expected: all existing tests pass (count unchanged from the v1.1 baseline of 425).

Run: `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add src/reminders.rs src/config.rs
git commit -m "$(cat <<'EOF'
feat(reminders): scaffold not_before / not_after fields on types

Adds optional Option<NaiveTime> fields to Reminder and
EvaluatedReminder, plus Option<String> mirrors on ReminderConfig.
All default to None. Behaviour unchanged — fields are not yet
read by `evaluate` or `load_reminders`, just present in the type
surface so subsequent tasks can fill them in.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Parse + validate `not_before` / `not_after` in `load_reminders`

**Files:**
- Modify: `src/reminders.rs`

Goal: parse the `Option<String>` fields from `ReminderConfig` into `Option<NaiveTime>` on `Reminder`, with hard-fail validation on bad input. Add a private `offset_minutes` helper (will also be used by Task 3's `within_time_window`).

- [ ] **Step 1: Add failing tests**

In `src/reminders.rs::tests`, append (just before the closing `}` of `mod tests`):

```rust
    #[test]
    fn load_parses_not_before_and_not_after() {
        let config = config_with(
            r#"
[reminders.brush_evening]
display = "Brush teeth (evening)"
interval_days = 1
watch = "metric"
target = "la_min"
not_before = "18:00"
not_after = "23:00"
"#,
        );
        let rs = load_reminders(&config).unwrap();
        assert_eq!(
            rs[0].not_before,
            Some(NaiveTime::from_hms_opt(18, 0, 0).unwrap())
        );
        assert_eq!(
            rs[0].not_after,
            Some(NaiveTime::from_hms_opt(23, 0, 0).unwrap())
        );
    }

    #[test]
    fn load_parses_not_before_only() {
        let config = config_with(
            r#"
[reminders.la]
display = "LA"
interval_days = 2
watch = "metric"
target = "la_min"
not_before = "10:00"
"#,
        );
        let rs = load_reminders(&config).unwrap();
        assert_eq!(
            rs[0].not_before,
            Some(NaiveTime::from_hms_opt(10, 0, 0).unwrap())
        );
        assert_eq!(rs[0].not_after, None);
    }

    #[test]
    fn load_parses_not_after_only() {
        let config = config_with(
            r#"
[reminders.morning]
display = "Morning"
interval_days = 1
watch = "metric"
target = "la_min"
not_after = "12:00"
"#,
        );
        let rs = load_reminders(&config).unwrap();
        assert_eq!(rs[0].not_before, None);
        assert_eq!(
            rs[0].not_after,
            Some(NaiveTime::from_hms_opt(12, 0, 0).unwrap())
        );
    }

    #[test]
    fn load_no_time_gates_yields_none() {
        let config = config_with(
            r#"
[reminders.la]
display = "LA"
interval_days = 2
watch = "metric"
target = "la_min"
"#,
        );
        let rs = load_reminders(&config).unwrap();
        assert_eq!(rs[0].not_before, None);
        assert_eq!(rs[0].not_after, None);
    }

    #[test]
    fn load_rejects_unparseable_not_before() {
        let config = config_with(
            r#"
[reminders.bad]
display = "Bad"
interval_days = 1
watch = "metric"
target = "la_min"
not_before = "25:00"
"#,
        );
        let err = load_reminders(&config).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("not_before"), "got: {msg}");
        assert!(msg.contains("25:00"), "got: {msg}");
        assert!(msg.contains("bad"), "got: {msg}");
    }

    #[test]
    fn load_rejects_unparseable_not_after() {
        let config = config_with(
            r#"
[reminders.bad]
display = "Bad"
interval_days = 1
watch = "metric"
target = "la_min"
not_after = "noon"
"#,
        );
        let err = load_reminders(&config).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("not_after"), "got: {msg}");
        assert!(msg.contains("noon"), "got: {msg}");
    }

    #[test]
    fn load_rejects_window_wraparound_default_day_start() {
        // day_start_hour = 0 (default): a window from 22:00 to 06:00 crosses
        // the effective-day boundary → reject.
        let config = config_with(
            r#"
[reminders.bad]
display = "Bad"
interval_days = 1
watch = "metric"
target = "la_min"
not_before = "22:00"
not_after = "06:00"
"#,
        );
        let err = load_reminders(&config).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("not_after"), "got: {msg}");
        assert!(msg.contains("not_before"), "got: {msg}");
    }

    #[test]
    fn load_accepts_window_wrapping_wall_clock_with_day_start_hour() {
        // day_start_hour = 4: a window 22:00 → 02:00 spans midnight in
        // wall-clock but stays within the effective day (offsets 1080 → 1320)
        // → accept.
        let toml_str = r#"
notes_dir = "/tmp/x"
day_start_hour = 4

[metrics]
la_min = { display = "LA", color = "red" }

[reminders.evening]
display = "Evening"
interval_days = 1
watch = "metric"
target = "la_min"
not_before = "22:00"
not_after = "02:00"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let rs = load_reminders(&config).unwrap();
        assert_eq!(
            rs[0].not_before,
            Some(NaiveTime::from_hms_opt(22, 0, 0).unwrap())
        );
        assert_eq!(
            rs[0].not_after,
            Some(NaiveTime::from_hms_opt(2, 0, 0).unwrap())
        );
    }

    #[test]
    fn load_rejects_window_crossing_effective_day_with_day_start_hour() {
        // day_start_hour = 4: not_before = 02:00 (offset 1320, late in
        // effective day D), not_after = 06:00 (offset 120, early in
        // effective day D+1) → reject.
        let toml_str = r#"
notes_dir = "/tmp/x"
day_start_hour = 4

[metrics]
la_min = { display = "LA", color = "red" }

[reminders.bad]
display = "Bad"
interval_days = 1
watch = "metric"
target = "la_min"
not_before = "02:00"
not_after = "06:00"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let err = load_reminders(&config).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("not_after"), "got: {msg}");
    }
```

- [ ] **Step 2: Run and confirm they fail**

Run: `cargo test --lib reminders::tests::load_parses_not_before` and similar.
Expected: at minimum the `load_parses_not_before_and_not_after`, `load_parses_not_before_only`, `load_parses_not_after_only` tests fail (the `Reminder` is still always built with `not_before: None, not_after: None`).

The `load_no_time_gates_yields_none` test should already pass — it verifies the Task-1 default behavior.

- [ ] **Step 3: Add the `offset_minutes` helper**

In `src/reminders.rs`, add this private function near the bottom of the file (just above the closing brace of the module, before `#[cfg(test)] mod tests`):

```rust
/// Minutes elapsed since the effective-day start. With `day_start_hour = 0`
/// this is just `t.hour() * 60 + t.minute()`; with non-zero `day_start_hour`
/// it wraps so a wall-clock time before the day-start gets attributed to
/// the previous effective day.
fn offset_minutes(t: NaiveTime, day_start_hour: u8) -> u32 {
    use chrono::Timelike;
    let t_mins = t.hour() * 60 + t.minute();
    let start_mins = day_start_hour as u32 * 60;
    if t_mins >= start_mins {
        t_mins - start_mins
    } else {
        t_mins + 24 * 60 - start_mins
    }
}
```

- [ ] **Step 4: Add the time-string parser**

Above `offset_minutes`, add:

```rust
/// Parse an `HH:MM` time-of-day string. Returns a clear eyre error
/// (with `.suggestion()`) on failure, mentioning the reminder id and
/// the field name.
fn parse_time_field(id: &str, label: &str, s: &str) -> Result<NaiveTime> {
    use color_eyre::Help;
    NaiveTime::parse_from_str(s, "%H:%M")
        .map_err(|_| {
            color_eyre::eyre::eyre!("reminder `{id}`: invalid {label} `{s}`")
        })
        .suggestion("Use HH:MM in 24-hour form, e.g., \"18:00\".")
}
```

- [ ] **Step 5: Wire parsing + validation into `load_reminders`**

In `src/reminders.rs::load_reminders`, the current per-reminder body looks like:

```rust
for (id, cfg) in &config.reminders {
    if cfg.interval_days < 1 {
        return Err(...).suggestion(...);
    }
    let watch = match cfg.watch.as_str() {
        // ...
    };
    out.push(Reminder {
        id: id.clone(),
        display: cfg.display.clone(),
        interval_days: cfg.interval_days,
        watch,
        not_before: None,
        not_after: None,
    });
}
```

After the `watch` match and before the `out.push`, insert the time-parsing block:

```rust
        let not_before = cfg
            .not_before
            .as_deref()
            .map(|s| parse_time_field(id, "not_before", s))
            .transpose()?;
        let not_after = cfg
            .not_after
            .as_deref()
            .map(|s| parse_time_field(id, "not_after", s))
            .transpose()?;
        if let (Some(nb), Some(na)) = (not_before, not_after) {
            let nb_off = offset_minutes(nb, config.day_start_hour);
            let na_off = offset_minutes(na, config.day_start_hour);
            if nb_off > na_off {
                use color_eyre::Help;
                return Err(color_eyre::eyre::eyre!(
                    "reminder `{id}`: not_after must not be earlier than not_before within the effective day (with day_start_hour = {})",
                    config.day_start_hour
                ))
                .suggestion(
                    "For overnight reminders, split into two reminders on the same metric — one with not_after, the other with not_before.",
                );
            }
        }
```

Then change the `out.push(Reminder { ... not_before: None, not_after: None })` to use the parsed values:

```rust
        out.push(Reminder {
            id: id.clone(),
            display: cfg.display.clone(),
            interval_days: cfg.interval_days,
            watch,
            not_before,
            not_after,
        });
```

- [ ] **Step 6: Run tests**

Run: `cargo test --lib reminders::tests`
Expected: all 9 new tests pass plus all existing ones.

Run: `cargo test`
Expected: full suite clean (still 425 + 9 = 434 unit tests in the reminders module).

Run: `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add src/reminders.rs
git commit -m "$(cat <<'EOF'
feat(reminders): parse + validate not_before / not_after

Adds HH:MM parsing for the new time-gate config fields, plus
hard-fail validation that rejects windows where not_after falls
earlier than not_before within the effective day. Uses offset
math against day_start_hour so an evening reminder with
day_start_hour = 4 (window 22:00 → 02:00 wall-clock) is correctly
accepted as contiguous within the effective day, while a window
that would actually cross an effective-day boundary is rejected.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Add `within_time_window` helper

**Files:**
- Modify: `src/reminders.rs`

Goal: a private pure function that returns true iff `now` falls inside the time window defined by `not_before`/`not_after`, using effective-day offsets. Used by `evaluate` in Task 4. Built TDD-style with comprehensive boundary tests.

- [ ] **Step 1: Add failing tests**

In `src/reminders.rs::tests`, append:

```rust
    #[test]
    fn within_time_window_no_gates_always_true() {
        let now = NaiveTime::from_hms_opt(3, 14, 0).unwrap();
        assert!(within_time_window(now, None, None, 0));
    }

    #[test]
    fn within_time_window_lower_only() {
        let nb = NaiveTime::from_hms_opt(18, 0, 0).unwrap();
        let before = NaiveTime::from_hms_opt(17, 59, 0).unwrap();
        let on = NaiveTime::from_hms_opt(18, 0, 0).unwrap();
        let after = NaiveTime::from_hms_opt(18, 1, 0).unwrap();
        assert!(!within_time_window(before, Some(nb), None, 0));
        assert!(within_time_window(on, Some(nb), None, 0));
        assert!(within_time_window(after, Some(nb), None, 0));
    }

    #[test]
    fn within_time_window_upper_only() {
        let na = NaiveTime::from_hms_opt(12, 0, 0).unwrap();
        let before = NaiveTime::from_hms_opt(11, 59, 0).unwrap();
        let on = NaiveTime::from_hms_opt(12, 0, 0).unwrap();
        let after = NaiveTime::from_hms_opt(12, 1, 0).unwrap();
        assert!(within_time_window(before, None, Some(na), 0));
        assert!(within_time_window(on, None, Some(na), 0));
        assert!(!within_time_window(after, None, Some(na), 0));
    }

    #[test]
    fn within_time_window_both_gates() {
        let nb = NaiveTime::from_hms_opt(18, 0, 0).unwrap();
        let na = NaiveTime::from_hms_opt(23, 0, 0).unwrap();
        assert!(!within_time_window(
            NaiveTime::from_hms_opt(17, 0, 0).unwrap(),
            Some(nb),
            Some(na),
            0,
        ));
        assert!(within_time_window(
            NaiveTime::from_hms_opt(20, 0, 0).unwrap(),
            Some(nb),
            Some(na),
            0,
        ));
        assert!(!within_time_window(
            NaiveTime::from_hms_opt(23, 30, 0).unwrap(),
            Some(nb),
            Some(na),
            0,
        ));
    }

    #[test]
    fn within_time_window_day_start_hour_zero_baseline() {
        // Sanity check: at day_start_hour = 0, offset math reduces to
        // wall-clock comparison.
        let nb = NaiveTime::from_hms_opt(6, 0, 0).unwrap();
        let na = NaiveTime::from_hms_opt(22, 0, 0).unwrap();
        assert!(!within_time_window(
            NaiveTime::from_hms_opt(5, 59, 0).unwrap(),
            Some(nb),
            Some(na),
            0,
        ));
        assert!(within_time_window(
            NaiveTime::from_hms_opt(12, 0, 0).unwrap(),
            Some(nb),
            Some(na),
            0,
        ));
        assert!(!within_time_window(
            NaiveTime::from_hms_opt(22, 1, 0).unwrap(),
            Some(nb),
            Some(na),
            0,
        ));
    }

    #[test]
    fn within_time_window_evening_gate_with_day_start_four() {
        // day_start_hour = 4, not_before = "22:00" → offset 1080.
        // Gate is open from 22:00 wall-clock through 03:59 the next
        // wall-day (offset 1439), then closes at 04:00 (offset 0).
        let nb = NaiveTime::from_hms_opt(22, 0, 0).unwrap();
        assert!(!within_time_window(
            NaiveTime::from_hms_opt(21, 59, 0).unwrap(),
            Some(nb),
            None,
            4,
        ));
        assert!(within_time_window(
            NaiveTime::from_hms_opt(22, 0, 0).unwrap(),
            Some(nb),
            None,
            4,
        ));
        // 01:00 wall-clock the next day — still within the effective day
        // that started at 04:00 yesterday.
        assert!(within_time_window(
            NaiveTime::from_hms_opt(1, 0, 0).unwrap(),
            Some(nb),
            None,
            4,
        ));
        // 04:00 wall-clock — new effective day starts, gate closes
        // until the next 22:00.
        assert!(!within_time_window(
            NaiveTime::from_hms_opt(4, 0, 0).unwrap(),
            Some(nb),
            None,
            4,
        ));
    }

    #[test]
    fn within_time_window_early_morning_gate_with_day_start_four() {
        // day_start_hour = 4, not_before = "02:00" → offset 1320 (late
        // in the effective day, near its end). Gate opens at 02:00
        // wall-clock the day after the effective day started.
        let nb = NaiveTime::from_hms_opt(2, 0, 0).unwrap();
        assert!(!within_time_window(
            NaiveTime::from_hms_opt(1, 59, 0).unwrap(),
            Some(nb),
            None,
            4,
        ));
        assert!(within_time_window(
            NaiveTime::from_hms_opt(2, 30, 0).unwrap(),
            Some(nb),
            None,
            4,
        ));
        // 04:00 — effective day rolls, gate closes.
        assert!(!within_time_window(
            NaiveTime::from_hms_opt(4, 0, 0).unwrap(),
            Some(nb),
            None,
            4,
        ));
    }
```

- [ ] **Step 2: Run and confirm they fail**

Run: `cargo test --lib reminders::tests::within_time_window`
Expected: compile failure (function does not exist).

- [ ] **Step 3: Add the helper**

In `src/reminders.rs`, add this private function near `offset_minutes` (just above the closing brace of the module, before `#[cfg(test)] mod tests`):

```rust
/// Returns true if `now` falls inside the effective-day window defined by
/// `not_before` and `not_after`. Either bound being `None` means "no
/// limit on that side". Uses `offset_minutes` so non-zero `day_start_hour`
/// values correctly attribute wall-clock times to the right effective day.
fn within_time_window(
    now: NaiveTime,
    not_before: Option<NaiveTime>,
    not_after: Option<NaiveTime>,
    day_start_hour: u8,
) -> bool {
    let now_off = offset_minutes(now, day_start_hour);
    let after_lower = not_before.map_or(true, |nb| now_off >= offset_minutes(nb, day_start_hour));
    let before_upper = not_after.map_or(true, |na| now_off <= offset_minutes(na, day_start_hour));
    after_lower && before_upper
}
```

- [ ] **Step 4: Run and confirm tests pass**

Run: `cargo test --lib reminders::tests::within_time_window`
Expected: all 7 new tests pass.

Run: `cargo test`
Expected: full suite clean.

Run: `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add src/reminders.rs
git commit -m "$(cat <<'EOF'
feat(reminders): within_time_window helper with day_start_hour awareness

Pure function that computes whether `now` falls inside the
[not_before, not_after] window using effective-day offsets.
Either bound is optional. Comprehensive tests cover the
boundaries plus the tricky day_start_hour > 0 cases that
attribute pre-day-start wall-clock times to the previous
effective day.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Wire the time gate into `evaluate`

**Files:**
- Modify: `src/reminders.rs`
- Modify: `src/cli/today_cmd.rs`
- Modify: `src/cli/status_cmd.rs`
- Modify: `tests/reminders.rs`

Goal: add `now: NaiveTime` to `evaluate`'s signature, AND the time-window check into the `due` formula, AND propagate `not_before`/`not_after` from `Reminder` into `EvaluatedReminder`. Update all call sites (cli + tests + integration tests). The new behavior is now testable end-to-end.

- [ ] **Step 1: Add failing tests for the gated due logic**

In `src/reminders.rs::tests`, append:

```rust
    fn noon() -> NaiveTime {
        NaiveTime::from_hms_opt(12, 0, 0).unwrap()
    }

    #[test]
    fn evaluate_data_overdue_before_window_not_due() {
        let conn = make_test_db();
        // Logged 3 days ago, interval=1 → data-overdue.
        insert_metric(&conn, "2026-05-09", "la_min", 15.0);
        let today = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        let r = Reminder {
            id: "evening".into(),
            display: "Evening".into(),
            interval_days: 1,
            watch: WatchSource::Metric {
                id: "la_min".into(),
                count_zero_as_logged: false,
            },
            not_before: Some(NaiveTime::from_hms_opt(18, 0, 0).unwrap()),
            not_after: None,
        };
        // Wall-clock 10:00 — before the gate.
        let now = NaiveTime::from_hms_opt(10, 0, 0).unwrap();
        let result = evaluate(&conn, today, now, &[r], &empty_config()).unwrap();
        let er = &result.reminders[0];
        assert_eq!(er.last_done, Some(NaiveDate::from_ymd_opt(2026, 5, 9).unwrap()));
        assert_eq!(er.days_since, Some(3));
        assert!(!er.due, "data-overdue but before window → not due");
    }

    #[test]
    fn evaluate_data_overdue_in_window_is_due() {
        let conn = make_test_db();
        insert_metric(&conn, "2026-05-09", "la_min", 15.0);
        let today = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        let r = Reminder {
            id: "evening".into(),
            display: "Evening".into(),
            interval_days: 1,
            watch: WatchSource::Metric {
                id: "la_min".into(),
                count_zero_as_logged: false,
            },
            not_before: Some(NaiveTime::from_hms_opt(18, 0, 0).unwrap()),
            not_after: Some(NaiveTime::from_hms_opt(23, 0, 0).unwrap()),
        };
        let now = NaiveTime::from_hms_opt(20, 0, 0).unwrap();
        let result = evaluate(&conn, today, now, &[r], &empty_config()).unwrap();
        assert!(result.reminders[0].due);
    }

    #[test]
    fn evaluate_data_overdue_after_window_not_due() {
        let conn = make_test_db();
        insert_metric(&conn, "2026-05-09", "la_min", 15.0);
        let today = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        let r = Reminder {
            id: "morning".into(),
            display: "Morning".into(),
            interval_days: 1,
            watch: WatchSource::Metric {
                id: "la_min".into(),
                count_zero_as_logged: false,
            },
            not_before: Some(NaiveTime::from_hms_opt(7, 0, 0).unwrap()),
            not_after: Some(NaiveTime::from_hms_opt(12, 0, 0).unwrap()),
        };
        // Wall-clock 14:00 — past the upper bound.
        let now = NaiveTime::from_hms_opt(14, 0, 0).unwrap();
        let result = evaluate(&conn, today, now, &[r], &empty_config()).unwrap();
        assert!(!result.reminders[0].due);
    }

    #[test]
    fn evaluate_not_data_overdue_in_window_not_due() {
        let conn = make_test_db();
        // Logged today — not data-overdue.
        insert_metric(&conn, "2026-05-12", "la_min", 15.0);
        let today = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        let r = Reminder {
            id: "evening".into(),
            display: "Evening".into(),
            interval_days: 1,
            watch: WatchSource::Metric {
                id: "la_min".into(),
                count_zero_as_logged: false,
            },
            not_before: Some(NaiveTime::from_hms_opt(18, 0, 0).unwrap()),
            not_after: None,
        };
        let now = NaiveTime::from_hms_opt(20, 0, 0).unwrap();
        let result = evaluate(&conn, today, now, &[r], &empty_config()).unwrap();
        assert!(!result.reminders[0].due);
    }

    #[test]
    fn evaluate_never_logged_outside_window_not_due() {
        let conn = make_test_db();
        // No history.
        let today = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        let r = Reminder {
            id: "evening".into(),
            display: "Evening".into(),
            interval_days: 1,
            watch: WatchSource::Metric {
                id: "la_min".into(),
                count_zero_as_logged: false,
            },
            not_before: Some(NaiveTime::from_hms_opt(18, 0, 0).unwrap()),
            not_after: None,
        };
        let now = NaiveTime::from_hms_opt(10, 0, 0).unwrap();
        let result = evaluate(&conn, today, now, &[r], &empty_config()).unwrap();
        assert_eq!(result.reminders[0].last_done, None);
        assert!(!result.reminders[0].due, "gate trumps 'never logged'");
    }

    #[test]
    fn evaluate_propagates_not_before_not_after_into_evaluated_reminder() {
        let conn = make_test_db();
        let today = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
        let nb = NaiveTime::from_hms_opt(18, 0, 0).unwrap();
        let na = NaiveTime::from_hms_opt(23, 0, 0).unwrap();
        let r = Reminder {
            id: "x".into(),
            display: "X".into(),
            interval_days: 1,
            watch: WatchSource::Metric {
                id: "la_min".into(),
                count_zero_as_logged: false,
            },
            not_before: Some(nb),
            not_after: Some(na),
        };
        let result = evaluate(&conn, today, noon(), &[r], &empty_config()).unwrap();
        assert_eq!(result.reminders[0].not_before, Some(nb));
        assert_eq!(result.reminders[0].not_after, Some(na));
    }
```

- [ ] **Step 2: Update `evaluate`'s signature and body**

In `src/reminders.rs`, the current `evaluate` (around line 381) looks like:

```rust
pub fn evaluate(
    conn: &Connection,
    today: NaiveDate,
    reminders: &[Reminder],
    config: &Config,
) -> Result<EvaluationResult> {
    let mut out = Vec::with_capacity(reminders.len());
    let mut warnings = Vec::new();
    for r in reminders {
        let last_done = query_last_done(conn, &r.watch)?;
        if let WatchSource::Metric { id, .. } = &r.watch {
            if last_done.is_none() && !config.metrics.contains_key(id) {
                warnings.push(format!(
                    "reminder `{}`: target metric `{id}` is not declared in [metrics]",
                    r.id
                ));
            }
        }
        let days_since = last_done.map(|d| (today - d).num_days());
        let due = match days_since {
            None => true,
            Some(n) => n >= r.interval_days as i64,
        };
        out.push(EvaluatedReminder {
            id: r.id.clone(),
            display: r.display.clone(),
            interval_days: r.interval_days,
            last_done,
            days_since,
            due,
            not_before: None,
            not_after: None,
        });
    }
    Ok(EvaluationResult {
        reminders: out,
        warnings,
    })
}
```

Change it to:

```rust
pub fn evaluate(
    conn: &Connection,
    today: NaiveDate,
    now: NaiveTime,
    reminders: &[Reminder],
    config: &Config,
) -> Result<EvaluationResult> {
    let mut out = Vec::with_capacity(reminders.len());
    let mut warnings = Vec::new();
    for r in reminders {
        let last_done = query_last_done(conn, &r.watch)?;
        if let WatchSource::Metric { id, .. } = &r.watch {
            if last_done.is_none() && !config.metrics.contains_key(id) {
                warnings.push(format!(
                    "reminder `{}`: target metric `{id}` is not declared in [metrics]",
                    r.id
                ));
            }
        }
        let days_since = last_done.map(|d| (today - d).num_days());
        let data_overdue = match days_since {
            None => true,
            Some(n) => n >= r.interval_days as i64,
        };
        let in_window =
            within_time_window(now, r.not_before, r.not_after, config.day_start_hour);
        let due = data_overdue && in_window;
        out.push(EvaluatedReminder {
            id: r.id.clone(),
            display: r.display.clone(),
            interval_days: r.interval_days,
            last_done,
            days_since,
            due,
            not_before: r.not_before,
            not_after: r.not_after,
        });
    }
    Ok(EvaluationResult {
        reminders: out,
        warnings,
    })
}
```

Also update the doc comment immediately above (around line 378):

```rust
/// Evaluate reminders against the current DB state, computing `last_done`
/// per watched source and applying the per-reminder time gate. `today` is
/// the effective today date (callers should pass `config.effective_today_date()`);
/// `now` is the current wall-clock time (callers should pass
/// `chrono::Local::now().time()`).
```

- [ ] **Step 3: Update the `today_cmd::execute` callsite**

In `src/cli/today_cmd.rs`, around line 94, the current call is:

```rust
        crate::reminders::evaluate(&conn, date, &reminders_defs, config)?
```

Change it to:

```rust
        crate::reminders::evaluate(
            &conn,
            date,
            chrono::Local::now().time(),
            &reminders_defs,
            config,
        )?
```

- [ ] **Step 4: Update the `today_cmd` in-file test (around line 1545)**

The existing test `execute_text_prepends_reminders_block_when_due` (or similar — locate via `grep -n "crate::reminders::evaluate" src/cli/today_cmd.rs`) calls `evaluate(&conn, date, &reminders, &config)`. Change to:

```rust
        let eval = crate::reminders::evaluate(
            &conn,
            date,
            chrono::NaiveTime::from_hms_opt(12, 0, 0).unwrap(),
            &reminders,
            &config,
        )
        .unwrap();
```

(Noon is well inside any reasonable window, so existing tests stay green.)

- [ ] **Step 5: Update the `status_cmd::assemble_status` callsite**

In `src/cli/status_cmd.rs`, around line 77:

```rust
        crate::reminders::evaluate(conn, config.effective_today_date(), &reminders_defs, config)?
```

becomes:

```rust
        crate::reminders::evaluate(
            conn,
            config.effective_today_date(),
            chrono::Local::now().time(),
            &reminders_defs,
            config,
        )?
```

- [ ] **Step 6: Update the integration tests in `tests/reminders.rs`**

There are 4 existing tests, each containing a call like:

```rust
    let eval = evaluate(&conn, date, &reminders, &config).unwrap();
```

Locate them via `grep -n "evaluate(" tests/reminders.rs`. Each becomes:

```rust
    let eval = evaluate(
        &conn,
        date,
        chrono::NaiveTime::from_hms_opt(12, 0, 0).unwrap(),
        &reminders,
        &config,
    )
    .unwrap();
```

Add the import if missing — at the top of `tests/reminders.rs`, change:

```rust
use chrono::NaiveDate;
```

to:

```rust
use chrono::{NaiveDate, NaiveTime};
```

(then you can write `NaiveTime::from_hms_opt(...)` without the `chrono::` prefix).

- [ ] **Step 7: Update existing `reminders::tests` calls to `evaluate`**

Inside `src/reminders.rs::tests`, every existing call to `evaluate(...)` takes 4 arguments. They each need a `now` time inserted as the third argument.

Locate via:

```bash
grep -n "evaluate(&conn" src/reminders.rs
```

Each such call (there are ~20) becomes one of the noon-style forms. Since the test module already imports `super::*`, you can write `noon()` directly (the helper added in Step 1). For consistency, use `noon()` wherever the test isn't specifically checking time-gate behavior; tests added in Step 1 use specific times.

For example:

```rust
        let result = evaluate(
            &conn,
            today,
            &[metric_reminder("la", 2, "la_min")],
            &empty_config(),
        )
        .unwrap();
```

becomes:

```rust
        let result = evaluate(
            &conn,
            today,
            noon(),
            &[metric_reminder("la", 2, "la_min")],
            &empty_config(),
        )
        .unwrap();
```

This is mechanical — one line inserted per call site.

- [ ] **Step 8: Run tests**

Run: `cargo test --lib reminders::tests`
Expected: all tests pass — existing ones still pass (noon is in-window for any sensible gate, and tests without gates still default to no gate), new ones from Step 1 also pass.

Run: `cargo test`
Expected: full suite clean.

Run: `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 9: Commit**

```bash
git add src/reminders.rs src/cli/today_cmd.rs src/cli/status_cmd.rs tests/reminders.rs
git commit -m "$(cat <<'EOF'
feat(reminders): apply time gate inside `evaluate`

`evaluate` now takes a `now: NaiveTime` parameter (callers pass
`chrono::Local::now().time()`); the `due` formula gates on both
data-overdue AND being inside the time window. `not_before` and
`not_after` propagate from Reminder into EvaluatedReminder so
downstream JSON consumers can see them.

All existing call sites in today_cmd, status_cmd, and the
integration tests are updated. Existing unit tests use a noon
default (always in-window) so their assertions stay valid;
new tests exercise before/in/after gate boundaries plus the
"gate trumps never-logged" case.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Emit `not_before` / `not_after` in JSON

**Files:**
- Modify: `src/reminders.rs`
- Modify: `src/cli/today_cmd.rs`
- Modify: `src/cli/status_cmd.rs`

Goal: extend `reminders::to_json` so every per-reminder JSON object includes `not_before` and `not_after` (string `"HH:MM"` or `null`). Update the JSON-shape tests in both `today_cmd::tests` and `status_cmd::tests`.

- [ ] **Step 1: Add failing tests**

In `src/cli/today_cmd.rs::tests`, append:

```rust
    #[test]
    fn render_json_includes_not_before_and_not_after() {
        let s = fixture_summary();
        let g = fixture_goals();
        let rs = vec![EvaluatedReminder {
            id: "evening".into(),
            display: "Evening".into(),
            interval_days: 1,
            last_done: None,
            days_since: None,
            due: false,
            not_before: Some(chrono::NaiveTime::from_hms_opt(18, 0, 0).unwrap()),
            not_after: Some(chrono::NaiveTime::from_hms_opt(23, 0, 0).unwrap()),
        }];
        let v = render_json_with_reminders(&s, &g, &rs, &[]);
        let r = &v["reminders"][0];
        assert_eq!(r["not_before"], "18:00");
        assert_eq!(r["not_after"], "23:00");
    }

    #[test]
    fn render_json_omits_time_gates_as_null_when_unset() {
        let s = fixture_summary();
        let g = fixture_goals();
        let rs = vec![EvaluatedReminder {
            id: "all_day".into(),
            display: "All day".into(),
            interval_days: 1,
            last_done: None,
            days_since: None,
            due: true,
            not_before: None,
            not_after: None,
        }];
        let v = render_json_with_reminders(&s, &g, &rs, &[]);
        let r = &v["reminders"][0];
        assert!(r["not_before"].is_null());
        assert!(r["not_after"].is_null());
    }
```

In `src/cli/status_cmd.rs::tests`, append:

```rust
    #[test]
    fn status_json_includes_time_gates_for_each_reminder() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in(
            dir.path(),
            r#"
[reminders.evening]
display = "Evening"
interval_days = 1
watch = "metric"
target = "la_min"
not_before = "18:00"
not_after = "23:00"
"#,
        );
        let v = build_status_json(&config).unwrap();
        let r = &v["reminders"][0];
        assert_eq!(r["not_before"], "18:00");
        assert_eq!(r["not_after"], "23:00");
    }
```

- [ ] **Step 2: Run and confirm they fail**

Run: `cargo test --lib today_cmd::tests::render_json_includes_not_before_and_not_after`
Run: `cargo test --lib status_cmd::tests::status_json_includes_time_gates_for_each_reminder`
Expected: both fail. The fields aren't in the JSON yet.

- [ ] **Step 3: Update `reminders::to_json`**

In `src/reminders.rs`, locate `to_json` (around line 495). Its current map step looks like:

```rust
    let rs: Vec<serde_json::Value> = reminders
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "display": r.display,
                "interval_days": r.interval_days,
                "last_done": r.last_done.map(|d| d.format("%Y-%m-%d").to_string()),
                "days_since": r.days_since,
                "due": r.due,
            })
        })
        .collect();
```

Add two new keys at the end:

```rust
    let rs: Vec<serde_json::Value> = reminders
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "display": r.display,
                "interval_days": r.interval_days,
                "last_done": r.last_done.map(|d| d.format("%Y-%m-%d").to_string()),
                "days_since": r.days_since,
                "due": r.due,
                "not_before": r.not_before.map(|t| t.format("%H:%M").to_string()),
                "not_after": r.not_after.map(|t| t.format("%H:%M").to_string()),
            })
        })
        .collect();
```

- [ ] **Step 4: Run tests**

Run: `cargo test --lib today_cmd::tests::render_json`
Run: `cargo test --lib status_cmd::tests::status_json`
Run: `cargo test`
Expected: full suite clean. Both new tests now pass.

Run: `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add src/reminders.rs src/cli/today_cmd.rs src/cli/status_cmd.rs
git commit -m "$(cat <<'EOF'
feat(reminders): expose not_before / not_after in JSON output

Both fields render as "HH:MM" strings (or null when unset) in
the `reminders` array of `vitalog today --json` and
`vitalog status`. Shape is always present so consumers can
rely on a stable schema.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Integration test + docs

**Files:**
- Modify: `tests/reminders.rs`
- Modify: `presets/default.toml`
- Modify: `README.md`

Goal: one new end-to-end test that exercises the gate path through `today_cmd`. Refresh user-facing docs.

- [ ] **Step 1: Add the new integration test**

In `tests/reminders.rs`, append a new test:

```rust
#[test]
fn today_text_silent_before_gate_visible_inside_window() {
    let (dir, config) = setup_with_reminders(
        r#"
[reminders.brush_evening]
display = "Brush teeth (evening)"
interval_days = 1
watch = "metric"
target = "la_min"
not_before = "18:00"
not_after = "23:00"
"#,
    );

    // Logged 3 days ago — data-overdue.
    write_note(
        dir.path(),
        "2026-05-09",
        "---\ndate: 2026-05-09\nla_min: 1\n---\n\n## Food\n",
    );

    let registry = modules::build_registry(&config);
    let conn = db::open_rw(&config.db_path()).unwrap();
    db::init_db(&conn, &registry).unwrap();
    modules::validate_module_tables(&registry).unwrap();
    vitalog::materializer::sync_all(&conn, &config.notes_dir_path(), &config, &registry).unwrap();

    let date = NaiveDate::from_ymd_opt(2026, 5, 12).unwrap();
    let reminders = load_reminders(&config).unwrap();

    // Before window — 10:00 wall-clock.
    let eval_before = evaluate(
        &conn,
        date,
        NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
        &reminders,
        &config,
    )
    .unwrap();
    let block_before = render_reminders_block(&eval_before.reminders, false);
    assert!(
        block_before.is_empty(),
        "block should be silent before gate; got:\n{block_before}"
    );

    // Inside window — 19:00 wall-clock.
    let eval_in = evaluate(
        &conn,
        date,
        NaiveTime::from_hms_opt(19, 0, 0).unwrap(),
        &reminders,
        &config,
    )
    .unwrap();
    let block_in = render_reminders_block(&eval_in.reminders, false);
    assert!(
        block_in.contains("Brush teeth (evening)"),
        "block should show inside gate; got:\n{block_in}"
    );
}
```

- [ ] **Step 2: Run the integration test**

Run: `cargo test --test reminders`
Expected: 5/5 tests pass (4 existing + 1 new).

- [ ] **Step 3: Extend the commented `[reminders]` example in `presets/default.toml`**

Open `presets/default.toml`. The existing commented `[reminders]` block (added during v1) lists 5 examples. Find the `[reminders.brush_morning]`-style guidance text or the first example block. After the existing comment block explaining `interval_days = N`, insert a new paragraph (still as TOML comments) describing the time gates:

```toml
#
# Time-of-day gates (optional, both independently optional):
#   - not_before = "HH:MM" — earliest wall-clock time the reminder may fire
#   - not_after  = "HH:MM" — latest wall-clock time it may fire
# When set, the reminder only counts as "due" inside the [not_before,
# not_after] window — useful for evening-task reminders that shouldn't
# nag at breakfast. Both fields are 24-hour. Overnight wrap-around is
# rejected; for overnight reminders, split into two reminders on the
# same metric, one with not_after, the other with not_before.
```

Then add two new example reminders to the existing commented examples:

```toml
#
# [reminders.brush_evening]
# display       = "Brush teeth (evening)"
# interval_days = 1
# not_before    = "18:00"
# not_after     = "23:00"
# watch         = "metric"
# target        = "brushed_evening"
#
# [reminders.brush_morning]
# display       = "Brush teeth (morning)"
# interval_days = 1
# not_before    = "07:00"
# not_after     = "12:00"
# watch         = "metric"
# target        = "brushed_morning"
```

(Place these near the existing `[reminders.weigh_in]` example so the brushing pair is visible together.)

- [ ] **Step 4: Extend the README**

In `README.md`, locate the `## Reminders` section (added in v1). Find the four-bullet list explaining the watch kinds; immediately after it, before the paragraph that says "A reminder fires when the most recent matching date is either absent…", insert a new paragraph:

```markdown
**Time-of-day gates.** Each reminder accepts optional `not_before` and `not_after` fields (24-hour `"HH:MM"`). When set, the reminder only counts as due inside the `[not_before, not_after]` window — so an evening-task reminder doesn't nag at breakfast. Both bounds are independently optional. For overnight reminders, split into two reminders on the same metric (one with `not_after`, the other with `not_before`) — explicit wrap-around windows are rejected at config-load.

```toml
[reminders.brush_evening]
display       = "Brush teeth (evening)"
interval_days = 1
not_before    = "18:00"
not_after     = "23:00"
watch         = "metric"
target        = "brushed_evening"
```
```

- [ ] **Step 5: Verify the README still renders cleanly via `vitalog readme`**

Run: `cargo run --quiet -- readme 2>/dev/null | grep -A 3 "Time-of-day gates"`
Expected: the new paragraph appears in the embedded README output.

- [ ] **Step 6: Run the full suite + lint one more time**

Run: `cargo test`
Expected: clean.

Run: `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add tests/reminders.rs presets/default.toml README.md
git commit -m "$(cat <<'EOF'
test+docs(reminders): time-gate integration test + user-facing docs

Adds an end-to-end test confirming the gate makes the today
block silent before its window and visible once `now` is inside.
Extends the README's Reminders section with a "Time-of-day
gates" paragraph and a worked example. Updates the commented
[reminders] block in presets/default.toml with the new fields
and two brushing-style example reminders.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Self-Review Checklist (run after completing all tasks)

- [ ] `cargo test` end-to-end on the final commit — all tests pass.
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` clean.
- [ ] `git log --oneline` reads as 6 coherent commits (Task 1–6).
- [ ] `cargo run -- today --help` still works (no CLI surface change).
- [ ] `cargo run -- status | jq '.reminders[0].not_before'` returns either a `"HH:MM"` string or `null` (run only if you have a `[reminders]` block in your real config).
- [ ] A config with `[reminders.X]` blocks but no `not_before` / `not_after` still works — behavior identical to v1.
- [ ] A config with `not_before = "22:00", not_after = "06:00"` (default `day_start_hour = 0`) fails `Config::load()` with the expected error message and `.suggestion()` mentioning the split-into-two-reminders workaround.
