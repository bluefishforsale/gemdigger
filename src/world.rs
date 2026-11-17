//! The endless mine. Every tile comes from a seeded hash, so any depth exists
//! without being stored; only the tiles the player changed are kept and saved.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Columns in the mine; everything outside is bedrock wall.
pub const WIDTH: i32 = 512;
/// Depth (in tiles) where each band starts: topsoil, clay, stone, deep, abyss.
pub const BAND_STARTS: [i32; 5] = [0, 50, 150, 300, 500];
/// Depth where each ore tier first shows up.
const ORE_UNLOCK: [i32; 6] = [0, 20, 60, 140, 260, 420];

/// One cell of the mine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tile {
    Air,
    Dirt,
    Boulder,
    Ore(u8),
    Mineral(Mineral),
    Wall,
}

/// Bomb ingredients. Worth little on their own; the shop crafts them into bombs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mineral {
    Coal,
    Sulfur,
    Potassium,
}

impl Mineral {
    pub const ALL: [Mineral; 3] = [Mineral::Coal, Mineral::Sulfur, Mineral::Potassium];

    pub fn name(self) -> &'static str {
        match self {
            Mineral::Coal => "Coal",
            Mineral::Sulfur => "Sulfur",
            Mineral::Potassium => "Potassium",
        }
    }

    /// Shallowest row it appears in; deeper means rarer and harder to reach.
    pub fn depth(self) -> i32 {
        match self {
            Mineral::Coal => 10,
            Mineral::Sulfur => 90,
            Mineral::Potassium => 180,
        }
    }
}

impl Tile {
    /// Whether the pod collides with this tile.
    pub fn is_solid(self) -> bool {
        self != Tile::Air
    }

    /// Whether the drill can break this tile.
    pub fn is_diggable(self) -> bool {
        matches!(self, Tile::Dirt | Tile::Ore(_) | Tile::Mineral(_))
    }
}

/// Which depth band a row belongs to, 0 (topsoil) to 4 (abyss).
pub fn band(y: i32) -> usize {
    BAND_STARTS.iter().rposition(|&s| y >= s).unwrap_or(0)
}

/// Band as a continuous value from 0.0 to 4.0, for crossfading music and color.
pub fn band_position(depth: f32) -> f32 {
    for i in 0..BAND_STARTS.len() - 1 {
        let (a, b) = (BAND_STARTS[i] as f32, BAND_STARTS[i + 1] as f32);
        if depth < b {
            return i as f32 + ((depth - a) / (b - a)).max(0.0);
        }
    }
    (BAND_STARTS.len() - 1) as f32
}

