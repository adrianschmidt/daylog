# Time gates on reminders (`not_before` / `not_after`)

## Background

The v1 reminders feature ([2026-05-12 design](2026-05-12-reminders-design.md))
evaluates "due" purely on calendar-day arithmetic: a reminder is due as soon
as the last matching event is `interval_days` or more days in the past, and
stays due all day until logged. That works for once-per-day habits with no
clock-time component ("do lactic-acid intervals every other day — at some
point") but doesn't fit reminders that are bound to a time of day.

The motivating case: two metrics for brushing teeth — `brushed_morning` and
`brushed_evening` — each with its own reminder. Under v1, the evening
reminder is "due" from midnight onward, so the morning view of `vitalog
today` would already be nagging about an evening task that isn't relevant
for another twelve hours. With per-reminder push notifications coming next,
this becomes louder: every check between 09:00 and 17:00 would ping about
evening tasks.

This spec adds two optional time-of-day fields per reminder — `not_before`
and `not_after` — that gate when an otherwise-data-overdue reminder is
treated as due. Gate always applies (a missed yesterday-evening brush stays
silent until tonight's window opens, rather than nagging at breakfast).

## Goals (in scope)

- Two optional fields on each `[reminders.X]` block:
  - `not_before = "HH:MM"` — earliest wall-clock time the reminder may
    fire on a given effective day.
  - `not_after = "HH:MM"` — latest wall-clock time it may fire.
- A reminder is **due** iff it is **data-overdue** (the existing
  `last_done` check) AND **inside the time window**. Either condition
  alone is not enough.
- The gate **always applies**, even to reminders that are days overdue —
  a missed evening brush from yesterday does not fire at breakfast the
  next morning; it waits for tonight's window.
- Either bound is independently optional. `not_before` alone gives a
  window `[not_before, end_of_effective_day]`; `not_after` alone gives
  `[start_of_effective_day, not_after]`; both unset preserves the v1
  behaviour (no gate, due all day).
- `Config::validate` rejects configs where `not_after` falls earlier
  than `not_before` within the effective day (after applying
  `day_start_hour`), with a `.suggestion()` pointing to the
  split-into-two-reminders workaround for overnight cases.
- `not_before` and `not_after` surface in `today --json` and
  `vitalog status` JSON alongside the existing per-reminder fields,
  serialized as `"HH:MM"` strings or `null`.
- Backwards compatible: any v1 config keeps working unchanged. Fields
  default to `None`. No DB migration.

## Non-goals

- Overnight wrap-around windows (`not_before = "22:00", not_after = "06:00"`
  meaning "active overnight"). The workaround is two reminders on the same
  metric, one with a `not_after` and one with a `not_before`. Keeps
  semantics unambiguous, especially when combined with `day_start_hour`.
- Day-of-week filters (e.g. "only Mondays"). YAGNI for the motivating use
  cases; can be added later as `weekdays = ["mon", "wed", "fri"]` without
  changing the time-gate logic.
- Second precision. Minute precision is sufficient — the gate is a UX
  fence, not a scheduling primitive.
- Snooze, "I'll do it later", or any other per-reminder runtime state.
  Same stance as v1: the watched event *is* the marker.
- Changes to the text rendering of the "Reminders" block. Format stays
  the same; the gate only affects *which* reminders pass through to be
  rendered.
- Per-reminder timezones. The gate is local wall-clock time, matching
  the rest of vitalog.

## Decisions

- **Field names:** `not_before` and `not_after`. Symmetric, declarative,
  matches the negative phrasing of "do not fire". Considered and
  rejected: `start_time` / `end_time` (sounds like the reminder is a
  scheduled event), `active_from` / `active_until` (verbose), `window`
  (would need a sub-table, more typing).
- **Format:** `"HH:MM"` 24-hour. Parsed via
  `NaiveTime::parse_from_str(s, "%H:%M")`. 12-hour-format support is
  not added in v1.1; users with `time_format = "12h"` in config still
  declare reminders in 24-hour form. (Worth revisiting if it bites; the
  config file already has a mix of representations and 24h is the more
  precise one.)
- **Storage:** `Option<NaiveTime>` on both `Reminder` and
  `EvaluatedReminder`. `ReminderConfig` keeps `Option<String>` and
  parses on the way in (consistent with how `target` is parsed).
- **Evaluator signature gains a `now: NaiveTime` parameter** so tests
  can pin a wall-clock moment instead of relying on `Local::now()`.
  Callers pass `Local::now().time()`.
- **`due` formula:** `data_overdue && in_window`. Both must be true.
  The "data-overdue" check is unchanged from v1.
- **Wrap-around with `day_start_hour`:** the time-window check uses
  *effective-day offsets* (minutes since `day_start_hour`) rather than
  raw wall-clock minutes. With `day_start_hour = 4` and
  `not_before = "22:00"`, the offset is `(22-4) * 60 = 1080` minutes
  into the effective day; the window correctly stays open from 22:00
  wall-clock until 03:59 of the next wall-clock day (the end of that
  effective day). For the default `day_start_hour = 0`, the offset
  math reduces to plain wall-clock comparison.
- **Validation uses the same offset comparison.** With
  `day_start_hour = 4, not_before = "22:00", not_after = "02:00"`,
  the offsets are 1080 → 1320 — accepted; the window spans midnight
  in wall-clock but is contiguous within the effective day. With
  `day_start_hour = 0, not_before = "22:00", not_after = "06:00"`,
  the offsets are 1320 → 360 — rejected; the window would cross an
  effective-day boundary. With
  `day_start_hour = 4, not_before = "02:00", not_after = "06:00"`,
  the offsets are 1320 → 120 — also rejected; the user is trying to
  span from the late part of one effective day to the early part of
  the next.
- **Equal bounds permitted:** `not_before = not_after` is a zero-duration
  window. Silly but not invalid; no special-casing.
- **No new validation locus:** `load_reminders` (which already runs from
  `Config::validate` since the v1 follow-up) gets the new checks. No
  changes to where validation lives.
- **JSON serialization:** `Option<NaiveTime>` → `"HH:MM"` string or
  `null`. Always present in the per-reminder object so consumers can
  rely on a stable shape.
- **Text rendering unchanged.** Only `due == true` reminders render in
  the `⏰ Reminders` block. Once the time gate trims the "due" set,
  silent blocks become more common — which is the point.

## Architecture

### Data flow

```
config.toml
    └── [reminders.X] adds not_before / not_after
         └── load_reminders(&Config)
              └── parses HH:MM, validates window, returns Vec<Reminder>

vitalog today / vitalog status
    └── Local::now().time() → now: NaiveTime
    └── reminders::evaluate(conn, today, now, &reminders, &config)
         └── data_overdue from last_done vs interval_days (unchanged)
         └── in_window from now vs (not_before, not_after, day_start_hour)
         └── due = data_overdue && in_window
    └── today: render_reminders_block filters due == true (unchanged)
    └── JSON: reminders::to_json adds not_before / not_after fields
```

### Types (delta from v1)

```rust
pub struct Reminder {
    pub id: String,
    pub display: String,
    pub interval_days: u32,
    pub watch: WatchSource,
    pub not_before: Option<NaiveTime>,   // new
    pub not_after: Option<NaiveTime>,    // new
}

pub struct EvaluatedReminder {
    pub id: String,
    pub display: String,
    pub interval_days: u32,
    pub last_done: Option<NaiveDate>,
    pub days_since: Option<i64>,
    pub due: bool,
    pub not_before: Option<NaiveTime>,   // new (echoed for JSON consumers)
    pub not_after: Option<NaiveTime>,    // new
}

// ReminderConfig (in src/config.rs) gains two optional String fields:
//   pub not_before: Option<String>,
//   pub not_after: Option<String>,
// parsed in load_reminders.
```

### Time-window helper

```rust
/// Returns true if `now` falls inside the effective-day window defined by
/// `not_before` and `not_after`. Either bound being `None` means "no
/// limit on that side". When `day_start_hour > 0`, comparisons use
/// effective-day offsets so a `not_before = "22:00"` with
/// `day_start_hour = 4` correctly stays open from 22:00 wall-clock
/// through 03:59 the next wall-clock day.
fn within_time_window(
    now: NaiveTime,
    not_before: Option<NaiveTime>,
    not_after: Option<NaiveTime>,
    day_start_hour: u8,
) -> bool {
    fn offset_minutes(t: NaiveTime, day_start_hour: u8) -> u32 {
        let t_mins = (t.hour() * 60 + t.minute()) as u32;
        let start_mins = (day_start_hour as u32) * 60;
        if t_mins >= start_mins {
            t_mins - start_mins
        } else {
            t_mins + 24 * 60 - start_mins
        }
    }
    let now_off = offset_minutes(now, day_start_hour);
    let after_lower = not_before.map_or(true, |nb| now_off >= offset_minutes(nb, day_start_hour));
    let before_upper = not_after.map_or(true, |na| now_off <= offset_minutes(na, day_start_hour));
    after_lower && before_upper
}
```

The helper is `fn`, not `pub fn` — it stays internal to `reminders.rs`.
Tests in `mod tests` reach it via `super::within_time_window`.

### `evaluate` signature change

```rust
pub fn evaluate(
    conn: &Connection,
    today: NaiveDate,
    now: NaiveTime,                       // new parameter
    reminders: &[Reminder],
    config: &Config,
) -> Result<EvaluationResult>
```

Callers (`today_cmd::execute`, `status_cmd::assemble_status`) update to
pass `chrono::Local::now().time()`. Both callsites already construct a
`NaiveDate` for `today`; adding the time argument is a one-line edit
each. Integration tests in `tests/reminders.rs` and unit tests in
`reminders::tests` pass synthetic times so behaviour is deterministic.

This is a breaking signature change for any external caller of
`evaluate`, but there are none in v1: the function is only invoked
from inside this crate.

### Due-logic update

```rust
let data_overdue = match days_since {
    None => true,
    Some(n) => n >= r.interval_days as i64,
};
let in_window = within_time_window(now, r.not_before, r.not_after, config.day_start_hour);
let due = data_overdue && in_window;
```

The existing unknown-metric warning gating (only emit when
`last_done.is_none()`) is unchanged. The time gate doesn't suppress
warnings — a typo'd metric target still surfaces regardless of when
in the day you run `today`.

### Config schema (TOML)

```toml
[reminders.brush_evening]
display       = "Brush teeth (evening)"
interval_days = 1
not_before    = "18:00"
not_after     = "23:00"
watch         = "metric"
target        = "brushed_evening"

[reminders.brush_morning]
display       = "Brush teeth (morning)"
interval_days = 1
not_before    = "07:00"
not_after     = "12:00"
watch         = "metric"
target        = "brushed_morning"

[reminders.lactic_acid]
display       = "Lactic acid training"
interval_days = 2
not_before    = "10:00"          # lower bound only — stays due from 10:00 until logged
watch         = "metric"
target        = "la_min"
```

### JSON output (delta from v1)

Each `reminders[]` entry gains two fields:

```json
{
  "id": "brush_evening",
  "display": "Brush teeth (evening)",
  "interval_days": 1,
  "last_done": "2026-05-11",
  "days_since": 1,
  "due": false,
  "not_before": "18:00",
  "not_after": "23:00"
}
```

Both always present (possibly `null`). Implemented in
`reminders::to_json` so both `today --json` and `status` pick the
change up automatically.

### Validation (in `load_reminders`)

For each reminder:

1. Parse `not_before` / `not_after` strings to `NaiveTime` via
   `NaiveTime::parse_from_str(s, "%H:%M")`. Unparseable → error with
   `.suggestion("Use HH:MM in 24-hour form, e.g., \"18:00\".")`.
2. If both bounds are set, compute effective-day offsets using
   `config.day_start_hour`. If `nb_offset > na_offset` → error
   `"reminder \`<id>\`: not_after must not be earlier than not_before within the effective day (with day_start_hour = N)"`
   plus a `.suggestion` mentioning the split-into-two-reminders
   workaround.
3. Equal bounds are allowed (zero-length window; pointless but
   not invalid).

### Files touched

- `src/reminders.rs` — types, parsing, `within_time_window`, evaluator,
  `to_json`, tests.
- `src/config.rs` — `ReminderConfig` gains `not_before` /
  `not_after: Option<String>`.
- `src/cli/today_cmd.rs` — pass `Local::now().time()` into `evaluate`;
  update existing tests that call `evaluate` to supply a `now`.
- `src/cli/status_cmd.rs` — same `now` plumbing in `assemble_status`.
- `tests/reminders.rs` — one new integration test for the gate path.
- `presets/default.toml` — extend the commented `[reminders]` example
  with `not_before` / `not_after` lines on the brush_morning entry.
- `README.md` — short note in the existing Reminders section.
- `CLAUDE.md` — no change needed; the file map already lists
  `reminders.rs`.

## Testing

Unit tests in `src/reminders.rs`:

- **`within_time_window`** (pure):
  - No gates → always true.
  - Lower-only: at `now < nb` → false; at `now == nb` → true; at `now > nb` → true.
  - Upper-only: mirror.
  - Both: `now` inside, on boundary, before, after.
  - `day_start_hour = 0`: baseline wall-clock semantics.
  - `day_start_hour = 4, not_before = "22:00"`: gate open at 22:00 wall-clock
    same calendar day; gate still open at 01:00 next wall-clock day (offset 1260
    minutes ≥ 1080); gate closed at 04:00 next wall-clock day (effective day
    has rolled, offset becomes 0).
  - `day_start_hour = 4, not_before = "02:00"`: gate open at 02:30 wall-clock
    (offset 1350 ≥ 1320, same effective day as 04:00 the previous wall-day).

- **`evaluate` with gates:**
  - Data-overdue + inside window → `due = true`.
  - Data-overdue + before window → `due = false`.
  - Data-overdue + after window → `due = false`.
  - Not data-overdue + inside window → `due = false` (gate alone doesn't make it due).
  - `last_done = None` + outside window → `due = false` (gate trumps "never logged").
  - Existing v1 evaluator tests continue to pass after the signature change
    (they pass an `in-window` `now` so behaviour is unchanged).

- **`load_reminders` parsing:**
  - Both bounds parse cleanly.
  - Single bound parses cleanly.
  - Unparseable `"25:00"` → error mentions field + suggestion.
  - `day_start_hour = 0, not_before = "22:00", not_after = "06:00"` → reject.
  - `day_start_hour = 4, not_before = "22:00", not_after = "02:00"` → accept
    (offsets 1080 → 1320; window stays within the effective day even
    though it crosses midnight in wall-clock).
  - `day_start_hour = 4, not_before = "02:00", not_after = "06:00"` → reject
    (offsets 1320 → 120; the window would cross an effective-day boundary —
    `02:00` falls in the late part of effective day D and `06:00` in the
    early part of effective day D+1).
  - Backwards-compatibility: a config without these fields still parses
    into a `Reminder` with `not_before = None, not_after = None`.

- **`to_json` shape:**
  - Both bounds set → both fields render as `"HH:MM"`.
  - Both unset → both fields render as `null`.
  - Mixed → one string, one null.

Integration test in `tests/reminders.rs`:

- A config with `[reminders.brush_evening]` and `not_before = "18:00"`,
  no logs in the DB. Evaluate at `now = 10:00` → block empty; evaluate at
  `now = 19:00` → block contains "Brush teeth (evening)". Confirms the
  full `today_cmd::execute` wiring including the new `now` plumbing.

Existing tests in `tests/reminders.rs` and `today_cmd::tests` get their
`evaluate` calls updated to pass `Local::now().time()` or a fixed
in-window time. No behavioural changes for those tests.

## Backwards compatibility

- `not_before` / `not_after` are optional. A v1 config (no time gates)
  produces identical behaviour to v1.
- No DB schema changes. No migration required.
- The `evaluate` signature change is internal to the crate. External
  consumers (none in v1) would need to add the `now` argument; this is
  the same kind of breaking-but-mechanical change that the rest of
  vitalog's library API allows during normal development.

## Open questions

None that block implementation. A few decisions explicitly punted:

- 12-hour-format input in TOML if `time_format = "12h"` is set in
  config. Reasonable to add later; v1.1 ships with 24-hour-only.
- A future `next_due_at` field on `EvaluatedReminder` (the wall-clock
  moment the gate will next open). Useful for a smart notifier that
  wants to sleep until the next due moment instead of polling every
  N minutes. Out of scope here; the v1.1 JSON already lets a notifier
  compute that itself from `not_before`.
- A `--list-reminders` subcommand. Punted to a future spec.
