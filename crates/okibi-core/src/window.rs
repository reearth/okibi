//! The day a digest covers.

use crate::time;

/// One day, as both the interval a digest record names and the bounds a query
/// filters on.
///
/// What day it is, is not decided here. A window is told which day it covers
/// and everything downstream reads that field — which is what keeps the
/// planner's inputs free of a clock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    date: String,
    days: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct NotADate(pub String);

impl std::fmt::Display for NotADate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} is not a date", self.0)
    }
}

impl std::error::Error for NotADate {}

impl Window {
    /// From `YYYY-MM-DD`.
    pub fn parse(date: &str) -> Result<Self, NotADate> {
        let days = time::date_of(date).ok_or_else(|| NotADate(date.to_string()))?;
        if date.len() != 10 {
            return Err(NotADate(date.to_string()));
        }
        Ok(Window {
            date: date.to_string(),
            days,
        })
    }

    pub fn date(&self) -> &str {
        &self.date
    }

    /// The ISO 8601 interval a record carries, e.g. `2026-08-23/P1D`.
    pub fn interval(&self) -> String {
        format!("{}/P1D", self.date)
    }

    /// The half-open bounds to filter on, as a datetime is spelled in SQL.
    pub fn bounds(&self) -> (String, String) {
        (
            format!("{} 00:00:00", self.date),
            format!("{} 00:00:00", time::civil_from_days(self.days + 1)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_is_the_day_it_names() {
        let window = Window::parse("2026-08-23").unwrap();
        assert_eq!(window.interval(), "2026-08-23/P1D");
        assert_eq!(
            window.bounds(),
            ("2026-08-23 00:00:00".into(), "2026-08-24 00:00:00".into())
        );
    }

    #[test]
    fn a_window_at_the_end_of_a_month_ends_in_the_next_one() {
        assert_eq!(
            Window::parse("2026-08-31").unwrap().bounds().1,
            "2026-09-01 00:00:00"
        );
        assert_eq!(
            Window::parse("2026-12-31").unwrap().bounds().1,
            "2027-01-01 00:00:00"
        );
        assert_eq!(
            Window::parse("2024-02-28").unwrap().bounds().1,
            "2024-02-29 00:00:00"
        );
    }

    #[test]
    fn refuses_what_is_not_a_date() {
        assert!(Window::parse("yesterday").is_err());
        assert!(Window::parse("2026-13-01").is_err());
        assert!(Window::parse("2026-08-23T00:00:00Z").is_err());
    }
}
