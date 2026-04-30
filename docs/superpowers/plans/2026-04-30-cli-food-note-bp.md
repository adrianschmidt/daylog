# CLI commands for Food, Notes, and Vitals — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `daylog food`, `daylog note`, and `daylog bp` top-level subcommands that append timestamped entries to `## Food` / `## Notes` / `## Vitals` sections (auto-inserting the sections in canonical order when missing), with nutrition-db lookup for food and YAML+body double-write for BP.

**Architecture:** Three sibling CLI modules (`src/cli/{food,note,bp}_cmd.rs`) share a new pure-function module `src/body.rs` for line-oriented `## Section` primitives (mirrors `src/frontmatter.rs`'s style). Atomic file writes via existing `frontmatter::atomic_write`. Commands write only to markdown; the watcher re-materializes the DB via the existing pipeline. All three commands accept shared `--date` and `--time` flags.

**Tech Stack:** Rust, clap (derive macros for subcommands), `color_eyre::Result` with `Section::suggestion` for actionable errors, `chrono::{NaiveDate, NaiveTime, Local}` for time/date handling, `rusqlite` for the read-only nutrition lookup.

**Spec:** [`docs/superpowers/specs/2026-04-30-cli-food-note-bp-design.md`](../specs/2026-04-30-cli-food-note-bp-design.md)

---

## File Structure

**Create:**
- `src/body.rs` — `ensure_section`, `append_line_to_section`. Pure-function module mirroring `frontmatter.rs` style.
- `src/cli/food_cmd.rs` — `daylog food` handler.
- `src/cli/note_cmd.rs` — `daylog note` handler.
- `src/cli/bp_cmd.rs` — `daylog bp` handler.

**Modify:**
- `src/lib.rs` — `pub mod body;`.
- `src/cli/mod.rs` — `pub mod food_cmd;`, `pub mod note_cmd;`, `pub mod bp_cmd;`, plus `Food`, `Note`, `Bp` variants in `Commands`.
- `src/main.rs` — dispatch new subcommands.
- `src/config.rs` — add `NotesConfig { aliases: HashMap<String, String> }` and `Config.notes` field.
- `templates/daily-note.md` — add `## Food` and `## Vitals` (above the existing `## Notes`).
- `presets/default.toml` — commented `[notes.aliases]` example.
- `README.md` — document the three new commands and `[notes.aliases]` config.
- `CLAUDE.md` — File Map updates.

**Test files:** All unit tests live in `#[cfg(test)] mod tests { ... }` inside the source files (project convention). Integration tests get one new test in `tests/integration.rs`.

---

## Build / test commands (recap)

- `just test` (or `cargo test`) — run all tests.
- `cargo test --lib body::tests` — run a specific module's tests.
- `just lint` — `cargo fmt --check && cargo clippy`.
- `just build` — `cargo build`.

Each task ends with `cargo test` (full suite) plus the targeted module test, then a commit.

---

## Task 1: `body.rs` — `ensure_section` primitive

**Files:**
- Create: `src/body.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Add module declaration**

In `src/lib.rs`, add `pub mod body;` next to the other `pub mod` lines. Run `cargo check` — expect failure (`body.rs` doesn't exist yet).

- [ ] **Step 2: Create `src/body.rs` skeleton**

Write the following to `src/body.rs`:

```rust
//! Line-oriented `## Section` primitives for the markdown body. Sibling
//! to `frontmatter.rs`. Pure functions over `&str`; no I/O, no DB.
//!
//! The canonical section order baked into `ensure_section` is the order
//! the daily-note template uses. Inserting a missing section walks
//! `CANONICAL_SECTION_ORDER`: a missing section lands after the last
//! existing predecessor and before the first existing successor.

pub const CANONICAL_SECTION_ORDER: &[&str] = &["Food", "Vitals", "Notes"];

/// Ensure a `## <section>` heading exists in the body, inserting it in
/// canonical order if missing. Returns the (possibly unchanged) content.
pub fn ensure_section(content: &str, section: &str) -> String {
    todo!("implemented in step 4")
}
```

- [ ] **Step 3: Write failing tests**

Append to `src/body.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const ONLY_NOTES: &str = "---\ndate: 2026-04-30\n---\n\n## Notes\n\n";
    const FOOD_AND_NOTES: &str = "---\ndate: 2026-04-30\n---\n\n## Food\n\n## Notes\n\n";
    const ONLY_FOOD: &str = "---\ndate: 2026-04-30\n---\n\n## Food\n\n";
    const FRONTMATTER_ONLY: &str = "---\ndate: 2026-04-30\n---\n";
    const NO_FRONTMATTER: &str = "## Notes\n\n";

    #[test]
    fn ensure_section_inserts_food_before_notes() {
        let result = ensure_section(ONLY_NOTES, "Food");
        let food_idx = result.find("## Food").expect("Food heading inserted");
        let notes_idx = result.find("## Notes").expect("Notes still present");
        assert!(food_idx < notes_idx, "Food must precede Notes:\n{result}");
    }

    #[test]
    fn ensure_section_inserts_vitals_between_food_and_notes() {
        let result = ensure_section(FOOD_AND_NOTES, "Vitals");
        let food_idx = result.find("## Food").unwrap();
        let vitals_idx = result.find("## Vitals").unwrap();
        let notes_idx = result.find("## Notes").unwrap();
        assert!(food_idx < vitals_idx && vitals_idx < notes_idx, "got:\n{result}");
    }

    #[test]
    fn ensure_section_inserts_at_end_when_no_later_section() {
        let result = ensure_section(ONLY_FOOD, "Notes");
        let food_idx = result.find("## Food").unwrap();
        let notes_idx = result.find("## Notes").unwrap();
        assert!(food_idx < notes_idx, "got:\n{result}");
    }

    #[test]
    fn ensure_section_idempotent_if_present() {
        let result1 = ensure_section(ONLY_NOTES, "Notes");
        let result2 = ensure_section(&result1, "Notes");
        assert_eq!(result1, result2);
        assert_eq!(result1.matches("## Notes").count(), 1);
    }

    #[test]
    fn ensure_section_handles_no_body() {
        let result = ensure_section(FRONTMATTER_ONLY, "Notes");
        assert!(result.contains("## Notes"));
    }

    #[test]
    fn ensure_section_handles_no_frontmatter() {
        let result = ensure_section(NO_FRONTMATTER, "Food");
        let food_idx = result.find("## Food").unwrap();
        let notes_idx = result.find("## Notes").unwrap();
        assert!(food_idx < notes_idx, "got:\n{result}");
    }

    #[test]
    fn ensure_section_preserves_frontmatter_exactly() {
        let result = ensure_section(ONLY_NOTES, "Food");
        assert!(result.starts_with("---\ndate: 2026-04-30\n---\n"));
    }
}
```

- [ ] **Step 4: Run tests to verify they fail**

```bash
cargo test --lib body::tests 2>&1 | tail -20
```
Expected: 7 tests panic with `not yet implemented` / `todo!`.

- [ ] **Step 5: Implement `ensure_section` and helpers**

Replace the `todo!()` body and append helpers:

```rust
pub fn ensure_section(content: &str, section: &str) -> String {
    let (header, body) = split_at_body(content);
    let body_lines: Vec<&str> = body.lines().collect();

    // Existing h2 headings in body, with their line indices.
    let mut existing: Vec<(usize, &str)> = Vec::new();
    for (i, line) in body_lines.iter().enumerate() {
        if let Some(name) = parse_h2_heading(line) {
            existing.push((i, name));
        }
    }

    if existing.iter().any(|(_, name)| *name == section) {
        return content.to_string();
    }

    let target_pos = canonical_position(section);
    let insert_at_line = existing
        .iter()
        .find(|(_, name)| canonical_position(name) > target_pos)
        .map(|(i, _)| *i);

    let new_body = match insert_at_line {
        Some(idx) => {
            // Insert heading + blank line before line `idx`.
            let mut out: Vec<String> =
                body_lines.iter().take(idx).map(|s| s.to_string()).collect();
            out.push(format!("## {section}"));
            out.push(String::new());
            out.extend(body_lines.iter().skip(idx).map(|s| s.to_string()));
            join_with_trailing_newline(&out, body)
        }
        None => {
            let mut out: Vec<String> = body_lines.iter().map(|s| s.to_string()).collect();
            // Drop trailing blank lines so we control separation precisely.
            while out.last().map(|l| l.is_empty()).unwrap_or(false) {
                out.pop();
            }
            // Add a blank line between previous content and the new heading
            // unless the body was empty.
            if !out.is_empty() {
                out.push(String::new());
            }
            out.push(format!("## {section}"));
            out.push(String::new());
            join_with_trailing_newline(&out, body)
        }
    };

    format!("{header}{new_body}")
}

/// Split content into (header, body) where header is everything up to
/// and including the closing `---\n` of frontmatter (or `""` if no
/// frontmatter is present), and body is the remainder.
fn split_at_body(content: &str) -> (&str, &str) {
    if !content.starts_with("---\n") {
        return ("", content);
    }
    // Skip the opening "---\n" line, then look for a line that is exactly
    // "---" terminated by '\n' or end-of-string.
    let after_open = 4; // "---\n"
    let rest = &content[after_open..];

    let mut cursor = after_open;
    for line in rest.split_inclusive('\n') {
        let line_len = line.len();
        let trimmed = line.trim_end_matches('\n');
        cursor += line_len;
        if trimmed == "---" {
            return (&content[..cursor], &content[cursor..]);
        }
    }
    ("", content) // no closing --- found; treat entire content as body
}

fn parse_h2_heading(line: &str) -> Option<&str> {
    line.strip_prefix("## ").map(|s| s.trim())
}

fn canonical_position(section: &str) -> usize {
    CANONICAL_SECTION_ORDER
        .iter()
        .position(|&s| s == section)
        .unwrap_or(usize::MAX)
}

fn join_with_trailing_newline(lines: &[String], original_body: &str) -> String {
    let mut s = lines.join("\n");
    if original_body.ends_with('\n') || !lines.is_empty() {
        s.push('\n');
    }
    s
}
```

- [ ] **Step 6: Run tests**

```bash
cargo test --lib body::tests
```
Expected: 7 passes, 0 failures.

- [ ] **Step 7: Run lint and full test suite**

```bash
just lint && cargo test
```
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add src/body.rs src/lib.rs
git commit -m "$(cat <<'EOF'
feat: body::ensure_section for canonical-order section insertion

Pure-function module sibling to frontmatter.rs. Inserts ## Food /
## Vitals / ## Notes headings in the canonical order baked into
CANONICAL_SECTION_ORDER. Idempotent if the target section already
exists.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: `body.rs` — `append_line_to_section`

**Files:**
- Modify: `src/body.rs`

- [ ] **Step 1: Add stub function**

Above the `#[cfg(test)]` line in `src/body.rs`, add:

```rust
/// Append `<line>` to the named section's body. The caller must call
/// `ensure_section` first; if the section is missing this function
/// returns content unchanged.
pub fn append_line_to_section(content: &str, section: &str, line: &str) -> String {
    todo!("implemented in step 3")
}
```

