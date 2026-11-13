//! The rules, with no drawing or sound, so the whole game runs headless in
//! tests. Each update returns `Event`s that the renderer and audio react to.

use serde::{Deserialize, Serialize};

use crate::monsters::{Monster, Phase, Size};
use crate::world::{self, Mineral, Tile, WIDTH, World};

/// Half the pod's width and height, in tiles.
pub const POD_HALF: f32 = 0.4;
/// Where the pod appears after dying: standing on the surface by the shop.
pub const SPAWN: (f32, f32) = (48.0, -POD_HALF);
/// The columns the shop covers on the surface.
pub const SHOP_COLUMNS: (f32, f32) = (45.0, 51.0);
pub const MAX_LEVEL: u8 = 5;
pub const ORE_NAMES: [&str; 6] = ["Copper", "Silver", "Gold", "Emerald", "Ruby", "Starcrystal"];
const ORE_VALUES: [u64; 6] = [10, 25, 60, 150, 400, 1000];
const FUEL_PRICE: f32 = 1.0;
const REPAIR_PRICE: f32 = 2.0;
const MINERAL_VALUE: u64 = 5;
/// Bombs carried at once; they ride in their own pouch, not the cargo hold.
pub const BOMB_POUCH: u8 = 3;
/// Seconds from dropping a bomb to the blast.
pub const FUSE: f32 = 2.5;
const BLAST_HURT: f32 = 30.0;
const MONSTER_DEPTH: f32 = 60.0;
const MAX_MONSTERS: usize = 3;

const GRAVITY: f32 = 20.0;
const THRUST: f32 = 34.0;
const MAX_RISE: f32 = 7.0;
const MAX_FALL: f32 = 15.0;
const MOVE_SPEED: f32 = 5.0;
const SAFE_LANDING: f32 = 11.0;
const TOP: f32 = -6.0;
const EPS: f32 = 0.001;

pub fn ore_value(tier: u8) -> u64 {
    ORE_VALUES.get(tier as usize).copied().unwrap_or(0)
}

/// Anything the cargo hold can carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Item {
    Gem(u8),
    Mineral(Mineral),
}

impl Item {
    pub fn name(self) -> &'static str {
        match self {
            Item::Gem(t) => ORE_NAMES.get(t as usize).copied().unwrap_or("Gem"),
            Item::Mineral(m) => m.name(),
        }
    }

    pub fn value(self) -> u64 {
        match self {
            Item::Gem(t) => ore_value(t),
            Item::Mineral(_) => MINERAL_VALUE,
        }
    }

    fn from_tile(tile: Tile) -> Option<Item> {
        match tile {
            Tile::Ore(t) => Some(Item::Gem(t)),
            Tile::Mineral(m) => Some(Item::Mineral(m)),
            _ => None,
        }
    }
}

/// What the player is pressing this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
    /// Pressed this frame (not held): drop a bomb.
    pub bomb: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Upgrade {
    Drill,
    Tank,
    Cargo,
    Hull,
    Lamp,
}

impl Upgrade {
    pub const ALL: [Upgrade; 5] = [
        Upgrade::Drill,
        Upgrade::Tank,
        Upgrade::Cargo,
        Upgrade::Hull,
        Upgrade::Lamp,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Upgrade::Drill => "Drill",
            Upgrade::Tank => "Fuel tank",
            Upgrade::Cargo => "Cargo hold",
            Upgrade::Hull => "Hull",
            Upgrade::Lamp => "Lamp",
        }
    }

    fn base_cost(self) -> f64 {
        match self {
            Upgrade::Drill => 120.0,
            Upgrade::Tank => 100.0,
            Upgrade::Cargo => 100.0,
            Upgrade::Hull => 120.0,
            Upgrade::Lamp => 80.0,
        }
    }

    /// Price to go from `level` to `level + 1`.
    pub fn cost(self, level: u8) -> u64 {
        (self.base_cost() * 2.4f64.powi(level as i32)).round() as u64
    }
}

/// Upgrade levels, each 0 to `MAX_LEVEL`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Levels {
    pub drill: u8,
    pub tank: u8,
    pub cargo: u8,
    pub hull: u8,
    pub lamp: u8,
}

impl Levels {
    pub fn get(&self, u: Upgrade) -> u8 {
        match u {
            Upgrade::Drill => self.drill,
            Upgrade::Tank => self.tank,
            Upgrade::Cargo => self.cargo,
            Upgrade::Hull => self.hull,
            Upgrade::Lamp => self.lamp,
        }
    }

    fn bump(&mut self, u: Upgrade) {
        let slot = match u {
            Upgrade::Drill => &mut self.drill,
            Upgrade::Tank => &mut self.tank,
            Upgrade::Cargo => &mut self.cargo,
            Upgrade::Hull => &mut self.hull,
            Upgrade::Lamp => &mut self.lamp,
        };
        *slot = (*slot + 1).min(MAX_LEVEL);
    }

    pub fn fuel_max(&self) -> f32 {
        40.0 + 25.0 * self.tank as f32
    }

    pub fn hull_max(&self) -> f32 {
        50.0 + 30.0 * self.hull as f32
    }

    pub fn cargo_cap(&self) -> usize {
        8 + 6 * self.cargo as usize
    }

    pub fn drill_speed(&self) -> f32 {
        1.0 + 0.4 * self.drill as f32
    }

    /// Extra light radius, in tiles.
    pub fn lamp_bonus(&self) -> f32 {
        1.0 * self.lamp as f32
    }
}

/// Cargo left where the pod died, waiting to be recovered.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stash {
    pub x: f32,
    pub y: f32,
    pub cargo: Vec<Item>,
    #[serde(default)]
    pub bombs: u8,
}

impl Stash {
    fn is_empty(&self) -> bool {
        self.cargo.is_empty() && self.bombs == 0
    }
}

/// A lit bomb, falling until it lands and then counting down.
#[derive(Clone, Debug, PartialEq)]
pub struct Bomb {
    pub x: f32,
    pub y: f32,
    pub vy: f32,
    pub fuse: f32,
}

