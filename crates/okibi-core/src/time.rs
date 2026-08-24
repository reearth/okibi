//! Reading the times the inputs carry.
//!
//! The planner never asks what time it is — a deadline arrives as a field, and
//! how old a window is, is measured against the invalidation that names it. So
//! all that is needed is to read what the documents say, which is a small
//! enough job to do here rather than to take a calendar library for.

/// Days since 1970-01-01 for a proleptic Gregorian date.
///
/// Howard Hinnant's `days_from_civil`, which is exact for every year this will
/// ever see and has no branches worth worrying about.
pub fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The date `days` after 1970-01-01, as `YYYY-MM-DD`.
///
/// Hinnant's `civil_from_days`, the inverse of the above. What it is for is
/// the day after a window: a window ends where the next one starts, and
/// "the next day" is a calendar question rather than an arithmetic one.
pub fn civil_from_days(days: i64) -> String {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    let year = year + i64::from(month <= 2);

    format!("{year:04}-{month:02}-{day:02}")
}

/// `YYYY-MM-DD` at the start of a string, as days since the epoch.
///
/// Takes the date off the front, so an ISO 8601 interval (`2026-08-23/P1D`) and
/// a timestamp (`2026-08-24T02:00:00Z`) both work. What follows the date does
/// not change which day it is.
pub fn date_of(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    if bytes.len() < 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year: i64 = text.get(0..4)?.parse().ok()?;
    let month: u32 = text.get(5..7)?.parse().ok()?;
    let day: u32 = text.get(8..10)?.parse().ok()?;

    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(days_from_civil(year, month, day))
}

/// An RFC 3339 timestamp as seconds since the epoch.
///
/// Accepts `Z` and numeric offsets. Fractional seconds are read and dropped:
/// nothing here is decided at a finer grain than a second.
pub fn timestamp_of(text: &str) -> Option<i64> {
    let days = date_of(text)?;
    let rest = text.get(10..)?;
    let rest = rest.strip_prefix(['T', 't', ' '])?;

    let hour: i64 = rest.get(0..2)?.parse().ok()?;
    let minute: i64 = rest.get(3..5)?.parse().ok()?;
    let second: i64 = rest.get(6..8).unwrap_or("00").parse().unwrap_or(0);
    if hour > 23 || minute > 59 || second > 60 {
        return None;
    }

    let offset = offset_of(rest)?;
    Some(days * 86_400 + hour * 3600 + minute * 60 + second - offset)
}

/// The zone offset in seconds, as the part after the time says it.
fn offset_of(time: &str) -> Option<i64> {
    if time.ends_with(['Z', 'z']) {
        return Some(0);
    }

    let sign_at = time.rfind(['+', '-'])?;
    let sign = time.get(sign_at..sign_at + 1)?;
    let offset = time.get(sign_at + 1..)?;
    let hours: i64 = offset.get(0..2)?.parse().ok()?;
    let minutes: i64 = offset.get(3..5).unwrap_or("00").parse().unwrap_or(0);
    let seconds = hours * 3600 + minutes * 60;

    Some(if sign == "-" { -seconds } else { seconds })
}

/// How many days older `window` is than `reference`, never negative.
///
/// A window from after the invalidation is not evidence from the future to be
/// discounted; it is as current as evidence gets.
pub fn age_in_days(window: &str, reference: &str) -> Option<f64> {
    let window = date_of(window)?;
    let reference = date_of(reference)?;
    Some(((reference - window).max(0)) as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_epoch_is_where_it_should_be() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(1969, 12, 31), -1);
        assert_eq!(days_from_civil(2026, 8, 24), 20689);
    }

    #[test]
    fn a_leap_day_is_a_day() {
        let feb = days_from_civil(2024, 2, 28);
        assert_eq!(days_from_civil(2024, 3, 1) - feb, 2);
        // 2100 is not a leap year, being a century that is not a fourth one.
        let feb = days_from_civil(2100, 2, 28);
        assert_eq!(days_from_civil(2100, 3, 1) - feb, 1);
    }

    /// Every day for a century, there and back again.
    #[test]
    fn a_day_survives_the_round_trip() {
        for days in 0..36_525 {
            let date = civil_from_days(days);
            assert_eq!(date_of(&date), Some(days), "{date}");
        }
    }

    #[test]
    fn the_day_after_a_month_is_in_the_next_one() {
        let last_of_august = days_from_civil(2026, 8, 31);
        assert_eq!(civil_from_days(last_of_august + 1), "2026-09-01");

        let leap = days_from_civil(2024, 2, 28);
        assert_eq!(civil_from_days(leap + 1), "2024-02-29");

        let not_leap = days_from_civil(2100, 2, 28);
        assert_eq!(civil_from_days(not_leap + 1), "2100-03-01");
    }

    #[test]
    fn a_date_is_read_off_the_front_of_whatever_carries_it() {
        let day = days_from_civil(2026, 8, 23);
        assert_eq!(date_of("2026-08-23"), Some(day));
        assert_eq!(date_of("2026-08-23/P1D"), Some(day));
        assert_eq!(date_of("2026-08-23T14:00:00Z"), Some(day));
    }

    #[test]
    fn refuses_what_is_not_a_date() {
        assert_eq!(date_of("yesterday"), None);
        assert_eq!(date_of("2026-13-01"), None);
        assert_eq!(date_of("2026-08"), None);
    }

    #[test]
    fn a_timestamp_is_seconds_from_the_epoch() {
        let midnight = days_from_civil(2026, 8, 24) * 86_400;
        assert_eq!(timestamp_of("2026-08-24T00:00:00Z"), Some(midnight));
        assert_eq!(timestamp_of("2026-08-24T02:00:00Z"), Some(midnight + 7200));
        assert_eq!(
            timestamp_of("2026-08-24T02:00:00.500Z"),
            Some(midnight + 7200)
        );
    }

    #[test]
    fn an_offset_moves_the_time_it_qualifies() {
        let utc = timestamp_of("2026-08-24T02:00:00Z").unwrap();
        assert_eq!(timestamp_of("2026-08-24T11:00:00+09:00"), Some(utc));
        assert_eq!(timestamp_of("2026-08-23T21:00:00-05:00"), Some(utc));
    }

    #[test]
    fn a_deadline_is_a_span_of_seconds_from_the_event() {
        let event = timestamp_of("2026-08-24T02:00:00Z").unwrap();
        let deadline = timestamp_of("2026-08-24T08:00:00Z").unwrap();
        assert_eq!(deadline - event, 6 * 3600);
    }

    #[test]
    fn evidence_from_after_the_event_is_not_discounted() {
        assert_eq!(
            age_in_days("2026-08-23/P1D", "2026-08-24T02:00:00Z"),
            Some(1.0)
        );
        assert_eq!(
            age_in_days("2026-08-24/P1D", "2026-08-24T02:00:00Z"),
            Some(0.0)
        );
        assert_eq!(
            age_in_days("2026-08-30/P1D", "2026-08-24T02:00:00Z"),
            Some(0.0)
        );
    }
}
