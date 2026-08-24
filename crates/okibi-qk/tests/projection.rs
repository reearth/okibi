//! Getting from a service's own tile numbering into the one shared space.

use okibi_qk::{LonLat, MERCATOR_MAX_LAT, Quadkey, Scheme, Tile};
use proptest::prelude::*;

/// Roughly Tokyo, which every service in scope serves and which is nowhere
/// near a pole or a meridian where things get interesting.
const TOKYO: LonLat = LonLat {
    lon: 139.7671,
    lat: 35.6812,
};

fn near(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() < tolerance
}

/// The claim the whole vocabulary rests on: three services numbering the same
/// ground three different ways land in one digest cell.
#[test]
fn different_schemes_agree_about_where_tokyo_is() {
    // Papers: a slippy map at z14.
    let papers = Tile::new(Scheme::WebMercator, 14, 14552, 6451).unwrap();
    // Buildings: Web Mercator subdivision, zoom standing in for tile size.
    let buildings = Tile::new(Scheme::WebMercator, 13, 7276, 3225).unwrap();
    // Terrain: Cesium's geographic tiling, two root tiles wide, y from south.
    let terrain = Tile::new(Scheme::GeographicTms, 14, 29108, 11439).unwrap();

    for tile in [papers, buildings, terrain] {
        let center = tile.center();
        assert!(near(center.lon, TOKYO.lon, 0.05), "{center:?}");
        assert!(near(center.lat, TOKYO.lat, 0.05), "{center:?}");

        let qk8 = tile.quadkey_at_own_level().unwrap().qk8();
        assert_eq!(qk8.to_string(), "13300211", "{tile:?}");
    }
}

#[test]
fn the_two_geographic_origins_mirror_each_other() {
    let (_, rows) = Scheme::GeographicTms.grid(14);
    let from_south = Tile::new(Scheme::GeographicTms, 14, 29108, 11439).unwrap();
    let from_north = Tile::new(Scheme::Geographic, 14, 29108, (rows - 1 - 11439) as u32).unwrap();

    assert_eq!(from_south.center(), from_north.center());
}

#[test]
fn geographic_is_twice_as_wide_as_it_is_tall() {
    assert_eq!(Scheme::Geographic.grid(3), (16, 8));
    assert_eq!(Scheme::WebMercator.grid(3), (8, 8));

    // The extra column exists, and the same number is out of the grid in
    // Mercator.
    assert!(Tile::new(Scheme::Geographic, 3, 15, 7).is_ok());
    assert!(Tile::new(Scheme::WebMercator, 3, 15, 7).is_err());
}

#[test]
fn the_corners_of_the_world_are_where_they_should_be() {
    let nw = Tile::new(Scheme::Geographic, 1, 0, 0).unwrap().center();
    assert!(near(nw.lon, -135.0, 1e-9), "{nw:?}");
    assert!(near(nw.lat, 45.0, 1e-9), "{nw:?}");

    let se = Tile::new(Scheme::Geographic, 1, 3, 1).unwrap().center();
    assert!(near(se.lon, 135.0, 1e-9), "{se:?}");
    assert!(near(se.lat, -45.0, 1e-9), "{se:?}");

    // Mercator's rows are not evenly spaced in latitude, so its top row's
    // centre sits far north of the geographic one's.
    let mercator_nw = Tile::new(Scheme::WebMercator, 1, 0, 0).unwrap().center();
    assert!(near(mercator_nw.lon, -90.0, 1e-9), "{mercator_nw:?}");
    assert!(mercator_nw.lat > 66.0, "{mercator_nw:?}");
}

