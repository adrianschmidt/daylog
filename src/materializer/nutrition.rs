use crate::config::Config;
use color_eyre::eyre::Result;
use rusqlite::Connection;
use std::path::Path;
use std::sync::LazyLock;
use yaml_rust2::{Yaml, YamlLoader};

#[derive(Debug, Clone)]
#[allow(dead_code)]
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

use crate::db::{FoodIngredient, FoodInsert, NutrientPanel, TotalPanel};
use color_eyre::eyre::eyre;

#[allow(dead_code)]
const KNOWN_TOP_LEVEL_KEYS: &[&str] = &[
    "per_100g",
    "per_100ml",
    "density_g_per_ml",
    "gi",
    "gl_per_100g",
    "gl_per_100ml",
    "ii",
    "aliases",
    "description",
    "ingredients",
    "total",
];

#[allow(dead_code)]
pub(crate) fn build_food_insert(entry: &ParsedEntry) -> Result<FoodInsert> {
    let yaml = &entry.yaml;
    if matches!(yaml, Yaml::BadValue | Yaml::Null) {
        return Err(eyre!(
            "entry '{}' (line {}): YAML block is empty or malformed",
            entry.name,
            entry.line_number
        ));
    }

    let per_100g = read_nutrient_panel(&yaml["per_100g"]);
    let per_100ml = read_nutrient_panel(&yaml["per_100ml"]);
    let total = read_total_panel(&yaml["total"]);
    if per_100g.is_none() && per_100ml.is_none() && total.is_none() {
        return Err(eyre!(
            "entry '{}' (line {}): must include per_100g, per_100ml, or total",
            entry.name,
            entry.line_number
        ));
    }

    let density_g_per_ml = read_real(&yaml["density_g_per_ml"]);
    if let Some(d) = density_g_per_ml {
        if d <= 0.0 {
            return Err(eyre!(
                "entry '{}' (line {}): density_g_per_ml must be > 0 (got {})",
                entry.name,
                entry.line_number,
                d
            ));
        }
    }

    let gi = read_real(&yaml["gi"]);
    let gl_per_100g = read_real(&yaml["gl_per_100g"]);
    let gl_per_100ml = read_real(&yaml["gl_per_100ml"]);
    let ii = read_real(&yaml["ii"]);
    for (label, val) in [("gi", gi), ("ii", ii)] {
        if let Some(v) = val {
            if !(0.0..=200.0).contains(&v) {
                eprintln!(
                    "Warning: entry '{}' (line {}): {label}={v} outside 0..200 (still stored)",
                    entry.name, entry.line_number
                );
            }
        }
    }

    let description = read_string(&yaml["description"]);
    let notes = entry.notes.clone();

    let mut aliases: Vec<String> = read_string_list(&yaml["aliases"])
        .into_iter()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    aliases.push(entry.name.trim().to_lowercase());
    aliases.sort();
    aliases.dedup();

    let ingredients = read_ingredients(&yaml["ingredients"], &entry.name, entry.line_number);

    if let Yaml::Hash(hash) = yaml {
        for (k, _) in hash.iter() {
            if let Yaml::String(key) = k {
                if !KNOWN_TOP_LEVEL_KEYS.contains(&key.as_str()) {
                    eprintln!(
                        "Warning: entry '{}' (line {}): unknown key '{key}' ignored",
                        entry.name, entry.line_number
                    );
                }
            }
        }
    }

    Ok(FoodInsert {
        name: entry.name.trim().to_string(),
        per_100g,
        per_100ml,
        density_g_per_ml,
        total,
        gi,
        gl_per_100g,
        gl_per_100ml,
        ii,
        description,
        notes,
        aliases,
        ingredients,
    })
}

#[allow(dead_code)]
fn read_nutrient_panel(node: &Yaml) -> Option<NutrientPanel> {
    if matches!(node, Yaml::BadValue | Yaml::Null) {
        return None;
    }
    let panel = NutrientPanel {
        kcal: read_real(&node["kcal"]),
        protein: read_real(&node["protein"]),
        carbs: read_real(&node["carbs"]),
        fat: read_real(&node["fat"]),
        sat_fat: read_real(&node["sat_fat"]),
        sugar: read_real(&node["sugar"]),
        salt: read_real(&node["salt"]),
        fiber: read_real(&node["fiber"]),
    };
    if panel == NutrientPanel::default() {
        None
    } else {
        Some(panel)
    }
}

