//! `plan(digests, invalidation, manifests) -> warm_plan`. See `spec/planner.md`.
//!
//! A pure function. No network, no clock, no randomness: the deadline is a
//! field on the event, and how old a window is, is measured against the event
//! that names it. The same inputs give the same plan, byte for byte, because a
//! plan carries a cost estimate someone acts on and a review someone signed —
//! neither of which means anything if re-running produced something else.

use std::collections::BTreeMap;

use okibi_qk::Quadkey;

use crate::{
    digest::{DigestRecord, Kind},
    estimate,
    invalidation::InvalidationEvent,
    manifest::{Epoch, MetaUrl, ServiceManifest, ZoomSemantics},
    plan::{DerivedFrom, Entry, Lane, Stats, WarmPlan},
    pricing::PricingTable,
    time,
};

/// Everything `plan` reads.
pub struct PlanInput<'a> {
    pub digests: &'a [DigestRecord],
    pub invalidation: &'a InvalidationEvent,
    pub manifests: &'a [ServiceManifest],
    pub pricing: &'a PricingTable,
    /// The tileset's epochs *after* the change, for filling URL templates.
    ///
    /// The event says which axis moved and to what; a URL may need all three.
    /// Both come from the same `okibi.epochs.json`, so they cannot disagree.
    pub epoch: Epoch,
    /// What `derived_from` will say. Hashing is the caller's job — a pure
    /// function should not be choosing a digest algorithm.
    pub sources: Sources,
    pub options: PlanOptions,
}

#[derive(Debug, Clone, Default)]
pub struct Sources {
    pub digest: Vec<String>,
    pub invalidation: String,
    pub manifests: BTreeMap<String, String>,
    pub pricing: String,
}

#[derive(Debug, Clone)]
pub struct PlanOptions {
    /// How fast older evidence stops counting.
    pub half_life_days: f64,
    /// Stop once the plan would cost this much.
    pub budget_usd: Option<f64>,
    /// Stop once this much of the demand is covered.
    pub coverage: Option<f64>,
    /// Whether the event's deadline bounds the plan.
    pub honour_deadline: bool,
}

