# daylog

A terminal dashboard that tracks your life from markdown notes.

![daylog demo](tapes/demo.gif)

## Quick Start

```bash
cargo install daylog
daylog init
daylog
```

Three commands to a working dashboard. No API keys, no Docker, no config files to write.

## What It Does

daylog reads your daily markdown notes (one per day, `YYYY-MM-DD.md`) and renders a live terminal dashboard. Edit a note, save it, see the TUI update in real time.

```yaml
---
date: 2026-03-28
sleep: "10:30pm-6:15am"
weight: 173.4
mood: 4
energy: 3
type: lifting
lifts:
  squat: 185x5, 205x3, 225x1
  pullup: BWx8, BWx6
resting_hr: 52
---

## Notes

Hit a squat PR today.
```

## Three Tiers of Extensibility

### Tier 1: Track any number (config only)

```toml
[metrics]
resting_hr = { display = "Resting HR", color = "red", unit = "bpm" }
```

Add a YAML field, get a sparkline. Zero code.

### Tier 2: Track any exercise (config only)

```toml
[exercises]
turkish_getup = { display = "Turkish Getup", color = "cyan" }
```

Training tab shows it. Trends tab shows 1RM progression. Zero code.

### Tier 3: Build a module (code required)

For domains needing custom tables and visualization. The climbing module is the reference implementation — one directory, one trait, one line in the registry.

## CLI

```bash
daylog                          # Launch the TUI
daylog log weight 173.4         # Log a value (no quotes needed)
daylog log lift squat 185x5     # Log a lift
daylog log sleep 10:30pm-6:15am # Log sleep
daylog log metric resting_hr 52 # Log a custom metric
daylog status --json            # Today's data as JSON
daylog edit                     # Open today's note in $EDITOR
daylog sync                     # Sync DB without launching TUI
daylog rebuild                  # Rebuild DB from all notes
```

## Tabs

- **Dashboard**: Today's vitals — sleep, weight, mood, energy, session context
- **Training**: Lifts, TSB gauge, session metrics
- **Trends**: 42-day sparklines for weight, exercises, and custom metrics
- **Climbing** (opt-in): Grade pyramid, weekly progression, session summary

## Config

`~/.config/daylog/config.toml`:

```toml
notes_dir = "~/daylog-notes"
# refresh_secs = 15

[modules]
# dashboard = true
# training = true
# trends = true
# climbing = false

[exercises]
squat = { display = "Squat", color = "cyan" }
bench = { display = "Bench", color = "green" }
deadlift = { display = "Deadlift", color = "yellow" }
ohp = { display = "OHP", color = "magenta" }
pullup = { display = "Pullup", color = "blue" }
rdl = { display = "RDL", color = "red" }

[metrics]
# resting_hr = { display = "Resting HR", color = "red", unit = "bpm" }
```

Exercises, metrics, and colors hot-reload without restart. Module enable/disable requires restart.

## AI-Native

daylog is designed for AI agents:

- `daylog log` lets your AI assistant track your workout
- `daylog status --json` provides structured data for AI analysis
- SQLite DB is directly queryable for complex questions
- Ships with a Claude Code skill for seamless integration
- `AGENTS.md` documents the full AI interface

## Architecture

Two threads, one SQLite database (WAL mode), no async runtime.

- **Watcher thread**: Detects file changes, parses YAML, writes to SQLite
- **TUI thread**: Reads from SQLite, renders with ratatui

The file is the source of truth. The database is a materialized view.

## Contributing

- **Submit your preset**: Use a different exercise set? Share your `config.toml`
- **Build a module**: See `AGENTS.md` for the scaffolding guide

## License

MIT
