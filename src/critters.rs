//! Small animals that den in sealed hollows of the mine. They can't dig, so
//! they only get out once something opens their den. After that they wander
//! the tunnels, bite the pod when it's in reach, and leave meat when shot.

use serde::{Deserialize, Serialize};

/// Half a critter's width and height, in tiles.
pub const HALF: f32 = 0.3;
/// Walking speed in tiles per second; well under the pod's.
pub const SPEED: f32 = 1.5;
/// Hull damage from one bite.
pub const BITE: f32 = 5.0;

/// Which animal this is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Mole,
    Beetle,
    Spider,
    Golem,
}

impl Kind {
    /// Who lives in a den in this depth band; `pick` is any random number.
    pub fn for_band(band: usize, pick: u64) -> Kind {
        if band <= 1 {
            return if pick.is_multiple_of(2) {
                Kind::Mole
            } else {
                Kind::Beetle
            };
        }
        match pick % 3 {
            0 => Kind::Spider,
            1 => Kind::Golem,
            _ => Kind::Beetle,
        }
    }

    /// Sprite name prefix; frames are `{prefix}_0` and `{prefix}_1`.
    pub fn sprite(self) -> &'static str {
        match self {
            Kind::Mole => "critter_mole",
            Kind::Beetle => "critter_beetle",
            Kind::Spider => "critter_spider",
            Kind::Golem => "critter_golem",
        }
    }
}

/// One critter out of its den.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Critter {
    pub kind: Kind,
    pub x: f32,
    pub y: f32,
    pub vy: f32,
    pub dir: f32,
}

impl Critter {
    /// Whether its body overlaps a box centered at (x, y).
    pub fn touches(&self, x: f32, y: f32, half: f32) -> bool {
        (self.x - x).abs() < HALF + half && (self.y - y).abs() < HALF + half
    }

    /// Whether a beam along row `y` from x0 to x1 passes through it.
    pub fn in_beam(&self, x0: f32, x1: f32, y: f32) -> bool {
        let (lo, hi) = (x0.min(x1), x0.max(x1));
        (self.y - y).abs() < HALF + 0.3 && self.x + HALF >= lo && self.x - HALF <= hi
    }
}