- [ ] **Step 2: Add failing tests**

Append inside the existing `mod tests`:

```rust
    #[test]
    fn append_into_existing_empty_section() {
        let content = "---\ndate: 2026-04-30\n---\n\n## Food\n\n## Notes\n\n";
        let result = append_line_to_section(content, "Food", "- **12:42** Tea");
        assert!(result.contains("## Food\n- **12:42** Tea"));
        assert!(result.contains("## Notes"));
    }

    #[test]
    fn append_after_existing_items() {
        let content =
            "---\ndate: 2026-04-30\n---\n\n## Food\n- **08:30** Coffee\n\n## Notes\n\n";
        let result = append_line_to_section(content, "Food", "- **12:42** Tea");
        let coffee_idx = result.find("- **08:30** Coffee").unwrap();
        let tea_idx = result.find("- **12:42** Tea").unwrap();
        assert!(coffee_idx < tea_idx);
        // Coffee must still be there.
        assert_eq!(result.matches("- **08:30** Coffee").count(), 1);
    }

    #[test]
    fn append_skips_trailing_blank_lines_within_section() {
        // Section content is followed by blank lines, then next heading.
        let content = "---\nx: 1\n---\n\n## Food\n- **08:30** A\n\n## Notes\n\n";
        let result = append_line_to_section(content, "Food", "- **09:00** B");
        // New line lands between A and the blank+next heading.
        let a_idx = result.find("- **08:30** A").unwrap();
        let b_idx = result.find("- **09:00** B").unwrap();
        let notes_idx = result.find("## Notes").unwrap();
        assert!(a_idx < b_idx && b_idx < notes_idx);
    }

    #[test]
    fn append_to_section_at_end_of_file() {
        let content = "---\ndate: 2026-04-30\n---\n\n## Food\n\n";
        let result = append_line_to_section(content, "Food", "- **12:42** Tea");
        assert!(result.contains("## Food\n- **12:42** Tea"));
    }

    #[test]
    fn append_preserves_subsequent_section() {
        let content =
            "---\nx: 1\n---\n\n## Food\n- **08:30** A\n\n## Notes\n- **09:00** Slept well\n";
        let result = append_line_to_section(content, "Food", "- **12:42** Tea");
        assert!(result.contains("- **09:00** Slept well"));
        assert!(result.contains("- **12:42** Tea"));
    }

    #[test]
    fn append_to_missing_section_is_no_op() {
        let content = "---\ndate: 2026-04-30\n---\n\n## Notes\n\n";
        let result = append_line_to_section(content, "Food", "- **12:42** Tea");
        assert_eq!(result, content);
    }
```

- [ ] **Step 3: Run failing tests**

```bash
cargo test --lib body::tests::append
```
Expected: 6 panics with `todo!`.

- [ ] **Step 4: Implement `append_line_to_section`**

Replace the `todo!()` body:

```rust
pub fn append_line_to_section(content: &str, section: &str, line: &str) -> String {
    let (header, body) = split_at_body(content);
    let body_lines: Vec<&str> = body.lines().collect();

    let heading_idx = match body_lines
        .iter()
        .position(|l| parse_h2_heading(l).map(|n| n == section).unwrap_or(false))
    {
        Some(i) => i,
        None => return content.to_string(),
    };

    // End-of-section: index of the next ## heading, or len if none.
    let next_idx = body_lines
        .iter()
        .enumerate()
        .skip(heading_idx + 1)
        .find_map(|(i, l)| parse_h2_heading(l).map(|_| i))
        .unwrap_or(body_lines.len());

    // Walk back from `next_idx - 1` skipping blank lines to find the
    // last non-blank line in the section.
    let mut insert_after = heading_idx;
    for i in (heading_idx + 1..next_idx).rev() {
        if !body_lines[i].is_empty() {
            insert_after = i;
            break;
        }
    }

    let mut out: Vec<String> = body_lines
        .iter()
        .take(insert_after + 1)
        .map(|s| s.to_string())
        .collect();
    out.push(line.to_string());
    out.extend(
        body_lines
            .iter()
            .skip(insert_after + 1)
            .map(|s| s.to_string()),
    );

    let new_body = join_with_trailing_newline(&out, body);
    format!("{header}{new_body}")
}
```

- [ ] **Step 5: Run tests**

```bash
cargo test --lib body::tests
```
Expected: all tests pass (13 total).

- [ ] **Step 6: Run lint and full suite**

```bash
just lint && cargo test
```

- [ ] **Step 7: Commit**

```bash
git add src/body.rs
git commit -m "$(cat <<'EOF'
feat: body::append_line_to_section preserves blank-line tail

Inserts the new line after the last non-blank line in the named section,
preserving any trailing blank lines that separate the section from the
next heading. No-op when the section is missing.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Daily-note template — add `## Food` and `## Vitals`

**Files:**
- Modify: `templates/daily-note.md`
- Modify: `src/template.rs`

- [ ] **Step 1: Write failing template tests**

Append to `src/template.rs` inside `mod tests`:

```rust
    #[test]
    fn renders_food_section() {
        let config = config_with_unit("lbs");
        let out = render_daily_note("2026-04-30", &config);
        assert!(out.contains("## Food"), "expected ## Food section, got:\n{out}");
    }

    #[test]
    fn renders_vitals_section() {
        let config = config_with_unit("lbs");
        let out = render_daily_note("2026-04-30", &config);
        assert!(out.contains("## Vitals"), "expected ## Vitals section, got:\n{out}");
    }

    #[test]
    fn renders_sections_in_canonical_order() {
        let config = config_with_unit("lbs");
        let out = render_daily_note("2026-04-30", &config);
        let food = out.find("## Food").expect("## Food");
        let vitals = out.find("## Vitals").expect("## Vitals");
        let notes = out.find("## Notes").expect("## Notes");
        assert!(food < vitals && vitals < notes, "wrong order:\n{out}");
    }
```

- [ ] **Step 2: Run failing tests**

```bash
cargo test --lib template::tests
```
Expected: 3 new tests fail (no `## Food`/`## Vitals` yet).

- [ ] **Step 3: Update the template**

Replace the trailing portion of `templates/daily-note.md` (everything from the closing `---` onward) with:

```markdown
---

## Food

## Vitals

## Notes

```

Full updated file should now end with these three sections, in this order, each followed by one blank line.

- [ ] **Step 4: Run tests**

```bash
cargo test --lib template::tests
```
Expected: all template tests pass.

- [ ] **Step 5: Lint + full suite**

```bash
just lint && cargo test
```

- [ ] **Step 6: Commit**

```bash
git add templates/daily-note.md src/template.rs
git commit -m "$(cat <<'EOF'
feat: daily-note template includes Food and Vitals sections

New notes are rendered with all three canonical body sections. Older
notes that lack one of these get the section auto-inserted by
body::ensure_section on the first command that writes to it.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: `config.rs` — `NotesConfig` with aliases

**Files:**
- Modify: `src/config.rs`

- [ ] **Step 1: Write failing tests**

Append inside `mod tests` in `src/config.rs`:

```rust
    #[test]
    fn parses_notes_aliases() {
        let toml_str = r#"
notes_dir = '/tmp/test'

[notes.aliases]
med-morning = "Morning meds"
med-evening = "Evening meds"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(
            config.notes.aliases.get("med-morning").map(String::as_str),
            Some("Morning meds")
        );
        assert_eq!(
            config.notes.aliases.get("med-evening").map(String::as_str),
            Some("Evening meds")
        );
    }

    #[test]
    fn notes_aliases_default_empty() {
        let config: Config = toml::from_str("notes_dir = '/tmp/test'\n").unwrap();
        assert!(config.notes.aliases.is_empty());
    }
```

- [ ] **Step 2: Run failing tests**

```bash
cargo test --lib config::tests::parses_notes_aliases config::tests::notes_aliases_default_empty
```
Expected: compile errors (`config.notes` field doesn't exist).

- [ ] **Step 3: Add `NotesConfig` struct and `Config.notes` field**

In `src/config.rs`, near the existing `ModulesConfig` struct, add:

```rust
#[derive(Debug, Clone, Default, Deserialize)]
pub struct NotesConfig {
    #[serde(default)]
    pub aliases: HashMap<String, String>,
}
```

In the `Config` struct, add the field (after the existing `metrics` field):

```rust
    #[serde(default)]
    pub notes: NotesConfig,
```

- [ ] **Step 4: Run tests**

```bash
cargo test --lib config::tests
```
Expected: all config tests pass.

- [ ] **Step 5: Lint + full suite**

```bash
just lint && cargo test
```

- [ ] **Step 6: Commit**

```bash
git add src/config.rs
git commit -m "$(cat <<'EOF'
feat: NotesConfig with [notes.aliases] mapping

Adds an optional [notes.aliases] table to config.toml mapping short
keys to longer note text. Consumed by daylog note in a follow-up.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: CLI definition — register subcommands and dispatch stubs

**Files:**
- Modify: `src/cli/mod.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Add module declarations**

In `src/cli/mod.rs`, near the top:

```rust
pub mod bp_cmd;
pub mod food_cmd;
pub mod note_cmd;
```

(Order: alphabetical to keep existing imports tidy.)

- [ ] **Step 2: Add `Food`, `Note`, `Bp` variants in the `Commands` enum**

Append inside `pub enum Commands { ... }`:

```rust
    /// Log a food entry to the day's `## Food` section
    Food {
        /// Name (literal or nutrition-db alias)
        name: String,
        /// Amount with optional unit (e.g., 500g, 250ml). Required for
        /// per_100g/per_100ml entries; optional for total-panel entries.
        amount: Option<String>,
        /// Custom kcal value (skips nutrition-db lookup; requires
        /// --protein, --carbs, --fat to also be set)
        #[arg(long)]
        kcal: Option<f64>,
        #[arg(long)]
        protein: Option<f64>,
        #[arg(long)]
        carbs: Option<f64>,
        #[arg(long)]
        fat: Option<f64>,
        #[arg(long)]
        gi: Option<f64>,
        #[arg(long)]
        gl: Option<f64>,
        #[arg(long)]
        ii: Option<f64>,
        /// Override target date (YYYY-MM-DD). Default: effective_today.
        #[arg(long)]
        date: Option<String>,
        /// Override entry time (HH:MM 24h or H:MMam/pm 12h). Default: now.
        #[arg(long)]
        time: Option<String>,
    },
    /// Log a free-text note to the day's `## Notes` section
    Note {
        #[arg(long)]
        date: Option<String>,
        #[arg(long)]
        time: Option<String>,
        /// Note text or [notes.aliases] key (joined; no shell quoting needed)
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
        #[arg(long)]
        date: Option<String>,
        #[arg(long)]
        time: Option<String>,
    },
```

- [ ] **Step 3: Stub out the three modules**

Create `src/cli/food_cmd.rs`:

```rust
//! `daylog food` — append a food entry to the day's `## Food` section.
//! Implementation lands in subsequent tasks.

