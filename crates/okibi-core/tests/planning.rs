//! What the planner decides, and why.

use okibi_core::{
    digest::{DigestRecord, Kind, TopEntry, TopTile},
    invalidation::{Axis, InvalidationEvent, Scope},
    manifest::{Billing, Cost, Epoch, ServiceManifest, ZoomSemantics},
    plan::Lane,
    planner::{PlanInput, PlanOptions, Sources},
    pricing::PricingTable,
};

const PAPERS_URL: &str = "https://papers.reearth.land/t/{tileset}/{id}?e={epoch.param}";

fn manifest(zoom: ZoomSemantics) -> ServiceManifest {
    ServiceManifest {
        manifest: "okibi-service/1".into(),
        service: "papers".into(),
        url_template: PAPERS_URL.into(),
        meta_urls: [(
            "tileset".to_string(),
            Some("https://papers.reearth.land/t/{tileset}/meta.json".to_string()),
        )]
        .into_iter()
        .collect(),
        cost: Cost {
            default_gen_ms: 30_000.0,
            default_bytes: 90_000.0,
            concurrency_limit: 4,
            rate_per_s: 2.0,
            billing: Some(Billing {
                pricing_profile: "cloudflare".into(),
                per_gen: [
                    ("cpu_ms".to_string(), None),
                    ("class_a_operation".to_string(), Some(1.0)),
                    ("egress_byte".to_string(), None),
                ]
                .into_iter()
                .collect(),
            }),
        },
        lanes: None,
        depends_on: vec![],
        zoom_semantics: zoom,
    }
}

fn pricing() -> PricingTable {
    serde_json::from_str(
        r#"{"pricing":"okibi-pricing/1","profile":"cloudflare","effective":"2026-08",
            "currency":"USD",
            "source":["https://developers.cloudflare.com/workers/platform/pricing/"],
            "retrieved":"2026-08-25",
            "units":{"cpu_ms":0.00000002,"class_a_operation":0.0000045,
            "egress_byte":0.0}}"#,
    )
    .unwrap()
}

fn event(scope: Scope) -> InvalidationEvent {
    InvalidationEvent {
        event: "okibi-invalidation/1".into(),
        service: "papers".into(),
        tileset: "style-aoi-04".into(),
        axis: Axis::Param,
        epoch_from: "style-aoi-04@r12".into(),
        epoch_to: "style-aoi-04@r13".into(),
        scope,
        occurred_at: "2026-08-24T02:00:00Z".into(),
        deadline: None,
    }
}

fn cell(qk8: &str, window: &str, req: f64, tiles: &[(&str, &str, f64)]) -> DigestRecord {
    let mut record = DigestRecord::new("papers", "style-aoi-04", Kind::Content, qk8, window);
    record.req = req;
    record.miss = req / 10.0;
    record.p50_gen_ms = Some(30_000.0);
    record.p95_gen_ms = Some(41_200.0);
    record.avg_bytes = Some(88_231.0);
    record.tiles_observed = tiles.len() as u64;
    record.top_qk = tiles
        .iter()
        .map(|(qk, id, req)| TopTile(qk.to_string(), id.to_string(), *req))
        .collect();
    record
}

fn metadata_cell(req: f64) -> DigestRecord {
    let mut record = DigestRecord::new(
        "papers",
        "style-aoi-04",
        Kind::Tileset,
        "-",
        "2026-08-23/P1D",
    );
    record.req = req;
    record.tiles_observed = 1;
    record.p50_gen_ms = Some(120.0);
    record.avg_bytes = Some(18_220.0);
    record.top_id = vec![TopEntry("meta.json".into(), req)];
    record
}

struct Case {
    digests: Vec<DigestRecord>,
    event: InvalidationEvent,
    manifests: Vec<ServiceManifest>,
    pricing: PricingTable,
    options: PlanOptions,
}

impl Case {
    fn new(digests: Vec<DigestRecord>) -> Self {
        Case {
            digests,
            event: event(Scope::All),
            manifests: vec![manifest(ZoomSemantics::Resolution)],
            pricing: pricing(),
            options: PlanOptions::default(),
        }
    }