impl Default for PlanOptions {
    fn default() -> Self {
        PlanOptions {
            half_life_days: 7.0,
            budget_usd: None,
            coverage: None,
            honour_deadline: true,
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum PlanError {
    /// No manifest for the service that was invalidated.
    NoManifest { service: String },
    /// A timestamp that could not be read.
    BadTime { field: &'static str, value: String },
    /// A URL template asks for an epoch that was not supplied.
    ///
    /// Better to refuse than to fetch `…?e={epoch.param}` a few thousand
    /// times: every one of those is a real request to a real origin, and they
    /// would all miss, and the tiles they were meant to warm would still be
    /// cold at the end of it.
    EpochMissing {
        axis: &'static str,
        template: String,
    },
    /// A document with no coordinates, and no `meta_urls` entry for its kind.
    ///
    /// Refused for the same reason as a missing epoch. The tile template is
    /// built out of coordinates and a document of this kind has none, so
    /// filling it in produces a URL of the right shape for somewhere that
    /// does not exist — and the plan then reads as covering a document it
    /// would in fact spend a request 404ing on.
    NoMetaUrl { service: String, kind: &'static str },
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlanError::NoManifest { service } => {
                write!(f, "no manifest for service {service:?}")
            }
            PlanError::BadTime { field, value } => {
                write!(f, "{field} is not a timestamp: {value:?}")
            }
            PlanError::EpochMissing { axis, template } => write!(
                f,
                "{template:?} needs {{epoch.{axis}}} and no {axis} epoch was given"
            ),
            PlanError::NoMetaUrl { service, kind } => write!(
                f,
                "{service}'s digest has {kind} documents and its manifest has no \
                 meta_urls.{kind} to fetch them by"
            ),
        }
    }
}

impl std::error::Error for PlanError {}

/// A candidate before it becomes an entry.
#[derive(Debug, Clone)]
struct Candidate {
    service: String,
    /// Metadata sorts ahead of content whatever its score, so this is a key
    /// rather than a flag.
    rank: Rank,
    /// Where it is, for scope matching, ancestry and ordering. Metadata has
    /// none.
    qk: Option<Quadkey>,
    id: String,
    url: String,
    score: f64,
    req: f64,
    gen_ms: f64,
    bytes: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
    /// A cold root document is not one slow tile: it is every client's first
    /// paint, before any tile is asked for.
    Metadata,
    Content,
}

// `depends_on` has no rank here, because it has nothing to order yet: an
// invalidation event names one service, so every entry in a plan belongs to
// that service. The constraint is carried in the manifest and will need a
// rank the day a plan can span two services.

/// A measurement, or the manifest's guess where there is none.
///
/// Zero counts as none. A tile that took no time to make does not exist, and a
/// runtime can report one anyway: Workers freeze their clocks between I/O for
/// Spectre reasons, so a generator that is pure CPU measures as zero however
/// long it ran. Believing that would give the cell a cost of zero, a priority
/// of zero, and a plan that warms nothing — silently, since nothing about it
/// is an error.
fn measured_or(value: Option<f64>, fallback: f64) -> f64 {
    match value {
        Some(value) if value > 0.0 => value,
        _ => fallback,
    }
}

/// What a cell contributed, once its windows were combined.
#[derive(Debug, Default, Clone)]
struct Cell {
    freq: f64,
    /// The freshest window's measurements win: an old p50 describes an old
    /// generator.
    newest: i64,
    gen_ms: Option<f64>,
    bytes: Option<f64>,
    p95_gen_ms: Option<f64>,
    tiles_observed: u64,
    tiles: BTreeMap<(String, String), f64>,
}

/// Derive a plan.
pub fn plan(input: &PlanInput<'_>) -> Result<WarmPlan, PlanError> {
    let event = input.invalidation;
    let manifest = input
        .manifests
        .iter()
        .find(|m| m.service == event.service)
        .ok_or_else(|| PlanError::NoManifest {
            service: event.service.clone(),
        })?;

    // Before anything is planned, because the alternative is a plan full of
    // URLs with a placeholder still in them.
    check_epochs(manifest, &input.epoch)?;

    let cells = collect_cells(input, manifest);
    let demand_in_scope: f64 = cells.values().map(|cell| cell.freq).sum();

    let expanded = expand(input, manifest, &cells)?;
    let unwarmable = expanded.unwarmable;
    let too_fast = expanded.too_fast;
    let mut candidates = expanded.candidates;
    promote_ancestors(manifest, &mut candidates);
    sort(&mut candidates);

    let cut = cut_off(input, manifest, &candidates)?;
    let kept = &candidates[..cut];
    let lane = if cut < candidates.len() {
        // What survived a cut is what must happen, so it is what gets the
        // urgency. Promoting everything when nothing was cut would make the
        // lane meaningless.
        Lane::Urgent
    } else {
        Lane::Warm
    };

    // Content is normalised against the hottest content, not against whatever
    // happens to be first. Metadata is first by rank rather than by score —
    // its score is small, being a cheap document — and dividing by it would
    // push every tile above one and flatten them all to the same number.
    let top = kept
        .iter()
        .filter(|c| c.rank == Rank::Content)
        .map(|c| c.score)
        .fold(0.0, f64::max);

    let entries: Vec<Entry> = kept
        .iter()
        .map(|candidate| Entry {
            url: candidate.url.clone(),
            service: candidate.service.clone(),
            priority: match candidate.rank {
                // Unconditionally at the head, so unconditionally at the top
                // of the scale.
                Rank::Metadata => 1.0,
                Rank::Content => normalise(candidate.score, top),
            },
            lane,
            not_before: None,
            expected_gen_ms: candidate.gen_ms,
            saved_req_estimate: Some(candidate.req),
        })
        .collect();

    let covered: f64 = kept.iter().map(|c| c.req).sum();
    let stats = Stats {
        total: entries.len(),
        sum_expected_gen_ms: kept.iter().map(|c| c.gen_ms).sum(),
        coverage_of_demand: ratio(covered, demand_in_scope),
        unwarmable,
        too_fast,
    };

    let estimate = estimate::estimate(estimate::Inputs {
        kept: &kept
            .iter()
            .map(|c| estimate::Warmed {
                req: c.req,
                gen_ms: c.gen_ms,
                bytes: c.bytes,
            })
            .collect::<Vec<_>>(),
        all: &candidates
            .iter()
            .map(|c| estimate::Warmed {
                req: c.req,
                gen_ms: c.gen_ms,
                bytes: c.bytes,
            })
            .collect::<Vec<_>>(),
        cells: &cells
            .values()
            .map(|cell| estimate::CellCost {
                tiles_observed: cell.tiles_observed,
                gen_ms: measured_or(cell.gen_ms, manifest.cost.default_gen_ms),
                p95_gen_ms: cell.p95_gen_ms,
                bytes: measured_or(cell.bytes, manifest.cost.default_bytes),
            })
            .collect::<Vec<_>>(),
        manifest,
        pricing: input.pricing,
        pricing_ref: input.sources.pricing.clone(),
    });

    Ok(WarmPlan {
        plan: crate::plan::PLAN_VERSION.to_string(),
        derived_from: DerivedFrom {
            digest: input.sources.digest.clone(),
            invalidation: input.sources.invalidation.clone(),
            manifests: input.sources.manifests.clone(),
        },
        entries,
        stats,
        estimate,
    })
}

/// A template may only ask for epochs that were supplied.
///
/// Most services do not ask at all — their version lives in a cache key rather
/// than in a URL — and requiring epochs from them would be requiring a file
/// for something nothing reads. The ones that do ask have to be given them.
fn check_epochs(manifest: &ServiceManifest, epoch: &Epoch) -> Result<(), PlanError> {
    let axes = [
        ("source", epoch.source.as_str()),
        ("algo", epoch.algo.as_str()),
        ("param", epoch.param.as_str()),
    ];

    let named = manifest.meta_urls.values().flatten();
    for template in std::iter::once(&manifest.url_template).chain(named) {
        for (axis, value) in axes {
            if value.is_empty() && template.contains(&format!("{{epoch.{axis}}}")) {
                return Err(PlanError::EpochMissing {
                    axis,
                    template: template.clone(),
                });
            }
        }
    }
    Ok(())
}

/// Combine the windows of every cell the invalidation touched.
fn collect_cells(input: &PlanInput<'_>, manifest: &ServiceManifest) -> BTreeMap<CellKey, Cell> {
    let event = input.invalidation;
    let mut cells: BTreeMap<CellKey, Cell> = BTreeMap::new();

    for record in input.digests {
        if record.service != event.service || record.tileset != event.tileset {
            continue;
        }
        if record.kind.is_placed() && !event.scope.touches_cell(&record.qk8) {
            continue;
        }

        let age = time::age_in_days(&record.window, &event.occurred_at).unwrap_or(0.0);
        let weight = decay(age, input.options.half_life_days);
        let window = time::date_of(&record.window).unwrap_or(0);

        let cell = cells.entry((record.kind, record.qk8.clone())).or_default();
        cell.freq += record.req * weight;
        cell.tiles_observed = cell.tiles_observed.max(record.tiles_observed);

        if window >= cell.newest {
            cell.newest = window;
            cell.gen_ms = record.p50_gen_ms;
            cell.p95_gen_ms = record.p95_gen_ms;
            cell.bytes = record.avg_bytes;
        }

        for tile in &record.top_qk {
            if !event.scope.covers_tile(tile.qk(), tile.id()) {
                continue;
            }
            *cell
                .tiles
                .entry((tile.qk().to_string(), tile.id().to_string()))
                .or_default() += tile.req() * weight;
        }
        for entry in &record.top_id {
            // A document with no coordinates cannot be ruled out by a region:
            // it is not anywhere, and a tileset.json is as dead as the tiles
            // it describes whichever part of the world was invalidated. Only
            // a scope that names ids has anything to say about it.
            if let crate::invalidation::Scope::Ids { ids } = &event.scope {
                if !ids.iter().any(|named| named == entry.id()) {
                    continue;
                }
            }
            *cell
                .tiles
                .entry((String::new(), entry.id().to_string()))
                .or_default() += entry.req() * weight;
        }
    }

    let _ = manifest;
    cells
}

type CellKey = (Kind, String);

/// Turn cells into the tiles a plan can actually name.
///
/// Only tiles the digest observed become entries. A cell knows how many tiles
/// were seen in it but not which, and a tile nobody has ever requested is the
/// long tail — which is what on-demand generation is for.
fn expand(
    input: &PlanInput<'_>,
    manifest: &ServiceManifest,
    cells: &BTreeMap<CellKey, Cell>,
) -> Result<Expanded, PlanError> {
    let event = input.invalidation;
    let mut candidates = Vec::new();
    let mut unwarmable = 0usize;
    let mut too_fast = 0usize;

    for ((kind, _qk8), cell) in cells {
        let gen_ms = measured_or(cell.gen_ms, manifest.cost.default_gen_ms);
        let bytes = measured_or(cell.bytes, manifest.cost.default_bytes);

        // A tile nobody waits for is a tile warming cannot help, whatever the
        // budget. Only a measurement can say so: an unmeasured cell is
        // carrying the manifest's fallback, and excluding it because the
        // fallback is small would be excluding it for never having been seen.
        if below_floor(manifest, cell.gen_ms) {
            too_fast += cell.tiles.len();
            continue;
        }

        for ((qk, id), req) in &cell.tiles {
            let (rank, url) = if kind.is_placed() {
                (
                    Rank::Content,
                    manifest.url_for(&event.tileset, id, &input.epoch),
                )
            } else {
                let kind_name = kind_name(*kind);
                match manifest.meta_url_for(kind_name, &event.tileset, id, &input.epoch) {
                    MetaUrl::At(url) => (Rank::Metadata, url),
                    // Asked for, and nothing warming it would achieve. Counted
                    // rather than dropped: a plan silently smaller than the
                    // demand it was derived from reads as a quiet day.
                    MetaUrl::NotWarmable => {
                        unwarmable += 1;
                        continue;
                    }
                    MetaUrl::Unnamed => {
                        return Err(PlanError::NoMetaUrl {
                            service: event.service.clone(),
                            kind: kind_name,
                        });
                    }
                }
            };

            candidates.push(Candidate {
                service: event.service.clone(),
                rank,
                qk: qk.parse().ok().filter(|_| kind.is_placed()),
                id: id.clone(),
                url,
                // Frequency times cost, which is what lets one formula serve
                // services three orders of magnitude apart: a slow generator
                // pays for itself on cells nobody would call hot.
                score: req * gen_ms,
                req: *req,
                gen_ms,
                bytes,
            });
        }
    }

    Ok(Expanded {
        candidates,
        unwarmable,
        too_fast,
    })
}

/// What `expand` found, and what it left out on purpose.
struct Expanded {
    candidates: Vec<Candidate>,
    unwarmable: usize,
    too_fast: usize,
}

/// Whether a measured generation time is below what the service warms at all.
fn below_floor(manifest: &ServiceManifest, measured: Option<f64>) -> bool {
    match (manifest.cost.warm_above_gen_ms, measured) {
        (Some(floor), Some(gen_ms)) if gen_ms > 0.0 => gen_ms < floor,
        _ => false,
    }
}

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Content => "content",
        Kind::Tileset => "tileset",
        Kind::Subtree => "subtree",
        Kind::Meta => "meta",
    }
}

/// Where zoom means resolution, an ancestor is worth at least what its
/// descendants are worth, because warming it rescues all of them.
///
/// It is not given a bonus, only floored: with equal scores the ordering below
/// already puts a prefix before what it is a prefix of. Under `size_bucket` a
/// shallower tile is not a coarser view of the same ground, so none of this
/// applies and measured frequency stands.
fn promote_ancestors(manifest: &ServiceManifest, candidates: &mut [Candidate]) {
    if manifest.zoom_semantics != ZoomSemantics::Resolution {
        return;
    }

    let placed: Vec<(Quadkey, f64)> = candidates
        .iter()
        .filter_map(|c| c.qk.map(|qk| (qk, c.score)))
        .collect();

    for candidate in candidates.iter_mut() {
        let Some(qk) = candidate.qk else { continue };
        let floor = placed
            .iter()
            .filter(|(other, _)| other.starts_with(&qk) && *other != qk)
            .map(|(_, score)| *score)
            .fold(candidate.score, f64::max);
        candidate.score = floor;
    }
}

/// The total order entries are written in.
///
/// Metadata first, then by priority; ties broken by service, quadkey and id
/// so that nothing is left to the order the inputs happened to arrive in. A quadkey sorts before the quadkeys it is a
/// prefix of, which is what makes an ancestor precede its descendants once
/// their scores are equal.
fn sort(candidates: &mut [Candidate]) {
    candidates.sort_by(|a, b| {
        a.rank
            .cmp(&b.rank)
            .then_with(|| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.service.cmp(&b.service))
            .then_with(|| a.qk.cmp(&b.qk))
            .then_with(|| a.id.cmp(&b.id))
    });
}

/// How many entries survive the deadline, the budget and the coverage target.
///
/// Whatever is cut shows up as coverage below one rather than as silence.
fn cut_off(
    input: &PlanInput<'_>,
    manifest: &ServiceManifest,
    candidates: &[Candidate],
) -> Result<usize, PlanError> {
    let deadline_s = match (&input.invalidation.deadline, input.options.honour_deadline) {
        (Some(deadline), true) => {
            let at = time::timestamp_of(deadline).ok_or_else(|| PlanError::BadTime {
                field: "deadline",
                value: deadline.clone(),
            })?;
            let from = time::timestamp_of(&input.invalidation.occurred_at).ok_or_else(|| {
                PlanError::BadTime {
                    field: "occurred_at",
                    value: input.invalidation.occurred_at.clone(),
                }
            })?;
            Some((at - from).max(0) as f64)
        }
        _ => None,
    };

    let total_demand: f64 = candidates.iter().map(|c| c.req).sum();
    let mut gen_ms = 0.0;
    let mut usd = 0.0;
    let mut covered = 0.0;

    for (i, candidate) in candidates.iter().enumerate() {
        gen_ms += candidate.gen_ms;
        usd += estimate::cost_of(manifest, input.pricing, candidate.gen_ms, candidate.bytes);
        covered += candidate.req;

        let seconds = estimate::wall_clock_s(manifest, gen_ms, i + 1);
        if deadline_s.is_some_and(|limit| seconds > limit) {
            return Ok(i);
        }
        if input.options.budget_usd.is_some_and(|limit| usd > limit) {
            return Ok(i);
        }
        if input
            .options
            .coverage
            .is_some_and(|limit| ratio(covered, total_demand) >= limit)
        {
            return Ok(i + 1);
        }
    }

    Ok(candidates.len())
}

/// Older evidence counts for less, halving every `half_life` days.
fn decay(age_days: f64, half_life: f64) -> f64 {
    if half_life <= 0.0 {
        return 1.0;
    }
    0.5f64.powf(age_days / half_life)
}

fn normalise(score: f64, top: f64) -> f64 {
    if top <= 0.0 {
        0.0
    } else {
        (score / top).clamp(0.0, 1.0)
    }
}

fn ratio(part: f64, whole: f64) -> f64 {
    if whole <= 0.0 {
        0.0
    } else {
        (part / whole).clamp(0.0, 1.0)
    }
}
