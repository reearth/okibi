//! The two queries a digest is made of.
//!
//! Both are built here as text and run elsewhere, so that what gets sent can
//! be read, tested and printed with `--print-sql` before anyone points it at a
//! real dataset.
//!
//! Two rules from `spec/bindings/wae-1.md` are structural here rather than
//! remembered: every frequency is multiplied by `_sample_interval`, and demand
//! counts organic requests only. A bare `count()` is not an approximation of
//! the truth, it is an arbitrary number, and warm requests counted as demand
//! are a feedback loop.

use crate::{config::Config, window::Window};

/// Single-quoted for SQL, with quotes doubled.
///
/// Service names come from a config file rather than from a request, but a
/// string interpolated into SQL is a string interpolated into SQL.
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn service_filter(config: &Config) -> String {
    if config.services.is_empty() {
        return String::new();
    }
    let list: Vec<String> = config.services.iter().map(|s| quote(s)).collect();
    format!("\n  AND index1 IN ({})", list.join(", "))
}

/// One row per `(service, tileset, kind, qk8)`: the cells themselves.
pub fn cells(config: &Config, window: &Window) -> String {
    let (from, to) = window.bounds();
    format!(
        "SELECT
  blob1 AS service,
  blob2 AS tileset,
  blob3 AS kind,
  blob6 AS qk8,
  SUM(IF(blob13 = 'organic', double1 * _sample_interval, 0)) AS req,
  SUM(IF(blob13 = 'organic' AND blob7 = 'miss', double1 * _sample_interval, 0)) AS miss,
  quantileWeighted(0.5)(double2, _sample_interval) AS p50_gen_ms,
  quantileWeighted(0.95)(double2, _sample_interval) AS p95_gen_ms,
  SUM(double2 * _sample_interval) AS sum_gen_ms,
  SUM(double4 * _sample_interval) AS bytes,
  SUM(double4 * _sample_interval) / SUM(double1 * _sample_interval) AS avg_bytes,
  COUNT(DISTINCT blob4) AS tiles_observed
FROM {dataset}
WHERE timestamp >= toDateTime({from})
  AND timestamp < toDateTime({to}){services}
GROUP BY service, tileset, kind, qk8
FORMAT JSON",
        dataset = config.dataset,
        from = quote(&from),
        to = quote(&to),
        services = service_filter(config),
    )
}

/// One row per tile, hottest first, for the `top_qk` and `top_id` lists.
///
/// Cells are rolled up from these rather than the other way round, and the
/// limit is a plain one rather than a per-cell one so that the query stays
/// portable SQL. What that costs is that the coldest cells may run out of rows
/// before they are described — which the caller reports rather than hides.
pub fn top_tiles(config: &Config, window: &Window) -> String {
    let (from, to) = window.bounds();
    format!(
        "SELECT
  blob1 AS service,
  blob2 AS tileset,
  blob3 AS kind,
  blob6 AS qk8,
  blob5 AS qk,
  blob4 AS id,
  SUM(double1 * _sample_interval) AS req
FROM {dataset}
WHERE timestamp >= toDateTime({from})
  AND timestamp < toDateTime({to})
  AND blob13 = 'organic'{services}
GROUP BY service, tileset, kind, qk8, qk, id
ORDER BY req DESC
LIMIT {rows}
FORMAT JSON",
        dataset = config.dataset,
        from = quote(&from),
        to = quote(&to),
        services = service_filter(config),
        rows = config.top_rows,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        serde_json::from_str(
            r#"{"config": "okibi-digest-config/1", "account_id": "acc",
                "services": ["papers", "terrain"]}"#,
        )
        .unwrap()
    }

    fn window() -> Window {
        Window::parse("2026-08-23").unwrap()
    }

    #[test]
    fn every_frequency_carries_the_sampling_weight() {
        let sql = cells(&config(), &window());
        for line in sql.lines() {
            if line.contains("double1") || line.contains("double4") {
                assert!(
                    line.contains("_sample_interval"),
                    "unweighted count: {line}"
                );
            }
        }
    }

    #[test]
    fn demand_counts_organic_requests_only() {
        assert!(cells(&config(), &window()).contains("IF(blob13 = 'organic'"));
        assert!(top_tiles(&config(), &window()).contains("blob13 = 'organic'"));
    }

    /// Generation time is a property of generating, not of who asked, so warm
    /// requests are good samples of it and are not filtered out.
    #[test]
    fn generation_time_is_measured_over_every_request() {
        let sql = cells(&config(), &window());
        let gen_lines: Vec<&str> = sql
            .lines()
            .filter(|line| line.contains("double2"))
            .collect();

        assert!(!gen_lines.is_empty());
        for line in gen_lines {
            assert!(!line.contains("organic"), "{line}");
        }
    }

    #[test]
    fn reads_one_day_and_no_more() {
        let sql = cells(&config(), &window());
        assert!(sql.contains("timestamp >= toDateTime('2026-08-23 00:00:00')"));
        assert!(sql.contains("timestamp < toDateTime('2026-08-24 00:00:00')"));
    }

    #[test]
    fn filters_on_the_index_so_the_filter_is_the_fast_one() {
        assert!(cells(&config(), &window()).contains("index1 IN ('papers', 'terrain')"));
    }

    #[test]
    fn reads_every_service_when_none_are_named() {
        let mut config = config();
        config.services.clear();
        assert!(!cells(&config, &window()).contains("index1"));
    }

    #[test]
    fn a_name_cannot_close_the_quote_it_is_in() {
        let mut config = config();
        config.services = vec!["it's".to_string()];
        assert!(cells(&config, &window()).contains("index1 IN ('it''s')"));
    }
}
