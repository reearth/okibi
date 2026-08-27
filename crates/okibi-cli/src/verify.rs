//! Asking whether a plan's URLs are URLs.
//!
//! The planner is a pure function and cannot check this: it fills a template
//! from a manifest and has no way to know the template was right. When it is
//! wrong the plan looks exactly like a correct one — the entries are ordered,
//! the coverage is a number, the cost is a number — and every request in it
//! answers 404. Warming the wrong thing looks the same as warming the right
//! thing, right up until nothing got faster.
//!
//! So the check lives here, outside the pure part, and it is the cheapest
//! possible one: take a handful of the URLs the plan actually contains and ask
//! the origin whether they exist.

use std::{collections::BTreeMap, time::Duration};

use anyhow::Result;
use okibi_core::WarmPlan;

use crate::warm::WARM_HEADER;

/// What one sampled URL answered.
#[derive(Debug, Clone)]
pub struct Checked {
    pub url: String,
    pub service: String,
    /// The position in the plan, because the first entries are the ones a
    /// plan puts first for a reason.
    pub rank: usize,
    pub status: Option<u16>,
    pub error: Option<String>,
}

impl Checked {
    /// Whether this is a URL that cannot exist, rather than an origin having
    /// a bad minute.
    ///
    /// A 4xx is the plan's fault: the template, the id, or the epoch produced
    /// somewhere that is not there. A 5xx or a timeout is the origin's, and an
    /// origin is allowed to have a bad minute without a pull request failing
    /// over it.
    pub fn is_wrong(&self) -> bool {
        matches!(self.status, Some(status) if (400..500).contains(&status))
    }

    pub fn ok(&self) -> bool {
        matches!(self.status, Some(status) if status < 400)
    }
}

/// Which entries to ask about.
///
/// The first of each service always, because a plan puts metadata first and a
/// metadata URL is built from a different template than the tiles — the one
/// place a plan can be half right. The rest are spread evenly rather than
/// taken from the top: the top of a plan is its hottest cell, and a template
/// that happens to work there can still be wrong three zoom levels down.
fn sample(plan: &WarmPlan, size: usize) -> Vec<(usize, &okibi_core::Entry)> {
    let mut by_service: BTreeMap<&str, Vec<(usize, &okibi_core::Entry)>> = BTreeMap::new();
    for (rank, entry) in plan.entries.iter().enumerate() {
        by_service
            .entry(entry.service.as_str())
            .or_default()
            .push((rank, entry));
    }

    let mut picked = Vec::new();
    for (_, entries) in by_service {
        let want = size.max(1).min(entries.len());
        // A stride rather than the first `want`, so the sample reaches the
        // tail of the plan as well as its head.
        let stride = entries.len().div_ceil(want);
        for chunk in entries.iter().step_by(stride.max(1)).take(want) {
            picked.push(*chunk);
        }
    }
    picked.sort_by_key(|(rank, _)| *rank);
    picked
}

