# CLI commands for Food, Notes, and Vitals sections

**Issue:** [adrianschmidt/daylog#6](https://github.com/adrianschmidt/daylog/issues/6)
**Date:** 2026-04-30

## Background

Today, `daylog log` only writes YAML frontmatter (`weight`, `sleep`, `mood`,
`energy`, `lift`, `climb`, `metric`). The freetext sections of a daily note —
`## Food`, `## Notes`, and (newly conventional) `## Vitals` — require manual
markdown editing. In practice that means each entry triggers a `Read` + `Edit`
cycle on a growing file when an LLM is doing the logging, and a context switch
out of the terminal when Adrian is. Issue #6 is the single biggest source of
friction in the daily logging workflow.

This spec adds three top-level commands — `daylog food`, `daylog note`,
`daylog bp` — that append timestamped entries to those sections and, where
useful, also update YAML frontmatter. They are designed to be safe to call
concurrently with the watcher and to compose with the structured nutrition
database introduced in issue #10.

## Goals

1. Top-level CLI subcommands `food`, `note`, `bp`. No new `daylog log`
   field cases — these are first-class commands.
2. `daylog food` integrates with the structured nutrition DB
   (`db::lookup_food_by_name_or_alias`), scaling per-100g / per-100ml panels
   by user-supplied amounts, and supports an explicit-flags fallback for
   one-off items not in the DB.
3. `daylog bp` is the only command that writes both YAML frontmatter and the
   markdown body in one atomic operation.
4. `daylog note` supports config-defined aliases for high-frequency standard
   text (e.g., `med-morning` → full morning-meds string).
5. A new `body.rs` module provides line-oriented `## Section` primitives,
   sibling to `frontmatter.rs`, that the three new commands share.
6. The daily-note template gains `## Food` and `## Vitals` sections (joining
   the existing `## Notes`). Older notes that lack a section are upgraded
   on the first command that writes to it.

## Non-goals

- Writing to the SQLite database directly from these commands. The watcher
  re-materializes on file change (debounced ~500 ms), same pattern as
  `daylog log`. If the user runs `daylog food` then `daylog status --json`
  immediately, the line is in the markdown but not yet in the DB. That's
  acceptable and consistent with existing behavior.
- Migrating existing `## Food` lines from Swedish labels to English. Adrian
  will do that locally via the daylog skill after this PR ships.
- A configurable output language. The binary's output is English by
  deliberate choice — daylog as a tool is in English, the daylog skill
  parses Swedish input from Adrian and translates as needed. Adding
  `[output] language = "sv" | "en"` is cheap to do later if a second user
  wants Swedish output, but pre-building the knob would just create
  half-translated output state-space.
- Adding `bp_*_pulse` to the existing `[metrics]` config. The CLI writes
  the YAML field; surfacing pulse in the materialized DB is a user-side
  config change, not blocking.
- A Nutrition tab or BP gauge in the TUI.
- Fuzzy / typo-tolerant section heading matching (`##  Food` vs `## Food`).
  Strict match keeps the implementation simple; not a real risk for
  machine-written notes.
- `daylog vitals` as a generic command. v1 has only BP; if other vitals
  surface later, they get their own commands or flags.
- Auto-consulting `.daylog-state.toml` pending sleep-start to pick the
  target day. `effective_today()` (driven by `day_start_hour`) is the
  canonical "what day is it" answer in daylog; an explicit `--date`
  override covers the rest. Pending-bedtime coupling is a possible
  follow-up if `day_start_hour` proves insufficient in practice.

## User-facing surface

### Shared flags (all three commands)

- **`--date YYYY-MM-DD`** — write to the named day's note instead of
  today's. Useful for catching up after a missed day or correcting a
  retroactive entry.
- **`--time HH:MM`** — use the given time for the entry's `**HH:MM**`
  prefix instead of `Local::now()`. Accepts both 24h (`08:30`) and 12h
  (`8:30am`) — same parser as `daylog sleep-start`. Without `--time`,
  `Local::now()` is used.
- **Day resolution** when `--date` is absent: `config.effective_today()`.
  This respects `day_start_hour`, so setting `day_start_hour = 4` makes
  00:40 land on the previous day automatically. *Pending sleep-start is
  not consulted in v1.*
- **BP slot detection** (`daylog bp` only) uses the entry's `--time`
  value if given, else `Local::now().time()`. So
  `daylog bp --time 08:00 141 96 70` at 14:30 still slots as `morning`.

### `daylog food` — append to `## Food`, optional nutrition lookup

```
daylog food <name> [<amount>]
daylog food --kcal N --protein N --carbs N --fat N
            [--gi N] [--gl N] [--ii N]
            [--date YYYY-MM-DD] [--time HH:MM]
            <name> [<amount>]
```

- **`<amount>`** accepts a unit suffix: `500g` or `250ml`. Bare numbers
  default to grams. Required when the looked-up entry has only `per_100g`
  or `per_100ml` panels; optional (and ignored) when the entry has a
  `total` panel and no amount is given.
- **Lookup mode** (no `--kcal` flags): consults
  `db::lookup_food_by_name_or_alias` (case-insensitive, includes the
  nutrition-db's auto-aliases).
- **Custom mode** (`--kcal/--protein/--carbs/--fat` all present): bypasses
  the DB. The four nutrient flags are required together. `--gi`, `--gl`,
  `--ii` are independently optional.
- **GL auto-compute** (both modes): if `gi` is known and final `carbs` are
  known and `gl` was not explicitly provided (custom flag *or* DB
  `gl_per_100g` / `gl_per_100ml` field), compute `gl = gi × carbs / 100`.
- **Output line:**
  ```
  - **HH:MM** <name> (<amt><unit>) (X kcal, Yg protein, Zg carbs, Wg fat) | GI ~N, GL ~N, II ~N
  ```
  - Time prefix per `config.time_format`.
  - Display name = `foods.name` (case-preserved heading) for lookups; the
    literal `<name>` arg for custom mode.
  - Amount segment shows the user's input unit (e.g., `(250ml)` if input
    was ml, even when scaled internally via density).
  - For `total`-panel foods with no amount: render `(weight_g g)` if
    `total.weight_g` is set; otherwise omit the parens entirely.
  - Rounding: kcal whole, macros 1 decimal, GL 1 decimal.
  - Glycemic segment (` | GI ~N, GL ~N, II ~N`): omit entirely if all
    three values are absent. If some present and some absent, show only
    the present ones (no dangling commas, no "GI ~?").

### `daylog note` — append to `## Notes`

```
daylog note [--date YYYY-MM-DD] [--time HH:MM] <text>
daylog note [--date YYYY-MM-DD] [--time HH:MM] <alias-key>
```

- `<text>` accepts multiple positional args, joined with spaces (no shell
  quoting needed for multi-word notes).
- The joined string is checked once against `[notes.aliases]`. If it
  matches a key, expand to the mapped value. Otherwise treat as literal.
  This means alias keys are conventionally single-word (`med-morning`); a
  multi-word note that happens to equal an alias key is an unusual edge
  case but will expand.
- Alias keys are case-sensitive (matches TOML key convention).
- Output line: `- **HH:MM** <expanded-or-literal text>`.

### `daylog bp` — write YAML + append to `## Vitals`

```
daylog bp [--date YYYY-MM-DD] [--time HH:MM] <sys> <dia> <pulse>
daylog bp --morning [--date YYYY-MM-DD] [--time HH:MM] <sys> <dia> <pulse>
daylog bp --evening [--date YYYY-MM-DD] [--time HH:MM] <sys> <dia> <pulse>
```

- **Auto slot:** measurement time before 14:00 = `morning`, ≥ 14:00 =
  `evening`. The measurement time is `--time` if given, else
  `Local::now().time()`. The 14:00 cutoff is hard-coded; configurability
  is out of scope for v1.
- **`--morning` / `--evening`** are mutually exclusive flags that override
  the auto choice.
- Writes three YAML scalars: `bp_<slot>_sys`, `bp_<slot>_dia`,
  `bp_<slot>_pulse`. Existing values for the chosen slot are overwritten.
  The other slot is left untouched.
- Appends to `## Vitals`:
  ```
  - **HH:MM** BP: <sys>/<dia>, pulse <pulse> bpm
  ```
  No `(morning)`/`(evening)` suffix — it's derivable from the time prefix
  and the cutoff, and the slot is already authoritative in YAML.
- Re-running same slot in the same day: YAML scalars are overwritten in
  place; `## Vitals` gets a new line appended each time, preserving a
  chronological history in the body.
- Validation: `sys` 50–300, `dia` 30–200, `pulse` 30–250. Out-of-range
  prints a `Warning:` to stderr but still writes — better to log a typo
  than block logging.

## Architecture

### File map

```
src/
  cli/
    mod.rs              + Food, Note, Bp variants in Commands enum
    food_cmd.rs         NEW
    note_cmd.rs         NEW
    bp_cmd.rs           NEW
    completions.rs      no source change; clap regenerates with new variants
  body.rs               NEW — line-oriented `## Section` primitives
  config.rs             + NotesConfig { aliases: HashMap<String, String> }
  main.rs               + dispatch for Food/Note/Bp
templates/
  daily-note.md         + `## Food` and `## Vitals` (`## Notes` already there)
presets/
  default.toml          + commented `[notes.aliases]` example
README.md               + section on the three new commands and notes.aliases
CLAUDE.md               + File Map entries for body.rs and the three cmd files
```

### Module boundaries

**`body.rs`** — pure functions over `&str`. No I/O, no DB. Mirrors
`frontmatter.rs`'s line-oriented style.

```rust
pub const CANONICAL_SECTION_ORDER: &[&str] = &["Food", "Vitals", "Notes"];

/// Insert `## <section>` heading in canonical order if missing.
/// Returns the (possibly unchanged) content.
pub fn ensure_section(content: &str, section: &str) -> String;

/// Append `<line>` to the named section's body. Caller must call
/// `ensure_section` first.
pub fn append_line_to_section(content: &str, section: &str, line: &str) -> String;
```

**`food_cmd.rs`** — argument parsing (amount-with-suffix, custom flag set),
nutrition scaling math, output-line formatting. Opens a read-only DB
connection for lookups; works without the DB in custom mode.

**`note_cmd.rs`** — alias resolution, body append. No DB access.

**`bp_cmd.rs`** — slot dispatch (cutoff at 14:00, `--morning` / `--evening`
overrides), three YAML scalar writes, one `## Vitals` line append, all
flushed in a single `frontmatter::atomic_write`.

### Why no DB writes from these commands

The watcher will re-materialize on file change (~500 ms debounce). Writing
the markdown is the source of truth; the DB is a derived cache. This
matches `daylog log` exactly. The `daylog status --json` consumer sees the
DB reflect the new entry within ~1s of the command finishing.

### Coordination with the nutrition-db

`food_cmd` opens a read-only DB connection (`db::open_ro`) and calls
`db::lookup_food_by_name_or_alias`. If the DB doesn't exist (user never
ran `daylog init` / `daylog sync`), lookup mode errors with a suggestion
to run `daylog sync`; custom mode (with `--kcal` etc.) works without any
DB at all.

