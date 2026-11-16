//! The rules, with no drawing or sound, so the whole game runs headless in
//! tests. Each update returns `Event`s that the renderer and audio react to.

use serde::{Deserialize, Serialize};

use crate::monsters::{Hit, Kind, Monster, Phase, Size, Weapon};
use crate::world::{self, Mineral, Tile, WIDTH, World};

/// Half the pod's width and height, in tiles.
pub const POD_HALF: f32 = 0.4;
/// Where the pod appears after dying: standing on the surface by the shop.
pub const SPAWN: (f32, f32) = (256.0, -POD_HALF);
/// The columns the shop covers on the surface.
pub const SHOP_COLUMNS: (f32, f32) = (253.0, 259.0);
/// Rows of ground under the shop that stay solid no matter what.
const FOUNDATION_DEPTH: i32 = 2;
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
const LASER_RANGE: f32 = 6.0;
const LASER_FUEL: f32 = 2.0;
/// The laser cuts this much slower than the drill.
const LASER_CUT_SLOWDOWN: f32 = 1.5;
const ROCKET_SPEED: f32 = 14.0;
const ROCKET_FUEL: f32 = 6.0;
/// Seconds of holding H before the ion cannon will fire.
pub const ION_CHARGE: f32 = 1.5;
const ION_FUEL: f32 = 15.0;
const ION_RANGE: f32 = 14.0;
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

/// Whether a tile holds up the shop. Neither drill nor bomb can break these,
/// so the shop never ends up floating over a hole.
pub fn is_foundation(tx: i32, ty: i32) -> bool {
    (0..FOUNDATION_DEPTH).contains(&ty)
        && (SHOP_COLUMNS.0 as i32..SHOP_COLUMNS.1 as i32).contains(&tx)
}

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
    /// Held: fire the laser.
    pub laser: bool,
    /// Pressed this frame: launch a rocket.
    pub rocket: bool,
    /// Held: charge the ion cannon; letting go fires it.
    pub ion: bool,
}

/// A weapon bought at the shop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gun {
    Laser,
    Rockets,
    Ion,
}

impl Gun {
    pub const ALL: [Gun; 3] = [Gun::Laser, Gun::Rockets, Gun::Ion];

    pub fn name(self) -> &'static str {
        match self {
            Gun::Laser => "Laser (hold F)",
            Gun::Rockets => "Rockets (G)",
            Gun::Ion => "Ion cannon (hold H)",
        }
    }

    pub fn price(self) -> u64 {
        match self {
            Gun::Laser => 800,
            Gun::Rockets => 2000,
            Gun::Ion => 6000,
        }
    }
}

/// Which guns she owns. Each is bought once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Arsenal {
    pub laser: bool,
    pub rockets: bool,
    pub ion: bool,
}

impl Arsenal {
    pub fn has(&self, g: Gun) -> bool {
        match g {
            Gun::Laser => self.laser,
            Gun::Rockets => self.rockets,
            Gun::Ion => self.ion,
        }
    }

    fn give(&mut self, g: Gun) {
        match g {
            Gun::Laser => self.laser = true,
            Gun::Rockets => self.rockets = true,
            Gun::Ion => self.ion = true,
        }
    }
}

/// A horizontal beam (laser or ion) from x0 to x1 at height y, for drawing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Beam {
    pub x0: f32,
    pub x1: f32,
    pub y: f32,
    /// Seconds left on screen.
    pub t: f32,
}

/// A rocket in flight.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rocket {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub life: f32,
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

/// Upgrade levels, each 0 to `MAX_LEVEL`, plus relics found, which stack
/// on top with no cap. Keeping relics here means every stat sees them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Levels {
    pub drill: u8,
    pub tank: u8,
    pub cargo: u8,
    pub hull: u8,
    pub lamp: u8,
    pub overdrive: u8,
    pub reserve_tank: u8,
    pub plating: u8,
    pub glowstone: u8,
}

/// A rare, permanent bonus dropped by monsters killed with a heavy weapon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relic {
    Overdrive,
    ReserveTank,
    Plating,
    Glowstone,
}

