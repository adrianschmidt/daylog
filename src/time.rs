use chrono::NaiveTime;
use crate::config::TimeFormat;

/// Format a `NaiveTime` per the given config format.
///
/// 24h: zero-padded `HH:MM` (`"06:05"`, `"22:30"`).
/// 12h: lowercase suffix, no zero-padding on hour (`"6:05am"`, `"10:30pm"`,
///      `"12:30am"` for midnight, `"12:00pm"` for noon). Always includes
///      minutes for clarity in stored entries.
pub fn format_time(t: NaiveTime, fmt: TimeFormat) -> String {
    use chrono::Timelike;
    let h = t.hour();
    let m = t.minute();
    match fmt {
        TimeFormat::TwentyFourHour => format!("{h:02}:{m:02}"),
        TimeFormat::TwelveHour => {
            let (display_h, suffix) = match h {
                0 => (12, "am"),
                1..=11 => (h, "am"),
                12 => (12, "pm"),
                _ => (h - 12, "pm"),
            };
            format!("{display_h}:{m:02}{suffix}")
        }
    }
}

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

/// Parse a `start-end` sleep range. Surrounding quotes and whitespace
/// around the dash are tolerated.
pub fn parse_sleep_range(s: &str) -> Option<(NaiveTime, NaiveTime)> {
    let s = s.trim().trim_matches('"').trim_matches('\'');
    let (start, end) = s.split_once('-')?;
    let start = parse_time(start)?;
    let end = parse_time(end)?;
    Some((start, end))
}

/// Format a sleep range as `"start-end"` per the given config format.
pub fn format_sleep_range(start: NaiveTime, end: NaiveTime, fmt: TimeFormat) -> String {
    format!("{}-{}", format_time(start, fmt), format_time(end, fmt))
}

/// Compute hours between two times. If end <= start, treats it as
/// crossing midnight (adds 24h). Equal times return 0.0.
/// Result is rounded to 2 decimal places.
pub fn sleep_hours(start: NaiveTime, end: NaiveTime) -> f64 {
    use chrono::Timelike;
    let start_min = (start.hour() * 60 + start.minute()) as i32;
    let end_min = (end.hour() * 60 + end.minute()) as i32;
    let duration = if end_min == start_min {
        0
    } else if end_min < start_min {
        (1440 - start_min) + end_min
    } else {
        end_min - start_min
    };
    let hours = duration as f64 / 60.0;
    (hours * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TimeFormat;

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

    #[test]
    fn format_24h_zero_padded() {
        let t = NaiveTime::from_hms_opt(22, 30, 0).unwrap();
        assert_eq!(format_time(t, TimeFormat::TwentyFourHour), "22:30");
        let t = NaiveTime::from_hms_opt(6, 5, 0).unwrap();
        assert_eq!(format_time(t, TimeFormat::TwentyFourHour), "06:05");
        let t = NaiveTime::from_hms_opt(0, 0, 0).unwrap();
        assert_eq!(format_time(t, TimeFormat::TwentyFourHour), "00:00");
    }

    #[test]
    fn format_12h_lowercase_no_hour_pad() {
        let t = NaiveTime::from_hms_opt(22, 30, 0).unwrap();
        assert_eq!(format_time(t, TimeFormat::TwelveHour), "10:30pm");
        let t = NaiveTime::from_hms_opt(6, 15, 0).unwrap();
        assert_eq!(format_time(t, TimeFormat::TwelveHour), "6:15am");
        let t = NaiveTime::from_hms_opt(0, 30, 0).unwrap();
        assert_eq!(format_time(t, TimeFormat::TwelveHour), "12:30am");
        let t = NaiveTime::from_hms_opt(12, 0, 0).unwrap();
        assert_eq!(format_time(t, TimeFormat::TwelveHour), "12:00pm");
    }

    #[test]
    fn format_then_parse_roundtrip() {
        for hour in 0..24 {
            for min in [0, 1, 30, 59] {
                let t = NaiveTime::from_hms_opt(hour, min, 0).unwrap();
                for fmt in [TimeFormat::TwelveHour, TimeFormat::TwentyFourHour] {
                    let s = format_time(t, fmt);
                    assert_eq!(parse_time(&s), Some(t), "roundtrip failed for {s}");
                }
            }
        }
    }

    #[test]
    fn parse_sleep_range_12h() {
        let (s, e) = parse_sleep_range("10:30pm-6:15am").unwrap();
        assert_eq!(s, NaiveTime::from_hms_opt(22, 30, 0).unwrap());
        assert_eq!(e, NaiveTime::from_hms_opt(6, 15, 0).unwrap());
    }

    #[test]
    fn parse_sleep_range_24h() {
        let (s, e) = parse_sleep_range("22:30-06:15").unwrap();
        assert_eq!(s, NaiveTime::from_hms_opt(22, 30, 0).unwrap());
        assert_eq!(e, NaiveTime::from_hms_opt(6, 15, 0).unwrap());
    }

    #[test]
    fn parse_sleep_range_quoted_and_spaces() {
        let (s, e) = parse_sleep_range("\"10:30pm - 6:15am\"").unwrap();
        assert_eq!(s, NaiveTime::from_hms_opt(22, 30, 0).unwrap());
        assert_eq!(e, NaiveTime::from_hms_opt(6, 15, 0).unwrap());
    }

    #[test]
    fn parse_sleep_range_no_dash() {
        assert!(parse_sleep_range("22:30").is_none());
    }

    #[test]
    fn parse_sleep_range_garbage() {
        assert!(parse_sleep_range("foo-bar").is_none());
        assert!(parse_sleep_range("").is_none());
    }

    #[test]
    fn format_sleep_range_uses_format() {
        let s = NaiveTime::from_hms_opt(22, 30, 0).unwrap();
        let e = NaiveTime::from_hms_opt(6, 15, 0).unwrap();
        assert_eq!(
            format_sleep_range(s, e, TimeFormat::TwelveHour),
            "10:30pm-6:15am"
        );
        assert_eq!(
            format_sleep_range(s, e, TimeFormat::TwentyFourHour),
            "22:30-06:15"
        );
    }

    #[test]
    fn sleep_hours_overnight() {
        let s = NaiveTime::from_hms_opt(22, 30, 0).unwrap();
        let e = NaiveTime::from_hms_opt(6, 15, 0).unwrap();
        assert!((sleep_hours(s, e) - 7.75).abs() < 0.01);
    }

    #[test]
    fn sleep_hours_same_day() {
        let s = NaiveTime::from_hms_opt(0, 28, 0).unwrap();
        let e = NaiveTime::from_hms_opt(6, 52, 0).unwrap();
        assert!((sleep_hours(s, e) - 6.4).abs() < 0.01);
    }

    #[test]
    fn sleep_hours_equal_returns_zero() {
        let s = NaiveTime::from_hms_opt(7, 0, 0).unwrap();
        assert_eq!(sleep_hours(s, s), 0.0);
    }
}
