//! The day a digest covers.

use anyhow::{Context, Result};
use jiff::{Timestamp, ToSpan, civil::Date};

/// One day, as both the interval a digest record names and the bounds a query
/// filters on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub date: Date,
}

impl Window {
    pub fn new(date: Date) -> Self {
        Window { date }
    }

    /// The day before today, in UTC.
    ///
    /// Reading a clock is the CLI's business and not the planner's: a digest
    /// says which day it covers, and everything downstream reads that field
    /// rather than asking what day it is.
    pub fn yesterday() -> Self {
        let today = Timestamp::now().to_zoned(jiff::tz::TimeZone::UTC).date();
        Window::new(today - 1.day())
    }

    pub fn parse(date: &str) -> Result<Self> {
        Ok(Window::new(
            date.parse::<Date>()
                .with_context(|| format!("{date} is not a date"))?,
        ))
    }

    /// The ISO 8601 interval a record carries, e.g. `2026-08-23/P1D`.
    pub fn interval(&self) -> String {
        format!("{}/P1D", self.date)
    }

    /// The half-open bounds to filter on, as the SQL API spells datetimes.
    pub fn bounds(&self) -> (String, String) {
        (
            format!("{} 00:00:00", self.date),
            format!("{} 00:00:00", self.date + 1.day()),
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
        let window = Window::parse("2026-08-31").unwrap();
        assert_eq!(window.bounds().1, "2026-09-01 00:00:00");
    }

    #[test]
    fn refuses_what_is_not_a_date() {
        assert!(Window::parse("yesterday").is_err());
        assert!(Window::parse("2026-13-01").is_err());
    }
}