## `body.rs` algorithms

### `ensure_section`

1. Split content into (frontmatter, body) using the same logic as
   `frontmatter::split_frontmatter`. Body editing only touches the body.
2. Scan body for `^## (.+)$` heading lines. Build a name → line-index map.
3. If the target section already exists, return content unchanged.
4. Otherwise, find the insertion line by walking
   `CANONICAL_SECTION_ORDER`. The target's position is `target_idx`. The
   insertion line is *just before* the first existing section whose
   canonical-order index is greater than `target_idx`. If no later
   section exists, insertion = end of body.
5. Insert `\n## <section>\n\n` at that line.
6. Reassemble (frontmatter, updated body) into a single string.

### `append_line_to_section`

1. Find `## <section>` line in body.
2. Find the end of that section: next `^## ` line, or end of body.
3. Walk backward from end-of-section, skipping trailing blank lines.
4. Insert `<line>\n` *after* the last non-blank line, preserving a
   blank-line tail at section end so multi-appends don't pile up against
   the next heading.

### Edge cases

| Case | Behavior |
|---|---|
| Body is empty (frontmatter-only file) | `ensure_section` adds heading at end-of-file with leading blank line |
| Section heading present but body empty | `append_line_to_section` inserts directly after heading + blank line |
| `##  Food` (extra space) or `### Food` (different level) | Strict match misses; would insert duplicate. Acceptable — machine-written notes. |
| Heading literal inside fenced code block | Not handled. None of `Food`/`Vitals`/`Notes` plausibly appear in code in daily notes. |
| Multiple `## Food` headings (user error) | First match wins; appends to the first one. |