/// A dig in progress: the pod slides from `from` into the tile at (tx, ty).
#[derive(Clone, Copy, Debug)]
pub struct Dig {
    pub tx: i32,
    pub ty: i32,
    pub from: (f32, f32),
    pub to: (f32, f32),
    pub t: f32,
    pub dur: f32,
}

#[derive(Clone, Debug)]
pub struct Pod {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub fuel: f32,
    pub hull: f32,
    pub facing: f32,
    pub dig: Option<Dig>,
    pub grounded: bool,
    pub thrusting: bool,
    pub invuln: f32,
}

impl Pod {
    fn fresh(levels: &Levels) -> Pod {
        Pod {
            x: SPAWN.0,
            y: SPAWN.1,
            vx: 0.0,
            vy: 0.0,
            fuel: levels.fuel_max(),
            hull: levels.hull_max(),
            facing: 1.0,
            dig: None,
            grounded: true,
            thrusting: false,
            invuln: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShopAction {
    Sell,
    SellMinerals,
    Craft,
    Refuel,
    Repair,
    Buy(Upgrade),
}

/// Something that happened this frame, for sound and visual effects.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    DigStarted {
        x: i32,
        y: i32,
    },
    Dug {
        x: i32,
        y: i32,
        tile: Tile,
    },
    Pickup {
        item: Item,
        x: f32,
        y: f32,
    },
    CargoFull,
    Landed {
        speed: f32,
    },
    Hurt {
        amount: f32,
    },
    Died {
        x: f32,
        y: f32,
        cause: String,
        dropped: usize,
    },
    Respawned,
    Sold {
        amount: u64,
    },
    Refueled,
    Repaired,
    Bought(Upgrade),
    CantAfford,
    LowFuel,
    MonsterWarning {
        size: Size,
        x: f32,
        y: f32,
    },
    MonsterEmerged {
        size: Size,
        x: f32,
        y: f32,
    },
    StashRecovered {
        x: f32,
        y: f32,
        count: usize,
    },
    Crafted,
    /// Craft pressed without the ingredients or with a full pouch.
    CantCraft,
    BombPlaced {
        x: f32,
        y: f32,
    },
    Exploded {
        x: f32,
        y: f32,
    },
    MonsterKilled {
        size: Size,
        x: f32,
        y: f32,
    },
    MonsterStunned {
        x: f32,
        y: f32,
    },
}

/// Small xorshift generator so game randomness is seedable in tests.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub fn f32(&mut self) -> f32 {
        world::unit(self.next_u64())
    }

    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f32()
    }
}

/// Everything about a game in progress.
pub struct Game {
    pub world: World,
    pub pod: Pod,
    pub money: u64,
    pub levels: Levels,
    pub cargo: Vec<Item>,
    pub bombs: u8,
    pub live_bombs: Vec<Bomb>,
    pub stashes: Vec<Stash>,
    pub monsters: Vec<Monster>,
    pub deaths: u32,
    pub max_depth: f32,
    /// Screen shake strength, roughly 0 to 1.
    pub shake: f32,
    pub time: f32,
    rng: Rng,
    monster_timer: f32,
    low_fuel_timer: f32,
    events: Vec<Event>,
}

impl Game {
    pub fn new(seed: u64) -> Game {
        let levels = Levels::default();
        Game {
            world: World::new(seed),
            pod: Pod::fresh(&levels),
            money: 0,
            levels,
            cargo: Vec::new(),
            bombs: 0,
            live_bombs: Vec::new(),
            stashes: Vec::new(),
            monsters: Vec::new(),
            deaths: 0,
            max_depth: 0.0,
            shake: 0.0,
            time: 0.0,
            rng: Rng::new(seed ^ 0xA5A5_5A5A),
            monster_timer: 20.0,
            low_fuel_timer: 0.0,
            events: Vec::new(),
        }
    }

    /// Depth of the pod in tiles (meters), never negative.
    pub fn depth(&self) -> f32 {
        self.pod.y.max(0.0)
    }

    pub fn at_shop(&self) -> bool {
        let p = &self.pod;
        p.y < 0.5 && p.dig.is_none() && (SHOP_COLUMNS.0..SHOP_COLUMNS.1).contains(&p.x)
    }

    /// What the gems in the hold sell for; minerals are kept for crafting.
    pub fn cargo_value(&self) -> u64 {
        self.cargo
            .iter()
            .filter(|i| matches!(i, Item::Gem(_)))
            .map(|i| i.value())
            .sum()
    }

    pub fn mineral_value(&self) -> u64 {
        self.cargo
            .iter()
            .filter(|i| matches!(i, Item::Mineral(_)))
            .map(|i| i.value())
            .sum()
    }

    pub fn count(&self, item: Item) -> usize {
        self.cargo.iter().filter(|&&i| i == item).count()
    }

    /// One of each mineral in the hold, and room in the pouch.
    pub fn can_craft(&self) -> bool {
        self.bombs < BOMB_POUCH
            && Mineral::ALL
                .iter()
                .all(|&m| self.count(Item::Mineral(m)) > 0)
    }

    pub fn refuel_cost(&self) -> u64 {
        ((self.levels.fuel_max() - self.pod.fuel).max(0.0) * FUEL_PRICE).ceil() as u64
    }

    pub fn repair_cost(&self) -> u64 {
        ((self.levels.hull_max() - self.pod.hull).max(0.0) * REPAIR_PRICE).ceil() as u64
    }

    /// Advances the game by `dt` seconds and returns what happened.
    pub fn update(&mut self, dt: f32, input: Input) -> Vec<Event> {
        self.time += dt;
        self.shake = (self.shake - dt * 1.5).max(0.0);
        self.pod.invuln = (self.pod.invuln - dt).max(0.0);
        self.update_pod(dt, input);
        if input.bomb {
            self.drop_bomb();
        }
        self.update_bombs(dt);
        self.update_monsters(dt);
        self.recover_stashes();
        self.check_fuel(dt);
        self.max_depth = self.max_depth.max(self.depth());
        std::mem::take(&mut self.events)
    }

