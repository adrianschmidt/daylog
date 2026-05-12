# Smart reminders on `vitalog today` and `vitalog status`

## Background

Some health habits are bound to a rhythm rather than a clock — "do 15 minutes
of lactic-acid intervals every other day." A phone reminder set to repeat
every 48 hours drifts out of phase as soon as you skip one: the next ping
arrives at a time unrelated to when you last actually trained.

What works better is a reminder that looks at *the data you already log* and
asks: "have I done this in the last N days?" If yes, stay quiet. If no,
surface a nudge the next time I open my daily summary.

Vitalog already aggregates exactly that data — training sessions, lifts,
custom metrics, and built-in day fields like weight and sleep all flow into
SQLite and are queried by `vitalog today` and `vitalog status`. This spec
adds a reminders layer on top, defined in `config.toml`, evaluated on
every `today`/`status` invocation, and rendered both as a human block at the
top of `vitalog today` and as a structured array in JSON for downstream
consumers (e.g. a future push-notification wrapper).

## Goals (in scope)

- A `[reminders]` table in `config.toml`, alongside `[metrics]` and
  `[exercises]`, declaring named reminders with an interval and a watch
  target.
- Four watch kinds covering the existing DB tables:
  - `metric` — any custom metric in `[metrics]`.
  - `session` — any row in the `sessions` table matching a text-equals or
    numeric-min-value predicate over a whitelisted column.
  - `lift` — any row in `lift_sets` for a given exercise, with optional
    `min_weight` and `min_reps` filters.
  - `day_field` — any non-null value of a built-in `days` column
    (`weight`, `sleep_hours`, `mood`, `energy`, `sleep_start`, `sleep_end`).
- Calendar-day evaluation: a reminder is due iff the most recent matching
  date is either absent or at least `interval_days` calendar days before
  the effective today (respecting `day_start_hour`).
- Rendering:
  - `vitalog today` text: a compact "Reminders" block prepended above the
    date header **only when at least one reminder is due**. Silent on a
    clean day.
  - `vitalog today --json` and `vitalog status` JSON: a `reminders` array
    listing **every** configured reminder with `last_done`, `days_since`,
    `due`, and config echo (id, display, interval_days).
- Hot reload: reminders are re-read from `config.toml` on every invocation
  (no daemon-side state).
- A clear "unknown target" path: a reminder whose target metric was
  deleted from `[metrics]` still parses, evaluates as "never logged →
  due", and surfaces a one-line warning analogous to the existing
  "unknown metric in goals.md" warning.

## Non-goals

- A `vitalog reminders` subcommand for v1. Editing `config.toml` plus
  running `vitalog today` is enough. The evaluator lives in a module
  that a future subcommand can call without redesign.
- Push notifications. Out of scope; a follow-up spec will wrap the v1
  JSON output. See "Push notifications (future)" below for the
  architectural shape the v1 design is preserving.
- Snooze, mark-done-from-CLI, or any reminder-side state. The watched
  event *is* the marker — to silence a reminder, log the activity.
- Watching frontmatter-only data (BP morning sys/dia/pulse) or
  notes-aliases. Both live outside the DB tables the watch kinds cover.
  Plausible v1.1 extensions; not in v1.
- Time-of-day scheduling, weekday filters, cron-style expressions. The
  "every N calendar days" model handles the motivating use case
  directly. Add scheduling axes later only if a real need appears.
- TUI integration. Reminders are CLI-only in v1. Adding a TUI panel
  later is purely additive.

## Decisions

- **Definition location:** `[reminders.<id>]` blocks inside the existing
  `~/.config/vitalog/config.toml`. Matches `[metrics]` and `[exercises]`,
  which are also config-table-driven extensions.
- **Schedule model:** a single `interval_days` integer (≥ 1). A reminder
  is due iff `last_done` is `None`, or
  `(effective_today − last_done) ≥ interval_days` measured in calendar
  days. Worked example: `interval_days = 2`, last logged on Monday at
  23:00 → on Wednesday the gap is 2 → due from the moment Wednesday
  begins (modulo `day_start_hour`), all day, until logged again.
  Logging on the same day a reminder fires silences it for that day.