/// Ask the origin about a sample of the plan.
///
/// `HEAD` where the service allows it, because verifying should not pay for a
/// generation — and falling back to `GET` where it does not, because a service
/// that answers 405 has told us nothing about whether the URL exists.
///
/// The warm header goes on either way. A verification counted as demand is
/// demand okibi invented, which is the same feedback loop warming has.
pub async fn verify(plan: &WarmPlan, size: usize, secret: Option<&str>) -> Result<Vec<Checked>> {
    let picked = sample(plan, size);
    if picked.is_empty() {
        return Ok(Vec::new());
    }

    let client = reqwest::Client::builder()
        .user_agent(concat!("okibi/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(60))
        .build()?;

    let mut checked = Vec::new();
    for (rank, entry) in picked {
        let (status, error) = ask(&client, &entry.url, secret).await;
        checked.push(Checked {
            url: entry.url.clone(),
            service: entry.service.clone(),
            rank,
            status,
            error,
        });
    }
    Ok(checked)
}

async fn ask(
    client: &reqwest::Client,
    url: &str,
    secret: Option<&str>,
) -> (Option<u16>, Option<String>) {
    match send(client, url, secret, true).await {
        // 405 is the service saying it does not do HEAD, which is not an
        // answer about the URL. Ask again the way a warm request would.
        Ok(405) => match send(client, url, secret, false).await {
            Ok(status) => (Some(status), None),
            Err(error) => (None, Some(error.to_string())),
        },
        Ok(status) => (Some(status), None),
        Err(error) => (None, Some(error.to_string())),
    }
}

async fn send(
    client: &reqwest::Client,
    url: &str,
    secret: Option<&str>,
    head: bool,
) -> Result<u16, reqwest::Error> {
    let mut request = if head {
        client.head(url)
    } else {
        client.get(url)
    };
    if let Some(secret) = secret {
        request = request.header(WARM_HEADER, secret);
    }
    Ok(request.send().await?.status().as_u16())
}

#[cfg(test)]
mod tests {
    use super::*;
    use okibi_core::{Entry, Lane};

    fn entry(url: &str, service: &str) -> Entry {
        Entry {
            url: url.to_string(),
            service: service.to_string(),
            priority: 1.0,
            lane: Lane::Warm,
            not_before: None,
            expected_gen_ms: 1.0,
            saved_req_estimate: None,
        }
    }

    fn plan(entries: Vec<Entry>) -> WarmPlan {
        let mut plan: WarmPlan = serde_json::from_str(
            r#"{"plan":"okibi-warm-plan/1",
                "derived_from":{"digest":[],"invalidation":"sha256:0","manifests":{}},
                "stats":{"total":0,"sum_expected_gen_ms":0,"coverage_of_demand":1},
                "entries":[],
                "estimate":{"pricing":"p",
                  "warm":{"tiles":0,"wall_clock_s":0,"cpu_ms":0,"usd":0,
                          "storage_delta_bytes":0},
                  "no_warm":{"affected_first_requests":0,"user_wait_ms_total":0,
                             "p95_first_byte_ms":0}}}"#,
        )
        .unwrap();
        plan.entries = entries;
        plan
    }

    /// The head of a plan is its hottest cell. A template that works there can
    /// still be wrong further down, which is the whole reason for looking.
    #[test]
    fn the_sample_reaches_past_the_top_of_the_plan() {
        let entries: Vec<Entry> = (0..100)
            .map(|i| entry(&format!("https://a.test/{i}"), "papers"))
            .collect();
        let plan = plan(entries);

        let ranks: Vec<usize> = sample(&plan, 4).iter().map(|(rank, _)| *rank).collect();

        assert_eq!(ranks.len(), 4);
        assert_eq!(ranks[0], 0, "the first entry, which is the metadata one");
        assert!(ranks[3] > 50, "and something from the tail: {ranks:?}");
    }

    /// Two services in one plan are two templates, and one of them can be
    /// wrong on its own.
    #[test]
    fn every_service_is_asked_about() {
        let plan = plan(vec![
            entry("https://a.test/1", "papers"),
            entry("https://a.test/2", "papers"),
            entry("https://b.test/1", "terrain"),
        ]);

        let services: Vec<&str> = sample(&plan, 1)
            .iter()
            .map(|(_, entry)| entry.service.as_str())
            .collect();

        assert!(services.contains(&"papers"), "{services:?}");
        assert!(services.contains(&"terrain"), "{services:?}");
    }

    #[test]
    fn asks_for_no_more_than_there_is() {
        let plan = plan(vec![entry("https://a.test/1", "papers")]);
        assert_eq!(sample(&plan, 20).len(), 1);
    }

    /// A 404 is the plan being wrong. A 503 is an origin having a bad minute,
    /// and an origin is allowed one without failing a pull request.
    #[test]
    fn a_missing_url_is_the_plans_fault_and_a_broken_origin_is_not() {
        let checked = |status: u16| Checked {
            url: "https://a.test/1".into(),
            service: "papers".into(),
            rank: 0,
            status: Some(status),
            error: None,
        };

        assert!(checked(404).is_wrong());
        assert!(checked(410).is_wrong());
        assert!(!checked(503).is_wrong());
        assert!(!checked(200).is_wrong());
        assert!(checked(200).ok());
        assert!(!checked(404).ok());
    }
}