    /// Runs one shop action; the outcome shows up in the next update's events.
    pub fn apply(&mut self, action: ShopAction) {
        if !self.at_shop() {
            return;
        }
        match action {
            ShopAction::Sell => {
                let amount = self.cargo_value();
                if amount > 0 {
                    self.money += amount;
                    self.cargo.retain(|i| !matches!(i, Item::Gem(_)));
                    self.events.push(Event::Sold { amount });
                }
            }
            ShopAction::SellMinerals => {
                let amount = self.mineral_value();
                if amount > 0 {
                    self.money += amount;
                    self.cargo.retain(|i| !matches!(i, Item::Mineral(_)));
                    self.events.push(Event::Sold { amount });
                }
            }
            ShopAction::Craft => {
                if !self.can_craft() {
                    self.events.push(Event::CantCraft);
                    return;
                }
                for m in Mineral::ALL {
                    if let Some(i) = self.cargo.iter().position(|&c| c == Item::Mineral(m)) {
                        self.cargo.remove(i);
                    }
                }
                self.bombs += 1;
                self.events.push(Event::Crafted);
            }
            ShopAction::Refuel => {
                let need = self.levels.fuel_max() - self.pod.fuel;
                if need <= 0.0 {
                    return;
                }
                // Partial fills are allowed, so a nearly broke player is never stranded.
                let units = need.min(self.money as f32 / FUEL_PRICE);
                if units <= 0.0 {
                    self.events.push(Event::CantAfford);
                    return;
                }
                self.money -= ((units * FUEL_PRICE).ceil() as u64).min(self.money);
                self.pod.fuel += units;
                self.events.push(Event::Refueled);
            }
            ShopAction::Repair => {
                let need = self.levels.hull_max() - self.pod.hull;
                if need <= 0.0 {
                    return;
                }
                let units = need.min(self.money as f32 / REPAIR_PRICE);
                if units <= 0.0 {
                    self.events.push(Event::CantAfford);
                    return;
                }
                self.money -= ((units * REPAIR_PRICE).ceil() as u64).min(self.money);
                self.pod.hull += units;
                self.events.push(Event::Repaired);
            }
            ShopAction::Buy(u) => {
                let level = self.levels.get(u);
                if level >= MAX_LEVEL {
                    return;
                }
                let cost = u.cost(level);
                if self.money < cost {
                    self.events.push(Event::CantAfford);
                    return;
                }
                self.money -= cost;
                self.levels.bump(u);
                // A bigger tank or hull arrives full.
                match u {
                    Upgrade::Tank => self.pod.fuel = self.levels.fuel_max(),
                    Upgrade::Hull => self.pod.hull = self.levels.hull_max(),
                    _ => {}
                }
                self.events.push(Event::Bought(u));
            }
        }
    }

    fn burn(&mut self, amount: f32) {
        self.pod.fuel = (self.pod.fuel - amount).max(0.0);
    }

    fn collides(&self, x: f32, y: f32) -> bool {
        let x0 = (x - POD_HALF + EPS).floor() as i32;
        let x1 = (x + POD_HALF - EPS).floor() as i32;
        let y0 = (y - POD_HALF + EPS).floor() as i32;
        let y1 = (y + POD_HALF - EPS).floor() as i32;
        (y0..=y1).any(|ty| (x0..=x1).any(|tx| self.world.get(tx, ty).is_solid()))
    }

    fn dig_duration(&self, ty: i32, tile: Tile) -> f32 {
        const BAND_TIME: [f32; 5] = [0.30, 0.42, 0.58, 0.78, 1.0];
        let base = BAND_TIME[world::band(ty)] + ty.max(0) as f32 / 4000.0;
        let ore = if matches!(tile, Tile::Ore(_)) {
            0.08
        } else {
            0.0
        };
        (base + ore) / self.levels.drill_speed()
    }

    fn try_dig(&mut self, tx: i32, ty: i32, to: (f32, f32)) -> bool {
        let tile = self.world.get(tx, ty);
        if !tile.is_diggable() {
            return false;
        }
        self.pod.dig = Some(Dig {
            tx,
            ty,
            from: (self.pod.x, self.pod.y),
            to,
            t: 0.0,
            dur: self.dig_duration(ty, tile),
        });
        self.pod.vx = 0.0;
        self.pod.vy = 0.0;
        self.events.push(Event::DigStarted { x: tx, y: ty });
        true
    }

    fn finish_dig(&mut self, dig: Dig) {
        let tile = self.world.get(dig.tx, dig.ty);
        self.world.set(dig.tx, dig.ty, Tile::Air);
        self.events.push(Event::Dug {
            x: dig.tx,
            y: dig.ty,
            tile,
        });
        self.collect(tile, dig.tx, dig.ty);
    }

    /// Puts a broken tile's gem or mineral in the hold, if it has one and there's room.
    fn collect(&mut self, tile: Tile, tx: i32, ty: i32) {
        let Some(item) = Item::from_tile(tile) else {
            return;
        };
        if self.cargo.len() >= self.levels.cargo_cap() {
            self.events.push(Event::CargoFull);
            return;
        }
        self.cargo.push(item);
        self.events.push(Event::Pickup {
            item,
            x: tx as f32 + 0.5,
            y: ty as f32 + 0.5,
        });
    }

    fn drop_bomb(&mut self) {
        // Underground only: no blowing up the shop.
        if self.bombs == 0 || self.pod.y < 1.0 || self.pod.dig.is_some() {
            return;
        }
        self.bombs -= 1;
        let (x, y) = (self.pod.x, self.pod.y);
        self.live_bombs.push(Bomb {
            x,
            y,
            vy: 0.0,
            fuse: FUSE,
        });
        self.events.push(Event::BombPlaced { x, y });
    }

    fn update_bombs(&mut self, dt: f32) {
        let mut blasts = Vec::new();
        for b in &mut self.live_bombs {
            b.fuse -= dt;
            if b.fuse <= 0.0 {
                blasts.push((b.x, b.y));
                continue;
            }
            b.vy = (b.vy + GRAVITY * dt).min(MAX_FALL);
            let ny = b.y + b.vy * dt;
            let below = (ny + 0.25).floor() as i32;
            if self.world.get(b.x.floor() as i32, below).is_solid() {
                b.y = below as f32 - 0.25;
                b.vy = 0.0;
            } else {
                b.y = ny;
            }
        }
        self.live_bombs.retain(|b| b.fuse > 0.0);
        for (x, y) in blasts {
            self.explode(x, y);
        }
    }