use color_eyre::eyre::Result;

use crate::config::Config;

#[allow(clippy::too_many_arguments)]
pub fn execute(
    _name: &str,
    _amount: Option<&str>,
    _kcal: Option<f64>,
    _protein: Option<f64>,
    _carbs: Option<f64>,
    _fat: Option<f64>,
    _gi: Option<f64>,
    _gl: Option<f64>,
    _ii: Option<f64>,
    _date: Option<&str>,
    _time: Option<&str>,
    _config: &Config,
) -> Result<()> {
    color_eyre::eyre::bail!("daylog food: not yet implemented")
}
```

Create `src/cli/note_cmd.rs`:

```rust
//! `daylog note` — append a free-text note to the day's `## Notes` section.

use color_eyre::eyre::Result;

use crate::config::Config;

pub fn execute(
    _text: &[String],
    _date: Option<&str>,
    _time: Option<&str>,
    _config: &Config,
) -> Result<()> {
    color_eyre::eyre::bail!("daylog note: not yet implemented")
}
```

Create `src/cli/bp_cmd.rs`:

```rust
//! `daylog bp` — write blood pressure to YAML + append a `## Vitals` line.

use color_eyre::eyre::Result;

use crate::config::Config;

pub fn execute(
    _sys: i32,
    _dia: i32,
    _pulse: i32,
    _morning: bool,
    _evening: bool,
    _date: Option<&str>,
    _time: Option<&str>,
    _config: &Config,
) -> Result<()> {
    color_eyre::eyre::bail!("daylog bp: not yet implemented")
}
```

- [ ] **Step 4: Wire dispatch in `src/main.rs`**

In the `match cli.command { ... }` block, add new arms (alongside `Some(Commands::Log { ... }) => ...`):

```rust
        Some(Commands::Food {
            name,
            amount,
            kcal,
            protein,
            carbs,
            fat,
            gi,
            gl,
            ii,
            date,
            time,
        }) => cmd_food(name, amount, kcal, protein, carbs, fat, gi, gl, ii, date, time),
        Some(Commands::Note { text, date, time }) => cmd_note(text, date, time),
        Some(Commands::Bp {
            sys,
            dia,
            pulse,
            morning,
            evening,
            date,
            time,
        }) => cmd_bp(sys, dia, pulse, morning, evening, date, time),
```

Add the helper functions at the bottom of `main.rs` (next to `cmd_sleep_start` etc.):

```rust
#[allow(clippy::too_many_arguments)]
fn cmd_food(
    name: String,
    amount: Option<String>,
    kcal: Option<f64>,
    protein: Option<f64>,
    carbs: Option<f64>,
    fat: Option<f64>,
    gi: Option<f64>,
    gl: Option<f64>,
    ii: Option<f64>,
    date: Option<String>,
    time: Option<String>,
) -> Result<()> {
    let config = Config::load()?;
    daylog::cli::food_cmd::execute(
        &name,
        amount.as_deref(),
        kcal,
        protein,
        carbs,
        fat,
        gi,
        gl,
        ii,
        date.as_deref(),
        time.as_deref(),
        &config,
    )
}

fn cmd_note(text: Vec<String>, date: Option<String>, time: Option<String>) -> Result<()> {
    let config = Config::load()?;
    daylog::cli::note_cmd::execute(&text, date.as_deref(), time.as_deref(), &config)
}

fn cmd_bp(
    sys: i32,
    dia: i32,
    pulse: i32,
    morning: bool,
    evening: bool,
    date: Option<String>,
    time: Option<String>,
) -> Result<()> {
    let config = Config::load()?;
    daylog::cli::bp_cmd::execute(
        sys,
        dia,
        pulse,
        morning,
        evening,
        date.as_deref(),
        time.as_deref(),
        &config,
    )
}
```

- [ ] **Step 5: Verify build**

```bash
just build
```
Expected: clean build (with `not yet implemented` errors only at runtime).

- [ ] **Step 6: Smoke-test that subcommands are registered**

```bash
cargo run -- food --help 2>&1 | head -5
cargo run -- note --help 2>&1 | head -5
cargo run -- bp --help 2>&1 | head -5
```
Expected: each prints clap-generated help text.

- [ ] **Step 7: Lint + full suite**

```bash
just lint && cargo test
```

- [ ] **Step 8: Commit**

```bash
git add src/cli/mod.rs src/cli/food_cmd.rs src/cli/note_cmd.rs src/cli/bp_cmd.rs src/main.rs
git commit -m "$(cat <<'EOF'
feat: register food/note/bp subcommands with stub handlers

Adds clap definitions for the three new subcommands plus shared
--date and --time flags. Handlers return 'not yet implemented' until
filled in by the next tasks.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: `daylog note` — date/time resolution + alias resolution + body append

**Files:**
- Modify: `src/cli/note_cmd.rs`

- [ ] **Step 1: Add date/time helper that all three commands will share**

In `src/cli/mod.rs`, append a small helper module that the three commands can use. (Putting it here keeps the helper colocated with the CLI rather than scattering it. If a different home becomes obvious later, easy to move.)

```rust
/// Helpers shared by food/note/bp for resolving --date and --time flags
/// and rendering the timestamp prefix per `config.time_format`.
pub mod resolve {
    use chrono::{Local, NaiveDate, NaiveTime};
    use color_eyre::eyre::Result;
    use color_eyre::Help;

    use crate::config::Config;
    use crate::time;

    /// Resolve the target date for a logging command. `--date` overrides;
    /// otherwise `config.effective_today_date()`.
    pub fn target_date(flag: Option<&str>, config: &Config) -> Result<NaiveDate> {
        match flag {
            Some(s) => NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
                .map_err(|_| color_eyre::eyre::eyre!("Invalid --date: '{s}'. Expected YYYY-MM-DD."))
                .suggestion("Use a date in YYYY-MM-DD form, e.g., 2026-04-30."),
            None => Ok(config.effective_today_date()),
        }
    }

    /// Resolve the timestamp for the `**HH:MM**` prefix and BP slot
    /// detection. `--time` overrides; otherwise `Local::now().time()`.
    pub fn target_time(flag: Option<&str>) -> Result<NaiveTime> {
        match flag {
            Some(s) => time::parse_time(s)
                .ok_or_else(|| {
                    color_eyre::eyre::eyre!(
                        "Invalid --time: '{s}'. Expected HH:MM (24h) or H:MMam/pm (12h)."
                    )
                })
                .suggestion("Examples: 22:30, 07:05, 10:30pm, 6:15am."),
            None => Ok(Local::now().time()),
        }
    }
}
```

- [ ] **Step 2: Write failing tests for note_cmd**

Append to `src/cli/note_cmd.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn config_with_alias(notes_dir: &Path, key: &str, value: &str) -> Config {
        let toml_str = format!(
            r#"
notes_dir = '{}'
time_format = '24h'

[notes.aliases]
{key} = "{value}"
"#,
            notes_dir.display().to_string().replace('\\', "/")
        );
        toml::from_str(&toml_str).expect("config parses")
    }

    fn read_today(notes_dir: &Path, config: &Config) -> String {
        let date = config.effective_today();
        std::fs::read_to_string(notes_dir.join(format!("{date}.md"))).unwrap()
    }

    #[test]
    fn note_literal_appends_with_timestamp() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_with_alias(dir.path(), "ignored", "ignored");
        execute(&["Attentin".into(), "10mg".into()], None, Some("12:30"), &config).unwrap();

        let note = read_today(dir.path(), &config);
        assert!(note.contains("## Notes"), "got:\n{note}");
        assert!(note.contains("- **12:30** Attentin 10mg"), "got:\n{note}");
    }

    #[test]
    fn note_alias_expands() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_with_alias(dir.path(), "med-morning", "Morgonmedicin (Elvanse 70mg)");
        execute(&["med-morning".into()], None, Some("07:55"), &config).unwrap();

        let note = read_today(dir.path(), &config);
        assert!(
            note.contains("- **07:55** Morgonmedicin (Elvanse 70mg)"),
            "got:\n{note}"
        );
    }

    #[test]
    fn note_alias_falls_through_when_missing() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_with_alias(dir.path(), "med-morning", "expanded");
        execute(&["unknown-key".into()], None, Some("08:00"), &config).unwrap();

        let note = read_today(dir.path(), &config);
        assert!(note.contains("- **08:00** unknown-key"), "got:\n{note}");
        assert!(!note.contains("expanded"));
    }

    #[test]
    fn note_empty_text_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_with_alias(dir.path(), "ignored", "ignored");
        let err = execute(&[], None, Some("08:00"), &config).unwrap_err();
        assert!(err.to_string().contains("Note text required"));
    }

    #[test]
    fn note_uses_explicit_date_flag() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_with_alias(dir.path(), "ignored", "ignored");
        execute(
            &["Late entry".into()],
            Some("2026-04-29"),
            Some("23:59"),
            &config,
        )
        .unwrap();

        let other = std::fs::read_to_string(dir.path().join("2026-04-29.md")).unwrap();
        assert!(other.contains("- **23:59** Late entry"), "got:\n{other}");
    }

    #[test]
    fn note_invalid_date_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_with_alias(dir.path(), "ignored", "ignored");
        let err = execute(
            &["x".into()],
            Some("2026-13-45"),
            Some("08:00"),
            &config,
        )
        .unwrap_err();
        assert!(err.to_string().contains("Invalid --date"));
    }

    #[test]
    fn note_invalid_time_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_with_alias(dir.path(), "ignored", "ignored");
        let err = execute(&["x".into()], None, Some("25:00"), &config).unwrap_err();
        assert!(err.to_string().contains("Invalid --time"));
    }

    #[test]
    fn note_uses_time_format_12h() {
        let dir = tempfile::TempDir::new().unwrap();
        let toml_str = format!(
            r#"
notes_dir = '{}'
time_format = '12h'
"#,
            dir.path().display().to_string().replace('\\', "/")
        );
        let config: Config = toml::from_str(&toml_str).unwrap();

        execute(&["Coffee".into()], None, Some("13:30"), &config).unwrap();
        let note = read_today(dir.path(), &config);
        assert!(note.contains("- **1:30pm** Coffee"), "got:\n{note}");
    }
}
```

- [ ] **Step 3: Run failing tests**

```bash
cargo test --lib note_cmd::tests
```
Expected: all 8 tests fail (the stub bails with `not yet implemented`).

- [ ] **Step 4: Implement `execute`**

Replace the body of `src/cli/note_cmd.rs`:

```rust
//! `daylog note` — append a free-text note to the day's `## Notes` section.

use color_eyre::eyre::{bail, Result};

use crate::body;
use crate::cli::resolve;
use crate::config::Config;
use crate::frontmatter;
use crate::time;