    fn plan(&self) -> okibi_core::WarmPlan {
        okibi_core::plan(&PlanInput {
            digests: &self.digests,
            invalidation: &self.event,
            manifests: &self.manifests,
            pricing: &self.pricing,
            epoch: Epoch {
                source: "osm-2026-08-18".into(),
                algo: "ezu-0.7.1".into(),
                param: "style-aoi-04@r13".into(),
            },
            sources: Sources {
                digest: vec!["r2://okibi/digests/2026-08-23.jsonl".into()],
                invalidation: "sha256:0".into(),
                manifests: [("papers".to_string(), "sha256:1".to_string())]
                    .into_iter()
                    .collect(),
                pricing: "pricing/cloudflare-2026-08.json@sha256:2".into(),
            },
            options: self.options.clone(),
        })
        .expect("plan")
    }
}

fn urls(plan: &okibi_core::WarmPlan) -> Vec<&str> {
    plan.entries.iter().map(|e| e.url.as_str()).collect()
}

#[test]
fn a_plan_names_the_hottest_tiles_first() {
    let plan = Case::new(vec![cell(
        "13300211",
        "2026-08-23/P1D",
        3364.0,
        &[
            ("13300211231023", "14/14553/6451", 1544.0),
            ("13300211231022", "14/14552/6451", 1820.0),
        ],
    )])
    .plan();

    assert_eq!(
        urls(&plan),
        [
            "https://papers.reearth.land/t/style-aoi-04/14/14552/6451?e=style-aoi-04@r13",
            "https://papers.reearth.land/t/style-aoi-04/14/14553/6451?e=style-aoi-04@r13",
        ]
    );
    assert_eq!(plan.entries[0].priority, 1.0);
    assert!(plan.entries[1].priority < 1.0);
}

/// A cold root document is every client's first paint, so it goes first even
/// though far fewer requests are riding on it than on a hot tile.
#[test]
fn metadata_goes_first_whatever_its_score() {
    let plan = Case::new(vec![
        cell(
            "13300211",
            "2026-08-23/P1D",
            1820.0,
            &[("13300211231022", "14/14552/6451", 1820.0)],
        ),
        metadata_cell(9.0),
    ])
    .plan();

    assert_eq!(
        urls(&plan)[0],
        "https://papers.reearth.land/t/style-aoi-04/meta.json"
    );
    assert!(plan.entries[0].expected_gen_ms < plan.entries[1].expected_gen_ms);
}

#[test]
fn demand_outside_the_invalidation_is_left_alone() {
    let mut case = Case::new(vec![
        cell(
            "13300211",
            "2026-08-23/P1D",
            1820.0,
            &[("13300211231022", "14/14552/6451", 1820.0)],
        ),
        cell(
            "13311111",
            "2026-08-23/P1D",
            9000.0,
            &[("13311111111111", "14/1/1", 9000.0)],
        ),
    ]);
    case.event = event(Scope::QkPrefixes {
        prefixes: vec!["133002".into()],
    });

    let plan = case.plan();
    assert_eq!(plan.entries.len(), 1);
    assert!(plan.entries[0].url.contains("14/14552/6451"));
}

/// Old evidence is still evidence, but less of it.
#[test]
fn older_windows_count_for_less() {
    let recent = Case::new(vec![cell(
        "13300211",
        "2026-08-23/P1D",
        100.0,
        &[("13300211231022", "14/14552/6451", 100.0)],
    )])
    .plan();

    let week_old = Case::new(vec![cell(
        "13300211",
        "2026-08-16/P1D",
        100.0,
        &[("13300211231022", "14/14552/6451", 100.0)],
    )])
    .plan();

    // The windows are exactly one half-life apart, so the older one counts
    // for exactly half as much, whatever the decay from each to the event.
    let saved = |plan: &okibi_core::WarmPlan| plan.entries[0].saved_req_estimate.unwrap();
    let ratio = saved(&week_old) / saved(&recent);
    assert!((ratio - 0.5).abs() < 1e-12, "{ratio}");
}

#[test]
fn several_windows_of_the_same_cell_add_up() {
    let plan = Case::new(vec![
        cell(
            "13300211",
            "2026-08-23/P1D",
            100.0,
            &[("13300211231022", "14/14552/6451", 100.0)],
        ),
        cell(
            "13300211",
            "2026-08-22/P1D",
            100.0,
            &[("13300211231022", "14/14552/6451", 100.0)],
        ),
    ])
    .plan();

    assert_eq!(plan.entries.len(), 1, "one tile, seen on two days");
    assert!(plan.entries[0].saved_req_estimate.unwrap() > 100.0);
}