- **Watch kinds and their predicates:** four kinds, each with a small
  set of structured fields. The Rust enum is closed; adding a fifth
  kind is a code change, not a config change.
  - `metric`: `target = "<metric_id>"`. Treats `value > 0` as "logged"
    by default; an opt-in `count_zero_as_logged = true` flips this for
    metrics where 0 is a real reading.
  - `session`: `target = { field = "<col>", equals = "..." }` for text
    columns or `target = { field = "<col>", min_value = N }` for
    numeric columns. Whitelisted columns are exactly the non-PK
    columns of the `sessions` schema: text — `type`, `block`,
    `vo2_intervals`; numeric — `duration`, `rpe`, `zone2_min`,
    `hr_avg`, `week`. `min_value` is required (no default) — for
    "any non-null value", use `min_value = 1` explicitly, matching
    the convention that vitalog stores `0` only when the user
    actually logged a zero.
  - `lift`: `target = { exercise = "<name>", min_weight = N?, min_reps = N? }`.
    `exercise` is required; the optional filters narrow which
    `lift_sets` rows count. `min_weight` is in lbs (the column unit).
  - `day_field`: `target = "<col>"`, one of `weight`, `sleep_hours`,
    `mood`, `energy`, `sleep_start`, `sleep_end`. Any non-null value
    counts as "logged".
- **Column whitelist rationale:** vitalog already hardcodes column
  names in every existing query (`today_cmd::load_day_fields`, training
  module SQL, etc.); reflecting the schema at runtime would be
  inconsistent with the rest of the codebase. The whitelist is for
  friendly error messages on config typos, not as a security boundary
  — this is a single-user local CLI, the user owns the config, and
  there is no untrusted-input flow.
- **Date granularity:** vitalog stores all watched events at
  date-only granularity (`YYYY-MM-DD` columns; no time component).
  "Last done" is therefore a `NaiveDate`, and interval comparisons are
  calendar-day subtractions — never 24-hour windows. This is what
  makes "logged at 23:00 Monday → due all day Wednesday" fall out
  naturally.
- **Effective-today usage:** the evaluator takes the date as a
  parameter so callers pass `config.effective_today_date()`, ensuring
  `day_start_hour` is honoured consistently with the rest of `today`.
- **Output position (text):** prepended above the existing
  `"<date> — Daily summary"` header, separated by a blank line.
  Reasons: due reminders are the most actionable line in the output;
  putting them at the top respects how the user is scanning. The
  block is suppressed entirely when no reminder is due, so the
  silent-day case stays clean.
- **Output position (JSON):** a top-level `reminders` array in both
  `today --json` and `status`. Always present, possibly empty.
  Always includes *every* reminder (not just due ones) so a
  downstream notifier can compute its own policy without re-querying
  vitalog.
- **Colour handling:** the due block follows the same convention as
  the existing red "below min" annotations — ANSI on a TTY, plain
  when piped, `NO_COLOR` honoured. No new colour primitives.
- **Code location:** a new top-level `src/reminders.rs`, sibling to
  `goals.rs` and `state.rs`. Owns the `Reminder` types, config
  parsing, and the evaluator. Both `today_cmd` and `cmd_status`
  depend on it directly. Promoting to a `src/reminders/` module is
  not needed until the file grows.
- **No new DB schema:** reminders query existing tables. No
  `reminders` table, no migration.

## Architecture

### Data flow

```
config.toml (hot-reloaded each invocation)
    └── [reminders.X] blocks
         └── parsed into Vec<Reminder> by reminders::load(...)

vitalog today / vitalog status
    └── existing materializer::sync_all (today only)
    └── reminders::evaluate(&conn, effective_today, &reminders)
         └── one MAX(date) query per reminder, by watch kind
         └── returns Vec<EvaluatedReminder>
    └── today: render_text prepends due block; render_json includes all
    └── status: JSON includes all under "reminders"
```

### Types (sketch)

```rust
// src/reminders.rs

pub struct Reminder {
    pub id: String,
    pub display: String,
    pub interval_days: u32,
    pub watch: WatchSource,
}

pub enum WatchSource {
    Metric { id: String, count_zero_as_logged: bool },
    Session(SessionMatch),
    Lift { exercise: String, min_weight: Option<f64>, min_reps: Option<u32> },
    DayField(DayColumn),
}

pub enum SessionMatch {
    TextEquals { column: SessionTextColumn, value: String },
    NumericAtLeast { column: SessionNumColumn, min: f64 },
}

pub enum SessionTextColumn { Type, Block, VoTwoIntervals }
pub enum SessionNumColumn  { Duration, Rpe, ZoneTwoMin, HrAvg, Week }
pub enum DayColumn          { Weight, SleepHours, Mood, Energy,
                              SleepStart, SleepEnd }

pub struct EvaluatedReminder {
    pub id: String,
    pub display: String,
    pub interval_days: u32,
    pub last_done: Option<NaiveDate>,
    pub days_since: Option<i64>,
    pub due: bool,
}

pub fn evaluate(
    conn: &Connection,
    today: NaiveDate,
    reminders: &[Reminder],
) -> Result<Vec<EvaluatedReminder>>;
```

