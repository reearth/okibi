//! Every example in `spec/examples/` validates against the schema it claims.
//!
//! The schemas are the original for what these documents are, and prose in
//! `spec/*.md` describes them rather than standing in for them. So the prose
//! can only stay honest if the examples it shows are the ones being checked —
//! which is what this test is for. It will grow into checking the crate's own
//! serde types against the same schemas, once those types exist.

use std::{fs, path::Path};

/// Which schema each example is an instance of. The mapping is written out
/// rather than inferred from the filename, so that adding an example forces a
/// decision about what it is an example of.
const CASES: &[(&str, &str)] = &[
    (
        "tile-demand-event.terrain.json",
        "tile-demand-event.schema.json",
    ),
    (
        "tile-demand-event.buildings.json",
        "tile-demand-event.schema.json",
    ),
    (
        "tile-demand-event.buildings-tileset.json",
        "tile-demand-event.schema.json",
    ),
    (
        "tile-demand-event.papers-warm.json",
        "tile-demand-event.schema.json",
    ),
    ("demand-digest.json", "demand-digest.schema.json"),
    ("demand-digest.tileset.json", "demand-digest.schema.json"),
    ("service-manifest.json", "service-manifest.schema.json"),
    ("invalidation-event.json", "invalidation-event.schema.json"),
    ("pricing-table.json", "pricing-table.schema.json"),
    ("warm-plan.json", "warm-plan.schema.json"),
];

fn spec_dir() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../spec"))
}

fn read_json(path: &Path) -> serde_json::Value {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn examples_match_their_schemas() {
    for (example, schema) in CASES {
        let schema_path = spec_dir().join("schema").join(schema);
        let example_path = spec_dir().join("examples").join(example);

        let validator = jsonschema::validator_for(&read_json(&schema_path))
            .unwrap_or_else(|e| panic!("{schema} is not a usable schema: {e}"));

        let instance = read_json(&example_path);
        let errors: Vec<String> = validator
            .iter_errors(&instance)
            .map(|e| e.to_string())
            .collect();
        assert!(
            errors.is_empty(),
            "{example} against {schema}:\n{}",
            errors.join("\n")
        );
    }
}

/// Weighted counts are fractional in general, so a whole one comes back as
/// `48210.0` where the file wrote `48210`. That is the same number and the
/// schema asks only for a number, so the comparison is made over values rather
/// than over how they were spelled.
fn as_numbers(value: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Value::Number(n) => serde_json::json!(n.as_f64().expect("finite")),
        Value::Array(items) => Value::Array(items.iter().map(as_numbers).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(k, v)| (k.clone(), as_numbers(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// The crate's own type has to be the document the specification describes,
/// not merely something with the same field names: reading an example and
/// writing it back must produce the same JSON, and that JSON must still
/// satisfy the schema.
#[test]
fn the_digest_type_round_trips_through_the_spec_examples() {
    let schema_path = spec_dir().join("schema").join("demand-digest.schema.json");
    let validator = jsonschema::validator_for(&read_json(&schema_path)).expect("schema");

    for example in ["demand-digest.json", "demand-digest.tileset.json"] {
        let original = read_json(&spec_dir().join("examples").join(example));

        let record: okibi_core::DigestRecord =
            serde_json::from_value(original.clone()).unwrap_or_else(|e| panic!("{example}: {e}"));
        let written = serde_json::to_value(&record).expect("serialise");

        assert_eq!(
            as_numbers(&written),
            as_numbers(&original),
            "{example} did not survive the round trip"
        );
        assert!(validator.is_valid(&written), "{example} left the schema");
    }
}

/// `tile.qk8` is `tile.qk` cut to eight characters, and a digest cell's `qk8`
/// is the cell its `top_qk` tiles fall in. A schema cannot say that one field
/// is a prefix of another, and an example where they disagree would be an
/// example of the aggregation not working.
#[test]
fn spatial_keys_agree_with_the_cells_they_fall_in() {
    for (example, _) in CASES {
        let doc = read_json(&spec_dir().join("examples").join(example));

        if let (Some(qk), Some(qk8)) = (doc.get("tile.qk"), doc.get("tile.qk8")) {
            let (qk, qk8) = (qk.as_str().unwrap(), qk8.as_str().unwrap());
            assert_eq!(qk8, &qk[..qk8.len().min(qk.len())], "{example}");
            assert_eq!(qk8.len(), qk.len().min(8), "{example}");
        }

        let (Some(cell), Some(top)) = (doc.get("qk8"), doc.get("top_qk")) else {
            continue;
        };
        let cell = cell.as_str().unwrap();
        for entry in top.as_array().unwrap() {
            let tile = entry[0].as_str().unwrap();
            assert!(tile.starts_with(cell), "{example}: {tile} is not in {cell}");
        }
    }
}

/// An example nobody validates is an example nobody maintains.
#[test]
fn every_example_is_covered() {
    let mut uncovered: Vec<String> = fs::read_dir(spec_dir().join("examples"))
        .expect("spec/examples")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.ends_with(".json"))
        .filter(|name| !CASES.iter().any(|(example, _)| example == name))
        .collect();
    uncovered.sort();

    assert!(
        uncovered.is_empty(),
        "not listed in CASES: {}",
        uncovered.join(", ")
    );
}
