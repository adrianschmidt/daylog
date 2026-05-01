# `daylog today [date]` — daily summary with goal comparison

## Background

Issue [#7](https://github.com/adrianschmidt/daylog/issues/7) asks for a one-shot
CLI command that prints a compact daily summary: today's macro totals from the
`## Food` section, plus weight, sleep, and BP morning, with optional comparison
against goals. The motivation is that totals are currently only obtainable by
manual parsing or by asking an LLM (which drifts mid-day across context
compactions).

This spec covers `daylog today [date]` end-to-end.

## Goals (in scope)

- Parse today's `## Food` section back into structured macro totals.
- Read goals from a structured block in `goals.md`.
- Print a compact, color-when-tty terminal summary covering food macros,
  weight (with delta vs previous logged day), sleep, BP morning, and any
  user-defined `[metrics]`.
- Support an optional date argument: `daylog today 2026-04-15`.
- Provide `--json` for scripting / LLM consumption.
- Allow `goals.md` to be missing or partial — the summary still works, just
  without goal annotations.

## Non-goals

- Materializing food entries into the database. They stay markdown-only;
  `daylog today` parses them on demand.
- Multi-day rollups (week/month summaries). Out of scope; could come later.
- Editing goals via the CLI.
- BP morning as a goal-able metric. Shown as a row, but no
  `bp_morning_*_max` parsing in v1.
- New external dependencies (no `regex`, no `nu-ansi-term`). Use stdlib +
  existing crates.

## Decisions

- **Command shape:** `daylog today [date]` — single command, optional
  positional date arg defaulting to `config.effective_today_date()`. Same
  shape as the existing `daylog edit [date]`.
- **Goals location:** YAML frontmatter in `{notes_dir}/goals.md`. The body
  of goals.md remains free-form prose (history, derivations, commentary)
  unaffected by daylog. Single source of truth, matching the project's
  central thesis (markdown is the source format for AI accessibility).
- **Goals schema:** suffix-based, fully agnostic. Any frontmatter key
  matching `<name>_target | _min | _max` is grouped by `<name>` into a
  threshold map. No hardcoded list of allowed names.
- **Known-metrics list:** hardcoded mapping from metric name to data
  source (food parser, `days` table, custom `[metrics]` table). The goals
  side is open; the data side is closed.
- **Output format:** matches the issue's example. Two text blocks (food,
  then everything else) separated by a blank line. Hint lines at bottom
  for missing goals, skipped food lines, and unknown-metric warnings.
- **Color:** ANSI escapes inline. Active when `stdout.is_terminal() &&
  NO_COLOR is unset && !json`. Respects the `NO_COLOR` env var standard;
  no `--no-color` flag.
- **Day boundary:** respects `config.day_start_hour`, same as every other
  CLI command.

## Architecture

Three new modules plus CLI wiring. No new dependencies.

```
src/
  cli/
    today_cmd.rs   # CLI entry: assembles + renders
  goals.rs         # parse goals.md frontmatter -> Goals map
  food_sum.rs      # parse ## Food section -> FoodTotals
```

Single-pass, read-only data flow:

```
date (today or positional arg)
  ├─> load_goals(notes_dir/goals.md) -> Goals
  ├─> read {date}.md, sum_food_section(content) -> FoodTotals
  └─> open_ro(db) -> day row + previous-day weight + custom metrics + BP morning
        |
        v
  assemble DaySummary -> render_text OR render_json -> stdout
```

No DB writes. No file watcher. Independent of the TUI loop.

## Components

### `goals.rs`

```rust
pub struct Threshold {
    pub target: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

pub struct Goals {
    pub thresholds: HashMap<String, Threshold>,
    pub source_path: PathBuf,
    pub present: bool, // true iff at least one threshold was parsed
}

pub fn load_goals(notes_dir: &Path) -> Result<Goals>;
```

Parsing rules:

- Read `{notes_dir}/goals.md`. Missing file → `Goals { present: false, .. }`.
  No error.
- Extract YAML frontmatter using existing `frontmatter.rs` helpers (or
  `yaml-rust2` directly).
- Top-level scalar key matching `<name>_target | _min | _max` →
  group by `<name>` into the threshold map. A `Threshold` with all
  three fields `None` never appears (a name only enters the map once
  at least one value parsed for it).
