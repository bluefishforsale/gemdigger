//! Things that live in the deep. They burrow sideways through rock but never
//! up or down, so flying straight up is always an escape. Size sets how hard
//! they hit, up to the biggest, which kills outright.

/// How big a monster is; bigger ones live deeper and hit harder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    Small,
    Medium,
    Large,
    Huge,
}

impl Size {
    pub const ALL: [Size; 4] = [Size::Small, Size::Medium, Size::Large, Size::Huge];

    /// Width and height in tiles.
    pub fn dims(self) -> (f32, f32) {
        match self {
            Size::Small => (0.9, 0.8),
            Size::Medium => (1.3, 1.0),
            Size::Large => (2.0, 1.8),
            Size::Huge => (3.6, 2.9),
        }
    }

    /// Sideways speed in tiles per second; always slower than the pod.
    pub fn speed(self) -> f32 {
        match self {
            Size::Small => 2.4,
            Size::Medium => 2.0,
            Size::Large => 1.5,
            Size::Huge => 1.2,
        }
    }

    /// Hull damage on contact, or `None` when contact is fatal.
    pub fn damage(self) -> Option<f32> {
        match self {
            Size::Small => Some(10.0),
            Size::Medium => Some(25.0),
            Size::Large => Some(42.0),
            Size::Huge => None,
        }
    }

    /// How hard the screen shakes while it's close.
    pub fn presence(self) -> f32 {
        match self {
            Size::Small => 0.0,
            Size::Medium => 0.08,
            Size::Large => 0.3,
            Size::Huge => 0.6,
        }
    }

    /// Seconds of rumbling before it breaks out of the rock.
    pub fn warning(self) -> f32 {
        match self {
            Size::Huge => 3.0,
            _ => 2.0,
        }
    }

    /// Hit points at full health.
    pub fn max_hp(self) -> f32 {
        match self {
            Size::Small => 3.0,
            Size::Medium => 8.0,
            Size::Large => 20.0,
            Size::Huge => 60.0,
        }
    }

    /// The biggest only feel bombs and the ion cannon.
    pub fn immune_to(self, weapon: Weapon) -> bool {
        self == Size::Huge && matches!(weapon, Weapon::Laser | Weapon::Rocket)
    }

    /// Relative chance of this size appearing at a depth (in tiles).
    pub fn weight(self, depth: f32) -> f32 {
        let ramp = |a: f32, b: f32| ((depth - a) / (b - a)).clamp(0.0, 1.0);
        match self {
            Size::Small => 1.0,
            Size::Medium => ramp(120.0, 300.0),
            Size::Large => 0.8 * ramp(250.0, 500.0),
            Size::Huge => 0.6 * ramp(420.0, 800.0),
        }
    }
}

/// Where a monster is in its life.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    /// Rumbling inside the rock; harmless, counting down to emerging.
    Warning(f32),
    Active,
}

/// One monster. `y` never changes after spawning.
#[derive(Clone, Debug)]
pub struct Monster {
    pub kind: Kind,
    pub size: Size,
    /// Hit points left; at zero it dies.
    pub hp: f32,
    /// Counts down after a hit that couldn't hurt it, so the health bar can say why.
    pub immune_flash: f32,
    pub x: f32,
    pub y: f32,
    pub dir: f32,
    pub phase: Phase,
    pub age: f32,
    /// Seconds left frozen by a bomb blast.
    pub stun: f32,
}

/// Which creature this is. Each belongs to one size tier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    VoidSpider,
    AncientWorm,
    CrystalGolem,
    MossTitan,
    ShadowLeviathan,
    DeepOne,
}

impl Kind {
    pub fn size(self) -> Size {
        match self {
            Kind::VoidSpider => Size::Small,
            Kind::AncientWorm => Size::Medium,
            Kind::CrystalGolem | Kind::MossTitan => Size::Large,
            Kind::ShadowLeviathan | Kind::DeepOne => Size::Huge,
        }
    }

    /// The usual creature for a size tier.
    pub fn of_size(size: Size) -> Kind {
        match size {
            Size::Small => Kind::VoidSpider,
            Size::Medium => Kind::AncientWorm,
            Size::Large => Kind::CrystalGolem,
            Size::Huge => Kind::DeepOne,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::VoidSpider => "a void spider",
            Kind::AncientWorm => "an ancient worm",
            Kind::CrystalGolem => "a crystal golem",
            Kind::MossTitan => "a moss titan",
            Kind::ShadowLeviathan => "the Shadow Leviathan",
            Kind::DeepOne => "the Deep One",
        }
    }

    /// Sprite name prefix; frames are `{prefix}_0` to `{prefix}_7`.
    pub fn sprite(self) -> &'static str {
        match self {
            Kind::VoidSpider => "monster_void_spider",
            Kind::AncientWorm => "monster_ancient_worm",
            Kind::CrystalGolem => "monster_crystal_golem",
            Kind::MossTitan => "monster_moss_titan",
            Kind::ShadowLeviathan => "monster_shadow_leviathan",
            Kind::DeepOne => "monster_deep_one",
        }
    }
}

/// What hit a monster. Damage is per hit, except the laser's, which is per second.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weapon {
    Laser,
    Rocket,
    Bomb,
    Ion,
}

impl Weapon {
    pub fn damage(self) -> f32 {
        match self {
            Weapon::Laser => 4.0,
            Weapon::Rocket => 8.0,
            Weapon::Bomb => 25.0,
            Weapon::Ion => 40.0,
        }
    }

    /// The heavy hitters, the only ones that can drop a relic.
    pub fn is_heavy(self) -> bool {
        matches!(self, Weapon::Rocket | Weapon::Bomb | Weapon::Ion)
    }
}

/// What a hit did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Hurt,
    Killed,
    /// Bounced off: this weapon can't hurt this size of monster.
    Immune,
}

impl Monster {
    pub fn new(kind: Kind, x: f32, y: f32, phase: Phase) -> Monster {
        let size = kind.size();
        Monster {
            kind,
            size,
            hp: size.max_hp(),
            immune_flash: 0.0,
            x,
            y,
            dir: 1.0,
            phase,
            age: 0.0,
            stun: 0.0,
        }
    }

    pub fn is_active(&self) -> bool {
        self.phase == Phase::Active
    }

    /// Applies `amount` of `weapon` damage.
    pub fn hit(&mut self, weapon: Weapon, amount: f32) -> Hit {
        if self.size.immune_to(weapon) {
            self.immune_flash = 1.5;
            return Hit::Immune;
        }
        self.hp -= amount;
        if self.hp <= 0.0 {
            Hit::Killed
        } else {
            Hit::Hurt
        }
    }

    /// Health left, 0 to 1.
    pub fn health(&self) -> f32 {
        (self.hp / self.size.max_hp()).clamp(0.0, 1.0)
    }

    /// Whether this monster's body overlaps a box centered at (x, y).
    pub fn touches(&self, x: f32, y: f32, half: f32) -> bool {
        // Slightly forgiving hitbox so grazes don't count.
        let (w, h) = self.size.dims();
        let (hw, hh) = (w * 0.42, h * 0.42);
        (self.x - x).abs() < hw + half && (self.y - y).abs() < hh + half
    }
}
