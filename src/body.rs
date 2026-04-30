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
            let mut out: Vec<String> = body_lines.iter().take(idx).map(|s| s.to_string()).collect();
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
        assert!(
            food_idx < vitals_idx && vitals_idx < notes_idx,
            "got:\n{result}"
        );
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