/// Geographic tiles reach the poles and Mercator does not, so a polar tile's
/// centre has to be pulled to the edge of the projection rather than refused —
/// otherwise Terrain could not write `tile.qk` for the tiles it serves there.
#[test]
fn polar_tiles_land_in_the_outermost_mercator_row() {
    // Level 8, because a shallower top row's centre is not yet past the
    // projection's limit: at level z it sits at 90 - 90/2^z degrees.
    const LEVEL: u8 = 8;
    let rows = 1u32 << LEVEL;

    let pole = Tile::new(Scheme::Geographic, LEVEL, 0, 0).unwrap();
    assert!(pole.center().lat > MERCATOR_MAX_LAT, "{:?}", pole.center());

    let (_, _, y) = pole.quadkey_at_own_level().unwrap().tile();
    assert_eq!(y, 0, "the northernmost row");

    let south = Tile::new(Scheme::Geographic, LEVEL, 0, rows - 1).unwrap();
    assert!(
        south.center().lat < -MERCATOR_MAX_LAT,
        "{:?}",
        south.center()
    );

    let (_, _, y) = south.quadkey_at_own_level().unwrap().tile();
    assert_eq!(y, rows - 1, "the southernmost row");
}

#[test]
fn a_point_on_the_antimeridian_stays_in_the_grid() {
    for level in [1u8, 8, 20] {
        let side = 1u32 << level;
        let east = LonLat::new(180.0, 0.0).unwrap().quadkey(level).unwrap();
        assert_eq!(east.tile().1, side - 1);

        let west = LonLat::new(-180.0, 0.0).unwrap().quadkey(level).unwrap();
        assert_eq!(west.tile().1, 0);
    }
}

#[test]
fn refuses_a_point_that_is_not_on_earth() {
    assert!(LonLat::new(181.0, 0.0).is_err());
    assert!(LonLat::new(0.0, 91.0).is_err());
    assert!(LonLat::new(180.0, 90.0).is_ok());
}

#[test]
fn projecting_to_the_root_gives_the_root() {
    let tile = Tile::new(Scheme::GeographicTms, 14, 29108, 11439).unwrap();
    assert_eq!(tile.quadkey(0).unwrap(), Quadkey::ROOT);
}

proptest! {
    /// A Mercator tile's centre is inside itself, so projecting it at the
    /// tile's own level has to give that tile back. If this ever fails, every
    /// digest cell is subtly in the wrong place.
    #[test]
    fn a_mercator_tile_projects_to_itself(level in 0u8..=20, seed in 0u64..u64::MAX) {
        let side = 1u32 << level;
        let x = (seed % u64::from(side)) as u32;
        let y = ((seed >> 32) % u64::from(side)) as u32;

        let tile = Tile::new(Scheme::WebMercator, level, x, y).unwrap();
        prop_assert_eq!(tile.quadkey_at_own_level().unwrap().tile(), (level, x, y));
    }

    /// Whatever the scheme, a tile's centre is a real place.
    #[test]
    fn every_centre_is_on_earth(
        level in 0u8..=18,
        seed in 0u64..u64::MAX,
        scheme in prop::sample::select(vec![
            Scheme::WebMercator,
            Scheme::WebMercatorTms,
            Scheme::Geographic,
            Scheme::GeographicTms,
        ]),
    ) {
        let (columns, rows) = scheme.grid(level);
        let x = (seed % columns) as u32;
        let y = ((seed >> 32) % rows) as u32;

        let center = Tile::new(scheme, level, x, y).unwrap().center();
        prop_assert!(LonLat::new(center.lon, center.lat).is_ok(), "{:?}", center);
    }

    /// Truncation is what the digest aggregates by, so a cell has to contain
    /// the tiles it claims to.
    #[test]
    fn a_tile_is_inside_every_cell_it_truncates_to(level in 1u8..=20, cut in 0u8..=20, seed in 0u64..u64::MAX) {
        let side = 1u32 << level;
        let x = (seed % u64::from(side)) as u32;
        let y = ((seed >> 32) % u64::from(side)) as u32;

        let qk = Quadkey::from_tile(level, x, y).unwrap();
        prop_assert!(qk.starts_with(&qk.truncate(cut)));
    }
}