    /// Clears the 3x3 around the bomb (boulders too, never walls), keeps what
    /// it can of the gems and minerals, and catches anything close.
    fn explode(&mut self, x: f32, y: f32) {
        let (cx, cy) = (x.floor() as i32, y.floor() as i32);
        for ty in cy - 1..=cy + 1 {
            for tx in cx - 1..=cx + 1 {
                let tile = self.world.get(tx, ty);
                if matches!(tile, Tile::Air | Tile::Wall) || ty < 0 {
                    continue;
                }
                self.world.set(tx, ty, Tile::Air);
                self.collect(tile, tx, ty);
            }
        }
        self.events.push(Event::Exploded { x, y });
        self.shake = self.shake.max(1.0);

        let mut killed = Vec::new();
        for (i, m) in self.monsters.iter_mut().enumerate() {
            let (w, _) = m.size.dims();
            if (m.x - x).hypot(m.y - y) > 2.2 + w * 0.3 {
                continue;
            }
            if m.size == Size::Huge {
                m.stun = 4.0;
                self.events.push(Event::MonsterStunned { x: m.x, y: m.y });
            } else {
                killed.push(i);
                self.events.push(Event::MonsterKilled {
                    size: m.size,
                    x: m.x,
                    y: m.y,
                });
            }
        }
        for i in killed.into_iter().rev() {
            self.monsters.remove(i);
        }

        if self.pod.invuln <= 0.0 && (self.pod.x - x).hypot(self.pod.y - y) < 2.0 {
            self.pod.invuln = 1.0;
            self.hurt(BLAST_HURT, "Caught in your own blast");
        }
    }

    fn update_pod(&mut self, dt: f32, input: Input) {
        if let Some(mut dig) = self.pod.dig {
            dig.t += dt;
            let k = (dig.t / dig.dur).min(1.0);
            self.pod.x = dig.from.0 + (dig.to.0 - dig.from.0) * k;
            self.pod.y = dig.from.1 + (dig.to.1 - dig.from.1) * k;
            self.burn(0.5 * dt);
            if k >= 1.0 {
                self.pod.dig = None;
                self.finish_dig(dig);
            } else {
                self.pod.dig = Some(dig);
            }
            return;
        }

        let dir = (input.right as i8 - input.left as i8) as f32;
        if dir != 0.0 {
            self.pod.facing = dir;
        }
        let accel = 30.0 * dt;
        self.pod.vx += (dir * MOVE_SPEED - self.pod.vx).clamp(-accel, accel);
        self.pod.thrusting = input.up && self.pod.fuel > 0.0;
        if self.pod.thrusting {
            self.pod.vy -= THRUST * dt;
            self.burn(1.0 * dt);
        }
        self.pod.vy = (self.pod.vy + GRAVITY * dt).clamp(-MAX_RISE, MAX_FALL);
        if dir != 0.0 && self.pod.grounded {
            self.burn(0.3 * dt);
        }

        // Horizontal step, snapping flush against whatever blocks it.
        let mut blocked_x = false;
        let nx = (self.pod.x + self.pod.vx * dt).clamp(POD_HALF, WIDTH as f32 - POD_HALF);
        if self.collides(nx, self.pod.y) {
            blocked_x = true;
            self.pod.x = if self.pod.vx > 0.0 {
                (nx + POD_HALF).floor() - POD_HALF - EPS
            } else {
                (nx - POD_HALF).floor() + 1.0 + POD_HALF + EPS
            };
            self.pod.vx = 0.0;
        } else {
            self.pod.x = nx;
        }

        // Vertical step.
        let ny = (self.pod.y + self.pod.vy * dt).max(TOP);
        self.pod.grounded = false;
        if self.collides(self.pod.x, ny) {
            if self.pod.vy > 0.0 {
                self.pod.y = (ny + POD_HALF).floor() - POD_HALF - EPS;
                self.pod.grounded = true;
                let speed = self.pod.vy;
                if speed > SAFE_LANDING {
                    self.events.push(Event::Landed { speed });
                    self.hurt((speed - SAFE_LANDING) * 2.5, "Crashed");
                } else if speed > 6.0 {
                    self.events.push(Event::Landed { speed });
                }
            } else {
                self.pod.y = (ny - POD_HALF).floor() + 1.0 + POD_HALF + EPS;
            }
            self.pod.vy = 0.0;
        } else {
            self.pod.y = ny;
        }

        if !self.pod.grounded || input.up || self.pod.fuel <= 0.0 {
            return;
        }
        let (x, y) = (self.pod.x, self.pod.y);
        if input.down {
            let (tx, ty) = (x.floor() as i32, (y + POD_HALF + 0.05).floor() as i32);
            self.try_dig(tx, ty, (tx as f32 + 0.5, ty as f32 + 0.5));
        } else if dir != 0.0 && blocked_x {
            let tx = (x + dir * (POD_HALF + 0.05)).floor() as i32;
            self.try_dig(tx, y.floor() as i32, (tx as f32 + 0.5, y));
        }
    }

    fn hurt(&mut self, amount: f32, cause: &str) {
        self.pod.hull -= amount;
        self.events.push(Event::Hurt { amount });
        self.shake = self.shake.max(0.5);
        if self.pod.hull <= 0.0 {
            self.die(cause.to_string());
        }
    }

    fn die(&mut self, cause: String) {
        let (x, y) = match self.pod.dig {
            // Mid-dig the pod overlaps rock; leave the stash where it started.
            Some(dig) => dig.from,
            None => (self.pod.x, self.pod.y),
        };
        let dropped = self.cargo.len() + self.bombs as usize;
        if dropped > 0 {
            self.stashes.push(Stash {
                x,
                y,
                cargo: std::mem::take(&mut self.cargo),
                bombs: std::mem::take(&mut self.bombs),
            });
        }
        self.deaths += 1;
        self.events.push(Event::Died {
            x,
            y,
            cause,
            dropped,
        });
        self.pod = Pod::fresh(&self.levels);
        self.pod.invuln = 2.0;
        self.monsters.clear();
        self.shake = 1.0;
        self.low_fuel_timer = 0.0;
        self.events.push(Event::Respawned);
    }

