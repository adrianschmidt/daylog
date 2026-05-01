//! Parse `{notes_dir}/goals.md` YAML frontmatter into a suffix-keyed
//! threshold map. Recognized suffixes: `_target`, `_min`, `_max`.
//! Non-matching keys are silently ignored. Body of goals.md is not
//! read (free-form prose for the user / LLMs).

use color_eyre::eyre::Result;
use color_eyre::Help;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use yaml_rust2::{Yaml, YamlLoader};

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Threshold {
    pub target: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct Goals {
    pub thresholds: HashMap<String, Threshold>,
    pub source_path: PathBuf,
    pub present: bool,
}

/// Load goals from `{notes_dir}/goals.md`.
///
/// Suffix matching (`_target`/`_min`/`_max`) is case-sensitive — `kcal_min`
/// is recognized; `kcal_Min` is silently ignored as a non-suffix key.
pub fn load_goals(notes_dir: &Path) -> Result<Goals> {
    let path = notes_dir.join("goals.md");
    let empty = Goals {
        thresholds: HashMap::new(),
        source_path: path.clone(),
        present: false,
    };
    if !path.exists() {
        return Ok(empty);
    }
    let content = std::fs::read_to_string(&path)?;
    let content = content.replace("\r\n", "\n");
    let yaml_str = match extract_frontmatter(&content) {
        Some(s) => s,
        None => return Ok(empty),
    };
    let docs = YamlLoader::load_from_str(yaml_str)
        .map_err(|e| color_eyre::eyre::eyre!("Failed to parse goals.md frontmatter: {e}"))
        .suggestion("Goals frontmatter must be valid YAML between `---` markers.")?;
    let doc = match docs.into_iter().next() {
        Some(d) => d,
        None => return Ok(empty),
    };
    let map = match doc {
        Yaml::Hash(h) => h,
        _ => return Ok(empty),
    };

    let mut thresholds: HashMap<String, Threshold> = HashMap::new();
    for (k, v) in map {
        let key = match k.as_str() {
            Some(s) => s.to_string(),
            None => continue,
        };
        let (name, slot) = match split_suffix(&key) {
            Some(p) => p,
            None => continue,
        };
        let value = yaml_to_f64(&v)
            .ok_or_else(|| {
                color_eyre::eyre::eyre!(
                    "goals.md `{key}` must be a number, got: {}",
                    yaml_to_display(&v)
                )
            })
            .suggestion("Set numeric values like `kcal_min: 1900`.")?;
        let entry = thresholds.entry(name.to_string()).or_default();
        match slot {
            "target" => entry.target = Some(value),
            "min" => entry.min = Some(value),
            "max" => entry.max = Some(value),
            _ => unreachable!("split_suffix only returns target/min/max"),
        }
    }

    Ok(Goals {
        present: !thresholds.is_empty(),
        thresholds,
        source_path: path,
    })
}

/// Return the YAML between leading `---\n` and the next `---\n` line, or
/// `None` if no frontmatter block exists.
fn extract_frontmatter(content: &str) -> Option<&str> {
    let body = content.strip_prefix("---\n")?;
    let close = body.find("\n---\n").or_else(|| {
        // Allow trailing close marker without final newline.
        if body.ends_with("\n---") {
            Some(body.len() - 4)
        } else {
            None
        }
    })?;
    Some(&body[..close])
}

/// Split `<name>_<slot>` where slot is one of `target`/`min`/`max`.
fn split_suffix(key: &str) -> Option<(&str, &str)> {
    for (suffix, slot) in [("_target", "target"), ("_min", "min"), ("_max", "max")] {
        if let Some(name) = key.strip_suffix(suffix) {
            if !name.is_empty() {
                return Some((name, slot));
            }
        }
    }
    None
}

fn yaml_to_f64(y: &Yaml) -> Option<f64> {
    match y {
        Yaml::Integer(i) => Some(*i as f64),
        Yaml::Real(s) => s.parse().ok(),
        _ => None,
    }
}

fn yaml_to_display(y: &Yaml) -> String {
    match y {
        Yaml::String(s) => format!("`{s}`"),
        Yaml::Boolean(b) => format!("`{b}`"),
        Yaml::Null => "`null`".into(),
        _ => "non-numeric value".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_goals(dir: &Path, body: &str) {
        std::fs::write(dir.join("goals.md"), body).unwrap();
    }

    #[test]
    fn missing_file_returns_empty_not_present() {
        let dir = tempfile::TempDir::new().unwrap();
        let g = load_goals(dir.path()).unwrap();
        assert!(!g.present);
        assert!(g.thresholds.is_empty());
    }

    #[test]
    fn parses_simple_kcal_min() {
        let dir = tempfile::TempDir::new().unwrap();
        write_goals(dir.path(), "---\nkcal_min: 1900\n---\n\n# notes\n");
        let g = load_goals(dir.path()).unwrap();
        assert!(g.present);
        let t = g.thresholds.get("kcal").unwrap();
        assert_eq!(t.min, Some(1900.0));
        assert_eq!(t.max, None);
        assert_eq!(t.target, None);
    }

    #[test]
    fn groups_three_suffixes_for_one_metric() {
        let dir = tempfile::TempDir::new().unwrap();
        write_goals(
            dir.path(),
            "---\nkcal_min: 1900\nkcal_max: 2200\nkcal_target: 2050\n---\n",
        );
        let g = load_goals(dir.path()).unwrap();
        let t = g.thresholds.get("kcal").unwrap();
        assert_eq!(t.min, Some(1900.0));
        assert_eq!(t.max, Some(2200.0));
        assert_eq!(t.target, Some(2050.0));
    }

    #[test]
    fn non_suffix_keys_ignored() {
        let dir = tempfile::TempDir::new().unwrap();
        write_goals(
            dir.path(),
            "---\nkcal_min: 1900\nnotes: \"hi\"\nfavourite_color: blue\n---\n",
        );
        let g = load_goals(dir.path()).unwrap();
        assert_eq!(g.thresholds.len(), 1);
        assert!(g.thresholds.contains_key("kcal"));
    }

    #[test]
    fn no_frontmatter_returns_not_present() {
        let dir = tempfile::TempDir::new().unwrap();
        write_goals(dir.path(), "# Just a heading, no YAML\n");
        let g = load_goals(dir.path()).unwrap();
        assert!(!g.present);
    }

    #[test]
    fn empty_thresholds_not_present() {
        let dir = tempfile::TempDir::new().unwrap();
        write_goals(dir.path(), "---\nnotes: \"only commentary\"\n---\n");
        let g = load_goals(dir.path()).unwrap();
        assert!(!g.present);
    }

    #[test]
    fn non_numeric_value_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        write_goals(dir.path(), "---\nkcal_min: \"foo\"\n---\n");
        let err = load_goals(dir.path()).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("kcal_min"), "got: {msg}");
        assert!(msg.contains("must be a number"), "got: {msg}");
    }

    #[test]
    fn float_values_parse() {
        let dir = tempfile::TempDir::new().unwrap();
        write_goals(dir.path(), "---\nweight_target: 110.5\n---\n");
        let g = load_goals(dir.path()).unwrap();
        let t = g.thresholds.get("weight").unwrap();
        assert_eq!(t.target, Some(110.5));
    }

    #[test]
    fn multi_word_metric_name_uses_full_prefix() {
        let dir = tempfile::TempDir::new().unwrap();
        write_goals(dir.path(), "---\nresting_hr_max: 65\n---\n");
        let g = load_goals(dir.path()).unwrap();
        let t = g.thresholds.get("resting_hr").unwrap();
        assert_eq!(t.max, Some(65.0));
    }

    #[test]
    fn handles_crlf_line_endings() {
        let dir = tempfile::TempDir::new().unwrap();
        write_goals(dir.path(), "---\r\nkcal_min: 1900\r\n---\r\n");
        let g = load_goals(dir.path()).unwrap();
        assert!(g.present);
        assert_eq!(g.thresholds.get("kcal").unwrap().min, Some(1900.0));
    }
}
