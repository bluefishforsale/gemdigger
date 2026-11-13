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

    pub fn name(self) -> &'static str {
        match self {
            Size::Small => "a cave worm",
            Size::Medium => "a shadow bat",
            Size::Large => "a rock crawler",
            Size::Huge => "the Deep Thing",
        }
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
    pub size: Size,
    pub x: f32,
    pub y: f32,
    pub dir: f32,
    pub phase: Phase,
    pub age: f32,
    /// Seconds left frozen by a bomb blast.
    pub stun: f32,
}

impl Monster {
    /// Whether this monster's body overlaps a box centered at (x, y).
    pub fn touches(&self, x: f32, y: f32, half: f32) -> bool {
        // Slightly forgiving hitbox so grazes don't count.
        let (w, h) = self.size.dims();
        let (hw, hh) = (w * 0.42, h * 0.42);
        (self.x - x).abs() < hw + half && (self.y - y).abs() < hh + half
    }
}
