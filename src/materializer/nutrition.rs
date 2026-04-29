use crate::config::Config;
use color_eyre::eyre::Result;
use rusqlite::Connection;
use std::path::Path;
use std::sync::LazyLock;
use yaml_rust2::{Yaml, YamlLoader};

#[derive(Debug, Clone)]
#[allow(dead_code)] // Wired into materialize_nutrition_db in a follow-up task.
pub(crate) struct ParsedEntry {
    pub name: String,
    pub yaml: Yaml,
    pub notes: Option<String>,
    pub line_number: usize,
}

#[allow(dead_code)] // Used by split_entries; wired into materializer in a follow-up task.
static RE_HEADING: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^##\s+(.+?)\s*$").unwrap());
#[allow(dead_code)] // Used by split_entries; wired into materializer in a follow-up task.
static RE_YAML_FENCE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^```yaml\s*$").unwrap());
#[allow(dead_code)] // Used by split_entries; wired into materializer in a follow-up task.
static RE_FENCE_CLOSE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^```\s*$").unwrap());

#[derive(Default)]
#[allow(dead_code)] // Used by split_entries; wired into materializer in a follow-up task.
struct PendingEntry {
    name: String,
    line_number: usize,
    yaml_lines: Vec<String>,
    notes_lines: Vec<String>,
    in_yaml_fence: bool,
    yaml_seen: bool,
}

#[allow(dead_code)] // Wired into materialize_nutrition_db in a follow-up task.
pub(crate) fn split_entries(content: &str) -> Vec<ParsedEntry> {
    let mut entries = Vec::new();
    let mut current: Option<PendingEntry> = None;

    for (idx, line) in content.lines().enumerate() {
        let lineno = idx + 1;

        if let Some(caps) = RE_HEADING.captures(line) {
            // Flush previous entry before starting a new one.
            if let Some(prev) = current.take() {
                if let Some(entry) = finalize(prev) {
                    entries.push(entry);
                }
            }
            current = Some(PendingEntry {
                name: caps.get(1).unwrap().as_str().to_string(),
                line_number: lineno,
                ..Default::default()
            });
            continue;
        }

        let Some(entry) = current.as_mut() else {
            continue; // before first ## heading: ignore
        };

        if entry.in_yaml_fence {
            if RE_FENCE_CLOSE.is_match(line) {
                entry.in_yaml_fence = false;
            } else {
                entry.yaml_lines.push(line.to_string());
            }
            continue;
        }

        if !entry.yaml_seen && RE_YAML_FENCE.is_match(line) {
            entry.in_yaml_fence = true;
            entry.yaml_seen = true;
            continue;
        }

        // Anything else (prose, dividers) goes to notes.
        entry.notes_lines.push(line.to_string());
    }

    if let Some(prev) = current.take() {
        if let Some(entry) = finalize(prev) {
            entries.push(entry);
        }
    }
    entries
}

#[allow(dead_code)] // Used by split_entries; wired into materializer in a follow-up task.
fn finalize(pending: PendingEntry) -> Option<ParsedEntry> {
    if !pending.yaml_seen {
        eprintln!(
            "Warning: nutrition-db.md entry '{}' (line {}) has no fenced YAML block — skipped",
            pending.name, pending.line_number
        );
        return None;
    }
    let yaml_str = pending.yaml_lines.join("\n");
    let yaml = match YamlLoader::load_from_str(&yaml_str) {
        Ok(mut docs) if !docs.is_empty() => docs.remove(0),
        Ok(_) => Yaml::Null,
        Err(_) => Yaml::BadValue,
    };
    let notes_joined = pending
        .notes_lines
        .join("\n")
        .trim_matches(|c: char| c == '\n' || c.is_whitespace())
        .to_string();
    let notes = if notes_joined.is_empty() {
        None
    } else {
        Some(notes_joined)
    };
    Some(ParsedEntry {
        name: pending.name,
        yaml,
        notes,
        line_number: pending.line_number,
    })
}

pub fn materialize_nutrition_db(
    _conn: &Connection,
    _file_path: &Path,
    _config: &Config,
) -> Result<usize> {
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use yaml_rust2::Yaml;

    #[test]
    fn split_basic_three_entries() {
        let content = r#"# Nutrition

## Kelda Skogssvampsoppa

```yaml
per_100g:
  kcal: 70
```

Some prose here.

---

## Laktosfri helmjölk 3%

```yaml
per_100ml:
  kcal: 62
```

## proteinshake

```yaml
description: shake
total:
  weight_g: 462
  kcal: 234
```
"#;
        let entries = split_entries(content);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].name, "Kelda Skogssvampsoppa");
        assert_eq!(entries[1].name, "Laktosfri helmjölk 3%");
        assert_eq!(entries[2].name, "proteinshake");
    }

    #[test]
    fn split_attaches_notes_below_yaml() {
        let content = r#"## Foo

```yaml
per_100g:
  kcal: 100
```

Free prose under the block.

More prose.
"#;
        let entries = split_entries(content);
        assert_eq!(entries.len(), 1);
        let notes = entries[0].notes.as_ref().unwrap();
        assert!(notes.contains("Free prose"));
        assert!(notes.contains("More prose"));
    }

    #[test]
    fn split_skips_heading_without_yaml_block() {
        let content = r#"## NoYaml

just some words.

## HasYaml

```yaml
per_100g:
  kcal: 50
```
"#;
        let entries = split_entries(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "HasYaml");
    }

    #[test]
    fn split_tolerates_h1_and_dividers() {
        let content = r#"# Top Title

---

## Foo

```yaml
per_100g:
  kcal: 1
```

---
"#;
        let entries = split_entries(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "Foo");
    }

    #[test]
    fn split_records_line_numbers() {
        let content = r#"## First

```yaml
per_100g:
  kcal: 1
```

## Second

```yaml
per_100ml:
  kcal: 1
```
"#;
        let entries = split_entries(content);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].line_number, 1);
        assert!(entries[1].line_number > entries[0].line_number);
    }

    #[test]
    fn split_empty_file_returns_empty() {
        assert!(split_entries("").is_empty());
        assert!(split_entries("# Just a title\n").is_empty());
    }

    #[test]
    fn split_yaml_with_syntax_error_still_returns_entry() {
        // The splitter shouldn't decide validity — that's build_food_insert's job.
        // Bad YAML → entry.yaml is Yaml::BadValue, but the entry is still returned.
        let content = r#"## Broken

```yaml
per_100g:
  kcal: : :
```
"#;
        let entries = split_entries(content);
        assert_eq!(entries.len(), 1);
        assert!(matches!(entries[0].yaml, Yaml::BadValue));
    }
}
