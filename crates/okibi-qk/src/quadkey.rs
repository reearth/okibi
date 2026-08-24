use core::{cmp::Ordering, fmt, str::FromStr};

use crate::error::{Error, Result};

/// A Web Mercator quadkey: the normalised spatial key every service's tiles
/// are projected into.
///
/// Held as a level and a bit pattern rather than a string, two bits per level,
/// most significant first. The digits are the same either way; this way a
/// prefix test is a shift and a comparison, and the planner does a great many
/// of those.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Quadkey {
    level: u8,
    bits: u64,
}

impl Quadkey {
    /// Two bits per level in a `u64`. Deeper than any tile scheme in use, and
    /// far deeper than anything anyone generates on demand.
    pub const MAX_LEVEL: u8 = 32;

    /// The whole world: the zero-length quadkey.
    pub const ROOT: Quadkey = Quadkey { level: 0, bits: 0 };

    /// From a Web Mercator tile, y counted from the north.
    ///
    /// Tiles in other schemes reach a quadkey through their centre point
    /// instead — see [`crate::Scheme`] — because a tile that is not a Web
    /// Mercator tile has no coordinates to reinterpret.
    pub fn from_tile(level: u8, x: u32, y: u32) -> Result<Self> {
        Self::check_level(level)?;

        let side = 1u64 << level;
        if u64::from(x) >= side || u64::from(y) >= side {
            return Err(Error::OutOfGrid {
                level,
                x,
                y,
                columns: side,
                rows: side,
            });
        }

        let mut bits = 0u64;
        for i in 0..level {
            let shift = level - 1 - i;
            let digit = ((x >> shift) & 1) | (((y >> shift) & 1) << 1);
            bits = (bits << 2) | u64::from(digit);
        }
        Ok(Quadkey { level, bits })
    }

    /// The Web Mercator tile this addresses, as `(level, x, y)`.
    pub fn tile(&self) -> (u8, u32, u32) {
        let (mut x, mut y) = (0u32, 0u32);
        for i in 0..self.level {
            let digit = self.digit(i);
            x = (x << 1) | u32::from(digit & 1);
            y = (y << 1) | u32::from((digit >> 1) & 1);
        }
        (self.level, x, y)
    }

    pub fn level(&self) -> u8 {
        self.level
    }

    pub fn is_root(&self) -> bool {
        self.level == 0
    }

    /// The digit at `i`, counting from the shallowest.
    pub fn digit(&self, i: u8) -> u8 {
        debug_assert!(i < self.level);
        ((self.bits >> (2 * (self.level - 1 - i))) & 0b11) as u8
    }

    /// This quadkey cut back to `level`, or unchanged if it is already
    /// shallower. Cutting to zero gives [`Quadkey::ROOT`].
    pub fn truncate(&self, level: u8) -> Self {
        if level >= self.level {
            return *self;
        }
        Quadkey {
            level,
            bits: self.bits >> (2 * (self.level - level)),
        }
    }

    /// The eight-character form the digest aggregates by.
    pub fn qk8(&self) -> Self {
        self.truncate(8)
    }

    /// The tile one level up, or `None` at the root.
    pub fn parent(&self) -> Option<Self> {
        (!self.is_root()).then(|| self.truncate(self.level - 1))
    }

    /// Every ancestor, shallowest first, starting at the root and stopping
    /// short of this quadkey itself.
    ///
    /// Shallowest first because that is the order they are worth warming in:
    /// an ancestor covers more of what anyone asked for than any one of its
    /// descendants does.
    pub fn ancestors(&self) -> impl Iterator<Item = Quadkey> + '_ {
        (0..self.level).map(|level| self.truncate(level))
    }

    /// Whether `prefix` is this quadkey or an ancestor of it.
    ///
    /// This is how an invalidation scope is matched: `qk_prefixes` names
    /// regions, and a tile is in scope when one of them is on its way down.
    pub fn starts_with(&self, prefix: &Quadkey) -> bool {
        prefix.level <= self.level && self.truncate(prefix.level).bits == prefix.bits
    }

    fn check_level(level: u8) -> Result<()> {
        if level > Self::MAX_LEVEL {
            return Err(Error::LevelTooDeep {
                level,
                max: Self::MAX_LEVEL,
            });
        }
        Ok(())
    }
}

impl fmt::Display for Quadkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for i in 0..self.level {
            write!(f, "{}", self.digit(i))?;
        }
        Ok(())
    }
}

impl FromStr for Quadkey {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let level = u8::try_from(s.len()).map_err(|_| Error::LevelTooDeep {
            level: u8::MAX,
            max: Self::MAX_LEVEL,
        })?;
        Self::check_level(level)?;

        let mut bits = 0u64;
        for c in s.chars() {
            let digit = match c {
                '0'..='3' => u64::from(c as u8 - b'0'),
                found => return Err(Error::NotADigit { found }),
            };
            bits = (bits << 2) | digit;
        }
        Ok(Quadkey { level, bits })
    }
}

/// Ordered the way the digits read, so that sorting quadkeys sorts them the
/// same whether they are structs here or strings in a plan someone is reading.
/// A plan's ordering is part of what makes it reproducible, so the two must
/// not be allowed to differ.
impl Ord for Quadkey {
    fn cmp(&self, other: &Self) -> Ordering {
        let common = self.level.min(other.level);
        match self.truncate(common).bits.cmp(&other.truncate(common).bits) {
            Ordering::Equal => self.level.cmp(&other.level),
            unequal => unequal,
        }
    }
}

impl PartialOrd for Quadkey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