- Non-numeric values → hard error with `.suggestion()`
  (e.g., `kcal_min must be a number, got 'foo'`).
- Keys not matching the suffix pattern → silently ignored (lets the user
  add commentary keys later without breakage).
- The body of goals.md is not read or validated.
- `present == true` iff `thresholds.is_empty() == false`. A goals.md that
  exists but contains no goal keys behaves the same as a missing file
  (single hint message at render time).

### `food_sum.rs`

```rust
pub struct FoodTotals {
    pub kcal: f64,
    pub protein: f64,
    pub carbs: f64,
    pub fat: f64,
    pub entry_count: usize,
    pub skipped_lines: usize,
}

pub fn sum_food_section(markdown: &str) -> FoodTotals;
```

Parsing strategy: locate `## Food`, walk lines until the next `## ` heading.
For each line starting with `- **`, extract the four macros via independent
literal-token scans for ` kcal`, `g protein`, `g carbs`, `g fat`. Hand-rolled
bytewise scan; no `regex` dep.

Tolerance:

- Line missing the kcal token → skip, `skipped_lines += 1`.
- Line with kcal but missing one macro → count what's there, missing macro = 0.
- Pure-prose lines under `## Food` (no `- **` prefix) → silently ignored.
- No `## Food` section → all zeros, `entry_count: 0`.

Round-trip property: every `RenderedEntry` produced by `food_cmd::format_line`
must parse back to its macros within float tolerance.

### `cli/today_cmd.rs`

Public surface:

```rust
pub fn execute(date_flag: Option<&str>, json: bool, config: &Config) -> Result<()>;
```

Internals:

```rust
struct DaySummary {
    date: NaiveDate,
    food: FoodTotals,
    day: Option<DayRow>,                    // weight/sleep/mood/energy
    weight_delta: Option<(f64, NaiveDate)>, // (delta, previous logged date)
    bp_morning: Option<BpReading>,
    custom_metrics: Vec<(String, f64)>,     // [metrics] config order
    food_skipped: usize,
    goals_warnings: Vec<String>,
}

fn assemble(date: NaiveDate, config: &Config, conn: &Connection) -> Result<DaySummary>;
fn render_text(summary: &DaySummary, goals: &Goals, color: bool) -> String;
fn render_json(summary: &DaySummary, goals: &Goals) -> serde_json::Value;
```

#### Known-metrics list

| Metric | Source |
|---|---|
| `kcal`, `protein`, `carbs`, `fat` | `FoodTotals` from `food_sum.rs` |
| `weight` | `days.weight` + previous logged date for delta |
| `sleep_hours` | `days.sleep_hours` |
| `mood`, `energy` | `days.mood`, `days.energy` |
| every key in `config.metrics` | `metrics` table for the date |

BP morning is shown as a composite row but is not goal-able in v1.

#### Weight delta

Use `db::load_weight_trend(conn, 60)` to find the most recent date strictly
before the target date that has a weight value. Render `(Δ +1.3 vs yesterday)`
when the previous date is calendar-yesterday; otherwise render the actual
date (`Δ +1.3 vs 2026-04-25`) to avoid lying about gap size.

#### Goals join

For every entry in `goals.thresholds`, look up the metric in the known
list. If the metric has no source → push a warning to
`summary.goals_warnings` ("unknown metric `mystery` in goals.md frontmatter").
At render time, every value row gets its annotation by looking up its name
in `goals.thresholds`.

#### Output format (text)

```
2026-04-30 — Daily summary

Calories:  1513 / 1900–2200 kcal     (387 below min)
Protein:    147 / ≥140 g              ✓ over minimum
Carbs:       77 g
Fat:         59 g

Weight:    121.5 kg  (Δ +1.3 vs yesterday)
Sleep:     6h 24min
BP morning:   not logged
```

Hint lines at the bottom (only when applicable):

- `(No goals defined — add `<metric>_min/_max/_target` keys to {path}/goals.md.)`
  when `!goals.present`.
- `(2 food lines couldn't be parsed)` when `food.skipped_lines > 0`.
- `(Unknown metric in goals.md: mystery)` per warning.

Color (when active):