## Daily-note template changes

`templates/daily-note.md` adds two sections so new notes always have all
three:

```markdown
---
... existing frontmatter ...
---

## Food

## Vitals

## Notes

```

For older notes lacking one of these sections, the first command that
targets it inserts via `body::ensure_section`. No batch rewrite of
historical files.

## Configuration

### `[notes.aliases]`

```toml
[notes.aliases]
med-morning = "Morgonmedicin (Elvanse 70mg, Escitalopram 20mg, Losartan/Hydro 100/12.5mg, Vialerg 10mg)"
med-evening = "Kvällsmedicin (Escitalopram 10mg, Losartan/Hydro 100/12.5mg)"
```

```rust
#[derive(Debug, Clone, Default, Deserialize)]
pub struct NotesConfig {
    #[serde(default)]
    pub aliases: HashMap<String, String>,
}

pub struct Config {
    // ... existing fields ...
    #[serde(default)]
    pub notes: NotesConfig,
}
```

`presets/default.toml` documents the option as a commented example.

### No new BP config

The 14:00 morning/evening cutoff is hard-coded. If Adrian wants a
configurable cutoff later, `[vitals]` table with `bp_morning_cutoff_hour`
is a trivial follow-up.

## Data flow examples

Starting from today's note (older, only `## Notes`):

