//! Which day to aggregate, when nobody said.

use anyhow::{Context, Result};
use jiff::{Timestamp, ToSpan};
use okibi_core::Window;

/// The day before today, in UTC.
///
/// Reading a clock is the command line's business and not the planner's. Once
/// a day is chosen it travels as a [`Window`], and everything downstream reads
/// which day it covers rather than asking what day it is.
pub fn yesterday() -> Result<Window> {
    let today = Timestamp::now().to_zoned(jiff::tz::TimeZone::UTC).date();
    let yesterday = today - 1.day();
    Window::parse(&yesterday.to_string()).context("today, minus a day")
}

pub fn parse(date: &str) -> Result<Window> {
    Window::parse(date).with_context(|| format!("reading {date:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yesterday_was_a_day() {
        let window = yesterday().unwrap();
        assert_eq!(window.date().len(), 10);
        assert!(window.interval().ends_with("/P1D"));
    }

    #[test]
    fn refuses_what_is_not_a_date() {
        assert!(parse("yesterday").is_err());
    }
}
