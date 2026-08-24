//! Turning query rows into digest records.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer};

use crate::{
    digest::{DigestRecord, Kind, TopEntry, TopTile, UNPLACED},
    window::Window,
};

/// One row of the cells query.
#[derive(Debug, Clone, Deserialize)]
pub struct CellRow {
    pub service: String,
    pub tileset: String,
    pub kind: String,
    pub qk8: String,
    #[serde(deserialize_with = "number")]
    pub req: f64,
    #[serde(deserialize_with = "number")]
    pub miss: f64,
    #[serde(default, deserialize_with = "maybe_number")]
    pub p50_gen_ms: Option<f64>,
    #[serde(default, deserialize_with = "maybe_number")]
    pub p95_gen_ms: Option<f64>,
    #[serde(default, deserialize_with = "maybe_number")]
    pub sum_gen_ms: Option<f64>,
    #[serde(default, deserialize_with = "maybe_number")]
    pub bytes: Option<f64>,
    #[serde(default, deserialize_with = "maybe_number")]
    pub avg_bytes: Option<f64>,
    #[serde(deserialize_with = "number")]
    pub tiles_observed: f64,
}

/// One row of the top-tiles query.
#[derive(Debug, Clone, Deserialize)]
pub struct TileRow {
    pub service: String,
    pub tileset: String,
    pub kind: String,
    pub qk8: String,
    pub qk: String,
    pub id: String,
    #[serde(deserialize_with = "number")]
    pub req: f64,
}

/// ClickHouse's JSON writes 64-bit integers as strings and floats as numbers,
/// and which a column is depends on the aggregate that produced it. Both are
/// the same number here.
///
/// Written as a visitor rather than through a JSON value, so that this crate
/// does not need a JSON library to read rows that arrive as JSON. What
/// arrives is the caller's business; what a number is, is not.
struct Number;

impl<'de> serde::de::Visitor<'de> for Number {
    type Value = f64;

    fn expecting(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("a number, or a number written as a string")
    }

    fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<f64, E> {
        Ok(value)
    }

    fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<f64, E> {
        Ok(value as f64)
    }

    fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<f64, E> {
        Ok(value as f64)
    }

    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<f64, E> {
        value.parse().map_err(serde::de::Error::custom)
    }

    fn visit_none<E: serde::de::Error>(self) -> Result<f64, E> {
        Ok(0.0)
    }

    fn visit_unit<E: serde::de::Error>(self) -> Result<f64, E> {
        Ok(0.0)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<f64, D::Error> {
        deserializer.deserialize_any(Number)
    }
}

fn number<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
    deserializer.deserialize_any(Number)
}

fn maybe_number<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<f64>, D::Error> {
    let value = deserializer.deserialize_option(Number)?;
    Ok(value.is_finite().then_some(value))
}

/// What the rows did not become, so that nothing is dropped quietly.
#[derive(Debug, Default, PartialEq, serde::Serialize)]
pub struct Skipped {
    /// Rows naming a kind this version of the vocabulary does not have.
    pub unknown_kind: usize,
    /// `content` rows with no cell to belong to, which should not exist.
    pub unplaceable: usize,
    /// Cells the top-tiles query ran out of rows before reaching.
    pub cells_without_top: usize,
}

fn parse_kind(kind: &str) -> Option<Kind> {
    match kind {
        "content" => Some(Kind::Content),
        "tileset" => Some(Kind::Tileset),
        "subtree" => Some(Kind::Subtree),
        "meta" => Some(Kind::Meta),
        _ => None,
    }
}

/// The key a cell and its tiles agree on.
type CellKey = (String, String, String, String);

fn cell_key(service: &str, tileset: &str, kind: &str, qk8: &str) -> CellKey {
    (
        service.to_string(),
        tileset.to_string(),
        kind.to_string(),
        qk8.to_string(),
    )
}