    fn check_fuel(&mut self, dt: f32) {
        let underground = self.pod.y > 1.0;
        if self.pod.fuel <= 0.0 && underground {
            self.die("Ran out of fuel".into());
            return;
        }
        if underground && self.pod.fuel < 0.2 * self.levels.fuel_max() {
            self.low_fuel_timer -= dt;
            if self.low_fuel_timer <= 0.0 {
                self.events.push(Event::LowFuel);
                self.low_fuel_timer = 4.0;
            }
        } else {
            self.low_fuel_timer = 0.0;
        }
    }

    fn recover_stashes(&mut self) {
        let cap = self.levels.cargo_cap();
        let (px, py) = (self.pod.x, self.pod.y);
        for stash in &mut self.stashes {
            if (stash.x - px).hypot(stash.y - py) >= 0.9 {
                continue;
            }
            let room = cap.saturating_sub(self.cargo.len());
            let items = room.min(stash.cargo.len());
            let bombs = BOMB_POUCH.saturating_sub(self.bombs).min(stash.bombs);
            if items + bombs as usize == 0 {
                continue;
            }
            self.cargo.extend(stash.cargo.drain(..items));
            self.bombs += bombs;
            stash.bombs -= bombs;
            self.events.push(Event::StashRecovered {
                x: stash.x,
                y: stash.y,
                count: items + bombs as usize,
            });
        }
        self.stashes.retain(|s| !s.is_empty());
    }

    fn spawn_monster(&mut self) {
        let depth = self.depth();
        let total: f32 = Size::ALL.iter().map(|s| s.weight(depth)).sum();
        let mut pick = self.rng.f32() * total;
        let mut size = Size::Small;
        for s in Size::ALL {
            pick -= s.weight(depth);
            if pick <= 0.0 {
                size = s;
                break;
            }
        }
        let (w, h) = size.dims();
        // Bottom-aligned with the pod's row, so it sweeps the row she's digging in.
        let y = self.pod.y.floor() + 1.0 - h / 2.0;
        let mut side = if self.rng.f32() < 0.5 { -1.0 } else { 1.0 };
        let offset = self.rng.range(8.0, 13.0);
        let (lo, hi) = (w / 2.0, WIDTH as f32 - w / 2.0);
        if !(lo..=hi).contains(&(self.pod.x + side * offset)) {
            side = -side;
        }
        let x = (self.pod.x + side * offset).clamp(lo, hi);
        self.monsters.push(Monster {
            size,
            x,
            y,
            dir: -side,
            phase: Phase::Warning(size.warning()),
            age: 0.0,
            stun: 0.0,
        });
        self.events.push(Event::MonsterWarning { size, x, y });
    }

    fn update_monsters(&mut self, dt: f32) {
        let depth = self.depth();
        if depth >= MONSTER_DEPTH {
            self.monster_timer -= dt;
            if self.monster_timer <= 0.0 {
                if self.monsters.len() < MAX_MONSTERS && self.pod.dig.is_none() {
                    self.spawn_monster();
                }
                let k = ((depth - MONSTER_DEPTH) / 540.0).clamp(0.0, 1.0);
                self.monster_timer = (30.0 - 21.0 * k) * self.rng.range(0.7, 1.3);
            }
        }

        let (px, py) = (self.pod.x, self.pod.y);
        let mut hit = None;
        for m in &mut self.monsters {
            m.age += dt;
            if let Phase::Warning(t) = m.phase {
                let left = t - dt;
                self.shake = self.shake.max(0.25 + m.size.presence());
                if left > 0.0 {
                    m.phase = Phase::Warning(left);
                    continue;
                }
                m.phase = Phase::Active;
                self.shake = self.shake.max(0.5 + m.size.presence());
                self.events.push(Event::MonsterEmerged {
                    size: m.size,
                    x: m.x,
                    y: m.y,
                });
            }
            if m.stun > 0.0 {
                m.stun -= dt;
                continue;
            }
            let (w, h) = m.size.dims();
            if (py - m.y).abs() < h / 2.0 + 1.2 {
                m.dir = if px > m.x { 1.0 } else { -1.0 };
            }
            let nx = m.x + m.dir * m.size.speed() * dt;
            let (lo, hi) = (w / 2.0, WIDTH as f32 - w / 2.0);
            if (lo..=hi).contains(&nx) {
                m.x = nx;
            } else {
                m.dir = -m.dir;
            }
            let near = 1.0 - ((m.x - px).hypot(m.y - py) / 12.0).min(1.0);
            self.shake = self.shake.max(near * m.size.presence());
            if hit.is_none() && m.touches(px, py, POD_HALF) {
                hit = Some((m.size, m.x));
            }
        }
        self.monsters
            .retain(|m| (m.y - py).abs() < 18.0 && m.age < 45.0);

        let Some((size, mx)) = hit else {
            return;
        };
        if self.pod.invuln > 0.0 {
            return;
        }
        let cause = format!("Caught by {}", size.name());
        match size.damage() {
            None => self.die(cause),
            Some(amount) => {
                self.pod.invuln = 1.5;
                if self.pod.dig.is_none() {
                    self.pod.vx = if px > mx { 7.0 } else { -7.0 };
                    self.pod.vy = -5.0;
                }
                self.hurt(amount, &cause);
            }
        }
    }

    /// Snapshot for saving.
    pub fn to_save(&self) -> SaveData {
        // Mid-dig the pod overlaps rock; save where the dig started instead.
        let (x, y) = self.pod.dig.map_or((self.pod.x, self.pod.y), |d| d.from);
        SaveData {
            version: SaveData::VERSION,
            seed: self.world.seed,
            money: self.money,
            levels: self.levels,
            x,
            y,
            fuel: self.pod.fuel,
            hull: self.pod.hull,
            cargo: self.cargo.clone(),
            bombs: self.bombs,
            stashes: self.stashes.clone(),
            changes: self.world.changes(),
            deaths: self.deaths,
            max_depth: self.max_depth,
        }
    }