impl Relic {
    pub const ALL: [Relic; 4] = [
        Relic::Overdrive,
        Relic::ReserveTank,
        Relic::Plating,
        Relic::Glowstone,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Relic::Overdrive => "Overdrive: +20% drill speed",
            Relic::ReserveTank => "Reserve Tank: +15 fuel",
            Relic::Plating => "Plating: +15 hull",
            Relic::Glowstone => "Glowstone: brighter light",
        }
    }
}

/// A relic lying where its monster fell, waiting to be picked up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RelicDrop {
    pub x: f32,
    pub y: f32,
    pub vy: f32,
    pub relic: Relic,
}

/// Chance a heavy-weapon kill drops a relic, by monster size.
fn drop_chance(size: Size) -> f32 {
    match size {
        Size::Small => 0.25,
        Size::Medium => 0.40,
        Size::Large => 0.70,
        Size::Huge => 1.0,
    }
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
        40.0 + 25.0 * self.tank as f32 + 15.0 * self.reserve_tank as f32
    }

    pub fn hull_max(&self) -> f32 {
        50.0 + 30.0 * self.hull as f32 + 15.0 * self.plating as f32
    }

    fn add_relic(&mut self, r: Relic) {
        let slot = match r {
            Relic::Overdrive => &mut self.overdrive,
            Relic::ReserveTank => &mut self.reserve_tank,
            Relic::Plating => &mut self.plating,
            Relic::Glowstone => &mut self.glowstone,
        };
        *slot = slot.saturating_add(1);
    }

    pub fn relics(&self) -> u32 {
        [
            self.overdrive,
            self.reserve_tank,
            self.plating,
            self.glowstone,
        ]
        .iter()
        .map(|&n| n as u32)
        .sum()
    }

    pub fn cargo_cap(&self) -> usize {
        8 + 6 * self.cargo as usize
    }

    pub fn drill_speed(&self) -> f32 {
        (1.0 + 0.4 * self.drill as f32) * (1.0 + 0.2 * self.overdrive as f32)
    }

    /// Extra light radius, in tiles.
    pub fn lamp_bonus(&self) -> f32 {
        1.0 * self.lamp as f32 + 1.0 * self.glowstone as f32
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
    BuyGun(Gun),
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
        kind: Kind,
        x: f32,
        y: f32,
        by: Weapon,
    },
    MonsterStunned {
        x: f32,
        y: f32,
    },
    BoughtGun(Gun),
    RelicDropped {
        x: f32,
        y: f32,
    },
    RelicFound(Relic),
    RocketFired,
    IonFired {
        x0: f32,
        x1: f32,
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
    pub arsenal: Arsenal,
    pub rockets: Vec<Rocket>,
    pub drops: Vec<RelicDrop>,
    /// The laser beam this frame, if it's firing.
    pub laser: Option<Beam>,
    /// The last ion shot, while it's still fading.
    pub ion_beam: Option<Beam>,
    /// Seconds H has been held.
    pub ion_charge: f32,
    /// The tile the laser is cutting, and how long it's been at it.
    laser_cut: Option<(i32, i32, f32)>,
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
            arsenal: Arsenal::default(),
            rockets: Vec::new(),
            drops: Vec::new(),
            laser: None,
            ion_beam: None,
            ion_charge: 0.0,
            laser_cut: None,
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
        self.update_guns(dt, input);
        self.update_drops(dt);
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
            ShopAction::BuyGun(g) => {
                if self.arsenal.has(g) {
                    return;
                }
                if self.money < g.price() {
                    self.events.push(Event::CantAfford);
                    return;
                }
                self.money -= g.price();
                self.arsenal.give(g);
                self.events.push(Event::BoughtGun(g));
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
        if !tile.is_diggable() || is_foundation(tx, ty) {
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
            self.explode(x, y, Weapon::Bomb);
        }
    }

    /// Clears the 3x3 around a bomb or rocket (boulders too, never walls),
    /// keeps what it can of the gems and minerals, and catches anything close.
    /// Only a bomb can hurt the pod; a rocket fired point-blank won't.
    fn explode(&mut self, x: f32, y: f32, weapon: Weapon) {
        let (cx, cy) = (x.floor() as i32, y.floor() as i32);
        for ty in cy - 1..=cy + 1 {
            for tx in cx - 1..=cx + 1 {
                let tile = self.world.get(tx, ty);
                if matches!(tile, Tile::Air | Tile::Wall) || ty < 0 || is_foundation(tx, ty) {
                    continue;
                }
                self.world.set(tx, ty, Tile::Air);
                self.collect(tile, tx, ty);
            }
        }
        self.events.push(Event::Exploded { x, y });
        self.shake = self.shake.max(1.0);

        self.strike(weapon, weapon.damage(), |m| {
            let (w, _) = m.size.dims();
            (m.x - x).hypot(m.y - y) <= 2.2 + w * 0.3
        });

        if weapon == Weapon::Bomb
            && self.pod.invuln <= 0.0
            && (self.pod.x - x).hypot(self.pod.y - y) < 2.0
        {
            self.pod.invuln = 1.0;
            self.hurt(BLAST_HURT, "Caught in your own blast");
        }
    }

    /// Relics fall to the floor and wait; touching one claims it for good.
    fn update_drops(&mut self, dt: f32) {
        let (px, py) = (self.pod.x, self.pod.y);
        let mut found = Vec::new();
        let world = &self.world;
        self.drops.retain_mut(|d| {
            d.vy = (d.vy + GRAVITY * dt).min(MAX_FALL);
            let ny = d.y + d.vy * dt;
            let below = (ny + 0.3).floor() as i32;
            if world.get(d.x.floor() as i32, below).is_solid() {
                d.y = below as f32 - 0.3;
                d.vy = 0.0;
            } else {
                d.y = ny;
            }
            if (d.x - px).hypot(d.y - py) < 0.9 {
                found.push(d.relic);
                return false;
            }
            true
        });
        for r in found {
            self.levels.add_relic(r);
            // The new headroom arrives full, like a bought upgrade.
            match r {
                Relic::ReserveTank => {
                    self.pod.fuel = (self.pod.fuel + 15.0).min(self.levels.fuel_max())
                }
                Relic::Plating => {
                    self.pod.hull = (self.pod.hull + 15.0).min(self.levels.hull_max())
                }
                _ => {}
            }
            self.events.push(Event::RelicFound(r));
        }
    }

    fn update_guns(&mut self, dt: f32, input: Input) {
        if let Some(b) = &mut self.ion_beam {
            b.t -= dt;
        }
        self.ion_beam = self.ion_beam.filter(|b| b.t > 0.0);
        self.fire_laser(dt, input.laser);
        if input.rocket {
            self.launch_rocket();
        }
        self.update_rockets(dt);
        self.charge_ion(dt, input.ion);
    }

    /// Monsters overlapping the row band at `y` between x0 and x1.
    fn in_beam(m: &Monster, x0: f32, x1: f32, y: f32) -> bool {
        let (w, h) = m.size.dims();
        let (lo, hi) = (x0.min(x1), x0.max(x1));
        m.is_active()
            && (m.y - y).abs() < h / 2.0 + 0.3
            && m.x + w / 2.0 >= lo
            && m.x - w / 2.0 <= hi
    }

    fn fire_laser(&mut self, dt: f32, held: bool) {
        self.laser = None;
        if !held || !self.arsenal.laser || self.pod.dig.is_some() || self.pod.fuel <= 0.0 {
            self.laser_cut = None;
            return;
        }
        self.burn(LASER_FUEL * dt);
        let (dir, y) = (self.pod.facing, self.pod.y);
        let x0 = self.pod.x + dir * POD_HALF;
        let mut x1 = x0 + dir * LASER_RANGE;
        let mut target = None;
        for step in 1..=(LASER_RANGE * 4.0) as i32 {
            let x = x0 + dir * step as f32 * 0.25;
            let (tx, ty) = (x.floor() as i32, y.floor() as i32);
            let tile = self.world.get(tx, ty);
            if tile.is_solid() {
                x1 = x;
                target = Some((tx, ty, tile));
                break;
            }
        }
        // Cutting: the same tile has to stay in the beam long enough.
        match target {
            Some((tx, ty, tile)) if tile.is_diggable() && !is_foundation(tx, ty) => {
                let spent = match self.laser_cut {
                    Some((cx, cy, t)) if (cx, cy) == (tx, ty) => t + dt,
                    _ => dt,
                };
                if spent >= self.dig_duration(ty, tile) * LASER_CUT_SLOWDOWN {
                    self.world.set(tx, ty, Tile::Air);
                    self.events.push(Event::Dug { x: tx, y: ty, tile });
                    self.collect(tile, tx, ty);
                    self.laser_cut = None;
                } else {
                    self.laser_cut = Some((tx, ty, spent));
                }
            }
            _ => self.laser_cut = None,
        }
        self.strike(Weapon::Laser, Weapon::Laser.damage() * dt, |m| {
            Game::in_beam(m, x0, x1, y)
        });
        self.laser = Some(Beam { x0, x1, y, t: 0.0 });
    }

    fn launch_rocket(&mut self) {
        if !self.arsenal.rockets || self.pod.dig.is_some() || self.pod.fuel < ROCKET_FUEL {
            return;
        }
        self.burn(ROCKET_FUEL);
        let dir = self.pod.facing;
        self.rockets.push(Rocket {
            x: self.pod.x + dir * POD_HALF,
            y: self.pod.y,
            vx: dir * ROCKET_SPEED,
            life: 3.0,
        });
        self.events.push(Event::RocketFired);
    }

    fn update_rockets(&mut self, dt: f32) {
        let mut booms = Vec::new();
        let world = &self.world;
        let monsters = &self.monsters;
        self.rockets.retain_mut(|r| {
            r.life -= dt;
            let nx = r.x + r.vx * dt;
            let hits_rock = world.get(nx.floor() as i32, r.y.floor() as i32).is_solid();
            let hits_monster = monsters
                .iter()
                .any(|m| m.is_active() && m.touches(nx, r.y, 0.2));
            if hits_rock || hits_monster || r.life <= 0.0 {
                booms.push((nx, r.y));
                return false;
            }
            r.x = nx;
            true
        });
        for (x, y) in booms {
            self.explode(x, y, Weapon::Rocket);
        }
    }

    fn charge_ion(&mut self, dt: f32, held: bool) {
        if held && self.arsenal.ion && self.pod.dig.is_none() {
            self.ion_charge += dt;
            return;
        }
        let charged = self.ion_charge >= ION_CHARGE;
        self.ion_charge = 0.0;
        if !charged || self.pod.fuel < ION_FUEL {
            return;
        }
        self.burn(ION_FUEL);
        let (dir, y) = (self.pod.facing, self.pod.y);
        let x0 = self.pod.x + dir * POD_HALF;
        let mut x1 = x0 + dir * ION_RANGE;
        // It bores a tunnel through anything but the walls and the shop's footing.
        let ty = y.floor() as i32;
        let mut tx = x0.floor() as i32;
        let end = x1.floor() as i32;
        while tx != end + dir as i32 {
            let tile = self.world.get(tx, ty);
            if tile == Tile::Wall {
                x1 = if dir > 0.0 {
                    tx as f32
                } else {
                    tx as f32 + 1.0
                };
                break;
            }
            if tile.is_solid() && !is_foundation(tx, ty) {
                self.world.set(tx, ty, Tile::Air);
                self.collect(tile, tx, ty);
            }
            tx += dir as i32;
        }
        self.strike(Weapon::Ion, Weapon::Ion.damage(), |m| {
            Game::in_beam(m, x0, x1, y)
        });
        self.shake = self.shake.max(0.8);
        self.ion_beam = Some(Beam { x0, x1, y, t: 0.35 });
        self.events.push(Event::IonFired { x0, x1, y });
    }

    /// Hits every monster `reached` picks with `amount` of `weapon` damage,
    /// removing and announcing the ones it kills.
    fn strike(&mut self, weapon: Weapon, amount: f32, reached: impl Fn(&Monster) -> bool) {
        let mut dead = Vec::new();
        for (i, m) in self.monsters.iter_mut().enumerate() {
            if !reached(m) {
                continue;
            }
            match m.hit(weapon, amount) {
                Hit::Killed => dead.push(i),
                // A bomb that doesn't finish a Huge one still knocks it senseless.
                Hit::Hurt if weapon == Weapon::Bomb && m.size == Size::Huge => {
                    m.stun = 4.0;
                    self.events.push(Event::MonsterStunned { x: m.x, y: m.y });
                }
                Hit::Hurt | Hit::Immune => {}
            }
        }
        for i in dead.into_iter().rev() {
            let m = self.monsters.remove(i);
            // Only a heavy weapon's kill can drop a relic; the laser is the everyday tool.
            if weapon.is_heavy() && self.rng.f32() < drop_chance(m.size) {
                let relic = Relic::ALL[(self.rng.next_u64() % Relic::ALL.len() as u64) as usize];
                self.drops.push(RelicDrop {
                    x: m.x,
                    y: m.y,
                    vy: 0.0,
                    relic,
                });
                self.events.push(Event::RelicDropped { x: m.x, y: m.y });
            }
            self.events.push(Event::MonsterKilled {
                size: m.size,
                kind: m.kind,
                x: m.x,
                y: m.y,
                by: weapon,
            });
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
        // Larger tiers have two creatures each; the Moss Titan only lives deeper.
        let kind = match size {
            Size::Large if depth > 350.0 && self.rng.f32() < 0.5 => Kind::MossTitan,
            Size::Huge if self.rng.f32() < 0.5 => Kind::ShadowLeviathan,
            s => Kind::of_size(s),
        };
        self.monsters.push(Monster {
            dir: -side,
            ..Monster::new(kind, x, y, Phase::Warning(size.warning()))
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
            m.immune_flash = (m.immune_flash - dt).max(0.0);
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
                hit = Some((m.size, m.kind, m.x));
            }
        }
        self.monsters
            .retain(|m| (m.y - py).abs() < 18.0 && m.age < 45.0);

        let Some((size, kind, mx)) = hit else {
            return;
        };
        if self.pod.invuln > 0.0 {
            return;
        }
        let cause = format!("Caught by {}", kind.name());
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
            arsenal: self.arsenal,
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
        game.arsenal = s.arsenal;
        game.stashes = s.stashes;
        game.deaths = s.deaths;
        game.max_depth = s.max_depth;
        game.repair_foundation();
        game.free_pod();
        game
    }

    /// Puts back any foundation an older save dug out, and lifts the pod out
    /// to the shop if it was parked in that hole.
    fn repair_foundation(&mut self) {
        let mut pod_inside = false;
        for ty in 0..FOUNDATION_DEPTH {
            for tx in SHOP_COLUMNS.0 as i32..SHOP_COLUMNS.1 as i32 {
                if self.world.get(tx, ty) != Tile::Air {
                    continue;
                }
                self.world.set(tx, ty, Tile::Dirt);
                let (cx, cy) = (tx as f32 + 0.5, ty as f32 + 0.5);
                pod_inside |= (self.pod.x - cx).abs() < 0.5 + POD_HALF
                    && (self.pod.y - cy).abs() < 0.5 + POD_HALF;
            }
        }
        if pod_inside {
            self.pod.x = SPAWN.0;
            self.pod.y = SPAWN.1;
        }
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
    #[serde(default)]
    pub arsenal: Arsenal,
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
            arsenal: Arsenal::default(),
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
            dir: 1.0,
            ..Monster::new(Kind::of_size(Size::Huge), g.pod.x, g.pod.y, Phase::Active)
        });
    }

    /// Digging straight down from the spawn point moves the pod deeper.
    #[test]
    fn holding_down_digs_deeper() {
        let mut g = Game::new(5);
        g.monster_timer = f32::INFINITY;
        g.pod.x = SHOP_COLUMNS.0 - 7.5; // beside the shop; the ground under it is off limits
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
            g.monsters.push(Monster::new(
                Kind::of_size(size),
                20.0,
                700.5,
                Phase::Active,
            ));
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
            dir: 1.0,
            ..Monster::new(
                Kind::of_size(Size::Huge),
                2.5,
                601.0 - Size::Huge.dims().1 / 2.0,
                Phase::Active,
            )
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
            g.monsters
                .push(Monster::new(Kind::of_size(size), x, 600.5, Phase::Active));
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

    /// The shop stands on ground the drill can't touch.
    #[test]
    fn digging_under_the_shop_is_refused() {
        let mut g = Game::new(5);
        g.monster_timer = f32::INFINITY;
        run(&mut g, 2.0, down());
        assert!(g.depth() < 0.5, "dug under the shop to {} m", g.depth());
        assert_eq!(g.world.get(SPAWN.0 as i32, 0), Tile::Dirt);
    }

    /// Tunnelling in sideways from next to the shop is refused too.
    #[test]
    fn tunnelling_into_the_foundation_is_refused() {
        let mut g = Game::new(5);
        g.monster_timer = f32::INFINITY;
        let s0 = SHOP_COLUMNS.0 as i32;
        for x in s0 - 5..s0 {
            g.world.set(x, 0, Tile::Air);
        }
        g.pod.x = SHOP_COLUMNS.0 - 0.5;
        g.pod.y = 1.0 - POD_HALF - EPS;
        run(
            &mut g,
            2.0,
            Input {
                right: true,
                ..Input::default()
            },
        );
        assert_eq!(
            g.world.get(s0, 0),
            Tile::Dirt,
            "foundation tunnelled from the side"
        );
        assert!(
            g.pod.x < SHOP_COLUMNS.0,
            "pod got under the shop: x={}",
            g.pod.x
        );
    }

    /// A bomb next to the foundation clears around it but leaves it whole.
    #[test]
    fn blast_spares_the_shop_foundation() {
        let mut g = Game::new(5);
        let m = SPAWN.0 as i32;
        g.world.set(m, 2, Tile::Air);
        g.pod.y = 30.0;
        g.live_bombs.push(Bomb {
            x: m as f32 + 0.5,
            y: 2.5,
            vy: 0.0,
            fuse: 0.01,
        });
        g.update(DT, Input::default());
        for x in m - 1..=m + 1 {
            assert_eq!(
                g.world.get(x, 1),
                Tile::Dirt,
                "foundation at ({x},1) blasted"
            );
            assert_eq!(g.world.get(x, 3), Tile::Air, "({x},3) under it not cleared");
        }
    }

    /// A save made before the rule, with the foundation already dug out,
    /// loads with the ground back under the shop and the pod out from under it.
    #[test]
    fn loading_repairs_a_dug_out_foundation() {
        let mut data = Game::new(5).to_save();
        let (s0, s1) = (SHOP_COLUMNS.0 as i32, SHOP_COLUMNS.1 as i32);
        data.changes = (s0..s1).map(|x| (x, 0, Tile::Air)).collect();
        data.x = SPAWN.0 + 0.5;
        data.y = 1.0 - POD_HALF - EPS;
        let g = Game::from_save(data);
        for x in s0..s1 {
            assert_eq!(
                g.world.get(x, 0),
                Tile::Dirt,
                "foundation at ({x},0) still dug"
            );
        }
        assert_eq!((g.pod.x, g.pod.y), SPAWN, "pod left under the shop");
    }

    /// A pocket at `depth` with every gun owned, a full tank, and a pod that can't be hurt.
    fn armed(depth: i32) -> Game {
        let mut g = game_at_depth(depth);
        g.arsenal = Arsenal {
            laser: true,
            rockets: true,
            ion: true,
        };
        g.pod.fuel = 1000.0;
        g.pod.invuln = f32::INFINITY;
        g
    }

    fn monster(g: &mut Game, size: Size, x: f32) -> f32 {
        let y = g.pod.y;
        g.monsters
            .push(Monster::new(Kind::of_size(size), x, y, Phase::Active));
        y
    }

    fn hold(secs: f32, input: Input, g: &mut Game) -> Vec<Event> {
        let mut events = Vec::new();
        for _ in 0..(secs / DT) as usize {
            g.pod.invuln = f32::INFINITY;
            events.extend(g.update(DT, input));
        }
        events
    }

    /// Bigger monsters take more hits: Large falls to one bomb, Huge needs three.
    #[test]
    fn bigger_monsters_take_more_bomb_hits() {
        for (size, bombs) in [
            (Size::Small, 1),
            (Size::Medium, 1),
            (Size::Large, 1),
            (Size::Huge, 3),
        ] {
            let mut g = armed(300);
            let y = monster(&mut g, size, 20.0);
            for n in 1..=bombs {
                assert_eq!(
                    g.monsters.len(),
                    1,
                    "{size:?} died after {} bombs, expected {bombs}",
                    n - 1
                );
                g.explode(20.0, y, Weapon::Bomb);
            }
            assert!(g.monsters.is_empty(), "{size:?} survived {bombs} bombs");
        }
    }

    /// The laser and rockets can't touch the biggest, and the bar says so.
    #[test]
    fn the_biggest_shrug_off_laser_and_rockets() {
        let mut g = armed(300);
        let y = monster(&mut g, Size::Huge, 20.0);
        g.strike(Weapon::Laser, 100.0, |_| true);
        g.explode(20.0, y, Weapon::Rocket);
        let m = &g.monsters[0];
        assert_eq!(
            m.hp,
            Size::Huge.max_hp(),
            "a Huge took laser or rocket damage"
        );
        assert!(m.immune_flash > 0.0, "health bar wasn't told it's immune");
    }

    /// The laser burns down a small monster in about a second.
    #[test]
    fn laser_kills_a_small_monster_quickly() {
        let mut g = armed(300);
        monster(&mut g, Size::Small, 8.0);
        hold(
            1.0,
            Input {
                laser: true,
                ..Input::default()
            },
            &mut g,
        );
        assert!(
            g.monsters.is_empty(),
            "small monster survived a second of laser"
        );
    }

    /// The ion cannon bores a tunnel and kills even a Huge in two shots.
    #[test]
    fn ion_cannon_kills_the_biggest_in_two_shots() {
        let mut g = armed(300);
        monster(&mut g, Size::Huge, 12.0);
        for shot in 1..=2 {
            assert_eq!(g.monsters.len(), 1, "Huge died before shot {shot}");
            hold(
                ION_CHARGE + 0.1,
                Input {
                    ion: true,
                    ..Input::default()
                },
                &mut g,
            );
            hold(DT, Input::default(), &mut g);
        }
        assert!(g.monsters.is_empty(), "Huge survived two ion shots");
        let row = g.pod.y.floor() as i32;
        assert_eq!(
            g.world.get(14, row),
            Tile::Air,
            "ion shot didn't bore a tunnel"
        );
    }

    /// A rocket fired point-blank at rock breaks it without hurting the pod.
    #[test]
    fn rockets_break_rock_but_spare_the_pod() {
        let mut g = armed(300);
        g.pod.invuln = 0.0;
        g.pod.x = 4.5;
        g.pod.facing = -1.0;
        let full = g.pod.hull;
        let row = g.pod.y.floor() as i32;
        g.launch_rocket();
        for _ in 0..30 {
            g.update(DT, Input::default());
        }
        assert_eq!(
            g.world.get(3, row),
            Tile::Air,
            "rocket didn't break the rock"
        );
        assert_eq!(g.pod.hull, full, "own rocket hurt the pod");
    }

    /// Guns are bought once, at the shop.
    #[test]
    fn guns_are_bought_once() {
        let mut g = Game::new(1);
        g.money = 10_000;
        g.apply(ShopAction::BuyGun(Gun::Laser));
        g.apply(ShopAction::BuyGun(Gun::Laser));
        assert!(g.arsenal.laser);
        assert_eq!(
            g.money,
            10_000 - Gun::Laser.price(),
            "charged twice for the laser"
        );
    }

    /// A heavy-weapon kill of a Huge always drops a relic; laser kills never do.
    #[test]
    fn relics_come_only_from_heavy_kills() {
        let mut g = armed(300);
        let y = monster(&mut g, Size::Huge, 20.0);
        for _ in 0..3 {
            g.explode(20.0, y, Weapon::Bomb);
        }
        assert_eq!(g.drops.len(), 1, "a bombed Huge didn't drop a relic");
        for seed in 0..40 {
            let mut g = armed(300);
            g.rng = Rng::new(seed);
            monster(&mut g, Size::Large, 8.0);
            g.strike(Weapon::Laser, 100.0, |_| true);
            assert!(
                g.drops.is_empty(),
                "laser kill dropped a relic (seed {seed})"
            );
        }
    }

    /// Relics are permanent and stack past the shop's max level.
    #[test]
    fn relics_stack_past_the_shop_max() {
        let mut g = armed(300);
        g.levels.tank = MAX_LEVEL;
        let before = g.levels.fuel_max();
        g.drops.push(RelicDrop {
            x: g.pod.x,
            y: g.pod.y,
            vy: 0.0,
            relic: Relic::ReserveTank,
        });
        let events = g.update(DT, Input::default());
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::RelicFound(Relic::ReserveTank)))
        );
        assert_eq!(g.levels.fuel_max(), before + 15.0);
        let back = Game::from_save(
            serde_json::from_str(&serde_json::to_string(&g.to_save()).unwrap()).unwrap(),
        );
        assert_eq!(
            back.levels.reserve_tank, 1,
            "relic lost in a save round trip"
        );
        assert!(back.arsenal.ion, "guns lost in a save round trip");
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
