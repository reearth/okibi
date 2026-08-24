//! What died. See `spec/okibi-contract.md`.

use serde::{Deserialize, Serialize};

pub const INVALIDATION_VERSION: &str = "okibi-invalidation/1";

/// The normalised form of an invalidation, whatever mechanism caused it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvalidationEvent {
    pub event: String,
    pub service: String,
    pub tileset: String,
    pub axis: Axis,
    pub epoch_from: String,
    pub epoch_to: String,
    pub scope: Scope,
    pub occurred_at: String,
    /// A time the warming should be finished by. An input, not a clock the
    /// planner reads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Axis {
    Source,
    Algo,
    Param,
}

/// What of the tileset died.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Scope {
    /// The whole tileset. With cache keys compiled into a service, this is
    /// most invalidations.
    All,
    /// Regions, named by quadkey prefix.
    QkPrefixes { prefixes: Vec<String> },
    /// Named tiles, by native id.
    Ids { ids: Vec<String> },
}

impl Scope {
    /// Whether a cell can hold anything this invalidation killed.
    ///
    /// A cell is eight quadkey characters and a prefix can be longer or
    /// shorter than that, so the test runs both ways: a cell inside the
    /// region, and a region inside the cell. Being wrong in the strict
    /// direction would drop tiles that did die.
    pub fn touches_cell(&self, qk8: &str) -> bool {
        match self {
            Scope::All => true,
            Scope::QkPrefixes { prefixes } => prefixes
                .iter()
                .any(|prefix| qk8.starts_with(prefix.as_str()) || prefix.starts_with(qk8)),
            // A named tile is somewhere, and which cell it is in is not
            // decidable from the id alone. The tile-level test settles it.
            Scope::Ids { .. } => true,
        }
    }

    /// Whether this exact tile died.
    pub fn covers_tile(&self, qk: &str, id: &str) -> bool {
        match self {
            Scope::All => true,
            Scope::QkPrefixes { prefixes } => prefixes
                .iter()
                .any(|prefix| qk.starts_with(prefix.as_str())),
            Scope::Ids { ids } => ids.iter().any(|named| named == id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_region_and_a_cell_can_contain_each_other() {
        let scope = Scope::QkPrefixes {
            prefixes: vec!["133002".into()],
        };

        // The cell is inside the region.
        assert!(scope.touches_cell("13300211"));
        // The region is inside the cell, so part of the cell died.
        assert!(scope.touches_cell("1330"));
        assert!(!scope.touches_cell("13300311"));
    }

    #[test]
    fn a_tile_is_covered_only_if_the_prefix_reaches_it() {
        let scope = Scope::QkPrefixes {
            prefixes: vec!["133002".into()],
        };

        assert!(scope.covers_tile("13300211231022", "14/14552/6451"));
        assert!(!scope.covers_tile("13300311231022", "14/1/1"));
    }

    #[test]
    fn named_ids_are_settled_at_the_tile_and_not_the_cell() {
        let scope = Scope::Ids {
            ids: vec!["14/14552/6451".into()],
        };

        assert!(scope.touches_cell("13300211"));
        assert!(scope.covers_tile("13300211231022", "14/14552/6451"));
        assert!(!scope.covers_tile("13300211231022", "14/14552/6452"));
    }

    #[test]
    fn everything_is_covered_when_everything_died() {
        assert!(Scope::All.touches_cell("13300211"));
        assert!(Scope::All.covers_tile("1", "anything"));
    }
}
