//! The plan: what to fetch, in what order, at what cost.
//!
//! See `spec/okibi-contract.md` for the document and `spec/planner.md` for how
//! it is derived.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const PLAN_VERSION: &str = "okibi-warm-plan/1";

/// A warm plan. Ordinary JSON, so it can be stored, diffed and reviewed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WarmPlan {
    pub plan: String,
    pub derived_from: DerivedFrom,
    pub entries: Vec<Entry>,
    pub stats: Stats,
    pub estimate: Estimate,
}

/// Which inputs produced this plan.
///
/// A plan is a derived artifact rather than a claim: the originals are the
/// digest, the event and the manifests, and the same three give the same plan
/// back whenever anyone asks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivedFrom {
    pub digest: Vec<String>,
    pub invalidation: String,
    pub manifests: BTreeMap<String, String>,
}

/// One thing to fetch. An executor reads these and needs nothing else.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub url: String,
    pub service: String,
    pub priority: f64,
    pub lane: Lane,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_before: Option<String>,
    pub expected_gen_ms: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved_req_estimate: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lane {
    /// The default: spare capacity only, within the manifest's limits.
    Warm,
    /// Promoted when the deadline is tight. Still behind interactive traffic
    /// at the origin.
    Urgent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    pub total: usize,
    pub sum_expected_gen_ms: f64,
    /// Below 1 when a deadline, a budget, or demand nobody named cut the plan
    /// short.
    pub coverage_of_demand: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Estimate {
    /// The pricing table used, by path and hash.
    pub pricing: String,
    pub warm: WarmCost,
    /// The other side of the comparison: what interactive traffic pays if
    /// nothing is warmed.
    pub no_warm: NoWarmCost,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reclaimable: Option<Reclaimable>,
    /// The cumulative curve, which is where the decision of where to stop is
    /// actually made.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub marginal: Vec<Marginal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WarmCost {
    pub tiles: usize,
    pub wall_clock_s: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_ms: Option<f64>,
    pub usd: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_delta_bytes: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoWarmCost {
    pub affected_first_requests: f64,
    pub user_wait_ms_total: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p95_first_byte_ms: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reclaimable {
    pub prev_epoch_bytes: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Marginal {
    pub coverage: f64,
    pub tiles: usize,
    pub usd: f64,
    pub wall_clock_s: f64,
}