pub fn execute(
    text: &[String],
    date_flag: Option<&str>,
    time_flag: Option<&str>,
    config: &Config,
) -> Result<()> {
    if text.is_empty() {
        bail!("Note text required.");
    }
    let joined = text.join(" ");
    let trimmed = joined.trim();
    if trimmed.is_empty() {
        bail!("Note text required.");
    }

    let date = resolve::target_date(date_flag, config)?;
    let date_str = date.format("%Y-%m-%d").to_string();
    let when = resolve::target_time(time_flag)?;

    let body_text = config
        .notes
        .aliases
        .get(trimmed)
        .map(String::as_str)
        .unwrap_or(trimmed);

    let formatted_time = time::format_time(when, config.time_format);
    let line = format!("- **{formatted_time}** {body_text}");

    let note_path = config.notes_dir_path().join(format!("{date_str}.md"));
    let content = if note_path.exists() {
        std::fs::read_to_string(&note_path)?
    } else {
        crate::template::render_daily_note(&date_str, config)
    };

    let updated = body::ensure_section(&content, "Notes");
    let updated = body::append_line_to_section(&updated, "Notes", &line);
    frontmatter::atomic_write(&note_path, &updated)?;

    eprintln!("Note logged: {date_str} {formatted_time}");
    Ok(())
}
```

- [ ] **Step 5: Run tests**

```bash
cargo test --lib note_cmd::tests
```
Expected: all 8 pass.

- [ ] **Step 6: Lint + full suite**

```bash
just lint && cargo test
```

- [ ] **Step 7: Manual smoke**

```bash
# In a temp notes dir to avoid touching ~/daylog-notes:
TMPNOTES=$(mktemp -d)
cat > "$TMPNOTES/config.toml" <<EOF
notes_dir = "$TMPNOTES"

[notes.aliases]
hi = "Hello"
EOF
DAYLOG_HOME=$HOME XDG_CONFIG_HOME=$TMPNOTES/cfg mkdir -p "$TMPNOTES/cfg/daylog"
cp "$TMPNOTES/config.toml" "$TMPNOTES/cfg/daylog/config.toml"
XDG_CONFIG_HOME=$TMPNOTES/cfg cargo run -- note "smoke test" 2>&1 | tail -3
ls "$TMPNOTES" | grep "\.md"
```
Expected: today's note exists in `$TMPNOTES`, contains a `## Notes` line. (If your shell doesn't have XDG_CONFIG_HOME plumbed, skip this and rely on the unit tests.)

- [ ] **Step 8: Commit**

```bash
git add src/cli/note_cmd.rs src/cli/mod.rs
git commit -m "$(cat <<'EOF'
feat: daylog note appends to ## Notes with alias and date/time flags

Resolves --date via Config::effective_today_date() (with explicit
override), --time via existing time::parse_time, and looks up
[notes.aliases] before falling through to literal text. Section is
auto-inserted if missing.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: `daylog bp` — slot detection + YAML + Vitals line

**Files:**
- Modify: `src/cli/bp_cmd.rs`

- [ ] **Step 1: Add a slot helper trait + tests up front**

We separate the pure slot-decision logic from I/O so it can be unit-tested directly. Append to `src/cli/bp_cmd.rs` (replacing the stub):

```rust
//! `daylog bp` — write blood pressure to YAML + append a `## Vitals` line.

use chrono::NaiveTime;
use color_eyre::eyre::{bail, Result};

use crate::body;
use crate::cli::resolve;
use crate::config::Config;
use crate::frontmatter;
use crate::time;

/// Morning/evening cutoff: time-of-measurement < 14:00 → morning,
/// otherwise evening. `--morning` and `--evening` flags override.
const MORNING_CUTOFF_HOUR: u32 = 14;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Morning,
    Evening,
}

impl Slot {
    fn yaml_prefix(self) -> &'static str {
        match self {
            Slot::Morning => "bp_morning",
            Slot::Evening => "bp_evening",
        }
    }
}

/// Decide the slot from explicit flags or the measurement time.
pub fn pick_slot(morning: bool, evening: bool, when: NaiveTime) -> Slot {
    if morning {
        return Slot::Morning;
    }
    if evening {
        return Slot::Evening;
    }
    use chrono::Timelike;
    if when.hour() < MORNING_CUTOFF_HOUR {
        Slot::Morning
    } else {
        Slot::Evening
    }
}

#[allow(clippy::too_many_arguments)]
pub fn execute(
    sys: i32,
    dia: i32,
    pulse: i32,
    morning: bool,
    evening: bool,
    date_flag: Option<&str>,
    time_flag: Option<&str>,
    config: &Config,
) -> Result<()> {
    if morning && evening {
        // clap's `conflicts_with` should already block this, but keep a
        // defensive bail in case the function is called programmatically.
        bail!("--morning and --evening are mutually exclusive.");
    }

    let date = resolve::target_date(date_flag, config)?;
    let date_str = date.format("%Y-%m-%d").to_string();
    let when = resolve::target_time(time_flag)?;
    let slot = pick_slot(morning, evening, when);

    validate_or_warn(sys, dia, pulse);

    let formatted_time = time::format_time(when, config.time_format);
    let prefix = slot.yaml_prefix();
    let body_line = format!("- **{formatted_time}** BP: {sys}/{dia}, pulse {pulse} bpm");

    let note_path = config.notes_dir_path().join(format!("{date_str}.md"));
    let content = if note_path.exists() {
        std::fs::read_to_string(&note_path)?
    } else {
        crate::template::render_daily_note(&date_str, config)
    };

    let updated = frontmatter::set_scalar(&content, &format!("{prefix}_sys"), &sys.to_string());
    let updated = frontmatter::set_scalar(&updated, &format!("{prefix}_dia"), &dia.to_string());
    let updated = frontmatter::set_scalar(&updated, &format!("{prefix}_pulse"), &pulse.to_string());
    let updated = body::ensure_section(&updated, "Vitals");
    let updated = body::append_line_to_section(&updated, "Vitals", &body_line);

    frontmatter::atomic_write(&note_path, &updated)?;
    eprintln!(
        "BP logged: {sys}/{dia}, pulse {pulse} bpm ({slot:?}) on {date_str}",
    );
    Ok(())
}

fn validate_or_warn(sys: i32, dia: i32, pulse: i32) {
    if !(50..=300).contains(&sys) {
        eprintln!("Warning: sys={sys} outside plausible range 50–300; logging anyway.");
    }
    if !(30..=200).contains(&dia) {
        eprintln!("Warning: dia={dia} outside plausible range 30–200; logging anyway.");
    }
    if !(30..=250).contains(&pulse) {
        eprintln!("Warning: pulse={pulse} outside plausible range 30–250; logging anyway.");
    }
}
```

- [ ] **Step 2: Append failing tests**

Append to `src/cli/bp_cmd.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveTime;
    use std::path::Path;

    fn t(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    fn config_in(notes_dir: &Path, fmt: &str) -> Config {
        let toml_str = format!(
            "notes_dir = '{}'\ntime_format = '{fmt}'\n",
            notes_dir.display().to_string().replace('\\', "/")
        );
        toml::from_str(&toml_str).unwrap()
    }

    fn read_today(notes_dir: &Path, config: &Config) -> String {
        let date = config.effective_today();
        std::fs::read_to_string(notes_dir.join(format!("{date}.md"))).unwrap()
    }

    // --- pick_slot pure logic ---

    #[test]
    fn slot_auto_morning_before_14() {
        assert_eq!(pick_slot(false, false, t(13, 59)), Slot::Morning);
        assert_eq!(pick_slot(false, false, t(7, 30)), Slot::Morning);
        assert_eq!(pick_slot(false, false, t(0, 0)), Slot::Morning);
    }

    #[test]
    fn slot_auto_evening_at_14_and_after() {
        assert_eq!(pick_slot(false, false, t(14, 0)), Slot::Evening);
        assert_eq!(pick_slot(false, false, t(20, 30)), Slot::Evening);
        assert_eq!(pick_slot(false, false, t(23, 59)), Slot::Evening);
    }

    #[test]
    fn slot_explicit_flags_override_time() {
        assert_eq!(pick_slot(true, false, t(20, 0)), Slot::Morning);
        assert_eq!(pick_slot(false, true, t(7, 0)), Slot::Evening);
    }

    // --- end-to-end via execute ---

    #[test]
    fn writes_three_yaml_fields_for_morning() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in(dir.path(), "24h");
        execute(141, 96, 70, false, false, None, Some("07:30"), &config).unwrap();

        let note = read_today(dir.path(), &config);
        assert!(note.contains("bp_morning_sys: 141"), "got:\n{note}");
        assert!(note.contains("bp_morning_dia: 96"));
        assert!(note.contains("bp_morning_pulse: 70"));
    }

    #[test]
    fn writes_three_yaml_fields_for_evening() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in(dir.path(), "24h");
        execute(133, 73, 62, false, false, None, Some("18:00"), &config).unwrap();

        let note = read_today(dir.path(), &config);
        assert!(note.contains("bp_evening_sys: 133"), "got:\n{note}");
        assert!(note.contains("bp_evening_dia: 73"));
        assert!(note.contains("bp_evening_pulse: 62"));
    }

    #[test]
    fn vitals_line_has_no_slot_suffix_and_includes_bpm() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in(dir.path(), "24h");
        execute(141, 96, 70, false, false, None, Some("07:30"), &config).unwrap();

        let note = read_today(dir.path(), &config);
        assert!(
            note.contains("- **07:30** BP: 141/96, pulse 70 bpm"),
            "got:\n{note}"
        );
        assert!(!note.contains("(morning)"));
        assert!(!note.contains("(evening)"));
    }

    #[test]
    fn explicit_evening_overrides_time() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in(dir.path(), "24h");
        execute(133, 73, 62, false, true, None, Some("09:00"), &config).unwrap();

        let note = read_today(dir.path(), &config);
        assert!(note.contains("bp_evening_sys: 133"));
        assert!(!note.contains("bp_morning_sys"));
    }

    #[test]
    fn rerun_morning_overwrites_yaml_appends_vitals() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in(dir.path(), "24h");
        execute(140, 95, 70, false, false, None, Some("07:00"), &config).unwrap();
        execute(135, 90, 65, false, false, None, Some("07:30"), &config).unwrap();

        let note = read_today(dir.path(), &config);
        // YAML overwritten in place: only the second value present.
        assert!(note.contains("bp_morning_sys: 135"));
        assert!(!note.contains("bp_morning_sys: 140"));
        // Vitals body keeps both lines chronologically.
        assert!(note.contains("- **07:00** BP: 140/95, pulse 70 bpm"));
        assert!(note.contains("- **07:30** BP: 135/90, pulse 65 bpm"));
    }

    #[test]
    fn creates_vitals_section_if_missing() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in(dir.path(), "24h");
        execute(141, 96, 70, false, false, None, Some("07:30"), &config).unwrap();

        let note = read_today(dir.path(), &config);
        assert!(note.contains("## Vitals"));
    }

    #[test]
    fn date_flag_writes_to_named_day() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in(dir.path(), "24h");
        execute(
            141,
            96,
            70,
            false,
            false,
            Some("2026-04-29"),
            Some("07:30"),
            &config,
        )
        .unwrap();

        let path = dir.path().join("2026-04-29.md");
        let note = std::fs::read_to_string(&path).unwrap();
        assert!(note.contains("bp_morning_sys: 141"));
        assert!(note.contains("- **07:30** BP: 141/96, pulse 70 bpm"));
    }

    #[test]
    fn invalid_date_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in(dir.path(), "24h");
        let err = execute(
            141,
            96,
            70,
            false,
            false,
            Some("2026-13-45"),
            Some("07:30"),
            &config,
        )
        .unwrap_err();
        assert!(err.to_string().contains("Invalid --date"));
    }

    #[test]
    fn invalid_time_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in(dir.path(), "24h");
        let err = execute(141, 96, 70, false, false, None, Some("25:00"), &config).unwrap_err();
        assert!(err.to_string().contains("Invalid --time"));
    }
}
```

- [ ] **Step 3: Run failing tests**

```bash
cargo test --lib bp_cmd::tests
```
Expected: 11 fail (the body of `execute` exists now, so most should pass — except the rerun one and possibly slot-suffix). If implementing in this order, tests pass on first run.

If any tests fail, fix the implementation until they pass.

- [ ] **Step 4: Lint + full suite**

```bash
just lint && cargo test
```

- [ ] **Step 5: Commit**

```bash
git add src/cli/bp_cmd.rs
git commit -m "$(cat <<'EOF'
feat: daylog bp writes YAML and Vitals body in one atomic pass

