# `vitalog trend <field> [days]` — visual feedback on metrics

## Background

Issue [#9](https://github.com/adrianschmidt/vitalog/issues/9) asks for a CLI
command that prints a sparkline or compact chart for recent values of any
tracked field. The motivation: daily values (weight especially) fluctuate
±1 kg from water/glycogen/sodium, so the actual trend only emerges over
2–4 weeks. Looking at one morning's number invites over-reaction; a chart
plus a slope makes the signal visible.

The TUI already has a Trends tab (42-day mini sparklines), but there's no
way to get that view from the command line — useful both for a quick
glance after logging and for piping into other tools.

This spec covers `vitalog trend <field> [days]` end-to-end.

## Goals (in scope)

- Print a multi-row ASCII chart, a one-line sparkline (`--compact`), or a
  JSON payload (`--json`) for any DB-resident field.
- Default window of 14 days; positional `[days]` overrides.
- Built-in field names: `weight`, `sleep_hours`, `mood`, `energy` — all
  served by the `days` table.
- Custom-metric field names: anything in `config.metrics` (or anything with
  historical rows in the `metrics` table) — served by the `metrics` table.
- Stats line: mean, min, max, slope (per day, per week) by ordinary least
  squares.
- Gaps in the window render as blank columns; stats compute only over
  available points.
- Sync notes to the DB before reading, matching `vitalog today`.

## Non-goals

- BP slots (`bp_morning_sys` etc.) and food macros (`kcal`, `protein`,
  `carbs`, `fat`). Path is left open in the design — see the
  `TrendField` enum — but v1 ships DB-resident only. BP is the most
  likely v2 add.
- Multi-field comparisons on one chart.
- Configurable chart height/width via flags. Chart renders to a fixed
  height with a width that fits the window count; `--compact` is the
  knob for "smaller please."
- Color in the chart output. Plain ASCII for v1 keeps it pipe-friendly
  and matches the issue mockup. (We can layer NO_COLOR-aware coloring
  later without changing the public surface.)
- Editing or deriving fields. Read-only command.

## Decisions

- **Command shape:** `vitalog trend <field> [days]` with `--compact` and
  `--json` flags (mutually exclusive). Mirrors the positional-then-flags
  shape of `vitalog today [date] --json`.
- **Default days:** 14. Captures one ovulatory/weekly cycle of body-weight
  noise, which is the issue's primary motivating example.
- **Field resolution:** small enum `TrendField`. Built-ins are a hardcoded
  table mapping name → `days` column + unit. Anything else falls through
  to the `metrics` table; display label and unit come from `config.metrics`
  when present, else the raw field name with no unit. An unknown field
  (no built-in, no config, no historical row) errors with a listing of
  available fields.
- **Window semantics:** inclusive `[today - (days-1) .. today]`. Always
  spans the full window even if rows are missing; gap days carry `None`.
  This matches the user's mental model better than "the last 14 logged
  values" (which can be a much wider date span if logging is sparse).
- **Stats:** mean / min / max over `Some(_)` values; OLS slope on
  `(day_index_from_window_start, value)`; `slope_per_week = slope_per_day * 7`.
  Slope omitted (text) or its keys absent (JSON) when fewer than two
  points exist.
- **Sync-on-run:** `materializer::sync_all()` runs first, same as
  `today_cmd`. Without it, edits made via `$EDITOR` don't show up.
- **Output format selection:**
  - default → multi-row chart + stats block
  - `--compact` → one-line sparkline + stats line
  - `--json` → structured payload, no chart text
- **Code location:** single new file `src/cli/trend_cmd.rs` containing the
  CLI entry, DB queries, rendering, and stats. Pure helpers stay private
  with `#[cfg(test)] mod tests`. Promoted to a top-level module only
  when a second consumer materializes.

## Architecture

One new file. CLI wiring in `src/cli/mod.rs` and `src/main.rs`. No new
dependencies — `chrono`, `rusqlite`, `clap`, and `color_eyre` already
cover everything needed.

```
src/
  cli/
    trend_cmd.rs   # CLI entry: resolve, query, render
    mod.rs         # add `Trend { field, days, compact, json }` variant
  main.rs          # dispatch to trend_cmd::execute
```

### Data flow

```
trend(field, days, mode)
  ├─ Config::load()
  ├─ materializer::sync_all()                  # keep DB current
  ├─ open_ro(db_path)
  ├─ TrendField::resolve(&field, &config, &conn) -> Result<TrendField>
  ├─ window = (today - days + 1) ..= today
  ├─ rows = match field { DaysColumn => SELECT date,<col> FROM days …
  │                       Metric     => SELECT date,value FROM metrics … }
  ├─ points = build_window(window, rows)       # Vec<(NaiveDate, Option<f64>)>
  ├─ stats  = compute_stats(&points)
  └─ match mode {
       Default => print(render_chart(...) + render_stats(stats)),
       Compact => print(render_compact(...) + render_stats_line(stats)),
       Json    => print(render_json(field, display, unit, points, stats)),
     }
```

### Types

```rust
pub enum TrendField {
    DaysColumn {
        name: String,           // "weight" | "sleep_hours" | "mood" | "energy"
        column: &'static str,   // SQL column name (always == name today)
        display: String,        // "weight" or per-config display
        unit: Option<String>,   // "kg" / "lbs" / "h" / None
    },
    Metric {
        name: String,           // matches metrics.name
        display: String,
        unit: Option<String>,
    },
}

pub struct TrendStats {
    pub count: usize,
    pub mean: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub slope_per_day: Option<f64>,   // None if count < 2
    pub slope_per_week: Option<f64>,  // None if count < 2
}

pub enum RenderMode { Chart, Compact, Json }
```

### Built-in field table

```rust
const BUILTINS: &[(&str, &str, Option<&str>)] = &[
    ("weight",       "weight",       None /* filled from weight_unit */),
    ("sleep_hours",  "sleep_hours",  Some("h")),
    ("mood",         "mood",         None),
    ("energy",       "energy",       None),
];
```

`weight`'s unit is set at resolve time from `config.weight_unit` so
`vitalog trend weight` always shows the right label.

## CLI surface

Add to `src/cli/mod.rs`:

```rust
/// Print a chart of recent values for any tracked field.
Trend {
    /// Field name (weight, sleep_hours, mood, energy, or any
    /// custom metric from your config).
    field: String,
    /// Window length in days (default 14).
    #[arg(default_value_t = 14)]
    days: u32,
    /// One-line sparkline instead of multi-row chart.
    #[arg(long, conflicts_with = "json")]
    compact: bool,
    /// Print structured JSON.
    #[arg(long)]
    json: bool,
},
```

Validation: clap's `value_parser` coerces `days` to `u32`; we additionally
reject `days == 0` in `execute()` with a friendly suggestion.

## Output formats

### Chart (default)

```
weight (last 14 days, kg)

123.5 ┤            ●
123.0 ┤  ●  ●        ●
122.5 ┤      ●     ●     ●
122.0 ┤●               ●     ●
121.5 ┤   ●     ●        ●     ●
121.0 ┤              ●
120.5 ┤                       ●
120.0 ┤                          ●
119.5 ┤                             ●
      └──────────────────────────────
       04-24                   05-07

mean: 121.6  min: 119.8  max: 123.5
linear trend: -0.18 kg/day  (≈ -1.3 kg/week)
```

- 8 data rows. Row count is fixed; if min == max we emit a single
  centered row to avoid a divide-by-zero.
- Y-axis labels right-aligned to a 5-character field, formatted to 1
  decimal (or 0 if the field is integer-typed: `mood`, `energy`).
- Each window day occupies a fixed-width column (2 chars wide is a
  reasonable starting point — gives 28 cols for a 14-day window).
  The exact width is an implementation detail tunable via snapshot
  tests.
- X-axis: a horizontal rule under the data, with the leftmost date as
  `MM-DD` aligned under the first column and the rightmost date as
  `MM-DD` right-aligned under the last column.
- Title line: `<display> (last <days> days[, <unit>])` — unit clause
  omitted when no unit.
- "no data" path: if `count == 0`, skip the chart, print
  `no data for <field> in the last <days> days` to stdout, exit 0.

### Compact (`--compact`)

```
weight (14d, kg): ▁▃▂▅▄▆▇▅▇█▇▅▃▁
mean 121.6  min 119.8  max 123.5
slope -0.18 kg/day  (≈ -1.3 kg/week)
```

- Eight blocks `▁▂▃▄▅▆▇█` mapped from `(value - min) / (max - min)`
  into 0..=7. Gap days render as a single space. If min == max, every
  filled day uses `▄` (mid block) so the line still has shape.
- Stats line omits slope when count < 2.

### JSON (`--json`)

```json
{
  "field": "weight",
  "display": "weight",
  "unit": "kg",
  "days": 14,
  "from": "2026-04-24",
  "to": "2026-05-07",
  "points": [
    {"date": "2026-04-24", "value": 122.0},
    {"date": "2026-04-25", "value": null}
  ],
  "stats": {
    "count": 12,
    "mean": 121.6,
    "min": 119.8,
    "max": 123.5,
    "slope_per_day": -0.18,
    "slope_per_week": -1.3
  }
}
```

- Points always cover the full window (one entry per day); missing
  values are `null`.
- `unit` is `null` when the field has no configured unit.
- `slope_per_day` and `slope_per_week` are `null` when count < 2.
- `mean`/`min`/`max` are `null` when count == 0.

## Error handling

- Unknown field name: `unknown field '<name>'. Known fields: weight,
  sleep_hours, mood, energy[, <configured metrics>]`. Suggestion via
  `color_eyre::Help::suggestion`.
- `days == 0`: `--days must be at least 1`.
- DB missing: same hint as `status` — "Run `vitalog init` or `vitalog
  sync` first."
- DB query failures bubble up as `color_eyre::Result` like the rest
  of the codebase.
- The "field exists in config.metrics but has no data in window" case
  is **not** an error — it falls through to the empty-window path
  ("no data" text / empty JSON payload).

## Tests

Unit, in `src/cli/trend_cmd.rs`:

- `compute_stats`:
  - empty input → all None except count=0.
  - single point → mean/min/max set, slope None.
  - linear input → slope matches expected to within 1e-9.
  - input with all-None days → treated as empty.
- `render_compact`:
  - fixture with known min/max → exact string snapshot.
  - all-equal values → all blocks `▄`.
  - all-None days → blocks all spaces, stats line omits slope.
- `render_chart`:
  - fixture snapshot — full multi-line string match for a 14-day
    weight series.
  - integer-valued field (`mood`) → axis labels render as integers.
  - empty window → returns the "no data" line, no axis.
- `TrendField::resolve`:
  - each built-in → DaysColumn variant with correct unit (weight
    picks up `config.weight_unit`).
  - configured metric → Metric variant with display/unit from config.
  - unknown name with no historical rows → error mentions known fields.
  - unknown name **with** historical rows → Metric variant, raw name
    as display, no unit. (Soft-resolve so `vitalog trend foo` works
    if `foo` was previously a configured metric since dropped.)
- `build_window`:
  - given DB rows for some days → returns full window with None gaps
    in the right positions.
  - empty rows → all None.

Integration, new file `tests/trend.rs`:

- Use the demo data generator (`vitalog::demo::generate_demo_data`)
  against a tempdir, then drive `trend_cmd::execute` directly (or shell
  out to the built binary, matching the pattern in `tests/today.rs`).
- Assert: exit 0; stdout contains the title line; the stats line
  contains "mean", "min", "max"; for a series with > 1 point, the
  slope line is present.
- `--json` run: parse stdout as JSON, assert keys, assert
  `points.len() == days`, assert any null entries match days that
  weren't in the demo set.
- `--compact` run: assert stdout has at most 4 lines (title+spark,
  stats line, optional slope) and contains a block character.

## Open questions

None blocking. The chart's y-padding policy ("nudge so extremes
aren't pinned to the border") is a tasteful detail — I'll start with
no padding and adjust if the snapshot test reveals a clipped extreme.
