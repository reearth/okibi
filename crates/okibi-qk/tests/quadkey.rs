//! The quadkey itself: digits, prefixes, ordering, parsing.

use std::str::FromStr;

use okibi_qk::{Error, Quadkey};

/// The example Microsoft's own tile-system documentation gives. It is the one
/// value in here not derived from this crate's own arithmetic, which is what
/// makes it worth having.
#[test]
fn matches_the_published_example() {
    assert_eq!(Quadkey::from_tile(3, 3, 5).unwrap().to_string(), "213");
}

#[test]
fn tiles_and_quadkeys_are_the_same_thing() {
    let qk = Quadkey::from_tile(14, 14552, 6451).unwrap();
    assert_eq!(qk.tile(), (14, 14552, 6451));
    assert_eq!(Quadkey::from_str(&qk.to_string()).unwrap(), qk);
}

#[test]
fn the_root_is_the_whole_world() {
    let root = Quadkey::from_tile(0, 0, 0).unwrap();
    assert_eq!(root, Quadkey::ROOT);
    assert_eq!(root.to_string(), "");
    assert!(root.is_root());
    assert_eq!(root.parent(), None);
    assert_eq!(root.ancestors().count(), 0);
}

#[test]
fn truncating_keeps_the_shallow_digits() {
    let qk = Quadkey::from_str("13300211231022").unwrap();
    assert_eq!(qk.truncate(8).to_string(), "13300211");
    assert_eq!(qk.qk8().to_string(), "13300211");
    assert_eq!(qk.truncate(0), Quadkey::ROOT);
}

/// The digest aggregates by eight characters, and says a shorter quadkey is
/// its own qk8 rather than something padded.
#[test]
fn qk8_of_something_shallower_is_itself() {
    let qk = Quadkey::from_str("1330").unwrap();
    assert_eq!(qk.qk8(), qk);
}

#[test]
fn ancestors_run_shallowest_first() {
    let qk = Quadkey::from_str("1302").unwrap();
    let ancestors: Vec<String> = qk.ancestors().map(|a| a.to_string()).collect();
    assert_eq!(ancestors, ["", "1", "13", "130"]);
}

#[test]
fn a_prefix_is_an_ancestor_or_itself() {
    let qk = Quadkey::from_str("133002").unwrap();

    assert!(qk.starts_with(&Quadkey::from_str("133").unwrap()));
    assert!(qk.starts_with(&qk));
    assert!(qk.starts_with(&Quadkey::ROOT));

    assert!(!qk.starts_with(&Quadkey::from_str("132").unwrap()));
    // Deeper than the tile, so it cannot be on the way down to it.
    assert!(!qk.starts_with(&Quadkey::from_str("1330021").unwrap()));
}

/// A plan sorts by quadkey and is compared byte for byte, so the order these
/// sort in has to be the order they read in.
#[test]
fn ordering_matches_the_written_form() {
    let mut keys: Vec<Quadkey> = ["13", "1", "", "132", "130", "2", "0"]
        .iter()
        .map(|s| Quadkey::from_str(s).unwrap())
        .collect();
    keys.sort();

    let written: Vec<String> = keys.iter().map(|k| k.to_string()).collect();
    let mut expected = written.clone();
    expected.sort();

    assert_eq!(written, expected);
    assert_eq!(written, ["", "0", "1", "13", "130", "132", "2"]);
}

#[test]
fn refuses_what_is_not_a_quadkey() {
    assert_eq!(
        Quadkey::from_str("1334"),
        Err(Error::NotADigit { found: '4' })
    );
    assert_eq!(
        Quadkey::from_str("13x0"),
        Err(Error::NotADigit { found: 'x' })
    );
}

#[test]
fn refuses_depth_it_cannot_address() {
    let too_deep = "0".repeat(usize::from(Quadkey::MAX_LEVEL) + 1);
    assert_eq!(
        Quadkey::from_str(&too_deep),
        Err(Error::LevelTooDeep {
            level: Quadkey::MAX_LEVEL + 1,
            max: Quadkey::MAX_LEVEL,
        })
    );
    assert!(Quadkey::from_tile(Quadkey::MAX_LEVEL + 1, 0, 0).is_err());
}

#[test]
fn refuses_a_tile_outside_its_level() {
    assert!(Quadkey::from_tile(2, 4, 0).is_err());
    assert!(Quadkey::from_tile(2, 0, 4).is_err());
    assert!(Quadkey::from_tile(2, 3, 3).is_ok());
}

#[test]
fn addresses_the_deepest_level_it_claims_to() {
    let level = Quadkey::MAX_LEVEL;
    let far = u32::MAX;
    let qk = Quadkey::from_tile(level, far, far).unwrap();
    assert_eq!(qk.tile(), (level, far, far));
    assert_eq!(qk.to_string().len(), usize::from(level));
}
