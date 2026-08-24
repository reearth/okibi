//! The same inputs give the same plan, byte for byte.
//!
//! A plan is reviewed by a person and carries a cost estimate someone acts on.
//! If re-running the planner produced a different file, neither the review nor
//! the estimate would mean anything, and `derived_from` would be a decoration
//! rather than a claim anyone can check. So the check is on the bytes, not on
//! the set of tiles or roughly the order.
//!
//! Each case is a directory under `tests/golden/`: the inputs, and the plan
//! they are expected to produce. Run with `UPDATE_GOLDEN=1` to rewrite the
//! expectations after a deliberate change — and read the diff before keeping
//! it, because that diff is the change.

use std::{fs, path::Path};

use okibi_core::{
    DigestRecord, InvalidationEvent, PricingTable, ServiceManifest,
    manifest::Epoch,
    planner::{PlanInput, PlanOptions, Sources},
};

const CASES: &[&str] = &["papers-param-change", "buildings-size-buckets"];

fn golden_dir() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/golden"))
}

fn read<T: serde::de::DeserializeOwned>(path: &Path) -> T {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn read_digests(path: &Path) -> Vec<DigestRecord> {
    fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap_or_else(|e| panic!("{line}: {e}")))
        .collect()
}

fn run_case(case: &str) -> String {
    let dir = golden_dir().join(case);

    let digests = read_digests(&dir.join("digest.jsonl"));
    let invalidation: InvalidationEvent = read(&dir.join("invalidation.json"));
    let manifests: Vec<ServiceManifest> = read(&dir.join("manifests.json"));
    let pricing: PricingTable = read(&dir.join("pricing.json"));
    let epoch: Epoch = read(&dir.join("epoch.json"));

    let plan = okibi_core::plan(&PlanInput {
        digests: &digests,
        invalidation: &invalidation,
        manifests: &manifests,
        pricing: &pricing,
        epoch,
        sources: Sources {
            digest: vec![format!("tests/golden/{case}/digest.jsonl")],
            invalidation: format!("sha256:{}", "0".repeat(64)),
            manifests: manifests
                .iter()
                .map(|m| (m.service.clone(), format!("sha256:{}", "1".repeat(64))))
                .collect(),
            pricing: format!("tests/golden/{case}/pricing.json@sha256:{}", "2".repeat(64)),
        },
        options: PlanOptions::default(),
    })
    .expect("plan");

    let mut json = serde_json::to_string_pretty(&plan).expect("serialise");
    json.push('\n');
    json
}

#[test]
fn plans_are_what_they_were() {
    let updating = std::env::var_os("UPDATE_GOLDEN").is_some();

    for case in CASES {
        let produced = run_case(case);
        let expected_path = golden_dir().join(case).join("plan.json");

        if updating {
            fs::write(&expected_path, &produced).expect("write");
            continue;
        }

        let expected = fs::read_to_string(&expected_path)
            .unwrap_or_else(|e| panic!("{}: {e}", expected_path.display()));

        assert_eq!(
            produced, expected,
            "{case} produced a different plan; run with UPDATE_GOLDEN=1 if that was the intention"
        );
    }
}

/// Running twice in one process has to give the same answer too. Iteration
/// order over a hash map is stable within a run but not across builds, so a
/// plan that only agrees with itself here would still drift in CI.
#[test]
fn a_plan_agrees_with_itself() {
    for case in CASES {
        assert_eq!(run_case(case), run_case(case), "{case}");
    }
}