The Rust enum sketch is illustrative; the exact field names are
implementation detail and the implementation plan can refine them.

### Query shapes

One prepared statement per watch kind, parameterised so the same
statement serves all reminders of that kind. Column names are
inserted via `format!` from the closed enum variants (never from
user-typed strings) so each variant maps to one fixed query.

| Watch kind                | Statement                                                                                  |
|---------------------------|--------------------------------------------------------------------------------------------|
| `metric`                  | `SELECT MAX(date) FROM metrics WHERE name = ?1 AND (value > 0 OR ?2 = 1)`                  |
| `session` text-equals     | `SELECT MAX(date) FROM sessions WHERE <col> = ?1`                                          |
| `session` numeric-at-least| `SELECT MAX(date) FROM sessions WHERE <col> IS NOT NULL AND <col> >= ?1`                   |
| `lift`                    | `SELECT MAX(date) FROM lift_sets WHERE exercise = ?1 [AND weight_lbs >= ?2] [AND reps >= ?3]` |
| `day_field`               | `SELECT MAX(date) FROM days WHERE <col> IS NOT NULL`                                       |

`<col>` is substituted from the matched enum variant. The `metric`
case uses `?2 = 1` as a SQL bool to inline the `count_zero_as_logged`
toggle without a second prepared statement.

### Config schema (TOML)

```toml
[reminders.lactic_acid]
display       = "Lactic acid training"
interval_days = 2
watch         = "metric"
target        = "la_min"

[reminders.zone2]
display       = "Zone 2 cardio"
interval_days = 3
watch         = "session"
target        = { field = "zone2_min", min_value = 1 }

[reminders.vo2_block]
display       = "VO2max intervals"
interval_days = 7
watch         = "session"
target        = { field = "type", equals = "vo2_max" }

[reminders.deadlifts]
display       = "Heavy deadlifts"
interval_days = 7
watch         = "lift"
target        = { exercise = "deadlift", min_weight = 200 }

[reminders.weigh_in]
display       = "Daily weigh-in"
interval_days = 1
watch         = "day_field"
target        = "weight"
```

### Output examples

**`vitalog today` text (with due reminders):**

```
⏰ Reminders
- Lactic acid training — overdue (last 3 days ago, 2026-05-09)
- Morning weigh-in — never logged

2026-05-12 — Daily summary

Calories: 1513 / 1900–2200 kcal     (387 below min)
…
```

**`vitalog today` text (no due reminders):** identical to today's
output, no extra lines.

**`vitalog today --json` / `vitalog status` JSON (excerpt):**

```json
"reminders": [
  {
    "id": "lactic_acid",
    "display": "Lactic acid training",
    "interval_days": 2,
    "last_done": "2026-05-09",
    "days_since": 3,
    "due": true
  },
  {
    "id": "weigh_in",
    "display": "Daily weigh-in",
    "interval_days": 1,
    "last_done": "2026-05-12",
    "days_since": 0,
    "due": false
  }
]
```

### Integration points (files touched)