Slot dispatch: --morning/--evening flags override an auto pick driven
by the measurement time vs. the 14:00 cutoff. YAML scalars overwrite
in place; the Vitals body line accumulates chronologically. Out-of-
range values warn but still write.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: `daylog food` — amount parsing

**Files:**
- Modify: `src/cli/food_cmd.rs`

This task wires up just the amount parser as a pure helper. Output formatting and DB lookup come in tasks 9 and 10.

- [ ] **Step 1: Replace the stub with a typed amount module**

Open `src/cli/food_cmd.rs` and replace its contents with:

```rust
//! `daylog food` — append a food entry to the day's `## Food` section.
//! Implementation is split across tasks: amount parsing here; nutrition
//! scaling, output formatting, and DB lookup in subsequent tasks.

use color_eyre::eyre::{bail, Result};
use color_eyre::Help;

use crate::config::Config;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AmountUnit {
    Gram,
    Milliliter,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Amount {
    pub value: f64,
    pub unit: AmountUnit,
}

impl Amount {
    pub fn unit_str(self) -> &'static str {
        match self.unit {
            AmountUnit::Gram => "g",
            AmountUnit::Milliliter => "ml",
        }
    }
}

/// Parse an amount with optional `g` / `ml` suffix. Bare numbers default
/// to grams. Whitespace between number and suffix is tolerated.
pub fn parse_amount(s: &str) -> Result<Amount> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        bail!("Invalid amount: empty.");
    }

    let lower = trimmed.to_ascii_lowercase();
    let (number_part, unit) = if let Some(rest) = lower.strip_suffix("ml") {
        (rest.trim_end(), AmountUnit::Milliliter)
    } else if let Some(rest) = lower.strip_suffix('g') {
        (rest.trim_end(), AmountUnit::Gram)
    } else {
        (lower.as_str(), AmountUnit::Gram)
    };

    let value: f64 = number_part.parse().map_err(|_| {
        color_eyre::eyre::eyre!(
            "Invalid amount: '{trimmed}'. Expected a number with optional 'g' or 'ml' suffix \
             (e.g., 500g, 250ml, or 500)."
        )
    })?;

    if value <= 0.0 {
        return Err(color_eyre::eyre::eyre!(
            "Invalid amount: '{trimmed}'. Must be positive."
        ))
        .suggestion("Pass a positive number, e.g., 500g.");
    }

    Ok(Amount { value, unit })
}

#[allow(clippy::too_many_arguments)]
pub fn execute(
    _name: &str,
    _amount: Option<&str>,
    _kcal: Option<f64>,
    _protein: Option<f64>,
    _carbs: Option<f64>,
    _fat: Option<f64>,
    _gi: Option<f64>,
    _gl: Option<f64>,
    _ii: Option<f64>,
    _date: Option<&str>,
    _time: Option<&str>,
    _config: &Config,
) -> Result<()> {
    bail!("daylog food: amount parsing only — full implementation in next task")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_grams_with_suffix() {
        let a = parse_amount("500g").unwrap();
        assert_eq!(a.value, 500.0);
        assert_eq!(a.unit, AmountUnit::Gram);
    }

    #[test]
    fn parse_ml_with_suffix() {
        let a = parse_amount("250ml").unwrap();
        assert_eq!(a.value, 250.0);
        assert_eq!(a.unit, AmountUnit::Milliliter);
    }

    #[test]
    fn parse_bare_number_defaults_to_grams() {
        let a = parse_amount("500").unwrap();
        assert_eq!(a.value, 500.0);
        assert_eq!(a.unit, AmountUnit::Gram);
    }

    #[test]
    fn parse_decimal_amount() {
        let a = parse_amount("12.5g").unwrap();
        assert_eq!(a.value, 12.5);
        assert_eq!(a.unit, AmountUnit::Gram);
    }

    #[test]
    fn parse_uppercase_suffix() {
        let a = parse_amount("250ML").unwrap();
        assert_eq!(a.unit, AmountUnit::Milliliter);
    }

    #[test]
    fn parse_with_internal_space() {
        let a = parse_amount("500 g").unwrap();
        assert_eq!(a.value, 500.0);
        assert_eq!(a.unit, AmountUnit::Gram);
    }

    #[test]
    fn parse_garbage_errors() {
        assert!(parse_amount("500abc").is_err());
        assert!(parse_amount("abc").is_err());
        assert!(parse_amount("").is_err());
    }

    #[test]
    fn parse_negative_or_zero_errors() {
        assert!(parse_amount("-5g").is_err());
        assert!(parse_amount("0g").is_err());
    }
}
```

- [ ] **Step 2: Run tests**

```bash
cargo test --lib food_cmd::tests
```
Expected: 8 pass.

- [ ] **Step 3: Lint + full suite**

```bash
just lint && cargo test
```

- [ ] **Step 4: Commit**

```bash
git add src/cli/food_cmd.rs
git commit -m "$(cat <<'EOF'
feat: food_cmd amount parser with g/ml suffix

Bare numbers default to grams. Decimal values, uppercase suffix, and
whitespace between number and suffix are all tolerated. Negative and
zero amounts are rejected.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: `daylog food` — nutrient scaling, output formatting

**Files:**
- Modify: `src/cli/food_cmd.rs`

This task adds the pure-logic core: scaling a `FoodLookup` (or custom flags) by an `Amount` to produce a `RenderedEntry`, then formatting that into the markdown line. No I/O, no DB.

- [ ] **Step 1: Add types and a `RenderedEntry` struct**

Append to `src/cli/food_cmd.rs` (above the `execute` stub):

```rust
use crate::db::{FoodLookup, NutrientPanel, TotalPanel};

#[derive(Debug, Clone, PartialEq)]
pub struct RenderedEntry {
    pub display_name: String,
    /// `(value, unit_str)` shown in the parens, or `None` to omit.
    pub amount_segment: Option<(f64, &'static str)>,
    pub kcal: Option<f64>,
    pub protein: Option<f64>,
    pub carbs: Option<f64>,
    pub fat: Option<f64>,
    pub gi: Option<f64>,
    pub gl: Option<f64>,
    pub ii: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
pub struct CustomNutrients {
    pub kcal: f64,
    pub protein: f64,
    pub carbs: f64,
    pub fat: f64,
    pub gi: Option<f64>,
    pub gl: Option<f64>,
    pub ii: Option<f64>,
}
```

- [ ] **Step 2: Add scaling functions with failing tests**

Append (still above `execute`):

```rust
/// Build a `RenderedEntry` from a custom-flag invocation.
pub fn render_custom(
    display_name: &str,
    amount: Option<Amount>,
    flags: CustomNutrients,
) -> RenderedEntry {
    let gl = flags.gl.or_else(|| auto_gl(flags.gi, Some(flags.carbs)));
    RenderedEntry {
        display_name: display_name.to_string(),
        amount_segment: amount.map(|a| (a.value, a.unit_str())),
        kcal: Some(flags.kcal),
        protein: Some(flags.protein),
        carbs: Some(flags.carbs),
        fat: Some(flags.fat),
        gi: flags.gi,
        gl,
        ii: flags.ii,
    }
}

/// Build a `RenderedEntry` from a nutrition-db lookup + optional amount.
/// Returns an error for invalid combinations (e.g., per_100g-only food
/// asked for in ml without a density).
pub fn render_lookup(food: &FoodLookup, amount: Option<Amount>) -> Result<RenderedEntry> {
    match amount {
        None => render_total_only(food),
        Some(a) => render_with_amount(food, a),
    }
}

fn render_total_only(food: &FoodLookup) -> Result<RenderedEntry> {
    let total = food
        .total
        .as_ref()
        .ok_or_else(|| {
            color_eyre::eyre::eyre!(
                "{} requires an amount (e.g., '500g' or '250ml'). It has \
                 per_100g/per_100ml values but no total panel.",
                food.name
            )
        })?;
    let amount_segment = total.weight_g.map(|g| (g, "g"));
    let gi = food.gi;
    let gl = total_gl(food, total);
    Ok(RenderedEntry {
        display_name: food.name.clone(),
        amount_segment,
        kcal: total.kcal,
        protein: total.protein,
        carbs: total.carbs,
        fat: total.fat,
        gi,
        gl,
        ii: food.ii,
    })
}

fn render_with_amount(food: &FoodLookup, amount: Amount) -> Result<RenderedEntry> {
    if food.per_100g.is_none() && food.per_100ml.is_none() && food.total.is_some() {
        eprintln!(
            "Warning: {} only has a `total` panel; ignoring amount {}{}.",
            food.name,
            amount.value,
            amount.unit_str()
        );
        return render_total_only(food);
    }

    // Resolve which panel to scale and what the scaling factor is.
    let (panel, factor) = match amount.unit {
        AmountUnit::Gram => match (&food.per_100g, &food.per_100ml, food.density_g_per_ml) {
            (Some(p), _, _) => (p, amount.value / 100.0),
            (None, Some(p), Some(d)) if d > 0.0 => {
                // Solid input on liquid-only food via density: g → ml.
                let ml = amount.value / d;
                (p, ml / 100.0)
            }
            (None, Some(_), _) => {
                bail!(
                    "{} is a liquid (per_100ml only) and has no density_g_per_ml. \
                     Use ml: 'daylog food {} {}ml'.",
                    food.name,
                    food.name,
                    amount.value
                );
            }
            (None, None, _) => bail!(
                "{} has no per_100g/per_100ml panels and no total. Cannot scale.",
                food.name
            ),
        },
        AmountUnit::Milliliter => match (&food.per_100ml, &food.per_100g, food.density_g_per_ml) {
            (Some(p), _, _) => (p, amount.value / 100.0),
            (None, Some(p), Some(d)) if d > 0.0 => {
                // Liquid input on solid-only food via density: ml → g.
                let g = amount.value * d;
                (p, g / 100.0)
            }
            (None, Some(_), _) => {
                bail!(
                    "{} is a solid (per_100g only) and has no density_g_per_ml. \
                     Use grams: 'daylog food {} {}g'.",
                    food.name,
                    food.name,
                    amount.value
                );
            }
            (None, None, _) => bail!(
                "{} has no per_100g/per_100ml panels and no total. Cannot scale.",
                food.name
            ),
        },
    };

    let kcal = panel.kcal.map(|v| v * factor);
    let protein = panel.protein.map(|v| v * factor);
    let carbs = panel.carbs.map(|v| v * factor);
    let fat = panel.fat.map(|v| v * factor);

    let gi = food.gi;
    let gl_from_panel = match amount.unit {
        AmountUnit::Gram => food.gl_per_100g.map(|v| v * factor),
        AmountUnit::Milliliter => food.gl_per_100ml.map(|v| v * factor),
    };
    let gl = gl_from_panel.or_else(|| auto_gl(gi, carbs));

    Ok(RenderedEntry {
        display_name: food.name.clone(),
        amount_segment: Some((amount.value, amount.unit_str())),
        kcal,
        protein,
        carbs,
        fat,
        gi,
        gl,
        ii: food.ii,
    })
}

/// GL auto-compute from GI and carbs: `gi * carbs / 100`.
fn auto_gl(gi: Option<f64>, carbs: Option<f64>) -> Option<f64> {
    match (gi, carbs) {
        (Some(g), Some(c)) => Some(g * c / 100.0),
        _ => None,
    }
}

fn total_gl(food: &FoodLookup, total: &TotalPanel) -> Option<f64> {
    food.gl_per_100g
        .and_then(|v| total.weight_g.map(|w| v * w / 100.0))
        .or_else(|| auto_gl(food.gi, total.carbs))
}

/// Format a fully-resolved entry as the `## Food` line. Caller supplies
/// the timestamp prefix (e.g., `"12:42"`).
pub fn format_line(entry: &RenderedEntry, timestamp: &str) -> String {
    let mut line = format!("- **{timestamp}** {}", entry.display_name);

    if let Some((value, unit)) = entry.amount_segment {
        line.push_str(&format!(" ({})", format_amount(value, unit)));
    }

    let nutrients = format_nutrient_segment(entry);
    if !nutrients.is_empty() {
        line.push_str(&format!(" ({nutrients})"));
    }

    let glycemic = format_glycemic_segment(entry);
    if !glycemic.is_empty() {
        line.push_str(&format!(" | {glycemic}"));
    }

    line
}

