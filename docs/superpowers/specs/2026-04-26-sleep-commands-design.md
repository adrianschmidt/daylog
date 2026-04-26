# Sleep commands and time-format normalization

**Issue:** [adrianschmidt/daylog#8](https://github.com/adrianschmidt/daylog/issues/8)
**Date:** 2026-04-26

## Background

The convention in daylog is: **sleep is recorded on the file for the day the user wakes up.** When bedtime is past midnight, the bedtime timestamp lives on a different calendar date than the file the entry belongs to. Today, this requires the user (or LLM acting on their behalf) to manually compute the right `YYYY-MM-DD.md`, format `"HH:MM-HH:MM"`, and remember the convention every night. It's a recurring small failure mode.

A second issue surfaces in the same area: the existing `sleep` field accepts both 12-hour (`"10:30pm-6:15am"`) and 24-hour (`"22:30-6:15"`) formats, but the database stores whatever string was typed. As a result, the dashboard displays inconsistent formats depending on which file was last edited, and there's no canonical form for downstream code.

## Goals

1. Two new top-level CLI commands (`sleep-start`, `sleep-end`) that handle the bedtime-past-midnight date math automatically.
2. A canonical 24-hour representation for sleep times in the database.
3. A `time_format` config option (default `"12h"` for upstream compatibility) that controls how times are written to markdown files and displayed in the TUI.
4. All time parsing and formatting consolidated in a single module so behavior is consistent across `log sleep`, the new commands, the materializer, and the dashboard.

## Non-goals

- Rewriting existing markdown files. Markdown stays whatever format the user chose; both 12h and 24h remain valid input.
- Making display configurable independently of file-write format — one `time_format` config controls both.
- Interactive prompts. The CLI must remain 100% scriptable / LLM-callable.
- Using `day_start_hour` for sleep date math. It exists for typing-after-midnight convenience in `log` and `edit`, but for sleep the wake-up time itself defines the new day, so calendar-today is correct.

## User-facing surface

### `daylog sleep-start [TIME]`

Records bedtime as pending state.

```
daylog sleep-start            # uses now
daylog sleep-start 00:28      # 24h
daylog sleep-start 12:28am    # 12h
```

Output (formatted per `time_format`):

```
Sleep start recorded: 12:28am
```

### `daylog sleep-end [TIME]`

Finalizes the sleep entry on today's file.

```
daylog sleep-end              # uses now
daylog sleep-end 06:52
daylog sleep-end 6:52am
```

Output:

```
Sleep recorded: 12:28am-6:52am (6.40h) on 2026-04-26
```

### Errors

- Invalid time: `Invalid time: '<x>'. Expected HH:MM (24h) or H:MMam/pm (12h).`
- No pending start: `No pending sleep-start. Run \`daylog sleep-start\` before bed, or use \`daylog log sleep "HH:MM-HH:MM"\` for a one-shot entry.`
- Stale pending (>24h): same as above plus `(ignored stale sleep-start from <YYYY-MM-DD HH:MM>)`. The stale entry is cleared so the next `sleep-start` runs clean.

## Architecture

### New module: `src/time.rs`

Single source of truth for all time parsing and formatting. Replaces the inline logic currently in `materializer::parse_sleep` and `materializer::parse_time_to_minutes`.

```rust
pub enum TimeFormat { TwelveHour, TwentyFourHour }

pub fn parse_time(s: &str) -> Option<NaiveTime>;
pub fn parse_sleep_range(s: &str) -> Option<(NaiveTime, NaiveTime)>;
pub fn format_time(t: NaiveTime, fmt: TimeFormat) -> String;
pub fn format_sleep_range(start: NaiveTime, end: NaiveTime, fmt: TimeFormat) -> String;
pub fn sleep_hours(start: NaiveTime, end: NaiveTime) -> f64;
```

`parse_time` accepts (case-insensitive, leading-zero optional):
- `"22:30"`, `"0:28"`, `"00:28"` (24h)
- `"10:30pm"`, `"10:30 PM"`, `"6am"`, `"12:30am"` (midnight), `"12:30pm"` (noon) (12h)

`format_time`:
- 24h: `"22:30"` / `"06:15"` (zero-padded)
- 12h: `"10:30pm"` / `"6:15am"` (no zero-padding on hour, lowercase suffix — matches existing convention)

`sleep_hours`: handles overnight by adding 24h when end ≤ start.

### New module: `src/state.rs`

Manages the pending-bedtime sidecar file.

```rust
#[derive(Serialize, Deserialize, Default)]
pub struct PendingState {
    pub sleep_start: Option<PendingSleepStart>,
}

#[derive(Serialize, Deserialize)]
pub struct PendingSleepStart {
    pub bedtime: NaiveTime,           // canonical 24h
    pub recorded_at: DateTime<Local>, // wall-clock timestamp of `sleep-start` invocation
}

pub fn load(notes_dir: &Path) -> PendingState;
pub fn save(notes_dir: &Path, state: &PendingState) -> Result<()>;
```

- File path: `{notes_dir}/.daylog-state.toml`.
- `load` returns empty state if the file is missing OR corrupt (warns to stderr in the corrupt case). Sleep state is recoverable — bailing on a malformed sidecar would just block the user from logging.
- `save` uses `frontmatter::atomic_write` semantics (write to `.daylog-state.toml.tmp-<pid>`, then rename).

### New module: `src/cli/sleep_cmd.rs`

```rust
pub fn cmd_sleep_start(time_arg: Option<&str>, config: &Config) -> Result<()>;
pub fn cmd_sleep_end(time_arg: Option<&str>, config: &Config) -> Result<()>;
```

`cmd_sleep_start`:
1. Resolve bedtime: parse `time_arg` if present, else `Local::now().time()`.
2. Load state, set `pending.sleep_start = { bedtime, recorded_at: Local::now() }`, save.
3. Print confirmation formatted per `config.time_format`.

`cmd_sleep_end`:
1. Resolve wake time: parse `time_arg` if present, else `Local::now().time()`.
2. Load state. If `pending.sleep_start` missing, error.
3. If `now() - recorded_at > 24h`, clear pending, save state, error (with stale-info suffix).
4. `wake_date = Local::now().date_naive()` (calendar today — does not consult `day_start_hour`).
5. Format range `start-end` per `time_format`.
6. Open `{notes_dir}/{wake_date}.md` (render from template if missing).
7. `frontmatter::set_scalar(content, "sleep", &format!("\"{}\"", formatted))`, `atomic_write`.
8. Clear pending, save state.
9. Print confirmation.

### Config additions: `src/config.rs`

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TimeFormat {
    #[default]
    #[serde(rename = "12h")]
    TwelveHour,
    #[serde(rename = "24h")]
    TwentyFourHour,
}

pub struct Config {
    // ... existing fields ...
    #[serde(default)]
    pub time_format: TimeFormat,
}
```

`Config::load` adds a focused suggestion when parsing `time_format` fails (mirrors the existing `weight_unit` pattern). `presets/default.toml` documents the option as a commented line.

### CLI definition: `src/cli/mod.rs`

Add two subcommands. Clap renders kebab-case automatically:

```rust
SleepStart { time: Option<String> },
SleepEnd { time: Option<String> },
```

`src/main.rs` dispatches them to `daylog::cli::sleep_cmd`.

### DB normalization: `src/materializer.rs`

`materialize_file` now uses `time::parse_sleep_range`:

```rust
let sleep_data = yaml_str_field(yaml, "sleep")
    .and_then(|s| time::parse_sleep_range(&s))
    .map(|(start, end)| {
        let hours = time::sleep_hours(start, end);
        // canonical 24h "HH:MM" for DB
        (time::format_time(start, TimeFormat::TwentyFourHour),
         time::format_time(end, TimeFormat::TwentyFourHour),
         hours)
    });
```

The DB schema is unchanged — `sleep_start` / `sleep_end` are still `TEXT`, but the contents are now always canonical 24h `"HH:MM"`. Migration: users run `daylog rebuild` (already documented as the schema-change answer in `CLAUDE.md`).

### Existing `log sleep`: `src/cli/log_cmd.rs`

Validation switches from "must contain a dash" to "must round-trip through `time::parse_sleep_range`". On write, the value is formatted per `config.time_format`. So `daylog log sleep "10:30pm-6:15am"` with a 24h config writes `"22:30-06:15"` to the file. This makes the file format consistent with whatever the user chose, regardless of input style.

### Dashboard display: `src/modules/dashboard.rs`

Reads canonical 24h `sleep_start` / `sleep_end` from the DB and reformats per `config.time_format` before rendering.

## Data flow examples

**Bedtime past midnight, default 12h config:**

```
22:00 Mon  daylog sleep-start 22:00      → state: { bedtime: 22:00, recorded_at: 2026-04-26T22:00 }
            (file: not yet written)

06:52 Tue  daylog sleep-end              → wake_date = Tue 2026-04-27
                                           formatted = "10:00pm-6:52am"
                                           writes to 2026-04-27.md: sleep: "10:00pm-6:52am"
                                           clears state
```

**Bedtime past midnight, 24h config:**

```
00:28 Tue  daylog sleep-start 00:28      → state: { bedtime: 00:28, recorded_at: 2026-04-27T00:28 }

06:52 Tue  daylog sleep-end              → wake_date = Tue 2026-04-27
                                           formatted = "00:28-06:52"
                                           writes to 2026-04-27.md: sleep: "00:28-06:52"
                                           clears state
```

**Forgot to run sleep-end (stale):**

```
22:00 Mon  daylog sleep-start            → state set
... 30h pass without sleep-end ...
04:00 Wed  daylog sleep-end              → recorded_at is 30h old, > 24h
                                           clears state
                                           errors with stale suggestion
```

## Error handling matrix

| Situation | Behavior |
|---|---|
| `sleep-start "abc"` | Invalid-time error |
| `sleep-end "abc"` | Invalid-time error |
| `sleep-end` with no pending | No-pending error |
| `sleep-end` with pending older than 24h | No-pending error + stale suffix; state cleared |
| Repeat `sleep-start` before `sleep-end` | Last wins (silently overwrites) |
| `.daylog-state.toml` corrupt | Warn to stderr, treat as empty; next `sleep-start` overwrites |
| Today's note doesn't exist | Render from template, then write |
| Today's note already has `sleep:` | Overwritten (same as `log sleep`) |
| Bedtime equals wake time | Hours = 0, entry still written |
| `time_format` value not `"12h"` or `"24h"` | Config load fails with suggestion |

## Testing strategy

- **`time`** — table-driven unit tests: parse all 12h variants, 24h variants, garbage; format roundtrip in both formats; `sleep_hours` over midnight, same-day, equal start/end.
- **`state`** — round-trip serialize, missing-file → empty, corrupt-file → empty with warning, atomic-write doesn't leave temp on success.
- **`sleep_cmd`** — happy path (writes correct range, clears state); explicit-time vs default-now; no-pending error; stale-pending error (state cleared); writes per `time_format`; creates today's file from template; uses calendar today regardless of `day_start_hour` setting; multiple `sleep-start` invocations → last wins.
- **DB normalization** — materialize a file with `sleep: "10:30pm-6:15am"` → DB has `sleep_start = "22:30"`, `sleep_end = "06:15"`; same for 24h input.
- **Existing `log sleep`** — accepts 12h and 24h, writes per config; rejects garbage with new clear error.
- **Dashboard** — given canonical DB values, formats per `config.time_format`.
- **Migration smoke test** — old DB with mixed 12h/24h strings, run `rebuild`, verify all canonical.

## Out of scope (explicitly)

- Migrating or rewriting existing markdown files.
- Adding `daylog sleep-status` / `daylog sleep-cancel` (could come later if useful).
- Time zones — daylog assumes local time everywhere; that doesn't change.
- Storing the bedtime calendar date in pending state. Wake-up day is calendar-today by definition; the bedtime time alone is sufficient state.