- Red for "below min" / "above max".
- Green for "✓ over minimum" / within range.
- Dim for "not logged".
- Default color otherwise.

#### Output format (JSON)

```json
{
  "date": "2026-04-30",
  "metrics": {
    "kcal":    { "value": 1513, "min": 1900, "max": 2200, "target": null },
    "protein": { "value": 147,  "min": 140,  "max": null, "target": null },
    "carbs":   { "value": 77,   "min": null, "max": null, "target": null },
    "fat":     { "value": 59,   "min": null, "max": null, "target": null },
    "weight":  { "value": 121.5, "target": 110, "delta": 1.3, "delta_vs_date": "2026-04-29" },
    "sleep_hours": { "value": 6.4 }
  },
  "bp_morning": null,
  "sleep": { "hours": 6.4, "start": "23:00", "end": "05:24" },
  "goals_present": true,
  "warnings": []
}
```

The `metrics` map contains every known metric whose data source returned
a value (used for goal comparison). The top-level `sleep` block is a
richer view (start/end times) that the text renderer uses; `sleep_hours`
also appears inside `metrics` for goal-comparison consistency. This small
redundancy lets script consumers pick whichever shape fits.

#### Error handling

- DB missing → hard error matching `daylog status`'s phrasing.
- Daily note missing → food totals = zeros, no skipped count; everything
  else still queried from DB.
- `goals.md` missing → empty `Goals`, hint line in output.
- `goals.md` frontmatter has invalid YAML or non-numeric value → hard
  error with `.suggestion()`.

### CLI wiring

`src/cli/mod.rs`:

```rust
/// Print today's daily summary (food totals + weight + sleep + BP)
/// with optional goal comparison from goals.md.
Today {
    /// Date in YYYY-MM-DD format (defaults to effective today)
    date: Option<String>,
    /// Print JSON instead of formatted text
    #[arg(long)]
    json: bool,
},
```

`src/main.rs`: a thin `cmd_today` that loads `Config` and delegates to
`daylog::cli::today_cmd::execute`.

## Testing

Unit tests:

- `goals.rs`:
  - empty / missing file → `present: false`
  - frontmatter only (no body) → parses
  - body only (no frontmatter) → `present: false`
  - three suffixes for one metric → grouped correctly
  - non-numeric value → error mentions field + suggestion
  - non-suffix keys → silently ignored
- `food_sum.rs`:
  - round-trip with `food_cmd::format_line` representative inputs
  - line missing kcal token → skipped
  - line with kcal but missing one macro → that macro = 0
  - prose line under `## Food` → ignored, no skip
  - no `## Food` section → zeros
  - multiple entries → sums
- `today_cmd.rs` rendering:
  - text full goals + food → matches issue example
  - text no goals → no annotations + hint
  - text missing data → `not logged` row
  - `NO_COLOR=1` strips escapes
  - weight delta vs non-yesterday → shows actual date
  - unknown-metric goal → warning row
  - json shape stable across the above

Integration test (`tests/today.rs`): end-to-end with `tempfile::TempDir`,
populated DB (reusing helpers from `food_cmd.rs::tests`), a fixture daily
note with `## Food`, and a fixture `goals.md`. Asserts both text and
`--json` outputs.

Not tested:

- Color escape codes themselves (rendering is a pure function of
  `color: bool`; the branching is what matters).
- TTY detection (stdlib `is_terminal()`).

## Risks and trade-offs

- **Markdown re-parse on every invocation.** The `## Food` section is
  parsed each time `daylog today` runs. Acceptable: a daily note has at
  most ~30 lines; cost is microseconds. Avoids cache invalidation
  complexity.
- **Suffix parser conflates fields by accident.** If a future field
  happens to end in `_min`/`_max`/`_target`, it gets pulled into
  thresholds. Mitigation: the suffix is specific enough that this is
  unlikely; collisions surface as "unknown metric" warnings.
- **Float precision in round-trip tests.** The food writer rounds kcal to
  integers and macros to one decimal. Round-trip tolerance has to allow
  `±0.05`. Documented in test code.
- **BP morning not goal-able in v1.** Conscious scope cut. Adding it
  later means extending the known-metrics list and BP storage, not a
  redesign.