#[allow(dead_code)]
fn read_total_panel(node: &Yaml) -> Option<TotalPanel> {
    if matches!(node, Yaml::BadValue | Yaml::Null) {
        return None;
    }
    let panel = TotalPanel {
        weight_g: read_real(&node["weight_g"]),
        kcal: read_real(&node["kcal"]),
        protein: read_real(&node["protein"]),
        carbs: read_real(&node["carbs"]),
        fat: read_real(&node["fat"]),
        sat_fat: read_real(&node["sat_fat"]),
        sugar: read_real(&node["sugar"]),
        salt: read_real(&node["salt"]),
        fiber: read_real(&node["fiber"]),
    };
    if panel == TotalPanel::default() {
        None
    } else {
        Some(panel)
    }
}

#[allow(dead_code)]
fn read_real(node: &Yaml) -> Option<f64> {
    match node {
        Yaml::Real(s) => s.parse().ok(),
        Yaml::Integer(i) => Some(*i as f64),
        Yaml::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

#[allow(dead_code)]
fn read_string(node: &Yaml) -> Option<String> {
    match node {
        Yaml::String(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        _ => None,
    }
}

#[allow(dead_code)]
fn read_string_list(node: &Yaml) -> Vec<String> {
    match node {
        Yaml::Array(arr) => arr
            .iter()
            .filter_map(|item| match item {
                Yaml::String(s) => Some(s.clone()),
                Yaml::Integer(i) => Some(i.to_string()),
                _ => None,
            })
            .collect(),
        _ => vec![],
    }
}

#[allow(dead_code)]
fn read_ingredients(node: &Yaml, entry_name: &str, lineno: usize) -> Vec<FoodIngredient> {
    let Yaml::Array(arr) = node else {
        return vec![];
    };
    let mut out = Vec::new();
    for item in arr {
        let Yaml::Hash(_) = item else {
            eprintln!(
                "Warning: entry '{entry_name}' (line {lineno}): ingredient is not a mapping — skipped"
            );
            continue;
        };
        let food = read_string(&item["food"]);
        let Some(food_name) = food else {
            eprintln!(
                "Warning: entry '{entry_name}' (line {lineno}): ingredient missing 'food' — skipped"
            );
            continue;
        };
        out.push(FoodIngredient {
            ingredient_name: food_name,
            amount_g: read_real(&item["amount_g"]),
        });
    }
    out
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

    use yaml_rust2::YamlLoader;

    fn parse(name: &str, yaml_str: &str) -> ParsedEntry {
        let yaml = YamlLoader::load_from_str(yaml_str)
            .ok()
            .and_then(|mut d| {
                if d.is_empty() {
                    None
                } else {
                    Some(d.remove(0))
                }
            })
            .unwrap_or(Yaml::Null);
        ParsedEntry {
            name: name.to_string(),
            yaml,
            notes: None,
            line_number: 1,
        }
    }

    #[test]
    fn build_basic_per_100g() {
        let entry = parse(
            "Kelda Skogssvampsoppa",
            "per_100g:\n  kcal: 70\n  protein: 1.4\ngi: 40\n",
        );
        let fi = build_food_insert(&entry).unwrap();
        assert_eq!(fi.name, "Kelda Skogssvampsoppa");
        assert_eq!(fi.per_100g.as_ref().unwrap().kcal, Some(70.0));
        assert_eq!(fi.per_100g.as_ref().unwrap().protein, Some(1.4));
        assert_eq!(fi.gi, Some(40.0));
        // Heading is auto-added as a lowercased alias.
        assert!(fi.aliases.contains(&"kelda skogssvampsoppa".to_string()));
    }

    #[test]
    fn build_basic_per_100ml_with_density() {
        let entry = parse(
            "Helmjölk",
            "per_100ml:\n  kcal: 62\ndensity_g_per_ml: 1.03\n",
        );
        let fi = build_food_insert(&entry).unwrap();
        assert_eq!(fi.per_100ml.as_ref().unwrap().kcal, Some(62.0));
        assert_eq!(fi.density_g_per_ml, Some(1.03));
    }

    #[test]
    fn build_total_only_is_valid() {
        let entry = parse(
            "proteinshake",
            "description: 62g pulver + 4 dl vatten\n\
             total:\n  weight_g: 462\n  kcal: 234\n  protein: 48\n",
        );
        let fi = build_food_insert(&entry).unwrap();
        assert!(fi.total.is_some());
        assert_eq!(fi.total.as_ref().unwrap().kcal, Some(234.0));
        assert_eq!(fi.description.as_deref(), Some("62g pulver + 4 dl vatten"));
    }

    #[test]
    fn build_rejects_no_panel() {
        let entry = parse("Empty", "gi: 40\n");
        let err = build_food_insert(&entry).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("Empty") && msg.contains("per_100g"),
            "expected error to mention entry name and missing panels: {msg}"
        );
    }

    #[test]
    fn build_rejects_zero_density() {
        let entry = parse("Bad", "per_100g:\n  kcal: 50\ndensity_g_per_ml: 0\n");
        let err = build_food_insert(&entry).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("density"), "expected density error: {msg}");
    }

    #[test]
    fn build_aliases_normalized_and_deduped() {
        let entry = parse(
            "Foo Bar",
            "per_100g:\n  kcal: 1\naliases: [Foo, \"FOO\", \"Bar Baz\", \"foo bar\"]\n",
        );
        let fi = build_food_insert(&entry).unwrap();
        // heading auto-added, all lowercased, deduped
        let mut aliases = fi.aliases.clone();
        aliases.sort();
        assert!(aliases.contains(&"foo".to_string()));
        assert!(aliases.contains(&"bar baz".to_string()));
        assert!(aliases.contains(&"foo bar".to_string()));
        // dedup
        let unique_count = {
            let mut a = aliases.clone();
            a.dedup();
            a.len()
        };
        assert_eq!(unique_count, aliases.len());
    }

    #[test]
    fn build_ingredients_preserve_order() {
        let entry = parse(
            "Composite",
            "total:\n  kcal: 100\n\
             ingredients:\n  - food: Whey\n    amount_g: 62\n  - food: Water\n  - food: Sugar\n    amount_g: 5\n",
        );
        let fi = build_food_insert(&entry).unwrap();
        assert_eq!(fi.ingredients.len(), 3);
        assert_eq!(fi.ingredients[0].ingredient_name, "Whey");
        assert_eq!(fi.ingredients[0].amount_g, Some(62.0));
        assert_eq!(fi.ingredients[1].ingredient_name, "Water");
        assert_eq!(fi.ingredients[1].amount_g, None);
        assert_eq!(fi.ingredients[2].ingredient_name, "Sugar");
    }

    #[test]
    fn build_ingredient_missing_food_skipped() {
        let entry = parse(
            "Composite",
            "total:\n  kcal: 100\n\
             ingredients:\n  - amount_g: 50\n  - food: Whey\n    amount_g: 62\n",
        );
        let fi = build_food_insert(&entry).unwrap();
        assert_eq!(fi.ingredients.len(), 1);
        assert_eq!(fi.ingredients[0].ingredient_name, "Whey");
    }

    #[test]
    fn build_unknown_top_level_key_warns_not_errors() {
        let entry = parse("Foo", "per_100g:\n  kcal: 1\ntags: [foo, bar]\n");
        // Should not panic / error; entry built normally.
        let fi = build_food_insert(&entry).unwrap();
        assert!(fi.per_100g.is_some());
    }

    #[test]
    fn build_uses_notes_when_present() {
        let mut entry = parse("Foo", "per_100g:\n  kcal: 1\n");
        entry.notes = Some("Some prose.".to_string());
        let fi = build_food_insert(&entry).unwrap();
        assert_eq!(fi.notes.as_deref(), Some("Some prose."));
    }
}