```markdown
---
date: 2026-04-30
sleep: "10:30pm-6:15am"
weight: 173.4
---

## Notes
- **08:30** Woke up
```

### `daylog food "kelda skogssvampsoppa" 500g` at 12:42

Lookup returns `per_100g: { kcal: 70, protein: 1.4, carbs: 4.8, fat: 5.0 }`,
`gi: 40`, `gl_per_100g: 2`, `ii: 35`, `name: "Kelda Skogssvampsoppa"`.
Scaling × 5 (since 500g / 100g) → 350 kcal, 7.0g protein, 24.0g carbs,
25.0g fat, GL = 10.0, GI/II raw.

```markdown
## Food
- **12:42** Kelda Skogssvampsoppa (500g) (350 kcal, 7.0g protein, 24.0g carbs, 25.0g fat) | GI ~40, GL ~10.0, II ~35

## Notes
- **08:30** Woke up
```

`## Food` was inserted before `## Notes` per canonical order.

### `daylog bp 141 96 70` at 07:30 (auto-morning)

YAML gains three scalars; `## Vitals` is inserted between `## Food` and
`## Notes`:

```markdown
---
date: 2026-04-30
sleep: "10:30pm-6:15am"
weight: 173.4
bp_morning_sys: 141
bp_morning_dia: 96
bp_morning_pulse: 70
---

## Food
- **12:42** ...

## Vitals
- **07:30** BP: 141/96, pulse 70 bpm

## Notes
- **08:30** Woke up
```

Both YAML and body changes flushed in a single `atomic_write`.

### `daylog note med-morning` at 07:55 (alias)

```markdown
## Notes
- **07:55** Morgonmedicin (Elvanse 70mg, Escitalopram 20mg, Losartan/Hydro 100/12.5mg, Vialerg 10mg)
- **08:30** Woke up
```

### `daylog food te` at 14:50 (total-panel lookup, no amount)

For an entry with `total: { weight_g: 200, kcal: 2, ... }`:

```markdown
- **14:50** Te, Earl Grey, hot (200g) (2 kcal, ...)
```

(`weight_g` is shown in the amount slot; if `weight_g` were absent, the
parens would be omitted entirely.)

### `daylog food --kcal 350 --protein 7 --carbs 24 --fat 25 --gi 50 "Random pasta dish" 500g`

Custom mode skips DB lookup; auto-computes `gl = 50 × 24 / 100 = 12.0`.
No `--ii` given → II omitted from glycemic segment:

```
- **HH:MM** Random pasta dish (500g) (350 kcal, 7.0g protein, 24.0g carbs, 25.0g fat) | GI ~50, GL ~12.0
```

### `daylog note --date 2026-04-29 --time 22:30 "Aritonin"` (retroactive)

Both flags resolve directly: target file is `2026-04-29.md` (not today),
time prefix is the parsed `--time` value:

```markdown
## Notes
- **22:30** Aritonin
```

Useful when catching up the next morning, or when day_start_hour didn't
catch a late-night entry.

### `daylog bp --time 08:00 141 96 70` at 14:30 (retroactive morning)

Slot detection uses the `--time` value (08:00), not `Local::now()`. So
this writes `bp_morning_*` even though it's actually 14:30 when the
command runs:

```markdown
## Vitals
- **08:00** BP: 141/96, pulse 70 bpm
```

## Error handling matrix