fn format_amount(value: f64, unit: &str) -> String {
    if (value - value.round()).abs() < 1e-9 {
        format!("{}{unit}", value.round() as i64)
    } else {
        format!("{value:.1}{unit}")
    }
}

fn format_nutrient_segment(entry: &RenderedEntry) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(kcal) = entry.kcal {
        parts.push(format!("{} kcal", kcal.round() as i64));
    }
    if let Some(p) = entry.protein {
        parts.push(format!("{p:.1}g protein"));
    }
    if let Some(c) = entry.carbs {
        parts.push(format!("{c:.1}g carbs"));
    }
    if let Some(f) = entry.fat {
        parts.push(format!("{f:.1}g fat"));
    }
    parts.join(", ")
}

fn format_glycemic_segment(entry: &RenderedEntry) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(g) = entry.gi {
        parts.push(format!("GI ~{}", round_glycemic(g)));
    }
    if let Some(g) = entry.gl {
        parts.push(format!("GL ~{}", round_glycemic_one_decimal(g)));
    }
    if let Some(g) = entry.ii {
        parts.push(format!("II ~{}", round_glycemic(g)));
    }
    parts.join(", ")
}

fn round_glycemic(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.1}")
    }
}

fn round_glycemic_one_decimal(v: f64) -> String {
    format!("{v:.1}")
}
```

- [ ] **Step 3: Add tests**

Append inside the existing `mod tests`:

```rust
    use crate::db::{FoodLookup, NutrientPanel, TotalPanel};

    fn lookup_per_100g() -> FoodLookup {
        FoodLookup {
            id: 1,
            name: "Kelda Skogssvampsoppa".into(),
            per_100g: Some(NutrientPanel {
                kcal: Some(70.0),
                protein: Some(1.4),
                carbs: Some(4.8),
                fat: Some(5.0),
                sat_fat: None,
                sugar: None,
                salt: None,
                fiber: None,
            }),
            per_100ml: None,
            density_g_per_ml: None,
            total: None,
            gi: Some(40.0),
            gl_per_100g: Some(2.0),
            gl_per_100ml: None,
            ii: Some(35.0),
            description: None,
            notes: None,
        }
    }

    fn lookup_per_100ml_with_density() -> FoodLookup {
        FoodLookup {
            id: 2,
            name: "Helmjölk".into(),
            per_100g: None,
            per_100ml: Some(NutrientPanel {
                kcal: Some(62.0),
                protein: Some(3.4),
                carbs: Some(4.8),
                fat: Some(3.0),
                sat_fat: None,
                sugar: None,
                salt: None,
                fiber: None,
            }),
            density_g_per_ml: Some(1.03),
            total: None,
            gi: Some(30.0),
            gl_per_100g: None,
            gl_per_100ml: None,
            ii: Some(90.0),
            description: None,
            notes: None,
        }
    }

    fn lookup_total_panel() -> FoodLookup {
        FoodLookup {
            id: 3,
            name: "Te, Earl Grey, hot".into(),
            per_100g: None,
            per_100ml: None,
            density_g_per_ml: None,
            total: Some(TotalPanel {
                weight_g: Some(200.0),
                kcal: Some(2.0),
                protein: Some(0.0),
                carbs: Some(0.4),
                fat: Some(0.0),
                sat_fat: None,
                sugar: None,
                salt: None,
                fiber: None,
            }),
            gi: None,
            gl_per_100g: None,
            gl_per_100ml: None,
            ii: None,
            description: None,
            notes: None,
        }
    }

    #[test]
    fn lookup_solid_with_grams_scales_per_100g() {
        let f = lookup_per_100g();
        let amt = parse_amount("500g").unwrap();
        let r = render_lookup(&f, Some(amt)).unwrap();
        assert_eq!(r.kcal, Some(350.0));
        assert!((r.protein.unwrap() - 7.0).abs() < 1e-9);
        assert_eq!(r.gl, Some(10.0));
        assert_eq!(r.gi, Some(40.0));
        assert_eq!(r.amount_segment, Some((500.0, "g")));
    }

    #[test]
    fn lookup_liquid_with_ml_scales_per_100ml() {
        let f = lookup_per_100ml_with_density();
        let amt = parse_amount("250ml").unwrap();
        let r = render_lookup(&f, Some(amt)).unwrap();
        assert_eq!(r.kcal, Some(155.0));
        assert!((r.protein.unwrap() - 8.5).abs() < 1e-9);
        assert_eq!(r.amount_segment, Some((250.0, "ml")));
    }

    #[test]
    fn lookup_solid_with_ml_uses_density() {
        // Build a solid with density to allow ml input via conversion.
        let mut f = lookup_per_100g();
        f.density_g_per_ml = Some(1.0);
        let amt = parse_amount("100ml").unwrap();
        let r = render_lookup(&f, Some(amt)).unwrap();
        // 100ml * 1.0 = 100g; same as 100g of soup.
        assert_eq!(r.kcal, Some(70.0));
        assert_eq!(r.amount_segment, Some((100.0, "ml")));
    }

    #[test]
    fn lookup_solid_with_ml_no_density_errors() {
        let f = lookup_per_100g();
        let amt = parse_amount("100ml").unwrap();
        let err = render_lookup(&f, Some(amt)).unwrap_err();
        assert!(err.to_string().contains("density"), "got: {err}");
    }

    #[test]
    fn lookup_total_panel_no_amount_uses_totals() {
        let f = lookup_total_panel();
        let r = render_lookup(&f, None).unwrap();
        assert_eq!(r.kcal, Some(2.0));
        assert_eq!(r.amount_segment, Some((200.0, "g")));
    }

    #[test]
    fn lookup_total_panel_no_amount_no_weight_g_omits_amount() {
        let mut f = lookup_total_panel();
        f.total.as_mut().unwrap().weight_g = None;
        let r = render_lookup(&f, None).unwrap();
        assert!(r.amount_segment.is_none());
    }

    #[test]
    fn lookup_per_100g_no_amount_errors() {
        let f = lookup_per_100g();
        let err = render_lookup(&f, None).unwrap_err();
        assert!(err.to_string().contains("requires an amount"));
    }

    #[test]
    fn custom_with_gi_carbs_no_gl_autocomputes() {
        let r = render_custom(
            "Random pasta",
            Some(parse_amount("500g").unwrap()),
            CustomNutrients {
                kcal: 350.0,
                protein: 7.0,
                carbs: 24.0,
                fat: 25.0,
                gi: Some(50.0),
                gl: None,
                ii: None,
            },
        );
        assert_eq!(r.gl, Some(12.0));
        assert_eq!(r.gi, Some(50.0));
    }

    #[test]
    fn format_line_full_lookup() {
        let f = lookup_per_100g();
        let r = render_lookup(&f, Some(parse_amount("500g").unwrap())).unwrap();
        let line = format_line(&r, "12:42");
        assert_eq!(
            line,
            "- **12:42** Kelda Skogssvampsoppa (500g) (350 kcal, 7.0g protein, 24.0g carbs, 25.0g fat) | GI ~40, GL ~10.0, II ~35"
        );
    }

    #[test]
    fn format_line_omits_glycemic_when_absent() {
        let r = render_custom(
            "Random pasta",
            Some(parse_amount("500g").unwrap()),
            CustomNutrients {
                kcal: 350.0,
                protein: 7.0,
                carbs: 24.0,
                fat: 25.0,
                gi: None,
                gl: None,
                ii: None,
            },
        );
        let line = format_line(&r, "13:00");
        assert!(!line.contains('|'), "got: {line}");
        assert!(line.contains("(350 kcal"));
    }

    #[test]
    fn format_line_glycemic_partial() {
        let r = render_custom(
            "Pasta",
            Some(parse_amount("500g").unwrap()),
            CustomNutrients {
                kcal: 350.0,
                protein: 7.0,
                carbs: 24.0,
                fat: 25.0,
                gi: Some(50.0),
                gl: None,
                ii: None,
            },
        );
        let line = format_line(&r, "13:00");
        assert!(line.contains("| GI ~50, GL ~12.0"));
        assert!(!line.contains("II"));
    }

    #[test]
    fn format_line_total_panel_no_amount_no_parens() {
        let mut f = lookup_total_panel();
        f.total.as_mut().unwrap().weight_g = None;
        let r = render_lookup(&f, None).unwrap();
        let line = format_line(&r, "14:50");
        // No `(...g)` segment when weight_g is missing.
        assert!(line.starts_with("- **14:50** Te, Earl Grey, hot ("),
                "expected nutrient segment to start; got: {line}");
        // The opening paren after the name should be the nutrient segment.
        let after_name = line.trim_start_matches("- **14:50** Te, Earl Grey, hot ");
        assert!(after_name.starts_with("(2 kcal"), "got: {after_name}");
    }
