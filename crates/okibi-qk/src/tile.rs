use crate::{
    error::{Error, Result},
    quadkey::Quadkey,
};

/// The northern and southern edge of Web Mercator, where the projection stops
/// being finite.
pub const MERCATOR_MAX_LAT: f64 = 85.051_128_779_806_59;

/// A point, in degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LonLat {
    pub lon: f64,
    pub lat: f64,
}

impl LonLat {
    pub fn new(lon: f64, lat: f64) -> Result<Self> {
        if !(-180.0..=180.0).contains(&lon) || !(-90.0..=90.0).contains(&lat) {
            return Err(Error::NotOnEarth { lon, lat });
        }
        Ok(LonLat { lon, lat })
    }

    /// The Web Mercator tile containing this point at `level`.
    ///
    /// Latitude is clamped to the projection's limit rather than refused: a
    /// tile scheme that reaches the poles has tiles there, and they should
    /// land in the northernmost or southernmost Mercator row rather than fail.
    pub fn quadkey(&self, level: u8) -> Result<Quadkey> {
        if level == 0 {
            return Ok(Quadkey::ROOT);
        }
        let side = 2f64.powi(i32::from(level));

        let lat = self.lat.clamp(-MERCATOR_MAX_LAT, MERCATOR_MAX_LAT);
        let sin = lat.to_radians().sin();

        let fx = (self.lon + 180.0) / 360.0;
        let fy = 0.5 - ((1.0 + sin) / (1.0 - sin)).ln() / (4.0 * core::f64::consts::PI);

        let last = side - 1.0;
        let x = (fx * side).floor().clamp(0.0, last) as u32;
        let y = (fy * side).floor().clamp(0.0, last) as u32;
        Quadkey::from_tile(level, x, y)
    }
}

/// How a service numbers its tiles.
///
/// These are not interchangeable coordinate systems with a conversion between
/// them: a geographic tile and a Mercator tile at the same numbers are
/// different pieces of ground. What they share is a centre point, which is why
/// that is the route to a quadkey.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    /// Web Mercator, `2^z` square, y counted from the north. Slippy maps, and
    /// the 3D Tiles tilesets that subdivide a Mercator region.
    WebMercator,
    /// Web Mercator, y counted from the south.
    WebMercatorTms,
    /// Geographic (EPSG:4326): `2^(z+1)` columns by `2^z` rows over the whole
    /// ellipsoid, y from the north.
    Geographic,
    /// Geographic, y from the south. What Cesium's terrain tiles use.
    GeographicTms,
}

impl Scheme {
    /// The grid at `level`, as `(columns, rows)`.
    pub fn grid(&self, level: u8) -> (u64, u64) {
        let side = 1u64 << level;
        match self {
            Scheme::WebMercator | Scheme::WebMercatorTms => (side, side),
            Scheme::Geographic | Scheme::GeographicTms => (side * 2, side),
        }
    }

    fn y_from_north(&self, level: u8, y: u32) -> u32 {
        match self {
            Scheme::WebMercator | Scheme::Geographic => y,
            Scheme::WebMercatorTms | Scheme::GeographicTms => {
                let (_, rows) = self.grid(level);
                (rows - 1 - u64::from(y)) as u32
            }
        }
    }
}

/// A tile as the service that served it numbered it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tile {
    pub scheme: Scheme,
    pub level: u8,
    pub x: u32,
    pub y: u32,
}

impl Tile {
    pub fn new(scheme: Scheme, level: u8, x: u32, y: u32) -> Result<Self> {
        if level > Quadkey::MAX_LEVEL {
            return Err(Error::LevelTooDeep {
                level,
                max: Quadkey::MAX_LEVEL,
            });
        }
        let (columns, rows) = scheme.grid(level);
        if u64::from(x) >= columns || u64::from(y) >= rows {
            return Err(Error::OutOfGrid {
                level,
                x,
                y,
                columns,
                rows,
            });
        }
        Ok(Tile {
            scheme,
            level,
            x,
            y,
        })
    }

    /// The centre of the tile.
    pub fn center(&self) -> LonLat {
        let (columns, rows) = self.scheme.grid(self.level);
        let y = self.scheme.y_from_north(self.level, self.y);

        let fx = (f64::from(self.x) + 0.5) / columns as f64;
        let fy = (f64::from(y) + 0.5) / rows as f64;

        let lon = fx * 360.0 - 180.0;
        let lat = match self.scheme {
            Scheme::WebMercator | Scheme::WebMercatorTms => {
                let n = core::f64::consts::PI * (1.0 - 2.0 * fy);
                n.sinh().atan().to_degrees()
            }
            Scheme::Geographic | Scheme::GeographicTms => 90.0 - fy * 180.0,
        };
        LonLat { lon, lat }
    }

    /// This tile's centre as a quadkey of `level` digits.
    ///
    /// The level is the caller's to choose, and services choose their own
    /// native zoom: what the digest compares is where the demand is, not how
    /// deep two unrelated schemes happen to number it.
    pub fn quadkey(&self, level: u8) -> Result<Quadkey> {
        self.center().quadkey(level)
    }

    /// The usual case: a quadkey as deep as the tile's own level.
    pub fn quadkey_at_own_level(&self) -> Result<Quadkey> {
        self.quadkey(self.level)
    }
}