    pub fn from_save(s: SaveData) -> Game {
        let mut game = Game::new(s.seed);
        game.world = World::from_changes(s.seed, s.changes);
        game.money = s.money;
        game.levels = s.levels;
        game.pod.x = s.x;
        game.pod.y = s.y;
        game.pod.fuel = s.fuel.clamp(0.0, game.levels.fuel_max());
        game.pod.hull = s.hull.clamp(1.0, game.levels.hull_max());
        game.cargo = s.cargo;
        game.bombs = s.bombs.min(BOMB_POUCH);
        game.stashes = s.stashes;
        game.deaths = s.deaths;
        game.max_depth = s.max_depth;
        game.free_pod();
        game
    }

    /// Clears rock (never walls) from under the pod, for saves made against a
    /// differently generated world.
    fn free_pod(&mut self) {
        let (x, y) = (self.pod.x, self.pod.y);
        let x0 = (x - POD_HALF + EPS).floor() as i32;
        let x1 = (x + POD_HALF - EPS).floor() as i32;
        let y0 = (y - POD_HALF + EPS).floor() as i32;
        let y1 = (y + POD_HALF - EPS).floor() as i32;
        for ty in y0..=y1 {
            for tx in x0..=x1 {
                let tile = self.world.get(tx, ty);
                if tile.is_solid() && tile != Tile::Wall {
                    self.world.set(tx, ty, Tile::Air);
                }
            }
        }
    }
}

/// Everything that persists between sessions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SaveData {
    pub version: u32,
    pub seed: u64,
    pub money: u64,
    pub levels: Levels,
    pub x: f32,
    pub y: f32,
    pub fuel: f32,
    pub hull: f32,
    pub cargo: Vec<Item>,
    #[serde(default)]
    pub bombs: u8,
    pub stashes: Vec<Stash>,
    pub changes: Vec<(i32, i32, Tile)>,
    pub deaths: u32,
    pub max_depth: f32,
}

impl SaveData {
    pub const VERSION: u32 = 2;
}

/// The first save format, where cargo was a list of gem tiers. Kept only so
/// those saves load; they're upgraded on the next write.
#[derive(Deserialize)]
pub struct SaveDataV1 {
    seed: u64,
    money: u64,
    levels: Levels,
    x: f32,
    y: f32,
    fuel: f32,
    hull: f32,
    cargo: Vec<u8>,
    stashes: Vec<StashV1>,
    changes: Vec<(i32, i32, Tile)>,
    deaths: u32,
    max_depth: f32,
}

#[derive(Deserialize)]
struct StashV1 {
    x: f32,
    y: f32,
    cargo: Vec<u8>,
}

