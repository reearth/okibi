//! The demand digest: aggregated demand, and the only demand data the planner
//! reads. See `spec/demand-digest.md`.

use serde::{Deserialize, Serialize};

/// The version every record carries, so a reader in a mixed period can branch
/// on what it was handed rather than guess from the shape.
pub const DIGEST_VERSION: &str = "tile-demand-digest/1";

/// What sort of request a record aggregates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Content,
    Tileset,
    Subtree,
    Meta,
}

impl Kind {
    /// Whether requests of this kind have tile coordinates, and so a cell to
    /// belong to.
    pub fn is_placed(&self) -> bool {
        matches!(self, Kind::Content)
    }
}

/// The `qk8` of a record with no coordinates, which cannot be placed in a cell.
pub const UNPLACED: &str = "-";

/// One aggregated cell: `(service, tileset, kind, qk8, window)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DigestRecord {
    pub digest: String,
    pub service: String,
    pub tileset: String,
    pub kind: Kind,
    /// Eight quadkey characters, or [`UNPLACED`].
    pub qk8: String,
    /// An ISO 8601 interval, e.g. `2026-08-23/P1D`.
    pub window: String,

    /// Requests with the sampling weight restored, counting organic only.
    pub req: f64,
    pub miss: f64,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub p50_gen_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p95_gen_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sum_gen_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg_bytes: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<f64>,

    /// Distinct tiles seen in this cell: the denominator estimates rest on.
    pub tiles_observed: u64,

    /// How many events one row stood for, at worst.
    ///
    /// Recorded and not acted on. A backend that samples keeps the totals
    /// right — the weight is restored when they are summed — but it cannot
    /// keep the tail right: a cell whose rows each stood for a hundred events
    /// knows roughly how much demand it had and very little about which tiles
    /// it was spread across. When an estimate turns out wrong, this is the
    /// first thing worth looking at, and only if it was written down while
    /// the rows still existed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_interval_max: Option<f64>,

    /// The cell's top tiles, as `[qk, id, req]`. Absent for unplaced records.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub top_qk: Vec<TopTile>,
    /// The same by native id, for records with no coordinates.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub top_id: Vec<TopEntry>,
}

/// A placed tile and how often it was asked for: `[qk, id, req]`.
///
/// Both keys are here because neither implies the other and both are needed.
/// The quadkey is what the invalidation scope is matched against and what
/// makes an ancestor recognisable; the native id is what a URL is built from,
/// and deriving one from the other would require knowing how the service
/// numbers its tiles — which is exactly what okibi does not know.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopTile(pub String, pub String, pub f64);

impl TopTile {
    pub fn qk(&self) -> &str {
        &self.0
    }

    pub fn id(&self) -> &str {
        &self.1
    }

    pub fn req(&self) -> f64 {
        self.2
    }
}

/// A document with no coordinates and how often it was asked for: `[id, req]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopEntry(pub String, pub f64);

impl TopEntry {
    pub fn id(&self) -> &str {
        &self.0
    }

    pub fn req(&self) -> f64 {
        self.1
    }
}

impl DigestRecord {
    /// A record with the version filled in and no measurements yet.
    pub fn new(
        service: impl Into<String>,
        tileset: impl Into<String>,
        kind: Kind,
        qk8: impl Into<String>,
        window: impl Into<String>,
    ) -> Self {
        DigestRecord {
            digest: DIGEST_VERSION.to_string(),
            service: service.into(),
            tileset: tileset.into(),
            kind,
            qk8: qk8.into(),
            window: window.into(),
            req: 0.0,
            miss: 0.0,
            p50_gen_ms: None,
            p95_gen_ms: None,
            sum_gen_ms: None,
            avg_bytes: None,
            bytes: None,
            tiles_observed: 0,
            sample_interval_max: None,
            top_qk: Vec::new(),
            top_id: Vec::new(),
        }
    }
}
