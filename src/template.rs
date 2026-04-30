use crate::config::{Config, TimeFormat};

const DAILY_NOTE: &str = include_str!("../templates/daily-note.md");

/// Render the daily-note template for `date`, substituting placeholders
/// (date, weight unit, sleep example) using values from `config`.
pub fn render_daily_note(date: &str, config: &Config) -> String {
    let sleep_example = match config.time_format {
        TimeFormat::TwelveHour => "10:30pm-6:15am",
        TimeFormat::TwentyFourHour => "22:30-06:15",
    };
    DAILY_NOTE
        .replace("DATE_PLACEHOLDER", date)
        .replace("WEIGHT_UNIT_PLACEHOLDER", &config.weight_unit.to_string())
        .replace("SLEEP_EXAMPLE_PLACEHOLDER", sleep_example)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn config_with_unit(unit: &str) -> Config {
        toml::from_str(&format!(
            "notes_dir = '/tmp/test'\nweight_unit = '{unit}'\n"
        ))
        .expect("config parses")
    }

    fn config_with_time_format(fmt: &str) -> Config {
        toml::from_str(&format!("notes_dir = '/tmp/test'\ntime_format = '{fmt}'\n"))
            .expect("config parses")
    }

    #[test]
    fn renders_date_placeholder() {
        let config = config_with_unit("lbs");
        let out = render_daily_note("2026-04-17", &config);
        assert!(
            out.contains("date: 2026-04-17"),
            "expected rendered date, got: {out}"
        );
        assert!(
            !out.contains("DATE_PLACEHOLDER"),
            "DATE_PLACEHOLDER should be replaced"
        );
    }

    #[test]
    fn renders_weight_unit_lbs() {
        let config = config_with_unit("lbs");
        let out = render_daily_note("2026-04-17", &config);
        assert!(
            out.contains("weight:                   # lbs"),
            "expected `# lbs` weight comment, got: {out}"
        );
    }

    #[test]
    fn renders_sleep_example_12h_default() {
        let config = config_with_unit("lbs");
        let out = render_daily_note("2026-04-17", &config);
        assert!(
            out.contains("# e.g., 10:30pm-6:15am"),
            "expected 12h sleep example, got: {out}"
        );
        assert!(
            !out.contains("SLEEP_EXAMPLE_PLACEHOLDER"),
            "placeholder should be replaced"
        );
    }

    #[test]
    fn renders_sleep_example_24h() {
        let config = config_with_time_format("24h");
        let out = render_daily_note("2026-04-17", &config);
        assert!(
            out.contains("# e.g., 22:30-06:15"),
            "expected 24h sleep example, got: {out}"
        );
        assert!(
            !out.contains("10:30pm"),
            "12h example should not appear with 24h config, got: {out}"
        );
    }

    #[test]
    fn renders_weight_unit_kg() {
        let config = config_with_unit("kg");
        let out = render_daily_note("2026-04-17", &config);
        assert!(
            out.contains("weight:                   # kg"),
            "expected `# kg` weight comment, got: {out}"
        );
        assert!(
            !out.contains("# lbs"),
            "lbs comment should not appear when unit is kg, got: {out}"
        );
    }

    #[test]
    fn renders_food_section() {
        let config = config_with_unit("lbs");
        let out = render_daily_note("2026-04-30", &config);
        assert!(
            out.contains("## Food"),
            "expected ## Food section, got:\n{out}"
        );
    }

    #[test]
    fn renders_vitals_section() {
        let config = config_with_unit("lbs");
        let out = render_daily_note("2026-04-30", &config);
        assert!(
            out.contains("## Vitals"),
            "expected ## Vitals section, got:\n{out}"
        );
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
}
