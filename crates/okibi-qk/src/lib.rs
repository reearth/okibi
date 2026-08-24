//! Quadkey projection.
//!
//! Services write `tile.qk` themselves, and they do not share a tiling scheme:
//! Terrain is TMS-Geographic, Buildings is 3D Tiles with size-bucket zooms,
//! Papers is Web Mercator. This crate is what lets all three land in one
//! comparable space, so a demand digest can be aggregated across services.
//!
//! Nothing here yet — see `spec/tile-demand.md` for what `tile.qk` must mean.