| Situation | Behavior |
|---|---|
| `daylog food <name>` — no amount, food has only `per_100g`/`per_100ml` | Error: requires amount; suggests `'500g'` or `'250ml'` |
| `daylog food <name> 500g` — food has only `per_100ml`, no density | Error: liquid only, needs ml input or density |
| `daylog food <name> 250ml` — food has only `per_100g`, no density | Mirror error |
| `daylog food <name> 500g` — food has both panels | Use `per_100g` (input unit decides) |
| `daylog food <name> 500g` — food has only `total` panel | Warning to stderr (amount ignored); use total as-is |
| `daylog food <name>` — name not in DB, no `--kcal/...` | Error: suggest add to nutrition-db.md, use known alias, or pass custom flags |
| `daylog food --kcal 350 <name> 500g` — partial flag set | clap-level error: all four macros required together |
| `daylog food <name> 500g` — DB doesn't exist | Error: suggest `daylog init` / `daylog sync` |
| `daylog food <name> 500abc` | Parse error: invalid amount, expects `g`/`ml` suffix or bare number |
| `daylog note <text>` — alias match | Expand alias |
| `daylog note <text>` — no alias match | Treat as literal |
| `daylog note ""` (empty) | Error: note text required |
| `daylog bp 141 96 70` — non-numeric arg | Error: integers required for sys/dia/pulse |
| `daylog bp 141 96` — only 2 args | clap-level usage error |
| `daylog bp` with both `--morning` and `--evening` | clap-level error (mutually exclusive group) |
| `daylog bp` — sys/dia/pulse out of plausible range | Warning to stderr, but write proceeds |
| Today's note doesn't exist | Render from updated template, then proceed |
| Section already exists | `ensure_section` is a no-op; append works |
| Section heading variant (`##  Food`, `### Food`) | Strict match misses → duplicate inserted (acknowledged) |
| Concurrent watcher reading mid-write | No issue — `atomic_write` is rename-into-place |
| `--date 2026-13-45` (invalid) | Parse error: `"Invalid --date: '<x>'. Expected YYYY-MM-DD."` |
| `--date 2099-01-01` (far future) | Allowed — daylog doesn't gate on plausibility; user takes responsibility |
| `--time 25:00` (invalid) | Parse error: `"Invalid --time: '<x>'. Expected HH:MM (24h) or H:MMam/pm (12h)."` (mirrors sleep-cmd) |
| `--time` and `--date` together targeting a different day | Both honored — entry written to `--date`'s note with `--time`'s prefix |

## CLI definition

In `src/cli/mod.rs`, three new variants:

