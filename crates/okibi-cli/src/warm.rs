//! The reference executor: read a plan from the top and fetch it.
//!
//! It understands nothing about tiles. A warm plan is a list of URLs in an
//! order someone else decided, and warming is asking for them — the on-demand
//! path is the generator, so the request is the whole of the work.
//!
//! `jq -r '.entries[].url' plan.json | xargs -P 4 -n 1 curl -sf -o /dev/null`
//! is a conforming executor. What this adds is the manifest's limits, the
//! header that keeps warm requests out of the demand ledger, and a report of
//! what did not work.

use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result};
use okibi_core::{ServiceManifest, WarmPlan};

/// The header that tells a service this request is okibi's own.
///
/// Without it the service writes `origin: organic` and okibi's warming becomes
/// tomorrow's demand — a loop that ends with the plan describing the planner's
/// own history.
pub const WARM_HEADER: &str = "X-Okibi-Warm";

pub struct Limits {
    pub concurrency: usize,
    pub rate_per_s: f64,
    pub timeout: Duration,
    pub retries: usize,
}

impl Limits {
    /// What the service said it would tolerate.
    pub fn from_manifest(manifest: &ServiceManifest) -> Self {
        Limits {
            concurrency: manifest.cost.concurrency_limit.max(1) as usize,
            rate_per_s: manifest.cost.rate_per_s,
            timeout: Duration::from_secs(120),
            retries: 1,
        }
    }
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            concurrency: 4,
            rate_per_s: 2.0,
            timeout: Duration::from_secs(120),
            retries: 1,
        }
    }
}

#[derive(Debug, Default)]
pub struct Report {
    pub fetched: usize,
    pub failed: Vec<(String, String)>,
}

/// Fetch every entry, in order, within the limits.
pub async fn warm(
    plan: &WarmPlan,
    limits: &BTreeMap<String, Limits>,
    default: &Limits,
    dry_run: bool,
) -> Result<Report> {
    let mut report = Report::default();
    if plan.entries.is_empty() {
        return Ok(report);
    }

    if dry_run {
        for entry in &plan.entries {
            println!("{}", entry.url);
        }
        report.fetched = plan.entries.len();
        return Ok(report);
    }

    let client = reqwest::Client::builder()
        .user_agent(concat!("okibi/", env!("CARGO_PKG_VERSION")))
        .build()?;

    // Grouped by service, because the limits are the service's. Two services
    // in one plan are two origins, and neither should be held back by the
    // other's patience.
    let mut by_service: BTreeMap<&str, Vec<&okibi_core::Entry>> = BTreeMap::new();
    for entry in &plan.entries {
        by_service
            .entry(entry.service.as_str())
            .or_default()
            .push(entry);
    }

    for (service, entries) in by_service {
        let limits = limits.get(service).unwrap_or(default);
        let failures = warm_service(&client, &entries, limits).await?;

        report.fetched += entries.len() - failures.len();
        report.failed.extend(failures);
    }

    Ok(report)
}

async fn warm_service(
    client: &reqwest::Client,
    entries: &[&okibi_core::Entry],
    limits: &Limits,
) -> Result<Vec<(String, String)>> {
    let in_flight = Arc::new(tokio::sync::Semaphore::new(limits.concurrency));
    let started = Arc::new(AtomicUsize::new(0));
    let mut tasks = Vec::new();

    // A start is spaced from the one before it rather than from the clock, so
    // that the pacing survives an origin that is slower than expected.
    let gap = if limits.rate_per_s > 0.0 {
        Duration::from_secs_f64(1.0 / limits.rate_per_s)
    } else {
        Duration::ZERO
    };

    for entry in entries {
        let permit = in_flight.clone().acquire_owned().await?;
        let client = client.clone();
        let url = entry.url.clone();
        let timeout = limits.timeout;
        let retries = limits.retries;
        let nth = started.fetch_add(1, Ordering::SeqCst);

        if nth > 0 && !gap.is_zero() {
            tokio::time::sleep(gap).await;
        }

        tasks.push(tokio::spawn(async move {
            let _permit = permit;
            let result = fetch(&client, &url, timeout, retries).await;
            result.err().map(|error| (url, error.to_string()))
        }));
    }

    let mut failures = Vec::new();
    for task in tasks {
        if let Some(failure) = task.await.context("a warm task did not finish")? {
            failures.push(failure);
        }
    }
    Ok(failures)
}

async fn fetch(
    client: &reqwest::Client,
    url: &str,
    timeout: Duration,
    retries: usize,
) -> Result<()> {
    let mut attempt = 0;
    loop {
        let response = client
            .get(url)
            .header(WARM_HEADER, "1")
            .timeout(timeout)
            .send()
            .await;

        let outcome = match response {
            Ok(response) if response.status().is_success() => return Ok(()),
            Ok(response) => anyhow::anyhow!("{}", response.status()),
            Err(error) => anyhow::anyhow!("{error}"),
        };

        if attempt >= retries {
            return Err(outcome);
        }
        attempt += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use okibi_core::manifest::{Cost, ZoomSemantics};

    #[test]
    fn limits_are_the_services_own() {
        let manifest = ServiceManifest {
            manifest: "okibi-service/1".into(),
            service: "papers".into(),
            url_template: "https://example.test/{id}".into(),
            meta_urls: Default::default(),
            cost: Cost {
                default_gen_ms: 1.0,
                default_bytes: 1.0,
                concurrency_limit: 7,
                rate_per_s: 3.0,
                billing: None,
            },
            lanes: None,
            depends_on: vec![],
            zoom_semantics: ZoomSemantics::Resolution,
        };

        let limits = Limits::from_manifest(&manifest);
        assert_eq!(limits.concurrency, 7);
        assert_eq!(limits.rate_per_s, 3.0);
    }
}