/// Where zoom means resolution, an ancestor rescues everything below it, so it
/// is worth at least what its descendants are worth and is fetched first.
#[test]
fn an_ancestor_is_warmed_before_what_it_covers() {
    let plan = Case::new(vec![cell(
        "13300211",
        "2026-08-23/P1D",
        1900.0,
        &[
            ("13300211231022", "14/14552/6451", 1820.0),
            ("1330021123", "10/909/403", 80.0),
        ],
    )])
    .plan();

    assert!(
        plan.entries[0].url.contains("10/909/403"),
        "{:?}",
        urls(&plan)
    );
    assert_eq!(plan.entries[0].priority, plan.entries[1].priority);
}

#[test]
fn a_size_bucket_service_follows_measured_frequency_only() {
    let mut case = Case::new(vec![cell(
        "13300211",
        "2026-08-23/P1D",
        1900.0,
        &[
            ("13300211231022", "14/14552/6451", 1820.0),
            ("1330021123", "10/909/403", 80.0),
        ],
    )]);
    case.manifests = vec![manifest(ZoomSemantics::SizeBucket)];

    let plan = case.plan();
    assert!(plan.entries[0].url.contains("14/14552/6451"));
    assert!(plan.entries[1].url.contains("10/909/403"));
}

#[test]
fn a_deadline_cuts_the_plan_and_makes_the_rest_urgent() {
    let tiles: Vec<(String, String, f64)> = (0..40)
        .map(|i| {
            (
                format!("133002112310{i:02}"),
                format!("14/145{i:02}/6451"),
                (100 - i) as f64,
            )
        })
        .collect();
    let borrowed: Vec<(&str, &str, f64)> = tiles
        .iter()
        .map(|(qk, id, req)| (qk.as_str(), id.as_str(), *req))
        .collect();

    let mut case = Case::new(vec![cell("13300211", "2026-08-23/P1D", 3000.0, &borrowed)]);
    // Four at a time at 30s each is a tile every 7.5 seconds, so two minutes
    // buys about sixteen of the forty.
    case.event.deadline = Some("2026-08-24T02:02:00Z".into());

    let plan = case.plan();
    assert!(plan.entries.len() < 40, "{}", plan.entries.len());
    assert!(plan.entries.iter().all(|e| e.lane == Lane::Urgent));
    assert!(plan.stats.coverage_of_demand < 1.0);
}

#[test]
fn nothing_is_urgent_when_nothing_was_cut() {
    let plan = Case::new(vec![cell(
        "13300211",
        "2026-08-23/P1D",
        1820.0,
        &[("13300211231022", "14/14552/6451", 1820.0)],
    )])
    .plan();

    assert!(plan.entries.iter().all(|e| e.lane == Lane::Warm));
    assert_eq!(plan.stats.coverage_of_demand, 1.0);
}

#[test]
fn a_budget_cuts_the_plan_too() {
    let tiles: Vec<(String, String, f64)> = (0..20)
        .map(|i| {
            (
                format!("133002112310{i:02}"),
                format!("14/145{i:02}/6451"),
                (100 - i) as f64,
            )
        })
        .collect();
    let borrowed: Vec<(&str, &str, f64)> = tiles
        .iter()
        .map(|(qk, id, req)| (qk.as_str(), id.as_str(), *req))
        .collect();

    let mut case = Case::new(vec![cell("13300211", "2026-08-23/P1D", 1000.0, &borrowed)]);
    let full = case.plan();
    case.options.budget_usd = Some(full.estimate.warm.usd / 2.0);

    let cut = case.plan();
    assert!(cut.entries.len() < full.entries.len());
    assert!(cut.estimate.warm.usd <= full.estimate.warm.usd / 2.0);
}

#[test]
fn a_plan_with_nothing_in_it_is_still_a_plan() {
    let plan = Case::new(vec![]).plan();

    assert!(plan.entries.is_empty());
    assert_eq!(plan.stats.total, 0);
    assert_eq!(plan.stats.coverage_of_demand, 0.0);
    assert_eq!(plan.estimate.warm.usd, 0.0);
}

#[test]
fn refuses_to_plan_for_a_service_it_knows_nothing_about() {
    let mut case = Case::new(vec![]);
    case.manifests = vec![];

    let error = okibi_core::plan(&PlanInput {
        digests: &case.digests,
        invalidation: &case.event,
        manifests: &case.manifests,
        pricing: &case.pricing,
        epoch: Epoch::default(),
        sources: Sources::default(),
        options: PlanOptions::default(),
    })
    .unwrap_err();

    assert_eq!(
        error,
        okibi_core::PlanError::NoManifest {
            service: "papers".into()
        }
    );
}

