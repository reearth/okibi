//! Deriving what died from what changed.
//!
//! A service does not write invalidation events. It edits `okibi.epochs.json`,
//! which is the same file its cache keys are built from, and the event is the
//! diff — so the event is a consequence of the commit rather than a second
//! description of it that can disagree with it.

use okibi_core::{
    InvalidationEvent, Scope,
    invalidation::{Axis, INVALIDATION_VERSION},
    manifest::Epoch,
};

use crate::inputs::EpochsFile;

/// Every invalidation between two versions of an epochs file.
///
/// One event per tileset that moved. A tileset whose epochs are untouched did
/// not die, and a tileset that only exists in the new file is new rather than
/// invalidated: nothing was cached under it to lose.
pub fn between(
    before: &EpochsFile,
    after: &EpochsFile,
    occurred_at: &str,
    deadline: Option<&str>,
) -> Vec<InvalidationEvent> {
    let mut events = Vec::new();

    for (tileset, new) in &after.tilesets {
        let Some(old) = before.tilesets.get(tileset) else {
            continue;
        };
        let Some(axis) = axis_that_moved(old, new) else {
            continue;
        };

        events.push(InvalidationEvent {
            event: INVALIDATION_VERSION.to_string(),
            service: after.service.clone(),
            tileset: tileset.clone(),
            axis,
            epoch_from: value_of(old, axis).to_string(),
            epoch_to: value_of(new, axis).to_string(),
            // Cache keys are compiled into a service, so a change to them
            // takes the whole tileset with it. A narrower scope would need a
            // narrower cause than a deploy.
            scope: Scope::All,
            occurred_at: occurred_at.to_string(),
            deadline: deadline.map(str::to_string),
        });
    }

    events
}

/// Which axis to report when more than one moved.
///
/// Source, then algorithm, then parameter. All of them invalidate the whole
/// tileset, so the plan is the same either way and the axis is what a reader
/// is told about the change — for which the most fundamental cause is the
/// useful one.
fn axis_that_moved(before: &Epoch, after: &Epoch) -> Option<Axis> {
    if before.source != after.source {
        Some(Axis::Source)
    } else if before.algo != after.algo {
        Some(Axis::Algo)
    } else if before.param != after.param {
        Some(Axis::Param)
    } else {
        None
    }
}

fn value_of(epoch: &Epoch, axis: Axis) -> &str {
    match axis {
        Axis::Source => &epoch.source,
        Axis::Algo => &epoch.algo,
        Axis::Param => &epoch.param,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn epochs(tilesets: &[(&str, &str, &str, &str)]) -> EpochsFile {
        EpochsFile {
            service: "papers".into(),
            tilesets: tilesets
                .iter()
                .map(|(name, source, algo, param)| {
                    (
                        name.to_string(),
                        Epoch {
                            source: source.to_string(),
                            algo: algo.to_string(),
                            param: param.to_string(),
                        },
                    )
                })
                .collect(),
        }
    }

    const AT: &str = "2026-08-24T02:00:00Z";

    #[test]
    fn a_changed_epoch_is_an_invalidation() {
        let before = epochs(&[("a", "osm-08-18", "ezu-0.7.1", "r12")]);
        let after = epochs(&[("a", "osm-08-18", "ezu-0.7.1", "r13")]);

        let events = between(&before, &after, AT, Some("2026-08-24T08:00:00Z"));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].axis, Axis::Param);
        assert_eq!(events[0].epoch_from, "r12");
        assert_eq!(events[0].epoch_to, "r13");
        assert_eq!(events[0].tileset, "a");
        assert_eq!(events[0].scope, Scope::All);
        assert_eq!(events[0].deadline.as_deref(), Some("2026-08-24T08:00:00Z"));
    }

    #[test]
    fn an_untouched_tileset_did_not_die() {
        let before = epochs(&[("a", "s", "g", "p"), ("b", "s", "g", "p")]);
        let after = epochs(&[("a", "s", "g", "p2"), ("b", "s", "g", "p")]);

        let events = between(&before, &after, AT, None);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].tileset, "a");
    }

    /// Nothing was cached under a tileset that did not exist, so there is
    /// nothing to warm and no event to raise.
    #[test]
    fn a_new_tileset_is_not_an_invalidation() {
        let before = epochs(&[("a", "s", "g", "p")]);
        let after = epochs(&[("a", "s", "g", "p"), ("b", "s", "g", "p")]);

        assert!(between(&before, &after, AT, None).is_empty());
    }

    #[test]
    fn a_removed_tileset_raises_nothing_either() {
        let before = epochs(&[("a", "s", "g", "p"), ("b", "s", "g", "p")]);
        let after = epochs(&[("a", "s", "g", "p")]);

        assert!(between(&before, &after, AT, None).is_empty());
    }

    #[test]
    fn the_most_fundamental_axis_is_the_one_reported() {
        let before = epochs(&[("a", "osm-07", "ezu-0.7.1", "r12")]);
        let after = epochs(&[("a", "osm-08", "ezu-0.8.0", "r13")]);

        let events = between(&before, &after, AT, None);
        assert_eq!(events[0].axis, Axis::Source);
        assert_eq!(events[0].epoch_from, "osm-07");
    }

    #[test]
    fn several_tilesets_come_back_in_a_settled_order() {
        let before = epochs(&[("c", "s", "g", "p"), ("a", "s", "g", "p")]);
        let after = epochs(&[("c", "s", "g", "p2"), ("a", "s", "g", "p2")]);

        let events = between(&before, &after, AT, None);
        let tilesets: Vec<&str> = events.iter().map(|e| e.tileset.as_str()).collect();
        assert_eq!(tilesets, ["a", "c"]);
    }
}
