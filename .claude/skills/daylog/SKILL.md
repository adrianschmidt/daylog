---
name: daylog
description: >-
  Track daily life data via the daylog terminal dashboard. Log workouts, sleep,
  weight, mood, and custom metrics from the command line. Query trends and
  status via JSON. Use when user asks to log a workout, check weight trend,
  record sleep, track a metric, or says "/daylog".
---

# daylog

Terminal dashboard that tracks daily life from markdown notes.

## Quick Reference

| Action | Command |
|--------|---------|
| Log a value | `daylog log weight 173.4` |
| Log a lift | `daylog log lift pullup BWx8, BWx6` |
| Log sleep | `daylog log sleep 10:30pm-6:15am` |
| Log a metric | `daylog log metric resting_hr 52` |
| Edit today's note | `daylog edit` |
| Edit a past note | `daylog edit 2026-03-25` |
| Sync DB (no TUI) | `daylog sync` |
| Check today | `daylog status --json` |
| Query DB | `sqlite3 <notes_dir>/.daylog.db "<query>"` |
| Open TUI | `daylog` |

All args after the field name are joined — no shell quoting needed.

## Reading Data

`daylog status --json` returns today's vitals, trends, and module data.
For complex queries, use the SQLite DB directly.

-> See [AGENTS.md](../../AGENTS.md) S1 "Reading data" for full schema, field
descriptions, and example queries.

## Writing Data

`daylog log <field> <value...>` writes to today's note. Creates the note
from template if missing. The file watcher picks up changes automatically.

-> See [AGENTS.md](../../AGENTS.md) S2 "Writing data" for all field types
and format examples.

## Extending

Add custom metrics or exercises via config (no code). Build domain modules
for custom visualizations.

-> See [AGENTS.md](../../AGENTS.md) S3 "Extending" for scaffolding guide.
-> See [CLAUDE.md](../../CLAUDE.md) S3 "Extension recipes" for code patterns.

## Config

`~/.config/daylog/config.toml` — hot-reloaded for exercises, metrics,
colors. Module enable/disable requires restart.

## Notes Format

Markdown files with YAML frontmatter in `YYYY-MM-DD.md` format.
-> See [templates/daily-note.md](../../templates/daily-note.md) for all
supported fields.
