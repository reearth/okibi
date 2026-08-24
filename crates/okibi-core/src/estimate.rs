//! What the plan costs, and what not warming costs instead.
//!
//! Computed with the plan and from the plan's own ordering, which is why there
//! is no separate "how much would this cost" pass anywhere in okibi: what it
//! costs is a property of what was decided.

use crate::{
    manifest::ServiceManifest,
    plan::{Estimate, Marginal, NoWarmCost, WarmCost},
    pricing::{PricingTable, unit},
};

/// One tile that is in the plan.
#[derive(Debug, Clone, Copy)]
pub struct Warmed {
    pub req: f64,
    pub gen_ms: f64,
    pub bytes: f64,
}

/// What a cell says about the tiles nobody named.
#[derive(Debug, Clone, Copy)]
pub struct CellCost {
    pub tiles_observed: u64,
    pub gen_ms: f64,
    pub p95_gen_ms: Option<f64>,
    pub bytes: f64,
}

pub struct Inputs<'a> {
    /// The entries the plan kept.
    pub kept: &'a [Warmed],
    /// Every candidate, including those cut, for the marginal curve.
    pub all: &'a [Warmed],
    pub cells: &'a [CellCost],
    pub manifest: &'a ServiceManifest,
    pub pricing: &'a PricingTable,
    pub pricing_ref: String,
}

/// How long a run of `count` tiles takes, in seconds.
///
/// Two limits, whichever binds: the origin will run `concurrency_limit` at
/// once, and it will accept `rate_per_s` starts a second. A plan of many
/// cheap tiles is bounded by the second, a plan of a few expensive ones by
/// the first.
pub fn wall_clock_s(manifest: &ServiceManifest, sum_gen_ms: f64, count: usize) -> f64 {
    let concurrency = manifest.cost.concurrency_limit.max(1) as f64;
    let by_work = sum_gen_ms / 1000.0 / concurrency;
    let by_rate = if manifest.cost.rate_per_s > 0.0 {
        count as f64 / manifest.cost.rate_per_s
    } else {
        0.0
    };
    by_work.max(by_rate)
}

/// What one generation costs, in the pricing table's currency.
pub fn cost_of(manifest: &ServiceManifest, pricing: &PricingTable, gen_ms: f64, bytes: f64) -> f64 {
    let Some(billing) = &manifest.cost.billing else {
        return 0.0;
    };
    if billing.pricing_profile != pricing.profile {
        // A table for another profile prices nothing here. Saying zero rather
        // than guessing keeps the estimate wrong in the direction someone will
        // notice.
        return 0.0;
    }

    // Summed in the map's own order, which is sorted, so the same manifest
    // gives the same floating-point total however it was written.
    billing
        .per_gen
        .iter()
        .map(|(resource, amount)| {
            amount_of(resource, *amount, gen_ms, bytes) * pricing.unit(resource)
        })
        .sum()
}

/// How much of one resource a generation spends.
///
/// A `null` amount says to measure it rather than to assume it, which only two
/// resources have a measurement for: a service that did not write down its CPU
/// per generation still spent the time the digest recorded, and one that did
/// not write down its egress still sent the bytes. Anything else left null is
/// counted as nothing — inventing a number for a resource nobody measured
/// would put it in the estimate as though it were known.
fn amount_of(resource: &str, amount: Option<f64>, gen_ms: f64, bytes: f64) -> f64 {
    match (amount, resource) {
        (Some(amount), _) => amount,
        (None, unit::CPU_MS) => gen_ms,
        (None, unit::EGRESS_BYTE) => bytes,
        (None, _) => 0.0,
    }
}

/// The four numbers, and the curve.
pub fn estimate(input: Inputs<'_>) -> Estimate {
    let warm = warm_cost(&input, input.kept);

    // What is not warmed is paid for by whoever asks first. Each cold tile is
    // generated once and then is not cold, so the count of tiles left out is
    // the count of people who wait.
    let observed: u64 = input.cells.iter().map(|cell| cell.tiles_observed).sum();
    let uncovered_tiles = observed.saturating_sub(input.kept.len() as u64);
    let user_wait_ms_total = weighted_wait(&input, uncovered_tiles);

    let p95 = input
        .cells
        .iter()
        .filter_map(|cell| cell.p95_gen_ms)
        .fold(f64::NAN, f64::max);

    Estimate {
        pricing: input.pricing_ref.clone(),
        warm,
        no_warm: NoWarmCost {
            affected_first_requests: uncovered_tiles as f64,
            user_wait_ms_total,
            p95_first_byte_ms: p95.is_finite().then_some(p95),
        },
        reclaimable: None,
        marginal: marginal(&input),
    }
}

fn warm_cost(input: &Inputs<'_>, tiles: &[Warmed]) -> WarmCost {
    let sum_gen_ms: f64 = tiles.iter().map(|t| t.gen_ms).sum();
    let usd: f64 = tiles
        .iter()
        .map(|t| cost_of(input.manifest, input.pricing, t.gen_ms, t.bytes))
        .sum();

    // What the plan will spend generating, which is the measured time unless
    // the service says otherwise.
    let cpu_ms = input
        .manifest
        .cost
        .billing
        .as_ref()
        .and_then(|billing| billing.per_gen.get(unit::CPU_MS).copied().flatten())
        .map(|per| per * tiles.len() as f64)
        .unwrap_or(sum_gen_ms);

    WarmCost {
        tiles: tiles.len(),
        wall_clock_s: wall_clock_s(input.manifest, sum_gen_ms, tiles.len()),
        cpu_ms: Some(cpu_ms),
        usd,
        storage_delta_bytes: Some(tiles.iter().map(|t| t.bytes).sum()),
    }
}

/// The waiting the uncovered tiles amount to, at the cells' own measured pace.
fn weighted_wait(input: &Inputs<'_>, uncovered_tiles: u64) -> f64 {
    let observed: u64 = input.cells.iter().map(|cell| cell.tiles_observed).sum();
    if observed == 0 || uncovered_tiles == 0 {
        return 0.0;
    }

    // Spread the uncovered tiles across the cells in proportion to how many
    // tiles each held, so that a slow cell's share is charged at its own pace
    // rather than at an average nobody experiences.
    let share = uncovered_tiles as f64 / observed as f64;
    input
        .cells
        .iter()
        .map(|cell| cell.tiles_observed as f64 * share * cell.gen_ms)
        .sum()
}

/// The cumulative curve: what each level of coverage would cost.
///
/// Power-law demand puts a visible knee in it, which is what makes a coverage
/// target something to read off rather than to guess at.
fn marginal(input: &Inputs<'_>) -> Vec<Marginal> {
    const LEVELS: [f64; 4] = [0.5, 0.8, 0.9, 0.95];

    let total: f64 = input.all.iter().map(|t| t.req).sum();
    if total <= 0.0 {
        return Vec::new();
    }

    let mut points = Vec::new();
    let mut level = LEVELS.iter().peekable();
    let (mut covered, mut gen_ms, mut usd) = (0.0, 0.0, 0.0);

    for (i, tile) in input.all.iter().enumerate() {
        covered += tile.req;
        gen_ms += tile.gen_ms;
        usd += cost_of(input.manifest, input.pricing, tile.gen_ms, tile.bytes);

        while let Some(&&target) = level.peek() {
            if covered / total < target {
                break;
            }
            points.push(Marginal {
                coverage: target,
                tiles: i + 1,
                usd,
                wall_clock_s: wall_clock_s(input.manifest, gen_ms, i + 1),
            });
            level.next();
        }
    }

    points
}