impl From<SaveDataV1> for SaveData {
    fn from(v1: SaveDataV1) -> SaveData {
        let gems = |tiers: Vec<u8>| tiers.into_iter().map(Item::Gem).collect();
        SaveData {
            version: SaveData::VERSION,
            seed: v1.seed,
            money: v1.money,
            levels: v1.levels,
            x: v1.x,
            y: v1.y,
            fuel: v1.fuel,
            hull: v1.hull,
            cargo: gems(v1.cargo),
            bombs: 0,
            stashes: v1
                .stashes
                .into_iter()
                .map(|s| Stash {
                    x: s.x,
                    y: s.y,
                    cargo: gems(s.cargo),
                    bombs: 0,
                })
                .collect(),
            changes: v1.changes,
            deaths: v1.deaths,
            max_depth: v1.max_depth,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn gems(tiers: &[u8]) -> Vec<Item> {
        tiers.iter().map(|&t| Item::Gem(t)).collect()
    }

    fn run(game: &mut Game, secs: f32, input: Input) -> Vec<Event> {
        let mut events = Vec::new();
        for _ in 0..(secs / DT) as usize {
            events.extend(game.update(DT, input));
        }
        events
    }

    fn down() -> Input {
        Input {
            down: true,
            ..Input::default()
        }
    }

    /// A game with the pod resting in a dug pocket at the given depth.
    fn game_at_depth(depth: i32) -> Game {
        let mut g = Game::new(11);
        g.monster_timer = f32::INFINITY;
        for x in 4..9 {
            g.world.set(x, depth, Tile::Air);
            g.world.set(x, depth + 1, Tile::Dirt);
        }
        g.pod.x = 6.5;
        g.pod.y = depth as f32 + 1.0 - POD_HALF - EPS;
        g
    }

    fn huge_monster_on(g: &mut Game) {
        g.monsters.push(Monster {
            size: Size::Huge,
            x: g.pod.x,
            y: g.pod.y,
            dir: 1.0,
            phase: Phase::Active,
            age: 0.0,
            stun: 0.0,
        });
    }

    /// Digging straight down from the spawn point moves the pod deeper.
    #[test]
    fn holding_down_digs_deeper() {
        let mut g = Game::new(5);
        g.monster_timer = f32::INFINITY;
        run(&mut g, 4.0, down());
        assert!(g.depth() > 3.0, "pod only reached {}", g.depth());
    }

    /// Dying keeps money and upgrades but leaves the cargo where she fell.
    #[test]
    fn death_drops_cargo_as_stash_and_keeps_money() {
        let mut g = game_at_depth(80);
        g.money = 500;
        g.levels.drill = 2;
        g.cargo = gems(&[2, 3, 3]);
        let at = (g.pod.x, g.pod.y);
        g.die("test".into());
        assert_eq!(g.money, 500);
        assert_eq!(g.levels.drill, 2);
        assert!(g.cargo.is_empty(), "cargo kept after death: {:?}", g.cargo);
        assert_eq!(
            g.stashes,
            vec![Stash {
                x: at.0,
                y: at.1,
                cargo: gems(&[2, 3, 3]),
                bombs: 0,
            }]
        );
        assert_eq!((g.pod.x, g.pod.y), SPAWN, "did not respawn at the surface");
    }

    /// Dying with an empty hold leaves no empty stash behind.
    #[test]
    fn death_with_empty_cargo_leaves_no_stash() {
        let mut g = game_at_depth(80);
        g.die("test".into());
        assert!(g.stashes.is_empty());
    }

    /// Every stash survives later deaths; only recovery removes one.
    #[test]
    fn repeated_deaths_keep_every_stash() {
        let mut g = game_at_depth(80);
        g.cargo = gems(&[1]);
        g.die("one".into());
        g.pod.y = 120.0;
        g.cargo = gems(&[4]);
        g.die("two".into());
        assert_eq!(g.stashes.len(), 2, "stashes: {:?}", g.stashes);
    }

    /// Recovering a stash fills the hold and leaves the rest waiting.
    #[test]
    fn stash_recovery_respects_cargo_capacity() {
        let mut g = game_at_depth(80);
        let cap = g.levels.cargo_cap();
        g.cargo = vec![Item::Gem(0); cap - 2];
        g.stashes.push(Stash {
            x: g.pod.x,
            y: g.pod.y,
            cargo: gems(&[5; 5]),
            bombs: 0,
        });
        g.update(DT, Input::default());
        assert_eq!(g.cargo.len(), cap);
        assert_eq!(g.stashes.len(), 1);
        assert_eq!(g.stashes[0].cargo.len(), 3, "remainder not left in stash");
    }

    /// The biggest monster kills in one touch, even with a maxed hull.
    #[test]
    fn huge_monster_kills_even_a_maxed_hull() {
        let mut g = game_at_depth(600);
        g.levels.hull = MAX_LEVEL;
        g.pod.hull = g.levels.hull_max();
        g.cargo = gems(&[5]);
        huge_monster_on(&mut g);
        let events = g.update(DT, Input::default());
        assert!(
            events.iter().any(|e| matches!(e, Event::Died { .. })),
            "survived: {events:?}"
        );
        assert_eq!(g.stashes.len(), 1);
    }

    /// Bigger monsters hit harder, and anything short of Huge is survivable.
    #[test]
    fn damage_grows_with_monster_size() {
        let dmg: Vec<f32> = [Size::Small, Size::Medium, Size::Large]
            .iter()
            .map(|s| s.damage().unwrap_or(f32::INFINITY))
            .collect();
        assert!(
            dmg.windows(2).all(|w| w[0] < w[1]),
            "damage not increasing: {dmg:?}"
        );
        assert!(dmg.iter().all(|&d| d < Levels::default().hull_max() + 1.0));
        assert_eq!(Size::Huge.damage(), None);
    }

    /// Monsters only move sideways, so they can never follow her up a shaft.
    #[test]
    fn monsters_never_move_vertically() {
        let mut g = game_at_depth(700);
        for size in Size::ALL {
            g.monsters.push(Monster {
                size,
                x: 20.0,
                y: 700.5,
                dir: 1.0,
                phase: Phase::Active,
                age: 0.0,
                stun: 0.0,
            });
        }
        for step in 0..(3.0 / DT) as usize {
            g.pod.invuln = f32::INFINITY;
            g.update(
                DT,
                Input {
                    up: step % 3 == 0,
                    left: true,
                    ..Input::default()
                },
            );
            for m in &g.monsters {
                assert_eq!(m.y, 700.5, "{:?} moved vertically", m.size);
            }
        }
        assert_eq!(
            g.monsters.len(),
            Size::ALL.len(),
            "monsters despawned early"
        );
    }

    /// Flying straight up gets her out of a monster's reach.
    #[test]
    fn flying_up_escapes_a_huge_monster() {
        let mut g = game_at_depth(600);
        for y in 590..600 {
            g.world.set(6, y, Tile::Air);
        }
        g.pod.x = 6.5;
        g.monsters.push(Monster {
            size: Size::Huge,
            x: 2.5,
            y: 601.0 - Size::Huge.dims().1 / 2.0,
            dir: 1.0,
            phase: Phase::Active,
            age: 0.0,
            stun: 0.0,
        });
        let events = run(
            &mut g,
            3.0,
            Input {
                up: true,
                ..Input::default()
            },
        );
        assert!(
            !events.iter().any(|e| matches!(e, Event::Died { .. })),
            "died while escaping upward: {events:?}"
        );
    }

    /// Running dry underground ends the run instead of leaving her stuck.
    #[test]
    fn out_of_fuel_underground_respawns() {
        let mut g = game_at_depth(80);
        g.pod.fuel = 0.0;
        let events = g.update(DT, Input::default());
        assert!(events.iter().any(|e| matches!(e, Event::Died { .. })));
        assert_eq!(g.pod.fuel, g.levels.fuel_max());
    }

    #[test]
    fn selling_converts_cargo_to_money() {
        let mut g = Game::new(1);
        g.cargo = gems(&[0, 2]);
        g.apply(ShopAction::Sell);
        assert_eq!(g.money, ore_value(0) + ore_value(2));
        assert!(g.cargo.is_empty());
    }

    /// Upgrades cost money and stop at the max level.
    #[test]
    fn buying_upgrades_charges_and_caps() {
        let mut g = Game::new(1);
        g.money = 1_000_000;
        for _ in 0..MAX_LEVEL + 2 {
            g.apply(ShopAction::Buy(Upgrade::Cargo));
        }
        assert_eq!(g.levels.cargo, MAX_LEVEL);
        let spent: u64 = (0..MAX_LEVEL).map(|l| Upgrade::Cargo.cost(l)).sum();
        assert_eq!(g.money, 1_000_000 - spent);
    }

    /// A broke player still gets whatever fuel they can pay for.
    #[test]
    fn refuel_is_partial_when_money_is_short() {
        let mut g = Game::new(1);
        g.pod.fuel = 0.0;
        g.money = 10;
        g.apply(ShopAction::Refuel);
        assert_eq!(g.money, 0);
        assert!((g.pod.fuel - 10.0).abs() < 0.01, "fuel {}", g.pod.fuel);
    }

    /// A save round trip restores progress, stashes and dug tunnels.
    #[test]
    fn save_round_trip_restores_progress() {
        let mut g = game_at_depth(90);
        g.money = 1234;
        g.levels.lamp = 3;
        g.cargo = gems(&[1, 2]);
        g.stashes.push(Stash {
            x: 3.5,
            y: 200.5,
            cargo: gems(&[4]),
            bombs: 0,
        });
        let data = g.to_save();
        let json = serde_json::to_string(&data).unwrap();
        let back = Game::from_save(serde_json::from_str(&json).unwrap());
        assert_eq!(back.to_save(), data);
        assert_eq!(back.world.get(5, 90), Tile::Air);
    }

    /// A save from an older or narrower world can land the pod in rock that
    /// generation now places there; loading must free it instead of trapping it.
    #[test]
    fn loading_frees_a_pod_buried_in_rock() {
        let mut data = Game::new(4).to_save();
        let (x, y) = (20.5, 300.5);
        data.x = x;
        data.y = y;
        let g = Game::from_save(data.clone());
        assert!(!g.collides(x, y), "pod still buried after load");
        assert!(
            g.world.get(x.floor() as i32, y.floor() as i32) == Tile::Air,
            "pod's own tile not cleared"
        );
    }

    fn minerals() -> Vec<Item> {
        Mineral::ALL.iter().map(|&m| Item::Mineral(m)).collect()
    }

    fn bomb_at(g: &mut Game, x: f32, y: f32) {
        g.live_bombs.push(Bomb {
            x,
            y,
            vy: 0.0,
            fuse: 0.01,
        });
        g.update(DT, Input::default());
    }

    /// Crafting turns exactly one Coal, Sulfur and Potassium into one bomb.
    #[test]
    fn crafting_uses_one_of_each_mineral() {
        let mut g = Game::new(1);
        g.cargo = minerals();
        g.cargo.push(Item::Mineral(Mineral::Coal));
        g.apply(ShopAction::Craft);
        assert_eq!(g.bombs, 1);
        assert_eq!(
            g.cargo,
            vec![Item::Mineral(Mineral::Coal)],
            "wrong ingredients used"
        );
    }

    /// Missing an ingredient, or a full pouch, crafts nothing and costs nothing.
    #[test]
    fn crafting_is_refused_without_ingredients_or_room() {
        let mut g = Game::new(1);
        g.cargo = vec![Item::Mineral(Mineral::Coal), Item::Mineral(Mineral::Sulfur)];
        g.apply(ShopAction::Craft);
        assert_eq!((g.bombs, g.cargo.len()), (0, 2));
        g.cargo = minerals();
        g.bombs = BOMB_POUCH;
        g.apply(ShopAction::Craft);
        assert_eq!(
            (g.bombs, g.cargo.len()),
            (BOMB_POUCH, 3),
            "crafted past a full pouch"
        );
    }

    /// "Sell gems" must never sell the minerals she's saving for a bomb.
    #[test]
    fn selling_gems_keeps_minerals() {
        let mut g = Game::new(1);
        g.cargo = minerals();
        g.cargo.push(Item::Gem(2));
        g.apply(ShopAction::Sell);
        assert_eq!(g.money, ore_value(2));
        assert_eq!(g.cargo, minerals());
    }

    /// Bombs open boulders (which can't be drilled) but never the bedrock walls.
    #[test]
    fn blast_breaks_boulders_but_not_walls() {
        let mut g = game_at_depth(200);
        g.pod.y = 150.0; // well clear of the blast
        g.world.set(0, 200, Tile::Boulder);
        g.world.set(1, 201, Tile::Boulder);
        bomb_at(&mut g, 0.5, 200.5);
        assert_eq!(g.world.get(0, 200), Tile::Air);
        assert_eq!(g.world.get(1, 201), Tile::Air);
        assert_eq!(g.world.get(-1, 200), Tile::Wall, "blast broke the wall");
    }

    /// A blast kills anything up to Large, but the Deep Thing is only frozen.
    #[test]
    fn blast_kills_monsters_but_only_stuns_the_deep_thing() {
        let mut g = game_at_depth(600);
        for (size, x) in [(Size::Large, 20.0), (Size::Huge, 40.0)] {
            g.monsters.push(Monster {
                size,
                x,
                y: 600.5,
                dir: 1.0,
                phase: Phase::Active,
                age: 0.0,
                stun: 0.0,
            });
        }
        bomb_at(&mut g, 20.0, 600.5);
        bomb_at(&mut g, 40.0, 600.5);
        assert_eq!(g.monsters.len(), 1, "monsters left: {:?}", g.monsters);
        let deep = g.monsters[0].clone();
        assert_eq!(deep.size, Size::Huge);
        for _ in 0..60 {
            g.update(DT, Input::default());
        }
        assert_eq!(g.monsters[0].x, deep.x, "stunned Deep Thing moved");
    }

    /// Standing on your own bomb hurts; getting clear doesn't.
    #[test]
    fn blast_hurts_the_pod_only_when_close() {
        let mut g = game_at_depth(100);
        let full = g.pod.hull;
        let (px, py) = (g.pod.x, g.pod.y);
        bomb_at(&mut g, px + 5.0, py);
        assert_eq!(g.pod.hull, full, "hurt from 5 tiles away");
        bomb_at(&mut g, px, py);
        assert!(g.pod.hull < full, "not hurt by a bomb underneath");
    }

    /// No bombs at the surface, so the shop is safe.
    #[test]
    fn bombs_cannot_be_dropped_at_the_surface() {
        let mut g = Game::new(1);
        g.bombs = 1;
        g.update(
            DT,
            Input {
                bomb: true,
                ..Input::default()
            },
        );
        assert_eq!((g.bombs, g.live_bombs.len()), (1, 0));
    }

    /// Death leaves bombs and minerals in the stash too, and recovery refills the pouch.
    #[test]
    fn stash_keeps_bombs_and_minerals() {
        let mut g = game_at_depth(80);
        g.cargo = minerals();
        g.bombs = 2;
        let at = (g.pod.x, g.pod.y);
        g.die("test".into());
        assert_eq!(g.bombs, 0);
        assert_eq!(g.stashes[0].bombs, 2);
        g.pod.x = at.0;
        g.pod.y = at.1;
        g.update(DT, Input::default());
        assert_eq!(
            (g.bombs, g.cargo.len()),
            (2, 3),
            "stash not fully recovered"
        );
        assert!(g.stashes.is_empty());
    }

    /// Saving mid-dig must not put the pod inside rock on reload.
    #[test]
    fn saving_mid_dig_uses_the_dig_start() {
        let mut g = game_at_depth(90);
        let start = (g.pod.x, g.pod.y);
        run(&mut g, 0.05, down());
        assert!(g.pod.dig.is_some(), "dig did not start");
        let data = g.to_save();
        assert_eq!((data.x, data.y), start);
    }
}
