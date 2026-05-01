//! Parse the `## Food` section of a daily note back into aggregate
//! macro totals. Inverse of `cli::food_cmd::format_line`.

#[derive(Debug, Default, Clone, PartialEq)]
pub struct FoodTotals {
    pub kcal: f64,
    pub protein: f64,
    pub carbs: f64,
    pub fat: f64,
    pub entry_count: usize,
    pub skipped_lines: usize,
}

pub fn sum_food_section(markdown: &str) -> FoodTotals {
    let mut totals = FoodTotals::default();
    let lines: Vec<&str> = markdown.lines().collect();

    let start = match lines.iter().position(|l| l.trim_end() == "## Food") {
        Some(i) => i + 1,
        None => return totals,
    };
    let end = lines
        .iter()
        .enumerate()
        .skip(start)
        .find_map(|(i, l)| l.starts_with("## ").then_some(i))
        .unwrap_or(lines.len());

    for line in &lines[start..end] {
        if !line.starts_with("- **") {
            continue;
        }
        match parse_food_line(line) {
            Some((kcal, protein, carbs, fat)) => {
                totals.kcal += kcal;
                totals.protein += protein;
                totals.carbs += carbs;
                totals.fat += fat;
                totals.entry_count += 1;
            }
            None => {
                totals.skipped_lines += 1;
            }
        }
    }
    totals
}

fn parse_food_line(line: &str) -> Option<(f64, f64, f64, f64)> {
    let kcal = extract_number_before(line, " kcal")?;
    let protein = extract_number_before(line, "g protein").unwrap_or(0.0);
    let carbs = extract_number_before(line, "g carbs").unwrap_or(0.0);
    let fat = extract_number_before(line, "g fat").unwrap_or(0.0);
    Some((kcal, protein, carbs, fat))
}

/// Find the rightmost occurrence of `suffix` in `s`, then walk backwards
/// past whitespace to capture a number (digits + optional decimal point).
fn extract_number_before(s: &str, suffix: &str) -> Option<f64> {
    let pos = s.rfind(suffix)?;
    let before = &s.as_bytes()[..pos];
    let mut end = before.len();
    while end > 0 && before[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    let mut start = end;
    while start > 0 {
        let c = before[start - 1];
        if c.is_ascii_digit() || c == b'.' {
            start -= 1;
        } else {
            break;
        }
    }
    if start == end {
        return None;
    }
    std::str::from_utf8(&before[start..end]).ok()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_returns_zeros() {
        assert_eq!(sum_food_section(""), FoodTotals::default());
    }

    #[test]
    fn sums_single_well_formed_line() {
        let md = "---\ndate: 2026-04-30\n---\n\n## Food\n- **12:42** Soup (500g) (350 kcal, 7.0g protein, 24.0g carbs, 25.0g fat)\n";
        let r = sum_food_section(md);
        assert_eq!(r.kcal, 350.0);
        assert!((r.protein - 7.0).abs() < 1e-6);
        assert!((r.carbs - 24.0).abs() < 1e-6);
        assert!((r.fat - 25.0).abs() < 1e-6);
        assert_eq!(r.entry_count, 1);
        assert_eq!(r.skipped_lines, 0);
    }

    #[test]
    fn sums_multiple_lines() {
        let md = "## Food\n- **08:00** A (100 kcal, 1.0g protein, 10.0g carbs, 2.0g fat)\n- **12:00** B (200 kcal, 5.0g protein, 20.0g carbs, 8.0g fat)\n";
        let r = sum_food_section(md);
        assert_eq!(r.kcal, 300.0);
        assert_eq!(r.entry_count, 2);
    }

    #[test]
    fn line_missing_kcal_token_is_skipped() {
        let md = "## Food\n- **12:00** Hand-edited line with no nutrients\n";
        let r = sum_food_section(md);
        assert_eq!(r.entry_count, 0);
        assert_eq!(r.skipped_lines, 1);
    }

    #[test]
    fn line_with_only_kcal_treats_missing_macros_as_zero() {
        let md = "## Food\n- **08:00** Coffee (5 kcal)\n";
        let r = sum_food_section(md);
        assert_eq!(r.kcal, 5.0);
        assert_eq!(r.protein, 0.0);
        assert_eq!(r.entry_count, 1);
        assert_eq!(r.skipped_lines, 0);
    }

    #[test]
    fn prose_lines_under_food_section_ignored() {
        let md = "## Food\nHad a great breakfast today.\n- **08:00** Eggs (200 kcal, 12.0g protein, 1.0g carbs, 15.0g fat)\n";
        let r = sum_food_section(md);
        assert_eq!(r.entry_count, 1);
        assert_eq!(r.skipped_lines, 0);
    }

    #[test]
    fn no_food_section_returns_zeros() {
        let md = "---\ndate: 2026-04-30\n---\n\n## Notes\n- Nothing\n";
        assert_eq!(sum_food_section(md), FoodTotals::default());
    }

    #[test]
    fn stops_at_next_section_heading() {
        let md = "## Food\n- **08:00** A (100 kcal, 1.0g protein, 10.0g carbs, 2.0g fat)\n## Notes\n- **09:00** B (999 kcal, 99.0g protein, 99.0g carbs, 99.0g fat)\n";
        let r = sum_food_section(md);
        assert_eq!(r.kcal, 100.0);
        assert_eq!(r.entry_count, 1);
    }

    #[test]
    fn round_trip_with_format_line() {
        use crate::cli::food_cmd::{format_line, RenderedEntry};
        let entry = RenderedEntry {
            display_name: "Test".into(),
            amount_segment: Some((500.0, "g")),
            kcal: Some(350.0),
            protein: Some(7.0),
            carbs: Some(24.0),
            fat: Some(25.0),
            gi: Some(40.0),
            gl: Some(10.0),
            ii: Some(35.0),
        };
        let line = format_line(&entry, "12:42");
        let md = format!("## Food\n{line}\n");
        let r = sum_food_section(&md);
        assert_eq!(r.kcal, 350.0);
        assert!((r.protein - 7.0).abs() < 1e-6);
        assert!((r.carbs - 24.0).abs() < 1e-6);
        assert!((r.fat - 25.0).abs() < 1e-6);
    }
}