/// Hottest first, with the key breaking ties so that two runs over the same
/// day produce the same list.
fn hottest_first(a_req: f64, b_req: f64, a_key: &str, b_key: &str) -> std::cmp::Ordering {
    b_req
        .partial_cmp(&a_req)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| a_key.cmp(b_key))
}

/// Build the digest for one window.
///
/// The result is ordered, because a digest is compared against the one before
/// it more often than it is read once.
pub fn assemble(
    cells: Vec<CellRow>,
    tiles: Vec<TileRow>,
    window: &Window,
    top_n: usize,
) -> (Vec<DigestRecord>, Skipped) {
    let mut skipped = Skipped::default();

    // A placed tile carries both keys: the quadkey places it and the id is
    // what a URL gets built from. An unplaced one has only the id.
    let mut placed: BTreeMap<CellKey, Vec<TopTile>> = BTreeMap::new();
    let mut unplaced: BTreeMap<CellKey, Vec<TopEntry>> = BTreeMap::new();
    for tile in tiles {
        let Some(kind) = parse_kind(&tile.kind) else {
            continue;
        };
        let key = cell_key(&tile.service, &tile.tileset, &tile.kind, &tile.qk8);
        if kind.is_placed() {
            placed
                .entry(key)
                .or_default()
                .push(TopTile(tile.qk, tile.id, tile.req));
        } else {
            unplaced
                .entry(key)
                .or_default()
                .push(TopEntry(tile.id, tile.req));
        }
    }

    let mut records: BTreeMap<CellKey, DigestRecord> = BTreeMap::new();
    for cell in cells {
        let Some(kind) = parse_kind(&cell.kind) else {
            skipped.unknown_kind += 1;
            continue;
        };
        if kind.is_placed() && cell.qk8.is_empty() {
            skipped.unplaceable += 1;
            continue;
        }

        let key = cell_key(&cell.service, &cell.tileset, &cell.kind, &cell.qk8);
        let qk8 = if kind.is_placed() {
            cell.qk8.clone()
        } else {
            UNPLACED.to_string()
        };

        let mut record =
            DigestRecord::new(cell.service, cell.tileset, kind, qk8, window.interval());
        record.req = cell.req;
        record.miss = cell.miss;
        record.p50_gen_ms = cell.p50_gen_ms;
        record.p95_gen_ms = cell.p95_gen_ms;
        record.sum_gen_ms = cell.sum_gen_ms;
        record.bytes = cell.bytes;
        record.avg_bytes = cell.avg_bytes;
        record.tiles_observed = cell.tiles_observed.max(0.0) as u64;

        if kind.is_placed() {
            let mut top = placed.remove(&key).unwrap_or_default();
            if top.is_empty() {
                skipped.cells_without_top += 1;
            }
            top.sort_by(|a, b| hottest_first(a.req(), b.req(), a.qk(), b.qk()));
            top.truncate(top_n);
            record.top_qk = top;
        } else {
            let mut top = unplaced.remove(&key).unwrap_or_default();
            if top.is_empty() {
                skipped.cells_without_top += 1;
            }
            top.sort_by(|a, b| hottest_first(a.req(), b.req(), a.id(), b.id()));
            top.truncate(top_n);
            record.top_id = top;
        }

        records.insert(key, record);
    }

    (records.into_values().collect(), skipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window() -> Window {
        Window::parse("2026-08-23").unwrap()
    }

    fn cell(kind: &str, qk8: &str, req: f64) -> CellRow {
        CellRow {
            service: "papers".into(),
            tileset: "style-aoi-04".into(),
            kind: kind.into(),
            qk8: qk8.into(),
            req,
            miss: 1.0,
            p50_gen_ms: Some(28900.0),
            p95_gen_ms: Some(41200.0),
            sum_gen_ms: Some(9016800.0),
            bytes: Some(4.2e9),
            avg_bytes: Some(88231.0),
            tiles_observed: 3.0,
        }
    }

    fn tile(kind: &str, qk8: &str, qk: &str, id: &str, req: f64) -> TileRow {
        TileRow {
            service: "papers".into(),
            tileset: "style-aoi-04".into(),
            kind: kind.into(),
            qk8: qk8.into(),
            qk: qk.into(),
            id: id.into(),
            req,
        }
    }

    #[test]
    fn a_cell_carries_its_hottest_tiles_first() {
        let (records, skipped) = assemble(
            vec![cell("content", "13300211", 48210.0)],
            vec![
                tile("content", "13300211", "13300211231023", "a", 1544.0),
                tile("content", "13300211", "13300211231022", "b", 1820.0),
            ],
            &window(),
            20,
        );

        let top: Vec<&str> = records[0].top_qk.iter().map(|t| t.qk()).collect();
        assert_eq!(top, ["13300211231022", "13300211231023"]);
        assert_eq!(records[0].top_qk[0].id(), "b");
        assert_eq!(records[0].window, "2026-08-23/P1D");
        assert_eq!(skipped, Skipped::default());
    }

    #[test]
    fn keeps_only_as_many_top_tiles_as_asked_for() {
        let tiles: Vec<TileRow> = (0..30)
            .map(|i| {
                tile(
                    "content",
                    "13300211",
                    &format!("133002110{i}"),
                    "x",
                    i as f64,
                )
            })
            .collect();

        let (records, _) = assemble(vec![cell("content", "13300211", 1.0)], tiles, &window(), 5);
        assert_eq!(records[0].top_qk.len(), 5);
        assert_eq!(records[0].top_qk[0].req(), 29.0);
    }

    /// Requests with no coordinates cannot be placed, so they collapse into
    /// one record per tileset and are named by what was asked for.
    #[test]
    fn requests_with_no_coordinates_are_named_by_their_id() {
        let (records, _) = assemble(
            vec![cell("tileset", "", 9120.0)],
            vec![tile("tileset", "", "", "tileset.json", 9120.0)],
            &window(),
            20,
        );

        assert_eq!(records[0].qk8, "-");
        assert!(records[0].top_qk.is_empty());
        assert_eq!(records[0].top_id[0].id(), "tileset.json");
    }

    #[test]
    fn reports_what_it_could_not_use() {
        let (records, skipped) = assemble(
            vec![
                cell("hologram", "13300211", 1.0),
                cell("content", "", 1.0),
                cell("content", "13300212", 1.0),
            ],
            vec![],
            &window(),
            20,
        );

        assert_eq!(records.len(), 1);
        assert_eq!(skipped.unknown_kind, 1);
        assert_eq!(skipped.unplaceable, 1);
        // The one surviving cell got no top tiles, because none were asked for.
        assert_eq!(skipped.cells_without_top, 1);
    }

    #[test]
    fn orders_records_so_two_days_can_be_compared() {
        let (records, _) = assemble(
            vec![
                cell("content", "13300213", 1.0),
                cell("content", "13300211", 1.0),
                cell("content", "13300212", 1.0),
            ],
            vec![],
            &window(),
            20,
        );

        let cells: Vec<&str> = records.iter().map(|r| r.qk8.as_str()).collect();
        assert_eq!(cells, ["13300211", "13300212", "13300213"]);
    }

    /// The SQL API writes 64-bit integers as strings and floats as numbers,
    /// depending on which aggregate produced the column.
    #[test]
    fn reads_a_count_whether_it_was_quoted_or_not() {
        let row: CellRow = serde_json::from_str(
            r#"{"service":"papers","tileset":"t","kind":"content","qk8":"13300211",
                "req":48210.5,"miss":"312","tiles_observed":"1240"}"#,
        )
        .unwrap();

        assert_eq!(row.req, 48210.5);
        assert_eq!(row.miss, 312.0);
        assert_eq!(row.tiles_observed, 1240.0);
        assert_eq!(row.p50_gen_ms, None);
    }
}
