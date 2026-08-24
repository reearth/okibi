//! Quadkey projection.
//!
//! Services write `tile.qk` themselves, and they do not share a tiling
//! scheme: Terrain is geographic, Buildings subdivides Web Mercator with zoom
//! standing in for tile size, Papers is a slippy map. Their tile coordinates
//! are not comparable and cannot be aggregated together.
//!
//! What they do share is ground. A tile's centre point projected into one
//! quadkey space is comparable across all three, and that is all a demand
//! digest needs to aggregate by space and all the planner needs to intersect
//! demand with an invalidation scope.
//!
//! ```
//! use okibi_qk::{Scheme, Tile};
//!
//! // A Papers tile over Tokyo.
//! let tile = Tile::new(Scheme::WebMercator, 14, 14552, 6451)?;
//! let qk = tile.quadkey_at_own_level()?;
//!
//! assert_eq!(qk.to_string(), "13300211231022");
//! assert_eq!(qk.qk8().to_string(), "13300211");
//! # Ok::<(), okibi_qk::Error>(())
//! ```
//!
//! What is not here is any way to enumerate a quadkey's descendants. The
//! planner never sweeps a cell downward — depth nobody has requested is the
//! long tail, and the long tail is what on-demand generation is for — so
//! nothing would call it.

mod error;
mod quadkey;
mod tile;

pub use error::{Error, Result};
pub use quadkey::Quadkey;
pub use tile::{LonLat, MERCATOR_MAX_LAT, Scheme, Tile};