- **New:** `src/reminders.rs`.
- **Modified:**
  - `src/config.rs` — add a `reminders: HashMap<String, ReminderConfig>`
    field on `Config` with `#[serde(default)]`, plus the
    `ReminderConfig` struct. Validation (interval ≥ 1, target shape
    matches kind, day-field/session-column membership) runs in
    `Config::validate`.
  - `src/lib.rs` — `pub mod reminders;`.
  - `src/cli/today_cmd.rs` — load reminders, call `evaluate`, prepend
    due block in `render_text`, add `reminders` array in
    `render_json`. The pre-read `sync_all` already runs before
    assembly, so the evaluator sees up-to-date data.
  - `src/main.rs::cmd_status` — load and evaluate reminders, add
    `reminders` array to the JSON. To keep reminders fresh after a
    just-logged event (the same situation that motivated the
    pre-read sync in `today_cmd`, issue #27), `cmd_status` will
    call `materializer::sync_all` before reading. This is a small
    behaviour change to `status` — it currently opens read-only
    and does not sync — but it is the right default given that
    the JSON is the contract a notifier will consume. Sync errors
    continue to be swallowed the same way `today_cmd` swallows
    them.
  - `CLAUDE.md` — add `reminders.rs` to the file map and mention the
    integration points on `today_cmd.rs` and `main.rs`.
  - `README.md` — short "Reminders" section with the TOML example
    and a paragraph on behaviour, mirroring the existing `[metrics]`
    and `[exercises]` docs.

### Unknown-target handling

A reminder whose target points at a metric/exercise that no longer
exists (or never existed) parses successfully and evaluates to
`last_done = None, due = true`. The evaluator also returns a list of
warnings (`Vec<String>`) that `today_cmd` appends to its existing
hint block — same visual treatment as "unknown metric in goals.md".
For JSON consumers, both `today --json` and `vitalog status` include
a sibling `reminder_warnings` array next to `reminders` (leaving
`today --json`'s existing `warnings` array alone for goals and
food-parse warnings — the two streams stay distinct so consumers
can route them separately).

### `vitalog --quiet`

`--quiet` is currently a per-command suppressor for the verbose
confirmation lines on logging commands; it does not affect `today`
or `status`. Reminders inherit that — the due block always prints on
`vitalog today`, regardless of `--quiet`. If a "silence reminders"
flag is wanted later, it can be added as `--no-reminders` on
`today`.

## Testing

Unit tests in `src/reminders.rs`, using an in-memory SQLite seeded
with rows tailored to each case:

- **Per watch kind**, for each of `metric` / `session text-equals` /
  `session numeric-at-least` / `lift` (with and without min filters) /
  `day_field`:
  - Never logged → `last_done = None`, `days_since = None`, `due = true`.
  - Logged today → `due = false`, `days_since = 0`.
  - Logged exactly `interval_days` calendar days before today → due.
  - Logged `interval_days - 1` calendar days before today → not due.
- **Calendar-day boundary:** with `interval_days = 2`, seed a metric
  row at `today - 2` and assert due. Reinforces that we compare on
  calendar days, even though the DB has no time component to confuse
  us — a regression test against a future schema change.
- **`count_zero_as_logged`:** default → a metric row of value 0 does
  not count; opt-in → it does.
- **Unknown target:** reminder pointing at a deleted metric or an
  exercise with no rows → `last_done = None, due = true`, warning
  emitted.
- **Multiple reminders:** evaluator returns them in a deterministic
  order (exact ordering — alphabetical-by-id vs. most-overdue-first —
  is a cosmetic detail noted in Open Questions; the test asserts
  determinism rather than a specific order).

Integration tests:

- **`today_cmd` text:** tempdir notes_dir; write a note with a metric
  logged 3 days ago; configure a reminder with `interval_days = 2`;
  run `today_cmd::execute`; assert the rendered text contains the
  "Reminders" header and the due line **above** the date header,
  and that the silent-day case (logged today) emits no reminders
  block.
- **`today_cmd` JSON:** assert the `reminders` array shape, including
  a non-due reminder with `days_since = 0`.
- **`cmd_status` JSON:** assert the array is included and matches
  the `today --json` shape for the same reminder set.
- **Config parsing:** valid configs round-trip; invalid
  (`interval_days = 0`, unknown session column, unknown day_field,
  missing `target.exercise` for a lift watch) produce a clear
  `Config::validate` error with a `.suggestion()`.

## Push notifications (future)

Out of scope for this spec. Recorded here only to confirm the v1
design isn't painting us into a corner:

- The v1 JSON output (`reminders` array on `vitalog status` /
  `vitalog today --json`) is the contract a notifier would consume.
- A separate small script (Rust, shell, or whatever) reads that
  array and POSTs each due reminder to an [ntfy.sh](https://ntfy.sh)
  topic. ntfy.sh is free, has a vanilla-Android app, and works
  without an account on a public-ish topic; self-hosted later if
  desired.
- A user-level launchd plist (macOS) fires the script daily at a
  fixed time (e.g. 09:00). No code in vitalog needs to know about
  the notifier.
- The topic URL and any auth header live in a separate config
  section (probably `[reminders.notify]`), introduced when that
  spec lands. v1 does not reserve the key.

No v1 work is contingent on this section.

## Open questions

None that block implementation. A few minor things to settle during
the implementation plan, not the design:

- Exact wording of the "overdue" line — "(last 3 days ago, 2026-05-09)"
  vs "(3 days ago, 2026-05-09)" vs "(2026-05-09, 3d ago)". Cosmetic.
- Whether the bell glyph (`⏰`) renders cleanly in all terminals
  Adrian uses; fall back to a plain `[!]` prefix if not. Cosmetic.
- Ordering of the rendered due lines (alphabetical by id, by
  `days_since` descending, or by config order). Lean toward
  `days_since` descending so "most overdue" reads first.
