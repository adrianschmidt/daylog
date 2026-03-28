use color_eyre::eyre::{Result, WrapErr};
use std::path::Path;

/// Split a file into (yaml_lines, body) where yaml_lines is the content
/// between the opening and closing `---` markers, and body is everything after.
fn split_frontmatter(content: &str) -> (Vec<String>, String) {
    let lines: Vec<&str> = content.lines().collect();

    if lines.is_empty() || lines[0].trim() != "---" {
        return (vec![], content.to_string());
    }

    // Find closing ---
    let mut close_idx = None;
    for (i, line) in lines.iter().enumerate().skip(1) {
        if line.trim() == "---" {
            close_idx = Some(i);
            break;
        }
    }

    match close_idx {
        Some(idx) => {
            let yaml_lines: Vec<String> = lines[1..idx].iter().map(|s| s.to_string()).collect();
            // Body includes the closing --- and everything after
            let body_start = lines[..=idx]
                .iter()
                .map(|l| l.len() + 1) // +1 for newline
                .sum::<usize>();
            let body = if body_start <= content.len() {
                content[body_start..].to_string()
            } else {
                String::new()
            };
            (yaml_lines, body)
        }
        None => {
            // No closing ---, treat everything as YAML
            let yaml_lines: Vec<String> = lines[1..].iter().map(|s| s.to_string()).collect();
            (yaml_lines, String::new())
        }
    }
}

/// Reassemble frontmatter and body into a complete file.
fn reassemble(yaml_lines: &[String], body: &str) -> String {
    let mut result = String::new();
    result.push_str("---\n");
    for line in yaml_lines {
        result.push_str(line);
        result.push('\n');
    }
    result.push_str("---\n");
    result.push_str(body);
    result
}

/// Set a top-level scalar field in the frontmatter.
/// If the field exists, replace its value. If not, append it.
pub fn set_scalar(content: &str, key: &str, value: &str) -> String {
    let (mut yaml_lines, body) = split_frontmatter(content);

    if yaml_lines.is_empty() && body == content {
        // No frontmatter — create it
        yaml_lines = vec![format!("{key}: {value}")];
        return reassemble(&yaml_lines, &body);
    }

    // Find existing key
    let key_pattern = format!("{}:", key);
    let mut found = false;
    for line in &mut yaml_lines {
        let trimmed = line.trim_start();
        if trimmed.starts_with(&key_pattern) {
            // Check it's a top-level key (no indentation or same indentation level)
            let indent = line.len() - trimmed.len();
            if indent == 0 {
                // Preserve any inline comment
                let comment = if let Some(hash_pos) = find_inline_comment(trimmed) {
                    trimmed[hash_pos..].trim_start()
                } else {
                    ""
                };
                if comment.is_empty() {
                    *line = format!("{key}: {value}");
                } else {
                    *line = format!("{key}: {value} {comment}");
                }
                found = true;
                break;
            }
        }
    }

    if !found {
        yaml_lines.push(format!("{key}: {value}"));
    }

    reassemble(&yaml_lines, &body)
}

/// Set a nested field: parent.child = value.
/// Creates the parent key if it doesn't exist.
pub fn set_nested(content: &str, parent: &str, child: &str, value: &str) -> String {
    let (mut yaml_lines, body) = split_frontmatter(content);

    if yaml_lines.is_empty() && body == content {
        yaml_lines = vec![format!("{parent}:"), format!("  {child}: {value}")];
        return reassemble(&yaml_lines, &body);
    }

    let parent_pattern = format!("{}:", parent);

    // Find the parent key
    let parent_idx = yaml_lines
        .iter()
        .position(|l| l.trim_start().starts_with(&parent_pattern) && !l.starts_with(' '));

    match parent_idx {
        Some(idx) => {
            // Find the child under this parent, or the end of this parent's block
            let child_pattern = format!("{child}:");
            let mut child_idx = None;
            let mut block_end = yaml_lines.len();

            for (i, line) in yaml_lines.iter().enumerate().skip(idx + 1) {
                let trimmed = line.trim_start();
                let indent = line.len() - trimmed.len();

                if indent == 0 && !trimmed.is_empty() && !trimmed.starts_with('#') {
                    block_end = i;
                    break;
                }

                if indent > 0 && trimmed.starts_with(&child_pattern) {
                    child_idx = Some(i);
                    break;
                }
            }

            match child_idx {
                Some(ci) => {
                    yaml_lines[ci] = format!("  {child}: {value}");
                }
                None => {
                    yaml_lines.insert(block_end, format!("  {child}: {value}"));
                }
            }
        }
        None => {
            yaml_lines.push(format!("{parent}:"));
            yaml_lines.push(format!("  {child}: {value}"));
        }
    }

    reassemble(&yaml_lines, &body)
}

/// Append an item to a YAML list.
/// Creates the list key if it doesn't exist.
pub fn append_to_list(content: &str, list_key: &str, item: &str) -> String {
    let (mut yaml_lines, body) = split_frontmatter(content);

    if yaml_lines.is_empty() && body == content {
        yaml_lines = vec![format!("{list_key}:"), format!("  - {item}")];
        return reassemble(&yaml_lines, &body);
    }

    let key_pattern = format!("{}:", list_key);

    // Find the list key (could be nested)
    let list_idx = yaml_lines
        .iter()
        .position(|l| l.trim_start().starts_with(&key_pattern));

    match list_idx {
        Some(idx) => {
            let base_indent = yaml_lines[idx].len() - yaml_lines[idx].trim_start().len();
            let item_indent = base_indent + 2;

            // Find the end of the list
            let mut insert_at = yaml_lines.len();
            for (i, line) in yaml_lines.iter().enumerate().skip(idx + 1) {
                let trimmed = line.trim_start();
                let indent = line.len() - trimmed.len();

                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }

                if indent <= base_indent && !trimmed.starts_with('-') {
                    insert_at = i;
                    break;
                }
            }

            let indent_str = " ".repeat(item_indent);
            yaml_lines.insert(insert_at, format!("{indent_str}- {item}"));
        }
        None => {
            yaml_lines.push(format!("{list_key}:"));
            yaml_lines.push(format!("  - {item}"));
        }
    }

    reassemble(&yaml_lines, &body)
}