/// The other side of the comparison: what interactive traffic pays for the
/// tiles the plan did not name.
#[test]
fn the_estimate_says_what_not_warming_costs() {
    let mut record = cell(
        "13300211",
        "2026-08-23/P1D",
        1820.0,
        &[("13300211231022", "14/14552/6451", 1820.0)],
    );
    // A hundred tiles were seen; one of them is named.
    record.tiles_observed = 100;

    let plan = Case::new(vec![record]).plan();

    assert_eq!(plan.entries.len(), 1);
    assert_eq!(plan.estimate.no_warm.affected_first_requests, 99.0);
    assert_eq!(plan.estimate.no_warm.user_wait_ms_total, 99.0 * 30_000.0);
    assert_eq!(plan.estimate.no_warm.p95_first_byte_ms, Some(41_200.0));
}

/// A tileset.json is not anywhere, so a region cannot rule it out. It died
/// with whatever part of the world the invalidation named, and every client
/// asks for it before it asks for a tile.
#[test]
fn metadata_survives_a_scope_that_names_a_region() {
    let mut case = Case::new(vec![
        cell(
            "13300211",
            "2026-08-23/P1D",
            1820.0,
            &[("13300211231022", "14/14552/6451", 1820.0)],
        ),
        metadata_cell(9120.0),
    ]);
    case.event = event(Scope::QkPrefixes {
        prefixes: vec!["133002".into()],
    });

    let plan = case.plan();
    assert_eq!(
        urls(&plan)[0],
        "https://papers.reearth.land/t/style-aoi-04/meta.json"
    );
}

/// A scope naming ids is the one that does have something to say about it.
#[test]
fn metadata_is_left_out_when_the_scope_names_other_things() {
    let mut case = Case::new(vec![metadata_cell(9120.0)]);
    case.event = event(Scope::Ids {
        ids: vec!["14/14552/6451".into()],
    });

    assert!(case.plan().entries.is_empty());
}

/// Workers freeze their clocks between I/O to blunt Spectre, so a generator
/// that is pure CPU measures as zero however long it ran. A cost of zero is a
/// priority of zero is a plan that warms nothing, and none of that is an
/// error — which is what makes it worth refusing to believe.
#[test]
fn a_generation_time_of_zero_is_not_a_measurement() {
    let mut unmeasurable = cell(
        "13300211",
        "2026-08-23/P1D",
        1820.0,
        &[("13300211231022", "14/14552/6451", 1820.0)],
    );
    unmeasurable.p50_gen_ms = Some(0.0);
    unmeasurable.avg_bytes = Some(0.0);

    let plan = Case::new(vec![unmeasurable]).plan();

    assert_eq!(plan.entries.len(), 1, "the tile is still worth warming");
    // The manifest's fallbacks, which is what a service says when the
    // measurement cannot be taken.
    assert_eq!(plan.entries[0].expected_gen_ms, 30_000.0);
    assert!(plan.entries[0].priority > 0.0);
    assert!(plan.estimate.warm.usd > 0.0, "and it is not free");
}

/// A real measurement still wins, so a fast service is not charged the
/// fallback for being fast.
#[test]
fn a_real_measurement_is_still_used() {
    let mut quick = cell(
        "13300211",
        "2026-08-23/P1D",
        1820.0,
        &[("13300211231022", "14/14552/6451", 1820.0)],
    );
    quick.p50_gen_ms = Some(12.0);

    let plan = Case::new(vec![quick]).plan();
    assert_eq!(plan.entries[0].expected_gen_ms, 12.0);
}

/// Most services keep their version in a cache key rather than in a URL, so
/// there is nothing for an epochs file to hold and no reason to demand one.
#[test]
fn a_template_that_asks_for_no_epoch_needs_none() {
    let mut case = Case::new(vec![cell(
        "13300211",
        "2026-08-23/P1D",
        1820.0,
        &[("13300211231022", "14/14552/6451", 1820.0)],
    )]);
    case.manifests[0].url_template = "https://papers.reearth.land/t/{tileset}/{id}".into();
    case.manifests[0].meta_urls.clear();

    let plan = okibi_core::plan(&PlanInput {
        digests: &case.digests,
        invalidation: &case.event,
        manifests: &case.manifests,
        pricing: &case.pricing,
        epoch: Epoch::default(),
        sources: Sources::default(),
        options: PlanOptions::default(),
    })
    .expect("no epoch is asked for, so none is needed");

    assert_eq!(
        plan.entries[0].url,
        "https://papers.reearth.land/t/style-aoi-04/14/14552/6451"
    );
}

