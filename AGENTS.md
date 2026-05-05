# vitalog AI Agent Interface

vitalog is designed for AI agents to read, write, and extend. This document is the authoritative reference for AI integration.

## 1. Reading Data

### Quick check: `vitalog status --json`

Returns today's vitals, trends, and module data as JSON:

```json
{
  "today": {
    "date": "2026-03-28",
    "sleep_start": "10:30pm",
    "sleep_end": "6:15am",
    "sleep_hours": 7.75,
    "sleep_quality": 4,
    "mood": 4,
    "energy": 3,
    "weight": 173.4,
    "notes": "Good session today."
  },
  "training": { ... },
  "climbing": { ... }
}
```

### Direct SQLite queries

The database is at `{notes_dir}/.vitalog.db`. Open read-only:

```bash
sqlite3 -readonly ~/vitalog-notes/.vitalog.db
```

#### Core tables

```sql
-- Today's vitals
SELECT * FROM days WHERE date = date('now');

-- Weight trend (last 30 days)
SELECT date, weight FROM days WHERE weight IS NOT NULL ORDER BY date DESC LIMIT 30;

-- Custom metrics
SELECT date, name, value FROM metrics WHERE name = 'resting_hr' ORDER BY date DESC LIMIT 14;

-- Sleep trend
SELECT date, sleep_hours FROM days WHERE sleep_hours IS NOT NULL ORDER BY date DESC LIMIT 14;
```

#### Training queries

```sql
-- Today's lifts
SELECT exercise, set_number, weight_lbs, reps, estimated_1rm
FROM lift_sets WHERE date = date('now') ORDER BY exercise, set_number;

-- Squat 1RM progression
SELECT date, MAX(estimated_1rm) as max_1rm
FROM lift_sets WHERE exercise = 'squat'
GROUP BY date ORDER BY date DESC LIMIT 42;

-- Training load for TSB calculation
SELECT date, s.rpe * s.duration as load
FROM sessions s WHERE date >= date('now', '-42 days');

-- Session history
SELECT date, session_type, duration, rpe FROM sessions ORDER BY date DESC LIMIT 14;
```

#### Climbing queries (if module enabled)

```sql
-- Grade pyramid (last 8 weeks)
SELECT grade_normalized, SUM(count) as total
FROM climbs WHERE climb_type = 'send' AND date >= date('now', '-56 days')
GROUP BY grade_normalized ORDER BY grade_normalized DESC;

-- Weekly max grade
SELECT strftime('%Y-W%W', date) as week, MAX(grade_normalized) as max_grade
FROM climbs WHERE climb_type = 'send'
GROUP BY week ORDER BY week DESC LIMIT 12;

-- Today's climbing
SELECT climb_type, grade_raw, count, board FROM climbs WHERE date = date('now');
```

## 2. Writing Data

### `vitalog log` command

Write values to today's daily note. All args after the field name are joined as the value — no shell quoting needed.

```bash
# Core fields
vitalog log weight 173.4
vitalog log sleep 10:30pm-6:15am
vitalog log mood 4
vitalog log energy 3

# Training fields (routed through training module)
vitalog log lift pullup BWx8, BWx6
vitalog log lift squat 185x5, 205x3, 225x1
vitalog log session strength
vitalog log duration 45
vitalog log rpe 7

# Climbing fields (routed through climbing module, if enabled)
vitalog log climb send V5
vitalog log climb attempt V7

# Custom metrics (any [metrics] key from config)
vitalog log metric resting_hr 52
vitalog log metric meditation_min 15
```

**IMPORTANT:** `vitalog log` calls must be serialized, not parallel.
- Safe: `vitalog log weight 173 && vitalog log mood 4`
- NOT safe: `vitalog log weight 173 & vitalog log mood 4`

### `vitalog edit`

Opens today's note in `$EDITOR`. Creates from template if missing.

```bash
vitalog edit                    # today
vitalog edit 2026-03-25         # specific date
```

### `vitalog sync`

Forces an incremental DB update without launching the TUI.

```bash
vitalog sync
```

### Notes format

Markdown files with YAML frontmatter, named `YYYY-MM-DD.md`:

```yaml
---
date: 2026-03-28
sleep: "10:30pm-6:15am"
sleep_quality: 4
mood: 4
energy: 3
weight: 173.4
type: lifting
week: 3
block: volume
duration: 45
rpe: 7
lifts:
  squat: 185x5, 205x3, 225x1
  bench: 135x8, 135x8
  pullup: BWx8, BWx6
resting_hr: 52
---

## Notes

Good session. Hit a squat PR.
```

## 3. Extending

### Tier 1: Add a custom metric (config only)

Add to `~/.config/vitalog/config.toml`:
```toml
[metrics]
resting_hr = { display = "Resting HR", color = "red", unit = "bpm" }
```

Add the field to your daily notes. Trends tab auto-renders a sparkline. Run `vitalog rebuild` to backfill historical data.

### Tier 2: Add an exercise (config only)

```toml
[exercises]
turkish_getup = { display = "Turkish Getup", color = "cyan" }
```

Use in YAML: `lifts:\n  turkish_getup: 35x5, 35x5`

### Tier 3: Build a custom module

For domains needing custom tables and visualization. The climbing module is the reference implementation.

1. Create `src/modules/yourmod/mod.rs`
2. Implement `Module` trait:
   - `id()` → unique string matching `[modules.yourmod]`
   - `name()` → tab display name
   - `schema()` → SQL CREATE TABLE statements
   - `normalize(date, yaml, config)` → return `Vec<InsertOp>`
   - `draw(frame, area, conn, config)` → render with ratatui
3. Add `pub mod yourmod;` to `src/modules/mod.rs`
4. Add to `build_registry()` (one line)
5. Add `[modules] yourmod = false` to config
6. Define YAML format for your domain
7. Add tests

Key constraints:
- Modules are stateless (config at construction only, immutable)
- `normalize()` runs on watcher thread — return InsertOps, don't write to DB
- `draw()` gets a read-only connection — cannot write from the render path
- Table names in InsertOp must be `&'static str` (compile-time strings)
- Module tables use `ON DELETE CASCADE` referencing `days(date)`