/// Write content to a file atomically (write temp + rename).
pub fn atomic_write(path: &Path, content: &str) -> Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| color_eyre::eyre::eyre!("Invalid file path: {}", path.display()))?;
    let temp_path = dir.join(format!(".daylog-tmp-{}", std::process::id()));
    std::fs::write(&temp_path, content)
        .wrap_err_with(|| format!("Failed to write temp file: {}", temp_path.display()))?;
    std::fs::rename(&temp_path, path)
        .wrap_err_with(|| format!("Failed to rename temp file to: {}", path.display()))?;
    Ok(())
}

/// Find the position of an inline comment (` #`) in a YAML value line.
/// Returns None if no inline comment is found.
fn find_inline_comment(line: &str) -> Option<usize> {
    // Look for ` #` that's not inside quotes
    let mut in_quote = false;
    let mut quote_char = ' ';
    let bytes = line.as_bytes();

    for i in 0..bytes.len() {
        let ch = bytes[i] as char;

        if in_quote {
            if ch == quote_char {
                in_quote = false;
            }
            continue;
        }

        if ch == '"' || ch == '\'' {
            in_quote = true;
            quote_char = ch;
            continue;
        }

        if ch == '#' && i > 0 && bytes[i - 1] == b' ' {
            return Some(i - 1);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "---
date: 2026-03-28
sleep: \"10:30pm-6:15am\"
weight: 173.4
mood: 4
lifts:
  squat: 185x5, 185x5
  bench: 135x8
---

## Notes

Good session.
";

    #[test]
    fn test_update_existing_scalar() {
        let result = set_scalar(SAMPLE, "weight", "175.0");
        assert!(result.contains("weight: 175.0"));
        assert!(!result.contains("173.4"));
        // Other fields preserved
        assert!(result.contains("mood: 4"));
        assert!(result.contains("## Notes"));
    }

    #[test]
    fn test_add_new_scalar() {
        let result = set_scalar(SAMPLE, "energy", "3");
        assert!(result.contains("energy: 3"));
        assert!(result.contains("weight: 173.4")); // existing preserved
    }

    #[test]
    fn test_update_nested_field() {
        let result = set_nested(SAMPLE, "lifts", "squat", "205x5, 205x5");
        assert!(result.contains("  squat: 205x5, 205x5"));
        assert!(result.contains("  bench: 135x8")); // other nested preserved
    }

    #[test]
    fn test_add_nested_to_existing_parent() {
        let result = set_nested(SAMPLE, "lifts", "pullup", "BWx8, BWx6");
        assert!(result.contains("  pullup: BWx8, BWx6"));
        assert!(result.contains("  squat: 185x5, 185x5")); // existing preserved
    }

    #[test]
    fn test_add_nested_new_parent() {
        let result = set_nested(SAMPLE, "cardio", "zone2_min", "30");
        assert!(result.contains("cardio:"));
        assert!(result.contains("  zone2_min: 30"));
    }

    #[test]
    fn test_append_to_existing_list() {
        let content = "---
date: 2026-03-28
climbs:
  sends:
    - V5
    - V4
---
";
        let result = append_to_list(content, "sends", "V6");
        assert!(result.contains("    - V5"));
        assert!(result.contains("    - V4"));
        assert!(result.contains("    - V6"));
    }

    #[test]
    fn test_append_to_new_list() {
        let result = append_to_list(SAMPLE, "sends", "V5");
        assert!(result.contains("sends:"));
        assert!(result.contains("  - V5"));
    }

    #[test]
    fn test_no_frontmatter_creates_it() {
        let content = "Just some markdown\n";
        let result = set_scalar(content, "weight", "173.4");
        assert!(result.starts_with("---\n"));
        assert!(result.contains("weight: 173.4"));
        assert!(result.contains("Just some markdown"));
    }

    #[test]
    fn test_empty_file() {
        let result = set_scalar("", "weight", "173.4");
        assert!(result.contains("---\nweight: 173.4\n---"));
    }

    #[test]
    fn test_preserves_comments_in_frontmatter() {
        let content = "---
date: 2026-03-28
weight: 173.4 # measured after breakfast
mood: 4
---
";
        let result = set_scalar(content, "weight", "175.0");
        assert!(result.contains("weight: 175.0 # measured after breakfast"));
    }

    #[test]
    fn test_preserves_markdown_body_with_triple_dash() {
        let content = "---
date: 2026-03-28
---

## Notes

Some text with --- in the middle.

---

More text.
";
        let result = set_scalar(content, "mood", "4");
        assert!(result.contains("mood: 4"));
        assert!(result.contains("Some text with --- in the middle."));
        assert!(result.contains("More text."));
    }

    #[test]
    fn test_roundtrip_preserves_content() {
        let result = set_scalar(SAMPLE, "weight", "173.4");
        // Setting to the same value should produce equivalent output
        assert!(result.contains("weight: 173.4"));
        assert!(result.contains("## Notes"));
        assert!(result.contains("Good session."));
    }

    #[test]
    fn test_atomic_write() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.md");
        atomic_write(&path, "hello world").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello world");
    }
}