```rust
/// Log a food entry to today's `## Food` section
Food {
    /// Name (literal or nutrition-db alias)
    name: String,
    /// Amount with optional unit (e.g., 500g, 250ml). Required for
    /// per_100g/per_100ml entries; optional for total-panel entries.
    amount: Option<String>,
    /// Custom kcal value (skips nutrition-db lookup)
    #[arg(long)] kcal: Option<f64>,
    #[arg(long)] protein: Option<f64>,
    #[arg(long)] carbs: Option<f64>,
    #[arg(long)] fat: Option<f64>,
    #[arg(long)] gi: Option<f64>,
    #[arg(long)] gl: Option<f64>,
    #[arg(long)] ii: Option<f64>,
    /// Override target date (YYYY-MM-DD). Default: effective_today.
    #[arg(long)] date: Option<String>,
    /// Override entry time (HH:MM 24h or H:MMam/pm 12h). Default: now.
    #[arg(long)] time: Option<String>,
},
/// Log a note to today's `## Notes` section
Note {
    #[arg(long)] date: Option<String>,
    #[arg(long)] time: Option<String>,
    /// Note text or config alias key (joined — no shell quoting needed)
    #[arg(trailing_var_arg = true)]
    text: Vec<String>,
},
/// Log a blood pressure reading (YAML + `## Vitals` line)
Bp {
    sys: i32,
    dia: i32,
    pulse: i32,
    #[arg(long, conflicts_with = "evening")]
    morning: bool,
    #[arg(long)]
    evening: bool,
    #[arg(long)] date: Option<String>,
    #[arg(long)] time: Option<String>,
},
```

`main.rs` dispatches them to the new modules, mirroring `cmd_log`.

### Custom-mode flag validation (food)

clap's derive doesn't natively express "these four flags are required as a
group, all or none" without verbose `requires_all` chains. The food command
does a simple runtime check: if *any* of `--kcal/--protein/--carbs/--fat`
is set, all four must be set, else error. `--gi/--gl/--ii` are
independently optional in both modes.

## Testing strategy

### `body.rs` (pure functions)

- `ensure_section_inserts_food_before_notes`
- `ensure_section_inserts_vitals_between_food_and_notes`
- `ensure_section_inserts_at_end_when_no_later_section`
- `ensure_section_idempotent_if_present`
- `ensure_section_handles_no_body` (frontmatter only)
- `append_line_to_existing_empty_section`
- `append_line_skips_trailing_blank_lines`
- `append_line_preserves_subsequent_section`
- `roundtrip_with_atomic_write`

### `food_cmd.rs`

- `parses_amount_with_g_suffix` / `parses_amount_with_ml_suffix` /
  `parses_bare_number_as_grams`
- `rejects_amount_garbage`
- `lookup_solid_with_grams_scales_per_100g`
- `lookup_liquid_with_ml_scales_per_100ml`
- `lookup_solid_with_ml_uses_density`
- `lookup_solid_with_ml_no_density_errors`
- `lookup_total_panel_no_amount_uses_totals_directly`
- `lookup_total_panel_no_amount_no_weight_g_omits_amount_segment`
- `lookup_missing_name_errors_with_suggestion`
- `custom_mode_requires_all_four_macros` (clap-level if possible)
- `custom_mode_with_gi_carbs_no_gl_autocomputes_gl`
- `custom_mode_omits_glycemic_segment_when_no_gi_gl_ii`
- `output_line_format_kcal_whole_macros_one_decimal`
- `output_line_omits_glycemic_segment_when_food_has_no_gi_gl_ii`
- `output_uses_time_format_12h` / `output_uses_time_format_24h`
- `db_missing_lookup_mode_errors_with_init_suggestion`
- `db_missing_custom_mode_works_without_db`
- `flag_date_writes_to_named_day_not_today`
- `flag_time_overrides_now_for_prefix`
- `flag_date_invalid_format_errors`
- `flag_time_invalid_format_errors`

### `note_cmd.rs`

- `note_literal_appends_with_timestamp`
- `note_alias_expands_then_appends`
- `note_alias_falls_through_when_key_not_found_treats_as_literal`
- `note_empty_text_errors`
- `note_creates_section_if_missing`
- `note_flag_date_writes_to_named_day` / `note_flag_time_overrides_now`

### `bp_cmd.rs`

- `bp_auto_morning_before_14`
- `bp_auto_evening_at_14` (boundary: 14:00 → evening)
- `bp_explicit_evening_overrides_time`
- `bp_writes_three_yaml_fields`
- `bp_appends_vitals_line_with_bpm_unit`
- `bp_rerun_morning_overwrites_yaml_appends_vitals` (YAML overwritten,
  body line appended)
- `bp_creates_vitals_section_if_missing`
- `bp_validates_numeric`
- `bp_warns_out_of_range_but_still_writes`
- `bp_atomic_write_yaml_and_body_in_one_pass` (single mtime change)
- `bp_slot_uses_time_flag_when_given` — `--time 08:00` at 14:30 → morning
- `bp_flag_date_writes_to_named_day`
- `bp_no_morning_evening_suffix_in_vitals_line`

### Integration

- `food_then_bp_then_note_full_day_e2e` — empty notes dir, run all three
  on a fresh today's note, assert resulting markdown matches expected
  fixture.
- `older_note_with_only_notes_section_gets_food_section_inserted_correctly`
- `template_renders_with_all_three_sections` —
  `template::render_daily_note` output contains all three section
  headings.

### Manual verification (in implementation plan, not spec)

- Run each new command on a real notes dir; inspect markdown.
- Watch live: edit triggers materializer; `daylog status --json` reflects
  updated YAML metrics for BP within ~1s.
- Tab-complete: `daylog food <TAB>` and `daylog note <TAB>` work via the
  existing completions infrastructure.

## Out of scope (explicitly)

- Migrating Adrian's existing Swedish-labeled `## Food` lines to English.
  Done locally via the daylog skill after this PR ships.
- Configurable BP morning/evening cutoff. Hard-coded at 14:00 for v1.
- Adding `bp_*_pulse` to `[metrics]` config. User-side change.
- A Nutrition tab or BP gauge in the TUI.
- Fuzzy section heading matching.
- `daylog food list` / `daylog food show <name>` — read-side commands
  for the nutrition DB. Possible follow-up.
- A generic `daylog vitals` command. v1 has BP; other vitals get their
  own commands or flags later.
- DB writes from the new commands. Watcher re-materializes; same
  pattern as `daylog log`.
- Auto-cleanup of duplicate Vitals lines from re-running same slot.
  Chronological accumulation in body is the intended behavior.