```

- [ ] **Step 4: Run tests**

```bash
cargo test --lib food_cmd::tests
```
Expected: all tests pass (8 amount tests + 13 new = 21 total).

- [ ] **Step 5: Lint + full suite**

```bash
just lint && cargo test
```

- [ ] **Step 6: Commit**

```bash
git add src/cli/food_cmd.rs
git commit -m "$(cat <<'EOF'
feat: food_cmd nutrient scaling and output line formatting

Introduces RenderedEntry as the boundary between input parsing/lookup
and output rendering. Scaling supports per_100g, per_100ml, and
density-driven g↔ml conversions; total-panel foods are used as-is
when no amount is given. GL auto-compute (gi × carbs / 100) kicks in
when GL isn't otherwise known. Macros print 1 decimal, kcal whole.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 10: `daylog food` — wire CLI handler (DB lookup, file write)

**Files:**
- Modify: `src/cli/food_cmd.rs`

- [ ] **Step 1: Replace the `execute` stub with the full handler**

Replace the body of `execute` in `src/cli/food_cmd.rs`:

```rust
#[allow(clippy::too_many_arguments)]
pub fn execute(
    name: &str,
    amount: Option<&str>,
    kcal: Option<f64>,
    protein: Option<f64>,
    carbs: Option<f64>,
    fat: Option<f64>,
    gi: Option<f64>,
    gl: Option<f64>,
    ii: Option<f64>,
    date_flag: Option<&str>,
    time_flag: Option<&str>,
    config: &Config,
) -> Result<()> {
    if name.trim().is_empty() {
        bail!("Food name required.");
    }

    let amt = match amount {
        Some(s) => Some(parse_amount(s)?),
        None => None,
    };

    let date = crate::cli::resolve::target_date(date_flag, config)?;
    let date_str = date.format("%Y-%m-%d").to_string();
    let when = crate::cli::resolve::target_time(time_flag)?;
    let formatted_time = crate::time::format_time(when, config.time_format);

    let any_macro = kcal.is_some() || protein.is_some() || carbs.is_some() || fat.is_some();
    let entry = if any_macro {
        let custom = require_custom_complete(kcal, protein, carbs, fat, gi, gl, ii)?;
        render_custom(name, amt, custom)
    } else {
        let lookup = lookup_food(config, name)?;
        render_lookup(&lookup, amt)?
    };

    let line = format_line(&entry, &formatted_time);

    let note_path = config.notes_dir_path().join(format!("{date_str}.md"));
    let content = if note_path.exists() {
        std::fs::read_to_string(&note_path)?
    } else {
        crate::template::render_daily_note(&date_str, config)
    };
    let updated = crate::body::ensure_section(&content, "Food");
    let updated = crate::body::append_line_to_section(&updated, "Food", &line);
    crate::frontmatter::atomic_write(&note_path, &updated)?;

    eprintln!("Food logged: {date_str} {formatted_time} {}", entry.display_name);
    Ok(())
}

fn require_custom_complete(
    kcal: Option<f64>,
    protein: Option<f64>,
    carbs: Option<f64>,
    fat: Option<f64>,
    gi: Option<f64>,
    gl: Option<f64>,
    ii: Option<f64>,
) -> Result<CustomNutrients> {
    let kcal = kcal.ok_or_else(missing_macros_err)?;
    let protein = protein.ok_or_else(missing_macros_err)?;
    let carbs = carbs.ok_or_else(missing_macros_err)?;
    let fat = fat.ok_or_else(missing_macros_err)?;
    Ok(CustomNutrients {
        kcal,
        protein,
        carbs,
        fat,
        gi,
        gl,
        ii,
    })
}

fn missing_macros_err() -> color_eyre::eyre::Report {
    color_eyre::eyre::eyre!(
        "Custom mode requires --kcal, --protein, --carbs, and --fat together. \
         Optional flags: --gi, --gl, --ii."
    )
}

fn lookup_food(config: &Config, name: &str) -> Result<FoodLookup> {
    let db_path = config.db_path();
    if !db_path.exists() {
        return Err(color_eyre::eyre::eyre!(
            "Database not found at {}. Run 'daylog init' or 'daylog sync' first, \
             or pass --kcal/--protein/--carbs/--fat for a one-off entry.",
            db_path.display()
        ));
    }

    let conn = crate::db::open_ro(&db_path)?;
    crate::db::lookup_food_by_name_or_alias(&conn, name)?.ok_or_else(|| {
        color_eyre::eyre::eyre!(
            "No nutrition entry for '{name}'. Add it to nutrition-db.md, \
             use a known alias, or pass --kcal/--protein/--carbs/--fat for a one-off."
        )
    })
}
```

- [ ] **Step 2: Add integration-style tests**

Append inside `mod tests`:

```rust
    use crate::db;

    fn config_in(notes_dir: &std::path::Path) -> Config {
        let toml_str = format!(
            "notes_dir = '{}'\ntime_format = '24h'\n",
            notes_dir.display().to_string().replace('\\', "/")
        );
        toml::from_str(&toml_str).unwrap()
    }

    fn read_today(notes_dir: &std::path::Path, config: &Config) -> String {
        let date = config.effective_today();
        std::fs::read_to_string(notes_dir.join(format!("{date}.md"))).unwrap()
    }

    fn populate_db(config: &Config) {
        let db_path = config.db_path();
        let conn = db::open_rw(&db_path).unwrap();
        db::init_db(&conn, &[]).unwrap();
        db::insert_food(
            &conn,
            &db::FoodInsert {
                name: "Kelda Skogssvampsoppa".into(),
                per_100g: Some(NutrientPanel {
                    kcal: Some(70.0),
                    protein: Some(1.4),
                    carbs: Some(4.8),
                    fat: Some(5.0),
                    sat_fat: None,
                    sugar: None,
                    salt: None,
                    fiber: None,
                }),
                per_100ml: None,
                density_g_per_ml: None,
                total: None,
                gi: Some(40.0),
                gl_per_100g: Some(2.0),
                gl_per_100ml: None,
                ii: Some(35.0),
                description: None,
                notes: None,
                aliases: vec!["kelda skogssvampsoppa".into()],
                ingredients: vec![],
            },
        )
        .unwrap();
    }

    #[test]
    fn execute_lookup_writes_food_section_and_line() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in(dir.path());
        populate_db(&config);

        execute(
            "kelda skogssvampsoppa",
            Some("500g"),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some("12:42"),
            &config,
        )
        .unwrap();

        let note = read_today(dir.path(), &config);
        assert!(note.contains("## Food"), "got:\n{note}");
        assert!(
            note.contains("- **12:42** Kelda Skogssvampsoppa (500g) (350 kcal, 7.0g protein, 24.0g carbs, 25.0g fat) | GI ~40, GL ~10.0, II ~35"),
            "got:\n{note}"
        );
    }

    #[test]
    fn execute_custom_mode_works_without_db() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in(dir.path());
        // No populate_db — custom mode shouldn't need it.

        execute(
            "Random pasta",
            Some("500g"),
            Some(350.0),
            Some(7.0),
            Some(24.0),
            Some(25.0),
            Some(50.0),
            None,
            None,
            None,
            Some("13:00"),
            &config,
        )
        .unwrap();

        let note = read_today(dir.path(), &config);
        assert!(note.contains("- **13:00** Random pasta (500g) (350 kcal, 7.0g protein, 24.0g carbs, 25.0g fat) | GI ~50, GL ~12.0"), "got:\n{note}");
    }

    #[test]
    fn execute_custom_mode_partial_macros_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in(dir.path());

        let err = execute(
            "x",
            Some("500g"),
            Some(350.0),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some("13:00"),
            &config,
        )
        .unwrap_err();
        assert!(err.to_string().contains("Custom mode requires"));
    }

    #[test]
    fn execute_lookup_no_db_errors_with_suggestion() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in(dir.path());

        let err = execute(
            "anything",
            Some("500g"),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some("12:42"),
            &config,
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Database not found"), "got: {msg}");
    }

    #[test]
    fn execute_lookup_unknown_name_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in(dir.path());
        populate_db(&config);

        let err = execute(
            "ghost food",
            Some("500g"),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some("12:42"),
            &config,
        )
        .unwrap_err();
        assert!(err.to_string().contains("No nutrition entry"));
    }

    #[test]
    fn execute_date_flag_writes_to_named_day() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = config_in(dir.path());

        execute(
            "Custom item",
            Some("500g"),
            Some(350.0),
            Some(7.0),
            Some(24.0),
            Some(25.0),
            None,
            None,
            None,
            Some("2026-04-29"),
            Some("22:00"),
            &config,
        )
        .unwrap();

        let path = dir.path().join("2026-04-29.md");
        let note = std::fs::read_to_string(&path).unwrap();
        assert!(note.contains("- **22:00** Custom item"));
    }
```

- [ ] **Step 3: Run tests**

```bash
cargo test --lib food_cmd::tests
```
Expected: all 27 tests pass.

- [ ] **Step 4: Lint + full suite**

```bash
just lint && cargo test
```

- [ ] **Step 5: Commit**

```bash
git add src/cli/food_cmd.rs
git commit -m "$(cat <<'EOF'
feat: daylog food handler — DB lookup, custom flags, file write

Wires the food_cmd::execute handler end-to-end: amount parsing,
read-only DB lookup, custom-flag fallback, RenderedEntry → markdown
line, and atomic write through frontmatter::atomic_write. Custom mode
works without a DB; lookup mode errors with a clear suggestion when
the DB is missing or the name is unknown.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 11: presets/default.toml + README + CLAUDE.md docs

**Files:**
- Modify: `presets/default.toml`
- Modify: `README.md`
- Modify: `CLAUDE.md`

- [ ] **Step 1: Add `[notes.aliases]` to the default preset**

In `presets/default.toml`, after the `[metrics]` block, append:

```toml

# [notes.aliases]
# Short keys that `daylog note <key>` expands to fixed text.
# med-morning = "Morgonmedicin (Elvanse 70mg, Escitalopram 20mg, ...)"
# med-evening = "Kvällsmedicin (Escitalopram 10mg, ...)"
```

- [ ] **Step 2: Update README**

Add a new section to `README.md` titled `### Logging food, notes, and BP from the CLI` (or matching the existing section style). Include:

```markdown
### Logging food, notes, and BP from the CLI

Three top-level subcommands append timestamped entries to the
`## Food`, `## Notes`, and `## Vitals` sections of the day's note,
auto-inserting the section if it's missing.