/// One that does ask is refused rather than fetched with the placeholder
/// still in it — a few thousand real requests that would all miss, leaving
/// the tiles they were meant to warm exactly as cold.
#[test]
fn a_template_that_asks_for_an_epoch_must_be_given_it() {
    let case = Case::new(vec![cell(
        "13300211",
        "2026-08-23/P1D",
        1820.0,
        &[("13300211231022", "14/14552/6451", 1820.0)],
    )]);

    let error = okibi_core::plan(&PlanInput {
        digests: &case.digests,
        invalidation: &case.event,
        manifests: &case.manifests,
        pricing: &case.pricing,
        // The fixture's template ends in `?e={epoch.param}`.
        epoch: Epoch::default(),
        sources: Sources::default(),
        options: PlanOptions::default(),
    })
    .unwrap_err();

    assert_eq!(
        error,
        okibi_core::PlanError::EpochMissing {
            axis: "param",
            template: PAPERS_URL.to_string(),
        }
    );
}

/// A document warming cannot help is left out of the plan and counted.
///
/// Warming works on things that stay warm. A root tileset composed per request
/// and served with a minute of freshness is neither: the request costs one
/// slot at the very front of the plan, where metadata sorts, and buys nothing.
/// The service is the one that knows this, so it is the manifest that says it.
#[test]
fn a_document_warming_cannot_help_is_counted_rather_than_fetched() {
    let mut case = Case::new(vec![
        metadata_cell(9120.0),
        cell(
            "13300211",
            "2026-08-23/P1D",
            1820.0,
            &[("13300211231022", "14/14552/6451", 1820.0)],
        ),
    ]);
    case.manifests[0].meta_urls.insert("tileset".into(), None);

    let plan = okibi_core::plan(&PlanInput {
        digests: &case.digests,
        invalidation: &case.event,
        manifests: &case.manifests,
        pricing: &case.pricing,
        epoch: Epoch {
            source: "osm-2026-08-18".into(),
            algo: "ezu-0.7.1".into(),
            param: "style-aoi-04@r13".into(),
        },
        sources: Sources::default(),
        options: PlanOptions::default(),
    })
    .unwrap();

    assert_eq!(plan.stats.unwarmable, 1, "the root document");
    assert!(
        plan.entries
            .iter()
            .all(|entry| !entry.url.contains("meta.json")),
        "{:?}",
        plan.entries.iter().map(|e| &e.url).collect::<Vec<_>>()
    );
    assert!(!plan.entries.is_empty(), "the tiles are still planned");
}

/// A metadata document the manifest names no URL for is refused rather than
/// fetched through the tile template.
///
/// The tile template is built out of coordinates and this document has none,
/// so filling it in yields a URL of exactly the right shape for somewhere
/// that does not exist. The plan would read as covering the root document and
/// would in fact spend its first and most important request on a 404 — which
/// is the request every client makes before it asks for a tile.
#[test]
fn a_metadata_document_with_no_url_is_refused_rather_than_guessed_at() {
    let mut case = Case::new(vec![metadata_cell(9120.0)]);
    case.manifests[0].meta_urls.clear();

    let error = okibi_core::plan(&PlanInput {
        digests: &case.digests,
        invalidation: &case.event,
        manifests: &case.manifests,
        pricing: &case.pricing,
        epoch: Epoch {
            source: "osm-2026-08-18".into(),
            algo: "ezu-0.7.1".into(),
            param: "style-aoi-04@r13".into(),
        },
        sources: Sources::default(),
        options: PlanOptions::default(),
    })
    .unwrap_err();

    assert_eq!(
        error,
        okibi_core::PlanError::NoMetaUrl {
            service: "papers".to_string(),
            kind: "tileset",
        }
    );
}

/// Including one only a metadata URL asks for, since a plan puts those first
/// and a broken one is everybody's first paint.
#[test]
fn a_metadata_template_is_checked_too() {
    let mut case = Case::new(vec![metadata_cell(9120.0)]);
    case.manifests[0].url_template = "https://papers.reearth.land/t/{tileset}/{id}".into();
    case.manifests[0].meta_urls.insert(
        "tileset".into(),
        Some("https://p/{tileset}/{epoch.algo}/meta.json".into()),
    );

    let error = okibi_core::plan(&PlanInput {
        digests: &case.digests,
        invalidation: &case.event,
        manifests: &case.manifests,
        pricing: &case.pricing,
        epoch: Epoch::default(),
        sources: Sources::default(),
        options: PlanOptions::default(),
    })
    .unwrap_err();

    assert!(matches!(
        error,
        okibi_core::PlanError::EpochMissing { axis: "algo", .. }
    ));
}
