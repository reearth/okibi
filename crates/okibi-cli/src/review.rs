//! Reading a plan rather than producing one.
//!
//! A plan is a derived artifact that gets stored and reviewed, which is only
//! worth anything if a person can ask it two questions: what changed since the
//! last one, and why is this URL where it is.

use std::collections::BTreeMap;

use okibi_core::{Entry, WarmPlan};

/// What moved between two plans.
#[derive(Debug, Default, PartialEq)]
pub struct Diff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    /// Same URL, different position: `(url, before, after)`.
    pub moved: Vec<(String, usize, usize)>,
    pub usd_before: f64,
    pub usd_after: f64,
    pub coverage_before: f64,
    pub coverage_after: f64,
}

pub fn diff(before: &WarmPlan, after: &WarmPlan) -> Diff {
    let index = |plan: &WarmPlan| -> BTreeMap<String, usize> {
        plan.entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.url.clone(), i))
            .collect()
    };
    let (old, new) = (index(before), index(after));

    Diff {
        added: new
            .keys()
            .filter(|u| !old.contains_key(*u))
            .cloned()
            .collect(),
        removed: old
            .keys()
            .filter(|u| !new.contains_key(*u))
            .cloned()
            .collect(),
        moved: old
            .iter()
            .filter_map(|(url, &was)| {
                let now = *new.get(url)?;
                (was != now).then(|| (url.clone(), was, now))
            })
            .collect(),
        usd_before: before.estimate.warm.usd,
        usd_after: after.estimate.warm.usd,
        coverage_before: before.stats.coverage_of_demand,
        coverage_after: after.stats.coverage_of_demand,
    }
}

/// Why one URL is where it is.
#[derive(Debug, PartialEq)]
pub struct Explanation<'a> {
    pub entry: &'a Entry,
    pub position: usize,
    pub of: usize,
    /// How much of the plan's demand this one entry accounts for.
    pub share_of_demand: f64,
}

pub fn explain<'a>(plan: &'a WarmPlan, url: &str) -> Option<Explanation<'a>> {
    let position = plan.entries.iter().position(|e| e.url == url)?;
    let entry = &plan.entries[position];

    let total: f64 = plan
        .entries
        .iter()
        .filter_map(|e| e.saved_req_estimate)
        .sum();
    let share = match (entry.saved_req_estimate, total) {
        (Some(req), total) if total > 0.0 => req / total,
        _ => 0.0,
    };

    Some(Explanation {
        entry,
        position,
        of: plan.entries.len(),
        share_of_demand: share,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use okibi_core::plan::{DerivedFrom, Estimate, Lane, NoWarmCost, Stats, WarmCost};

    fn entry(url: &str, req: f64) -> Entry {
        Entry {
            url: url.into(),
            service: "papers".into(),
            priority: 1.0,
            lane: Lane::Warm,
            not_before: None,
            expected_gen_ms: 30_000.0,
            saved_req_estimate: Some(req),
        }
    }

    fn plan(entries: Vec<Entry>, usd: f64, coverage: f64) -> WarmPlan {
        WarmPlan {
            plan: "okibi-warm-plan/1".into(),
            derived_from: DerivedFrom {
                digest: vec![],
                invalidation: "sha256:0".into(),
                manifests: Default::default(),
            },
            stats: Stats {
                total: entries.len(),
                sum_expected_gen_ms: 0.0,
                coverage_of_demand: coverage,
                unwarmable: 0,
                too_fast: 0,
            },
            entries,
            estimate: Estimate {
                pricing: "p@sha256:0".into(),
                warm: WarmCost {
                    tiles: 0,
                    wall_clock_s: 0.0,
                    cpu_ms: None,
                    usd,
                    storage_delta_bytes: None,
                },
                no_warm: NoWarmCost {
                    affected_first_requests: 0.0,
                    user_wait_ms_total: 0.0,
                    p95_first_byte_ms: None,
                },
                reclaimable: None,
                marginal: vec![],
            },
        }
    }

    #[test]
    fn a_diff_says_what_came_and_went_and_what_only_moved() {
        let before = plan(vec![entry("a", 10.0), entry("b", 5.0)], 1.0, 0.9);
        let after = plan(vec![entry("b", 5.0), entry("c", 1.0)], 2.0, 0.8);

        let diff = diff(&before, &after);
        assert_eq!(diff.added, ["c"]);
        assert_eq!(diff.removed, ["a"]);
        assert_eq!(diff.moved, [("b".to_string(), 1, 0)]);
        assert_eq!((diff.usd_before, diff.usd_after), (1.0, 2.0));
        assert_eq!((diff.coverage_before, diff.coverage_after), (0.9, 0.8));
    }

    #[test]
    fn two_runs_of_the_same_plan_differ_in_nothing() {
        let plan = plan(vec![entry("a", 10.0)], 1.0, 0.9);
        assert_eq!(
            diff(&plan, &plan),
            Diff {
                usd_before: 1.0,
                usd_after: 1.0,
                coverage_before: 0.9,
                coverage_after: 0.9,
                ..Default::default()
            }
        );
    }

    #[test]
    fn an_explanation_places_the_entry_in_the_plan() {
        let plan = plan(vec![entry("a", 30.0), entry("b", 10.0)], 1.0, 0.9);

        let explanation = explain(&plan, "b").unwrap();
        assert_eq!(explanation.position, 1);
        assert_eq!(explanation.of, 2);
        assert_eq!(explanation.share_of_demand, 0.25);

        assert!(explain(&plan, "nowhere").is_none());
    }
}