```bash
# Food — nutrition-db lookup with gram or ml amount
daylog food "kelda skogssvampsoppa" 500g
daylog food "helmjölk" 250ml

# Food — total-panel foods need no amount
daylog food te
daylog food proteinshake

# Food — one-off custom item, all four macros required together;
# --gi / --gl / --ii independently optional. GL auto-computes when
# GI and carbs are both known.
daylog food --kcal 350 --protein 7 --carbs 24 --fat 25 \
            --gi 50 "Random pasta dish" 500g

# Note — literal text or a [notes.aliases] key
daylog note "Attentin 10mg"
daylog note med-morning

# BP — sys dia pulse; auto-picks bp_morning_* or bp_evening_*
# based on the measurement time vs. the 14:00 cutoff. --morning /
# --evening override.
daylog bp 141 96 70
daylog bp --evening 133 73 62

# Shared flags: --date YYYY-MM-DD and --time HH:MM (or H:MMam/pm)
# for retroactive entries.
daylog note --date 2026-04-29 --time 23:30 "Aritonin"
daylog bp --time 08:00 141 96 70   # logged at 14:30 — still morning
```

`[notes.aliases]` in `config.toml` lets you map short keys to
longer note text:

```toml
[notes.aliases]
med-morning = "Morgonmedicin (Elvanse 70mg, Escitalopram 20mg, Losartan/Hydro 100/12.5mg, Vialerg 10mg)"
```

These commands write the markdown only; the watcher re-materializes
the database within ~500 ms.
```

- [ ] **Step 3: Update CLAUDE.md File Map**

In the File Map block in `CLAUDE.md`, add entries:

- Under `src/`, between `frontmatter.rs` and `cli/`:

```
  body.rs              Line-oriented `## Section` primitives (ensure_section,
                       append_line_to_section). Sibling to frontmatter.rs.
                       Pure functions over &str.
```

- Under `cli/`, alongside the existing entries:

```
    food_cmd.rs        `daylog food` — nutrition-db lookup, scaling, custom flags
    note_cmd.rs        `daylog note` — alias resolution + body append
    bp_cmd.rs          `daylog bp` — slot dispatch + YAML scalars + Vitals line
```

- [ ] **Step 4: Verify documentation builds and lint**

```bash
just lint
```
Expected: clean (formatting unchanged for non-Rust files).

- [ ] **Step 5: Commit**

```bash
git add presets/default.toml README.md CLAUDE.md
git commit -m "$(cat <<'EOF'
docs: README + CLAUDE.md + default preset for food/note/bp

Documents the three new subcommands, the [notes.aliases] config
table, and adds File Map entries for body.rs, food_cmd.rs,
note_cmd.rs, bp_cmd.rs.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 12: Integration test — full day round-trip

**Files:**
- Modify: `tests/integration.rs`

- [ ] **Step 1: Add an integration test covering all three commands**

Append to `tests/integration.rs`:

```rust
/// End-to-end: run food + bp + note on a fresh today's note and verify
/// the resulting markdown has all three sections in canonical order
/// with their respective entries.
#[test]
fn test_food_note_bp_full_day() {
    use daylog::db::{FoodInsert, NutrientPanel};

    let (dir, config) = setup_test_env();
    let registry = modules::build_registry(&config);
    let _conn = setup_db(&config, &registry);

    // Seed the nutrition DB with one entry for the food lookup.
    let conn = db::open_rw(&config.db_path()).unwrap();
    db::insert_food(
        &conn,
        &FoodInsert {
            name: "Kelda Skogssvampsoppa".into(),
            per_100g: Some(NutrientPanel {
                kcal: Some(70.0),
                protein: Some(1.4),
                carbs: Some(4.8),
                fat: Some(5.0),
                sat_fat: None,
                sugar: None,
                salt: None,
                fiber: None,
            }),
            per_100ml: None,
            density_g_per_ml: None,
            total: None,
            gi: Some(40.0),
            gl_per_100g: Some(2.0),
            gl_per_100ml: None,
            ii: Some(35.0),
            description: None,
            notes: None,
            aliases: vec!["kelda skogssvampsoppa".into()],
            ingredients: vec![],
        },
    )
    .unwrap();
    drop(conn);

    daylog::cli::bp_cmd::execute(141, 96, 70, false, false, None, Some("07:30"), &config).unwrap();
    daylog::cli::food_cmd::execute(
        "kelda skogssvampsoppa",
        Some("500g"),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        Some("12:42"),
        &config,
    )
    .unwrap();
    daylog::cli::note_cmd::execute(
        &["Attentin".into(), "10mg".into()],
        None,
        Some("13:00"),
        &config,
    )
    .unwrap();

    let date = config.effective_today();
    let note = std::fs::read_to_string(dir.path().join(format!("{date}.md"))).unwrap();

    // YAML scalars from BP.
    assert!(note.contains("bp_morning_sys: 141"), "got:\n{note}");
    assert!(note.contains("bp_morning_dia: 96"));
    assert!(note.contains("bp_morning_pulse: 70"));

    // Sections in canonical order.
    let food = note.find("## Food").expect("## Food");
    let vitals = note.find("## Vitals").expect("## Vitals");
    let notes_h = note.find("## Notes").expect("## Notes");
    assert!(food < vitals && vitals < notes_h, "wrong order:\n{note}");

    // Each section has its line.
    assert!(note.contains("- **07:30** BP: 141/96, pulse 70 bpm"));
    assert!(note.contains("- **12:42** Kelda Skogssvampsoppa (500g)"));
    assert!(note.contains("- **13:00** Attentin 10mg"));
}
```

- [ ] **Step 2: Run integration test**

```bash
cargo test --test integration test_food_note_bp_full_day
```
Expected: PASS.

- [ ] **Step 3: Run full suite**

```bash
just lint && cargo test
```

- [ ] **Step 4: Commit**

```bash
git add tests/integration.rs
git commit -m "$(cat <<'EOF'
test: integration coverage for food + bp + note round-trip

Seeds the nutrition DB, runs all three commands on a single day,
and asserts the resulting markdown has YAML scalars, all three
canonical sections in order, and one entry per section.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 13: Manual smoke + open PR

- [ ] **Step 1: Manual smoke against a real notes dir (optional but recommended)**

In a throwaway `~/daylog-notes-smoke/` (or similar), run:

```bash
mkdir -p ~/daylog-notes-smoke
# Bootstrap a fresh config pointing at the smoke dir:
mkdir -p ~/daylog-smoke-cfg/daylog
cat > ~/daylog-smoke-cfg/daylog/config.toml <<EOF
notes_dir = "$HOME/daylog-notes-smoke"
time_format = "24h"

[modules]
dashboard = true
training = true
trends = true
EOF

XDG_CONFIG_HOME=~/daylog-smoke-cfg cargo run -- food --kcal 100 --protein 5 --carbs 10 --fat 3 "Smoke food" 200g
XDG_CONFIG_HOME=~/daylog-smoke-cfg cargo run -- note "smoke test"
XDG_CONFIG_HOME=~/daylog-smoke-cfg cargo run -- bp 120 80 65

ls ~/daylog-notes-smoke
cat ~/daylog-notes-smoke/$(date +%Y-%m-%d).md
```

Expected: today's note contains all three sections in canonical order with one entry each.

Cleanup:
```bash
rm -rf ~/daylog-notes-smoke ~/daylog-smoke-cfg
```

- [ ] **Step 2: Push branch and open PR (targeting fork, not upstream)**

```bash
git push -u origin cli-food-note-bp
gh pr create -R adrianschmidt/daylog --base main --head adrianschmidt:cli-food-note-bp \
    --title "feat: daylog food/note/bp CLI commands (closes #6)" \
    --body "$(cat <<'EOF'
## Summary
- Three new top-level subcommands — `daylog food`, `daylog note`, `daylog bp` — that append timestamped entries to `## Food`, `## Notes`, and `## Vitals` sections.
- `daylog food` integrates with the structured nutrition DB (issue #10), scaling per-100g/per-100ml panels by `<amount>g` / `<amount>ml`, with a `--kcal/--protein/--carbs/--fat` custom-flag fallback.
- `daylog bp` writes YAML scalars and a `## Vitals` body line in one atomic pass, with auto morning/evening dispatch (cutoff at 14:00) and `--morning`/`--evening` overrides.
- New `body.rs` module provides `ensure_section` + `append_line_to_section` primitives, mirroring `frontmatter.rs`'s line-oriented style. Daily-note template gains `## Food` and `## Vitals` (joining `## Notes`).
- Shared `--date YYYY-MM-DD` and `--time HH:MM` flags on all three commands for retroactive entries.

Spec: `docs/superpowers/specs/2026-04-30-cli-food-note-bp-design.md`
Closes #6.

## Test plan
- [x] `body.rs` unit tests cover ensure_section ordering and append edge cases
- [x] `food_cmd.rs` unit tests cover amount parsing, scaling, GL auto-compute, output formatting, custom-mode, lookup-mode, and DB-missing paths
- [x] `note_cmd.rs` unit tests cover alias expansion, fall-through, empty-text error, and `--date`/`--time` flags
- [x] `bp_cmd.rs` unit tests cover slot dispatch, YAML overwrite + body append on rerun, all flag combinations
- [x] Integration test asserts a full food + bp + note round-trip on a fresh today's note
- [x] `cargo test` and `just lint` clean
EOF
)"
```

- [ ] **Step 3: Verify PR**

The `gh pr create` output prints the PR URL. Open it and confirm:
- Base: `adrianschmidt/daylog:main` (NOT `tfolkman/daylog:main`)
- Title and body render as expected
- CI passes

If the base is wrong, close the PR immediately and re-run with the correct `-R` and `--head` flags.

---

## Self-Review Notes

After writing the plan:

1. **Spec coverage:** Each spec section has at least one task:
   - User-facing surface → Tasks 5, 6, 7, 9, 10
   - Architecture / file map → Tasks 1, 5, 6, 7, 9, 10
   - `body.rs` algorithms → Tasks 1, 2
   - Daily-note template changes → Task 3
   - Configuration → Task 4, 11
   - Data flow examples → Task 12 (integration) plus per-command tests
   - Error handling matrix → covered in unit tests across Tasks 6, 7, 9, 10
   - CLI definition → Task 5
   - Custom-mode flag validation → Task 10 (`require_custom_complete`)
   - Testing strategy → Tasks 1–10, 12

2. **Type consistency:** `RenderedEntry`, `Amount`, `CustomNutrients`, `Slot`, `pick_slot`, `ensure_section`, `append_line_to_section`, `parse_amount`, `render_lookup`, `render_custom`, `format_line`, `lookup_food`, `validate_or_warn`, `target_date`, `target_time`, `CANONICAL_SECTION_ORDER` — all defined in earlier tasks and referenced consistently in later ones.

3. **No placeholders:** No `TODO`, `TBD`, "fill in", or "similar to Task N" references remain.

4. **Frequent commits:** 13 tasks, each ending with a commit. Tasks are small enough to land independently.

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-04-30-cli-food-note-bp.md`.
