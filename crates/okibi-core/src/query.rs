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

use serde::{Deserialize, Serialize};

use crate::window::Window;

pub const QUERY_VERSION: &str = "okibi-digest-config/1";

/// What a digest run needs to know beyond credentials.
///
/// One run covers every service, because the dataset is one dataset indexed by
/// service. An empty `services` reads all of them, which is the usual case:
/// the dataset is already the list of who writes events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DigestQuery {
    #[serde(default = "default_dataset")]
    pub dataset: String,
    #[serde(default)]
    pub services: Vec<String>,
    #[serde(default = "default_top_n")]
    pub top_n: usize,
    #[serde(default = "default_top_rows")]
    pub top_rows: usize,
}

fn default_dataset() -> String {
    "tile_demand_1".to_string()
}

fn default_top_n() -> usize {
    20
}

fn default_top_rows() -> usize {
    10_000
}

impl Default for DigestQuery {
    fn default() -> Self {
        DigestQuery {
            dataset: default_dataset(),
            services: Vec::new(),
            top_n: default_top_n(),
            top_rows: default_top_rows(),
        }
    }
}

/// Single-quoted for SQL, with quotes doubled.
///
/// Service names come from a config file rather than from a request, but a
/// string interpolated into SQL is a string interpolated into SQL.
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn service_filter(config: &DigestQuery) -> String {
    if config.services.is_empty() {
        return String::new();
    }
    let list: Vec<String> = config.services.iter().map(|s| quote(s)).collect();
    format!("\n  AND index1 IN ({})", list.join(", "))
}

/// One row per `(service, tileset, kind, qk8)`: the cells themselves.
pub fn cells(config: &DigestQuery, window: &Window) -> String {
    let (from, to) = window.bounds();
    format!(
        "SELECT
  blob1 AS service,
  blob2 AS tileset,
  blob3 AS kind,
  blob6 AS qk8,
  SUM(IF(blob13 = 'organic', double1 * _sample_interval, 0.0)) AS req,
  SUM(IF(blob13 = 'organic' AND blob7 = 'miss', double1 * _sample_interval, 0.0)) AS miss,
  quantileWeighted(0.5)(double2, IF(blob7 = 'miss', _sample_interval, 0)) AS p50_gen_ms,
  quantileWeighted(0.95)(double2, IF(blob7 = 'miss', _sample_interval, 0)) AS p95_gen_ms,
  SUM(double2 * _sample_interval) AS sum_gen_ms,
  SUM(double4 * _sample_interval) AS bytes,
  SUM(double4 * _sample_interval) / SUM(double1 * _sample_interval) AS avg_bytes,
  COUNT(DISTINCT blob4) AS tiles_observed,
  MAX(_sample_interval) AS sample_interval_max
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
/// portable SQL: neither `LIMIT n BY` nor a window function is available to
/// take a top slice per cell. What that costs is that the coldest cells may
/// run out of rows before they are described — which the caller reports
/// rather than hides.
///
/// The limit is spent per service rather than across all of them, which is
/// why this takes one. A single query ordered by demand gives every row to
/// the busiest service, and the services that most need warming are the slow
/// ones, not the busy ones — the busiest service would be the only one
/// planned, and no error would say so.
pub fn top_tiles(config: &DigestQuery, window: &Window, service: &str) -> String {
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
  AND blob13 = 'organic'
  AND index1 = {service}
GROUP BY service, tileset, kind, qk8, qk, id
ORDER BY req DESC
LIMIT {rows}
FORMAT JSON",
        dataset = config.dataset,
        from = quote(&from),
        to = quote(&to),
        service = quote(service),
        rows = config.top_rows,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> DigestQuery {
        DigestQuery {
            services: vec!["papers".into(), "terrain".into()],
            ..DigestQuery::default()
        }
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
        assert!(top_tiles(&config(), &window(), "papers").contains("blob13 = 'organic'"));
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

    /// Not a frequency, so not weighted: this is the weight itself, and
    /// multiplying it by anything would be asking how heavily the sampling
    /// was sampled.
    #[test]
    fn asks_how_hard_the_rows_were_sampled() {
        let sql = cells(&config(), &window());
        assert!(
            sql.contains("MAX(_sample_interval) AS sample_interval_max"),
            "{sql}"
        );
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

    /// The limit is a row count, and one query ordered by demand gives every
    /// row to the busiest service. Warming is for the slow services, not the
    /// busy ones, so spending the limit per service is what makes them
    /// plannable at all — and nothing errors when it is spent wrongly.
    #[test]
    fn a_top_tiles_query_is_for_one_service() {
        let sql = top_tiles(&config(), &window(), "papers");
        assert!(sql.contains("AND index1 = 'papers'"), "{sql}");
        assert!(!sql.contains("index1 IN"), "{sql}");
    }

    /// A hit generated nothing. Weighting hits at zero is how they leave the
    /// quantile, there being no conditional aggregate to say it with: a cell
    /// that mostly hits would otherwise report a median generation of zero,
    /// which reads as free rather than as unmeasured.
    #[test]
    fn generation_quantiles_are_taken_over_misses_only() {
        let sql = cells(&config(), &window());
        for line in sql.lines().filter(|line| line.contains("quantileWeighted")) {
            assert!(
                line.contains("IF(blob7 = 'miss', _sample_interval, 0)"),
                "{line}"
            );
        }
    }

    #[test]
    fn a_name_cannot_close_the_quote_it_is_in() {
        let mut config = config();
        config.services = vec!["it's".to_string()];
        assert!(cells(&config, &window()).contains("index1 IN ('it''s')"));
    }
}