/// Deterministic 64-bit hash of a seed, a cell, and a salt (splitmix64 finish).
pub fn hash(seed: u64, x: i32, y: i32, salt: u64) -> u64 {
    let mut z = seed
        ^ (x as u32 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (y as u32 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ salt.wrapping_mul(0x1656_67B1_9E37_79F9);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Maps a hash to a float in [0, 1).
pub fn unit(h: u64) -> f32 {
    (h >> 40) as f32 / (1u64 << 24) as f32
}

/// Rows between deep outposts, on average; each is 150 to 250 below the last.
pub const OUTPOST_SPACING: i32 = 200;
/// Half the width of an outpost's cavern, in tiles.
pub const OUTPOST_HALF: i32 = 4;
const OUTPOST_HEIGHT: i32 = 4;

/// Where outpost `k` (1 = shallowest) stands: its center column and the row
/// it's built on. NOTE: x follows a slow wave plus a little jitter so each
/// outpost is at most 59 columns sideways from the one above; that keeps the
/// next one in reach on a single tank. Pure function of the seed, so any tile
/// can ask in constant time.
pub fn outpost(seed: u64, k: i32) -> Option<(i32, i32)> {
    if k < 1 {
        return None;
    }
    let h = hash(seed, k, 0, 40);
    let phase = unit(hash(seed, 0, 0, 41)) * std::f32::consts::TAU;
    let wave = 135.0 * (0.35 * k as f32 + phase).sin();
    let jitter = (h % 13) as i32 - 6;
    let x = WIDTH / 2 + wave as i32 + jitter;
    let y = OUTPOST_SPACING * k + ((h >> 16) % 51) as i32 - 25;
    Some((x, y))
}

/// The outposts whose caverns could reach row `y`.
pub fn outposts_near(seed: u64, y: i32) -> impl Iterator<Item = (i32, i32)> {
    let k = (y as f32 / OUTPOST_SPACING as f32).round() as i32;
    (k - 1..=k + 1).filter_map(move |k| outpost(seed, k))
}

/// Whether (x, y) is an outpost's floor, which never breaks.
pub fn is_outpost_floor(seed: u64, x: i32, y: i32) -> bool {
    outposts_near(seed, y).any(|(ox, oy)| {
        (ox - OUTPOST_HALF..ox + OUTPOST_HALF).contains(&x) && (oy..=oy + 1).contains(&y)
    })
}

fn in_outpost_cavern(seed: u64, x: i32, y: i32) -> bool {
    outposts_near(seed, y).any(|(ox, oy)| {
        (ox - OUTPOST_HALF..ox + OUTPOST_HALF).contains(&x)
            && (oy - OUTPOST_HEIGHT..oy).contains(&y)
    })
}

/// Side of the square cells the mine is divided into. Each holds at most one
/// den or crate, which is what keeps both rare.
pub const CELL: i32 = 32;
/// Width and height of a den, in tiles.
pub const DEN_SIZE: (i32, i32) = (3, 2);
/// Nothing is sealed in the rock above this row.
const POCKET_MIN_DEPTH: i32 = 12;
const DEN_CHANCE: f32 = 0.3;
/// Twice the den chance: crates are half as rare as dens.
const CRATE_CHANCE: f32 = 0.6;
/// Width and height of a ruin's chamber, in tiles.
pub const RUIN_SIZE: (i32, i32) = (6, 4);
/// Ruins are the rarest find, and never near the surface.
const RUIN_CHANCE: f32 = 0.06;
const RUIN_MIN_DEPTH: i32 = 30;

/// A small hollow sealed in the rock until something digs into it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pocket {
    /// A cave critters live in; (x, y) is its top-left tile.
    Den { x: i32, y: i32 },
    /// A supply crate in a hollow one tile big.
    Crate { x: i32, y: i32 },
    /// A buried chamber with a ruin and its relic; (x, y) is its top-left tile.
    Ruin { x: i32, y: i32 },
}

impl Pocket {
    /// Top-left tile, then width and height.
    fn rect(self) -> (i32, i32, i32, i32) {
        match self {
            Pocket::Den { x, y } => (x, y, DEN_SIZE.0, DEN_SIZE.1),
            Pocket::Crate { x, y } => (x, y, 1, 1),
            Pocket::Ruin { x, y } => (x, y, RUIN_SIZE.0, RUIN_SIZE.1),
        }
    }

    fn contains(self, tx: i32, ty: i32) -> bool {
        let (x, y, w, h) = self.rect();
        (x..x + w).contains(&tx) && (y..y + h).contains(&ty)
    }
}

/// The cell a tile belongs to.
pub fn cell_of(x: i32, y: i32) -> (i32, i32) {
    (x.div_euclid(CELL), y.div_euclid(CELL))
}

fn raw_pocket(seed: u64, cx: i32, cy: i32) -> Option<Pocket> {
    if !(0..WIDTH / CELL).contains(&cx) || cy < 0 {
        return None;
    }
    let h = hash(seed, cx, cy, 60);
    let r = unit(h);
    // NOTE: one roll, cut into slices. A new kind takes the unused top of the
    // range so the dens and crates of an existing mine stay where they were.
    let (w, rows, min_depth) = match r {
        r if r < DEN_CHANCE => (DEN_SIZE.0, DEN_SIZE.1, POCKET_MIN_DEPTH),
        r if r < DEN_CHANCE + CRATE_CHANCE => (1, 1, POCKET_MIN_DEPTH),
        r if r < DEN_CHANCE + CRATE_CHANCE + RUIN_CHANCE => {
            (RUIN_SIZE.0, RUIN_SIZE.1, RUIN_MIN_DEPTH)
        }
        _ => return None,
    };
    // One tile in from the cell's edge on every side, so pockets never touch
    // each other or the walls of the mine.
    let x = cx * CELL + 1 + ((h >> 8) % (CELL - w - 1) as u64) as i32;
    let y = cy * CELL + 1 + ((h >> 16) % (CELL - rows - 1) as u64) as i32;
    if y < min_depth {
        return None;
    }
    Some(match (w, rows) {
        DEN_SIZE => Pocket::Den { x, y },
        RUIN_SIZE => Pocket::Ruin { x, y },
        _ => Pocket::Crate { x, y },
    })
}

/// The den, crate or ruin in cell (cx, cy), if it has one. NOTE: `generate` lets an
/// outpost win any overlap, so a pocket touching one is dropped here; without
/// that the game would look for a crate inside an outpost's floor.
pub fn pocket(seed: u64, cx: i32, cy: i32) -> Option<Pocket> {
    let p = raw_pocket(seed, cx, cy)?;
    let (x, y, w, h) = p.rect();
    let clear = (y..y + h).all(|ty| {
        (x..x + w).all(|tx| !in_outpost_cavern(seed, tx, ty) && !is_outpost_floor(seed, tx, ty))
    });
    clear.then_some(p)
}

fn in_pocket(seed: u64, x: i32, y: i32) -> bool {
    let (cx, cy) = cell_of(x, y);
    raw_pocket(seed, cx, cy).is_some_and(|p| p.contains(x, y))
}

fn in_tunnel(seed: u64, x: i32, y: i32) -> bool {
    if y < 40 {
        return false;
    }
    let h = hash(seed, 0, y, 2);
    if !h.is_multiple_of(7) {
        return false;
    }
    let start = ((h >> 8) % WIDTH as u64) as i32;
    let len = 6 + ((h >> 20) % 18) as i32;
    (start..start + len).contains(&x)
}

fn ore_tier(seed: u64, x: i32, y: i32) -> u8 {
    let max = ORE_UNLOCK.iter().rposition(|&u| y >= u).unwrap_or(0) as u8;
    let r = unit(hash(seed, x, y, 3));
    let below_max = match r {
        r if r < 0.30 => 1,
        r if r < 0.55 => 0,
        r if r < 0.80 => 2,
        _ => 3,
    };
    max.saturating_sub(below_max)
}

fn generate(seed: u64, x: i32, y: i32) -> Tile {
    if y < 0 {
        return Tile::Air;
    }
    if !(0..WIDTH).contains(&x) {
        return Tile::Wall;
    }
    if y < 2 {
        return Tile::Dirt;
    }
    if in_outpost_cavern(seed, x, y) {
        return Tile::Air;
    }
    if is_outpost_floor(seed, x, y) {
        return Tile::Dirt;
    }
    if in_tunnel(seed, x, y) || in_pocket(seed, x, y) {
        return Tile::Air;
    }
    let d = y as f32;
    let r = unit(hash(seed, x, y, 1));
    let cave = (d / 2000.0).min(0.05);
    let boulder = ((d - 8.0) / 2500.0).clamp(0.0, 0.12);
    let ore = 0.09 + (d / 6000.0).min(0.04);
    if r < cave {
        Tile::Air
    } else if r < cave + boulder {
        Tile::Boulder
    } else if r < cave + boulder + ore {
        Tile::Ore(ore_tier(seed, x, y))
    } else {
        mineral(seed, x, y).map_or(Tile::Dirt, Tile::Mineral)
    }
}

// Minerals only replace what would have been plain dirt, using their own hash,
// so adding them didn't move any existing ore, boulder or cave.
fn mineral(seed: u64, x: i32, y: i32) -> Option<Mineral> {
    let m = unit(hash(seed, x, y, 4));
    let kind = match m {
        m if m < 0.008 => Mineral::Potassium,
        m if m < 0.022 => Mineral::Sulfur,
        m if m < 0.06 => Mineral::Coal,
        _ => return None,
    };
    (y >= kind.depth()).then_some(kind)
}

/// The mine: a seed plus every tile the player has changed.
#[derive(Clone, Debug)]
pub struct World {
    pub seed: u64,
    changes: HashMap<(i32, i32), Tile>,
}

impl World {
    pub fn new(seed: u64) -> World {
        World {
            seed,
            changes: HashMap::new(),
        }
    }

    /// Rebuilds a world from a seed and its saved changes.
    pub fn from_changes(seed: u64, changes: impl IntoIterator<Item = (i32, i32, Tile)>) -> World {
        let mut world = World::new(seed);
        for (x, y, t) in changes {
            world.set(x, y, t);
        }
        world
    }

    pub fn get(&self, x: i32, y: i32) -> Tile {
        self.changes
            .get(&(x, y))
            .copied()
            .unwrap_or_else(|| generate(self.seed, x, y))
    }

    pub fn set(&mut self, x: i32, y: i32, tile: Tile) {
        if generate(self.seed, x, y) == tile {
            self.changes.remove(&(x, y));
        } else {
            self.changes.insert((x, y), tile);
        }
    }

    /// Every changed tile, sorted so the same world always saves the same bytes.
    pub fn changes(&self) -> Vec<(i32, i32, Tile)> {
        let mut out: Vec<_> = self.changes.iter().map(|(&(x, y), &t)| (x, y, t)).collect();
        out.sort_unstable_by_key(|&(x, y, _)| (y, x));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same seed must always produce the same mine, or a reload reshuffles it.
    #[test]
    fn generation_is_deterministic_per_seed() {
        let (a, b) = (World::new(42), World::new(42));
        for y in 0..600 {
            for x in 0..WIDTH {
                assert_eq!(a.get(x, y), b.get(x, y), "tile ({x},{y}) differs");
            }
        }
    }

    /// A dug tile must stay dug even though generation says otherwise.
    #[test]
    fn changes_override_generation() {
        let mut w = World::new(7);
        w.set(5, 30, Tile::Air);
        assert_eq!(w.get(5, 30), Tile::Air);
        let rebuilt = World::from_changes(7, w.changes());
        assert_eq!(
            rebuilt.get(5, 30),
            Tile::Air,
            "change lost in save round trip"
        );
    }

    /// Setting a tile back to its generated value must not bloat the save.
    #[test]
    fn restoring_generated_tile_drops_the_change() {
        let mut w = World::new(7);
        let original = w.get(3, 10);
        w.set(3, 10, Tile::Air);
        w.set(3, 10, original);
        assert!(
            w.changes().is_empty(),
            "stale change kept: {:?}",
            w.changes()
        );
    }

    /// Sky is open and the sides are walls, so the pod can't leave the mine.
    #[test]
    fn sky_is_air_and_sides_are_walls() {
        let w = World::new(1);
        assert_eq!(w.get(10, -1), Tile::Air);
        assert_eq!(w.get(-1, 5), Tile::Wall);
        assert_eq!(w.get(WIDTH, 5), Tile::Wall);
    }

    /// The map goes on forever: very deep rows still generate diggable ground.
    #[test]
    fn deep_rows_still_have_diggable_ground() {
        let w = World::new(3);
        let diggable = (0..WIDTH)
            .filter(|&x| w.get(x, 100_000).is_diggable())
            .count();
        assert!(
            diggable > WIDTH as usize / 2,
            "only {diggable} diggable tiles at 100 km"
        );
    }

    /// Valuable ore only appears once its depth is reached.
    #[test]
    fn ore_tiers_respect_unlock_depth() {
        let w = World::new(9);
        for y in 0..ORE_UNLOCK[1] {
            for x in 0..WIDTH {
                if let Tile::Ore(t) = w.get(x, y) {
                    assert_eq!(t, 0, "tier {t} ore at shallow depth {y}");
                }
            }
        }
    }

    /// Bomb ingredients stay below their depth, so bombs stay hard to get.
    #[test]
    fn minerals_respect_their_depth() {
        let w = World::new(21);
        let mut seen = [false; 3];
        for y in 0..400 {
            for x in 0..WIDTH {
                if let Tile::Mineral(m) = w.get(x, y) {
                    assert!(y >= m.depth(), "{m:?} at {y} m, above {} m", m.depth());
                    seen[m as usize] = true;
                }
            }
        }
        assert_eq!(seen, [true; 3], "some mineral never generated");
    }

    /// Each outpost is 150-250 rows below the last and at most 59 columns
    /// sideways, so one tank always reaches the next; and its cavern is open.
    #[test]
    fn outposts_stay_in_reach_of_each_other() {
        for seed in 0..20 {
            let w = World::new(seed);
            let mut last = (WIDTH / 2, 0);
            for k in 1..60 {
                let (x, y) = outpost(seed, k).unwrap();
                let (dx, dy) = ((x - last.0).abs(), y - last.1);
                assert!(
                    k == 1 || (150..=250).contains(&dy),
                    "seed {seed} outpost {k}: {dy} rows below the last"
                );
                assert!(
                    k == 1 || dx <= 59,
                    "seed {seed} outpost {k}: {dx} columns sideways"
                );
                assert!(
                    (OUTPOST_HALF..WIDTH - OUTPOST_HALF).contains(&x),
                    "outpost {k} at x={x} off the map"
                );
                assert_eq!(w.get(x, y - 1), Tile::Air, "outpost {k} cavern not open");
                assert!(
                    is_outpost_floor(seed, x, y),
                    "outpost {k} floor not protected"
                );
                last = (x, y);
            }
        }
    }

    /// A den or crate the game is told about must really be a hollow in the
    /// rock, below the quiet top rows, or its critters and loot sit in dirt.
    #[test]
    fn pockets_are_hollow_and_below_the_top_rows() {
        for seed in 0..10 {
            let w = World::new(seed);
            for cy in 0..40 {
                for cx in 0..WIDTH / CELL {
                    let Some(p) = pocket(seed, cx, cy) else {
                        continue;
                    };
                    let (x, y, pw, ph) = p.rect();
                    assert!(y >= POCKET_MIN_DEPTH, "seed {seed}: {p:?} too shallow");
                    assert_eq!(cell_of(x, y), (cx, cy), "seed {seed}: {p:?} left its cell");
                    for ty in y..y + ph {
                        for tx in x..x + pw {
                            assert_eq!(w.get(tx, ty), Tile::Air, "seed {seed}: {p:?} not hollow");
                        }
                    }
                }
            }
        }
    }

    /// Dens stay rare, about one cell in three, crates turn up about twice as
    /// often, and ruins are much rarer than either.
    #[test]
    fn dens_are_rare_and_crates_twice_as_common() {
        let (mut dens, mut crates, mut ruins, mut cells) = (0, 0, 0, 0);
        for seed in 0..10 {
            for cy in 1..60 {
                for cx in 0..WIDTH / CELL {
                    cells += 1;
                    match pocket(seed, cx, cy) {
                        Some(Pocket::Den { .. }) => dens += 1,
                        Some(Pocket::Crate { .. }) => crates += 1,
                        Some(Pocket::Ruin { .. }) => ruins += 1,
                        None => {}
                    }
                }
            }
        }
        let den_share = dens as f32 / cells as f32;
        let ratio = crates as f32 / dens as f32;
        assert!(
            (0.25..0.35).contains(&den_share),
            "{dens} dens in {cells} cells"
        );
        assert!(
            (1.8..2.2).contains(&ratio),
            "{crates} crates to {dens} dens"
        );
        assert!(
            ruins > 0 && ruins * 3 < dens,
            "{ruins} ruins to {dens} dens"
        );
    }

    #[test]
    fn band_position_is_continuous_across_boundaries() {
        assert_eq!(band_position(0.0), 0.0);
        assert!((band_position(49.9) - 1.0).abs() < 0.01);
        assert!((band_position(50.0) - 1.0).abs() < 0.01);
        assert_eq!(band_position(10_000.0), 4.0);
        assert_eq!(band(10_000), 4);
    }
}
