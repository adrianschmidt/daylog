use chrono::NaiveTime;

/// Parse a time string in either 12-hour (`10:30pm`, `6am`) or
/// 24-hour (`22:30`, `0:28`, `06:52`) format. Case-insensitive.
/// Whitespace between digits and am/pm is tolerated.
pub fn parse_time(s: &str) -> Option<NaiveTime> {
    let s = s.trim().trim_matches('"').trim_matches('\'').trim();
    if s.is_empty() {
        return None;
    }
    let lower = s.to_lowercase();

    // Detect am/pm suffix
    let (clock_part, suffix) = if let Some(rest) = lower.strip_suffix("am") {
        (rest.trim_end(), Some(false))
    } else if let Some(rest) = lower.strip_suffix("pm") {
        (rest.trim_end(), Some(true))
    } else {
        (lower.as_str(), None)
    };

    if clock_part.is_empty() {
        return None;
    }

    // Parse "H" or "H:M"
    let (hour_str, minute_str) = match clock_part.split_once(':') {
        Some((h, m)) => (h, m),
        None => (clock_part, "0"),
    };
    let hour: u32 = hour_str.trim().parse().ok()?;
    let minute: u32 = minute_str.trim().parse().ok()?;

    let hour_24 = match suffix {
        Some(is_pm) => {
            // 12h: hour must be 1..=12
            if !(1..=12).contains(&hour) {
                return None;
            }
            match (is_pm, hour) {
                (false, 12) => 0,    // 12am = 00:00
                (false, h) => h,     // 1am..11am
                (true, 12) => 12,    // 12pm = 12:00
                (true, h) => h + 12, // 1pm..11pm
            }
        }
        None => {
            if hour > 23 {
                return None;
            }
            hour
        }
    };

    NaiveTime::from_hms_opt(hour_24, minute, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_24h_basic() {
        assert_eq!(parse_time("22:30"), NaiveTime::from_hms_opt(22, 30, 0));
        assert_eq!(parse_time("06:52"), NaiveTime::from_hms_opt(6, 52, 0));
        assert_eq!(parse_time("0:28"), NaiveTime::from_hms_opt(0, 28, 0));
        assert_eq!(parse_time("00:28"), NaiveTime::from_hms_opt(0, 28, 0));
        assert_eq!(parse_time("23:59"), NaiveTime::from_hms_opt(23, 59, 0));
    }

    #[test]
    fn parse_12h_basic() {
        assert_eq!(parse_time("10:30pm"), NaiveTime::from_hms_opt(22, 30, 0));
        assert_eq!(parse_time("6:15am"), NaiveTime::from_hms_opt(6, 15, 0));
        assert_eq!(parse_time("11pm"), NaiveTime::from_hms_opt(23, 0, 0));
        assert_eq!(parse_time("6am"), NaiveTime::from_hms_opt(6, 0, 0));
    }

    #[test]
    fn parse_12h_case_insensitive_and_spaces() {
        assert_eq!(parse_time("10:30PM"), NaiveTime::from_hms_opt(22, 30, 0));
        assert_eq!(parse_time("10:30 pm"), NaiveTime::from_hms_opt(22, 30, 0));
        assert_eq!(parse_time("10:30 PM"), NaiveTime::from_hms_opt(22, 30, 0));
    }

    #[test]
    fn parse_12h_midnight_and_noon() {
        assert_eq!(parse_time("12:00am"), NaiveTime::from_hms_opt(0, 0, 0));
        assert_eq!(parse_time("12:30am"), NaiveTime::from_hms_opt(0, 30, 0));
        assert_eq!(parse_time("12:00pm"), NaiveTime::from_hms_opt(12, 0, 0));
        assert_eq!(parse_time("12:30pm"), NaiveTime::from_hms_opt(12, 30, 0));
    }

    #[test]
    fn parse_invalid_returns_none() {
        assert!(parse_time("").is_none());
        assert!(parse_time("abc").is_none());
        assert!(parse_time("25:00").is_none());
        assert!(parse_time("12:60").is_none());
        assert!(parse_time("13pm").is_none()); // 13 is not a 12h hour
        assert!(parse_time("24:00").is_none());
        assert!(parse_time("-1:00").is_none());
    }

    #[test]
    fn parse_strips_quotes() {
        assert_eq!(parse_time("\"22:30\""), NaiveTime::from_hms_opt(22, 30, 0));
        assert_eq!(parse_time("'10:30pm'"), NaiveTime::from_hms_opt(22, 30, 0));
    }
}
