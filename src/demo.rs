use chrono::{Datelike, Duration, Local, NaiveDate};
use color_eyre::eyre::Result;
use std::fmt::Write as FmtWrite;
use std::path::Path;

/// Generate 14 days of realistic demo data.
pub fn generate_demo_data(notes_dir: &Path) -> Result<u32> {
    let today = Local::now().date_naive();
    let mut count = 0;

    for day_offset in (0..14).rev() {
        let date = today - Duration::days(day_offset);
        let filename = format!("{}.md", date.format("%Y-%m-%d"));
        let path = notes_dir.join(&filename);

        if path.exists() {
            continue;
        }

        let content = generate_day(date, day_offset as usize)?;
        std::fs::write(&path, content)?;
        count += 1;
    }

    Ok(count)
}

fn generate_day(date: NaiveDate, offset: usize) -> Result<String> {
    let mut s = String::with_capacity(1024);
    writeln!(s, "---")?;
    writeln!(s, "date: {}", date.format("%Y-%m-%d"))?;

    // Sleep: roughly 10pm-6:30am with variance
    let sleep_hour = 10 + (offset % 2); // 10 or 11 pm
    let sleep_min = (offset * 7 + 15) % 60;
    let wake_hour = 5 + (offset + 1) % 3; // 5, 6, or 7 am
    let wake_min = (offset * 13 + 10) % 60;
    writeln!(
        s,
        "sleep: \"{}:{:02}pm-{}:{:02}am\"",
        sleep_hour, sleep_min, wake_hour, wake_min
    )?;

    // Sleep quality, mood, energy: 2-5 range
    let sq = 2 + (offset * 3 + 1) % 4;
    let mood = 2 + (offset * 5 + 2) % 4;
    let energy = 2 + (offset * 7 + 3) % 4;
    writeln!(s, "sleep_quality: {sq}")?;
    writeln!(s, "mood: {mood}")?;
    writeln!(s, "energy: {energy}")?;

    // Weight: 172-175 range
    let weight = 172.0 + (offset as f64 * 0.3) % 3.0;
    writeln!(s, "weight: {weight:.1}")?;

    // Session type rotates
    let day_of_week = date.weekday().num_days_from_monday();
    let (session_type, has_lifts, has_cardio) = match day_of_week {
        0 => ("lifting", true, false),   // Monday
        1 => ("cardio", false, true),    // Tuesday
        2 => ("lifting", true, false),   // Wednesday
        3 => ("rest", false, false),     // Thursday
        4 => ("lifting", true, false),   // Friday
        5 => ("climbing", false, false), // Saturday
        _ => ("rest", false, false),     // Sunday
    };

    writeln!(s, "type: {session_type}")?;
    let week = 1 + (offset / 7);
    writeln!(s, "week: {week}")?;
    writeln!(s, "block: volume")?;

    if session_type != "rest" {
        let duration = match session_type {
            "lifting" => 45 + (offset % 3) * 10,
            "cardio" => 30 + (offset % 2) * 15,
            "climbing" => 60 + (offset % 2) * 15,
            _ => 0,
        };
        if duration > 0 {
            writeln!(s, "duration: {duration}")?;
        }

        let rpe = 5.0 + (offset as f64 * 0.3) % 4.0;
        writeln!(s, "rpe: {rpe:.0}")?;
    }

    // Lifts
    if has_lifts {
        writeln!(s, "lifts:")?;
        let progression = offset as f64 * 2.5;
        match day_of_week {
            0 => {
                // Monday: squat + bench
                let sq_w = 185.0 + progression;
                let bench_w = 135.0 + progression * 0.6;
                writeln!(s, "  squat: {sq_w:.0}x5, {sq_w:.0}x5, {sq_w:.0}x5")?;
                writeln!(s, "  bench: {bench_w:.0}x8, {bench_w:.0}x8")?;
            }
            2 => {
                // Wednesday: deadlift + OHP
                let dl_w = 225.0 + progression;
                let ohp_w = 95.0 + progression * 0.4;
                writeln!(s, "  deadlift: {dl_w:.0}x5, {dl_w:.0}x3")?;
                writeln!(s, "  ohp: {ohp_w:.0}x8, {ohp_w:.0}x8, {ohp_w:.0}x8")?;
            }
            4 => {
                // Friday: pullup + RDL
                let rdl_w = 135.0 + progression * 0.7;
                writeln!(s, "  pullup: BWx8, BWx6, BWx5")?;
                writeln!(s, "  rdl: {rdl_w:.0}x8, {rdl_w:.0}x8")?;
            }
            _ => {}
        }
    }

    // Cardio
    if has_cardio {
        let zone2 = 25 + (offset % 3) * 5;
        let hr = 130 + (offset % 4) * 5;
        writeln!(s, "zone2_min: {zone2}")?;
        writeln!(s, "hr_avg: {hr}")?;
    }

    // Climbing (Saturday)
    if session_type == "climbing" {
        writeln!(s, "climbs:")?;
        writeln!(s, "  board: gym")?;
        writeln!(s, "  sends:")?;
        let base_grade = 3 + (offset % 3);
        writeln!(s, "    - V{base_grade}")?;
        writeln!(s, "    - V{} x2", base_grade.saturating_sub(1).max(2))?;
        writeln!(s, "    - V{}", base_grade + 1)?;
        writeln!(s, "  attempts:")?;
        writeln!(s, "    - V{}", base_grade + 2)?;
    }

    // Custom metrics (Tier 1 demo)
    let resting_hr = 48 + (offset % 8);
    writeln!(s, "resting_hr: {resting_hr}")?;

    writeln!(s, "---")?;
    writeln!(s)?;
    writeln!(s, "## Notes")?;
    writeln!(s)?;

    match session_type {
        "lifting" => writeln!(s, "Good session. Felt strong on the main lifts.")?,
        "cardio" => writeln!(s, "Easy zone 2 session. Kept heart rate steady.")?,
        "climbing" => writeln!(s, "Fun session at the gym. Sent a few new problems.")?,
        "rest" => writeln!(s, "Recovery day. Walked and stretched.")?,
        _ => {}
    }

    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_generate_demo_data() {
        let dir = TempDir::new().unwrap();
        let count = generate_demo_data(dir.path()).unwrap();
        assert_eq!(count, 14);

        // Verify files exist and have valid YAML frontmatter
        let files: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(files.len(), 14);

        for entry in files {
            let content = std::fs::read_to_string(entry.path()).unwrap();
            assert!(
                content.starts_with("---\n"),
                "File should start with YAML frontmatter"
            );
            assert!(content.contains("date:"), "File should have date field");
            assert!(content.contains("sleep:"), "File should have sleep field");
            assert!(content.contains("weight:"), "File should have weight field");
        }
    }

    #[test]
    fn test_idempotent_generation() {
        let dir = TempDir::new().unwrap();
        let count1 = generate_demo_data(dir.path()).unwrap();
        let count2 = generate_demo_data(dir.path()).unwrap();
        assert_eq!(count1, 14);
        assert_eq!(count2, 0); // No new files on second run
    }
}
