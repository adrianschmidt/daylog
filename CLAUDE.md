# daylog

A terminal dashboard that tracks your life from markdown notes. Single binary, no async, no external APIs.

## Architecture

```
Two threads, one database, no channels.

Watcher Thread                    TUI Thread
  file changed →                    every 15s →
  parse YAML →                      open read-only conn →
  module.normalize() →              module.draw(conn) →
  INSERT to SQLite (WAL) →          render frame
```

SQLite WAL mode is the coordination mechanism. Watcher writes, TUI reads on a fresh connection each refresh.

## Tech Stack

| Dep | Why |
|-----|-----|
| ratatui + crossterm | TUI rendering |
| rusqlite (bundled) | Embedded database, no external SQLite needed |
| yaml-rust2 | YAML parsing with manual control over error recovery |
| notify | Cross-platform file watching |
| clap | CLI argument parsing |
| color-eyre | Error handling with actionable suggestions |
| chrono | Date handling |

## File Map

```
src/
  main.rs              CLI dispatch (init, run, log, status, sync, edit, rebuild)
  lib.rs               Module declarations
  app.rs               TUI event loop, tab management, signal handling
  config.rs            TOML config loading, hot-reload, tilde expansion
  db.rs                Core tables (days, metrics, sync_meta, foods, food_aliases,
                       food_ingredients), migrations, queries, FoodInsert/FoodLookup
                       types, insert_food, lookup_food_by_name_or_alias, nutrition_status
  demo.rs              14-day demo data generator
  materializer/
    mod.rs             Re-exports for daily and nutrition parsers
    daily.rs           Daily-note YAML preprocessor + parser, file watcher dispatch,
                       FileKind + materialized_file_kind
    nutrition.rs       nutrition-db.md parser (## headings + fenced YAML), foods
                       materialization with DELETE-then-INSERT-all
  frontmatter.rs       Line-oriented YAML frontmatter editor (preserves comments)
  body.rs              Line-oriented `## Section` primitives (ensure_section,
                       append_line_to_section). Sibling to frontmatter.rs.
                       Pure functions over &str.
  cli/
    mod.rs             Clap CLI definition
    bp_cmd.rs          `daylog bp` — slot dispatch + YAML scalars + Vitals line
    completions.rs     Shell completion generation
    food_cmd.rs        `daylog food` — nutrition-db lookup, scaling, custom flags
    log_cmd.rs         `daylog log` — write to today's note
    note_cmd.rs        `daylog note` — alias resolution + body append
    readme_cmd.rs      `daylog readme` — print embedded README.md to stdout
  modules/
    mod.rs             Module trait + registry + InsertOp + YamlPath + parse_color
    dashboard.rs       Today's vitals (sleep, weight, mood, energy)
    training.rs        Sessions, lifts, TSB gauge (owns sessions + lift_sets tables)
    trends.rs          Sparklines for exercises + custom metrics
    climbing/
      mod.rs           Opt-in reference module: normalize, draw, grade parsing
```

## Development

```bash
just build          # cargo build
just test           # cargo test
just lint           # cargo fmt --check && cargo clippy
just run            # cargo run (launches TUI)
just demo           # init + run with demo data
```

## Extension Recipes

### Adding a new module (7 steps)

1. Create `src/modules/yourmod/` with `mod.rs`
2. Copy climbing module as template
3. Implement Module trait: `id()`, `name()`, `schema()`, `normalize()`, `draw()`
4. Add `pub mod yourmod;` to `src/modules/mod.rs`
5. Add to `build_registry()` in `src/modules/mod.rs` (one line)
6. Add `[modules] yourmod = false` to config
7. Add tests

### Adding a custom metric (no code, 2 steps)

1. Add to config.toml:
   ```toml
   [metrics]
   resting_hr = { display = "Resting HR", color = "red", unit = "bpm" }
   ```
2. Add to daily note YAML: `resting_hr: 52`
3. Trends tab auto-renders a sparkline. Run `daylog rebuild` to backfill history.

### Adding an exercise (no code, 1 step)

1. Add to config.toml:
   ```toml
   [exercises]
   turkish_getup = { display = "Turkish Getup", color = "cyan" }
   ```
2. Use in YAML: `lifts:\n  turkish_getup: 35x5, 35x5`

## Debugging

- `daylog rebuild` — delete DB and re-parse all notes
- `daylog status --json` — inspect current state
- `daylog sync` — update DB without launching TUI
- DB is at `{notes_dir}/.daylog.db`, inspectable with `sqlite3`
- Watcher logs to stderr

## Code Conventions

- No `.unwrap()` in library code. Use `color_eyre::Result`.
- `rustfmt` + `clippy` clean.
- Each module is self-contained. Modules never import from another module.
- Core tables in `db.rs`. Module tables in module's own code via `schema()`.
- Every user-facing error has a `.suggestion()` with a concrete next step.

## v1 Notes

- Sessions table supports multiple per day (PK is `(date, session_number)`). v1 YAML maps to session_number=1.
- Module enable/disable requires restart (registry rebuild).
- `daylog rebuild` is the schema migration answer — fast (<1s for hundreds of notes).
- Module registry is compile-time. Community modules require recompiling.
