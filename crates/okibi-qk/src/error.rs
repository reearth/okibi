use core::fmt;

/// What can be wrong with a quadkey or a tile coordinate.
#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// A level past what a quadkey can address here.
    LevelTooDeep { level: u8, max: u8 },
    /// A character that is not a quadkey digit.
    NotADigit { found: char },
    /// A tile coordinate outside the grid its level and scheme define.
    OutOfGrid {
        level: u8,
        x: u32,
        y: u32,
        columns: u64,
        rows: u64,
    },
    /// A latitude or longitude outside the ranges they are defined over.
    NotOnEarth { lon: f64, lat: f64 },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::LevelTooDeep { level, max } => {
                write!(f, "level {level} is deeper than {max}")
            }
            Error::NotADigit { found } => {
                write!(f, "{found:?} is not a quadkey digit (0-3)")
            }
            Error::OutOfGrid {
                level,
                x,
                y,
                columns,
                rows,
            } => write!(
                f,
                "tile {level}/{x}/{y} is outside the {columns}x{rows} grid at that level"
            ),
            Error::NotOnEarth { lon, lat } => {
                write!(f, "({lon}, {lat}) is not a longitude and latitude")
            }
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = core::result::Result<T, Error>;
