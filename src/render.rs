//! Everything on screen. Draws the game, turns events into particles, toasts
//! and flashes, and makes the deep feel dark; it never changes the rules.

use std::f32::consts::{FRAC_PI_2, TAU};

use macroquad::prelude::*;
use macroquad::rand::gen_range;

use crate::art::Art;
use crate::critters;
use crate::game::{
    BASE_KIT_PRICE, BOMB_POUCH, Event, FUSE, Game, Gun, Item, Loot, MAX_BASE_KITS, MAX_LEVEL,
    POD_HALF, SHOP_COLUMNS, ShopAction, SiteKind, Upgrade,
};
use crate::monsters::{Monster, Phase, Size};
use crate::world::Mineral;
use crate::world::{self, Tile, WIDTH};

/// Pixels per tile.
pub const TILE: f32 = 48.0;

const ORE_COLORS: [Color; 6] = [
    Color::new(0.85, 0.50, 0.25, 1.0),
    Color::new(0.85, 0.88, 0.92, 1.0),
    Color::new(1.00, 0.80, 0.25, 1.0),
    Color::new(0.25, 0.90, 0.45, 1.0),
    Color::new(0.95, 0.20, 0.30, 1.0),
    Color::new(0.75, 0.45, 1.00, 1.0),
];
const BAND_COLORS: [Color; 5] = [
    Color::new(0.47, 0.33, 0.22, 1.0),
    Color::new(0.70, 0.36, 0.16, 1.0),
    Color::new(0.45, 0.45, 0.47, 1.0),
    Color::new(0.28, 0.33, 0.43, 1.0),
    Color::new(0.17, 0.13, 0.23, 1.0),
];
/// Height of the green ridge in front of the mountains, before its waves.
const RIDGE: f32 = 64.0;
const GOLD: Color = Color::new(1.0, 0.82, 0.3, 1.0);
const PANEL: Color = Color::new(0.06, 0.05, 0.09, 0.82);

struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    life: f32,
    max: f32,
    size: f32,
    color: Color,
    gravity: f32,
    glow: bool,
}

struct Toast {
    text: String,
    age: f32,
    /// How many times this same message arrived in a row.
    count: u32,
}

/// A clickable row in the shop panel.
pub struct ShopButton {
    pub rect: Rect,
    pub action: ShopAction,
    pub key: Option<KeyCode>,
    label: String,
    detail: String,
    enabled: bool,
}

pub struct Renderer {
    particles: Vec<Particle>,
    toasts: Vec<Toast>,
    cam: Vec2,
    view: Vec2,
    light: Texture2D,
    glow: Texture2D,
    t: f32,
    flash: f32,
    birds: Vec<Bird>,
    effects: Vec<Effect>,
    next_flock: f32,
}

/// A one-shot sprite animation (an explosion, a smoke puff) in world space.
struct Effect {
    /// Frames are `{prefix}_0`, `{prefix}_1`, ...
    prefix: &'static str,
    x: f32,
    y: f32,
    t: f32,
    dur: f32,
    /// Size in tiles.
    size: f32,
    /// Tiles per second it drifts upward.
    rise: f32,
    /// Plays the frames in order; otherwise holds one frame picked by `variant`.
    animate: bool,
    variant: usize,
}

/// A small bird crossing the sky, in screen space above the horizon.
struct Bird {
    x: f32,
    height: f32,
    vx: f32,
    phase: f32,
    size: f32,
}

fn radial(size: u16, alpha: impl Fn(f32) -> f32) -> Texture2D {
    let mut img = Image::gen_image_color(size, size, Color::new(1.0, 1.0, 1.0, 0.0));
    let c = size as f32 / 2.0;
    for y in 0..size as u32 {
        for x in 0..size as u32 {
            let r = ((x as f32 + 0.5 - c).hypot(y as f32 + 0.5 - c) / c).min(1.0);
            img.set_pixel(x, y, Color::new(1.0, 1.0, 1.0, alpha(r)));
        }
    }
    let tex = Texture2D::from_image(&img);
    tex.set_filter(FilterMode::Linear);
    tex
}

fn with_alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * a)
}

fn mix(a: Color, b: Color, k: f32) -> Color {
    Color::new(
        a.r + (b.r - a.r) * k,
        a.g + (b.g - a.g) * k,
        a.b + (b.b - a.b) * k,
        a.a + (b.a - a.a) * k,
    )
}

fn money(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    format!("${out}")
}

fn text(s: &str, x: f32, y: f32, size: f32, color: Color) {
    // NOTE: whole pixels only. A label that follows something moving (the
    // edge arrows) otherwise lands between pixels and shimmers as it travels.
    let (x, y) = (x.round(), y.round());
    draw_text(s, x + 2.0, y + 2.0, size, with_alpha(BLACK, color.a * 0.7));
    draw_text(s, x, y, size, color);
}

fn text_centered(s: &str, cx: f32, y: f32, size: f32, color: Color) {
    let w = measure_text(s, None, size as u16, 1.0).width;
    text(s, cx - w / 2.0, y, size, color);
}

fn tile_color(tile: Tile, y: i32) -> Color {
    match tile {
        Tile::Ore(t) => ORE_COLORS[(t as usize).min(5)],
        Tile::Mineral(m) => mineral_color(m),
        Tile::Boulder => Color::new(0.6, 0.6, 0.6, 1.0),
        _ => BAND_COLORS[world::band(y)],
    }
}

fn mineral_color(m: Mineral) -> Color {
    match m {
        Mineral::Coal => Color::new(0.16, 0.15, 0.18, 1.0),
        Mineral::Sulfur => Color::new(0.97, 0.86, 0.2, 1.0),
        Mineral::Potassium => Color::new(0.93, 0.86, 0.96, 1.0),
    }
}

/// Chunks of a mineral set into a dirt tile: coal lumps, sulfur crystals,
/// pale potassium spars.
fn draw_mineral(p: Vec2, m: Mineral, h: u64) {
    let color = mineral_color(m);
    let rim = mix(color, WHITE, 0.45);
    let (sides, base) = match m {
        Mineral::Coal => (5u8, 8.0),
        Mineral::Sulfur => (6, 6.5),
        Mineral::Potassium => (4, 7.0),
    };
    for i in 0..4u64 {
        let bits = h >> (i * 12);
        let ox = 10.0 + (bits & 0x1f) as f32 / 31.0 * (TILE - 20.0);
        let oy = 10.0 + ((bits >> 5) & 0x1f) as f32 / 31.0 * (TILE - 20.0);
        let r = base * (0.7 + ((bits >> 10) & 0x3) as f32 * 0.15);
        let rot = (bits & 0xff) as f32;
        draw_poly(
            p.x + ox,
            p.y + oy,
            sides,
            r + 1.5,
            rot,
            Color::new(0.0, 0.0, 0.0, 0.45),
        );
        draw_poly(p.x + ox, p.y + oy, sides, r, rot, color);
        draw_circle(p.x + ox - r * 0.3, p.y + oy - r * 0.3, r * 0.25, rim);
    }
}

fn item_color(item: Item) -> Color {
    match item {
        Item::Gem(t) => ORE_COLORS[(t as usize).min(5)],
        Item::Mineral(m) => mineral_color(m),
        Item::Meat => Color::new(0.85, 0.35, 0.3, 1.0),
    }
}

/// One tree of four kinds; `shade` picks its green (rarely an autumn gold).
fn draw_tree(x: f32, base: f32, s: f32, kind: u64, shade: f32) {
    let leaf = match shade {
        k if k < 0.35 => Color::new(0.2, 0.44, 0.24, 1.0),
        k if k < 0.65 => Color::new(0.3, 0.5, 0.22, 1.0),
        k if k < 0.9 => Color::new(0.18, 0.42, 0.34, 1.0),
        _ => Color::new(0.62, 0.5, 0.2, 1.0),
    };
    let light = mix(leaf, Color::new(0.95, 0.95, 0.7, 1.0), 0.22);
    let dark = mix(leaf, BLACK, 0.25);
    let bark = Color::new(0.36, 0.25, 0.17, 1.0);
    match kind {
        // Round broadleaf: a trunk under a cloud of three leafy puffs.
        0 => {
            let (tw, th) = (4.0 * s, 13.0 * s);
            draw_rectangle(x - tw / 2.0, base - th, tw, th, bark);
            let cy = base - th - 7.0 * s;
            draw_circle(x - 7.0 * s, cy + 3.0 * s, 8.0 * s, dark);
            draw_circle(x + 7.0 * s, cy + 3.0 * s, 8.0 * s, dark);
            draw_circle(x, cy - 2.0 * s, 10.5 * s, leaf);
            draw_circle(x - 3.5 * s, cy - 5.5 * s, 4.5 * s, light);
        }
        // Conifer: short trunk, three stacked tiers.
        1 => {
            let (tw, th) = (3.0 * s, 6.0 * s);
            draw_rectangle(x - tw / 2.0, base - th, tw, th, bark);
            for (k, w) in [(0.0, 11.0), (1.0, 8.5), (2.0, 6.0)] {
                let bottom = base - th - k * 8.0 * s;
                let top = bottom - 13.0 * s;
                let fill = if k == 2.0 { leaf } else { dark };
                draw_triangle(
                    vec2(x - w * s, bottom),
                    vec2(x + w * s, bottom),
                    vec2(x, top),
                    fill,
                );
                draw_triangle(
                    vec2(x - w * s * 0.2, bottom - 2.0 * s),
                    vec2(x, top),
                    vec2(x - w * s * 0.75, bottom - s),
                    light,
                );
            }
        }
        // Poplar: tall and narrow.
        2 => {
            let (tw, th) = (3.0 * s, 8.0 * s);
            draw_rectangle(x - tw / 2.0, base - th, tw, th, bark);
            draw_ellipse(x, base - th - 14.0 * s, 6.5 * s, 16.0 * s, 0.0, leaf);
            draw_ellipse(
                x - 2.0 * s,
                base - th - 18.0 * s,
                2.5 * s,
                8.0 * s,
                0.0,
                light,
            );
        }
        // Bush: low and wide, no trunk showing.
        _ => {
            draw_circle(x - 6.0 * s, base - 4.0 * s, 6.0 * s, dark);
            draw_circle(x + 6.0 * s, base - 4.0 * s, 6.5 * s, dark);
            draw_circle(x, base - 7.0 * s, 7.5 * s, leaf);
            draw_circle(x - 2.5 * s, base - 9.5 * s, 3.0 * s, light);
        }
    }
}

/// How dark the deep is (0 = daylight) at a depth in tiles.
fn darkness(depth: f32) -> f32 {
    let deep = ((depth - 10.0) / 350.0).clamp(0.0, 1.0).powf(0.8) * 0.96;
    // NOTE: a hard edge at the shaft mouth: daylight stops the moment she's
    // below the surface, so the shop above reads as the one lit room.
    if depth < 0.6 { 0.0 } else { deep.max(0.3) }
}

/// How far the pod can see, in tiles.
fn light_radius(depth: f32, lamp: f32) -> f32 {
    let k = ((depth - 10.0) / 450.0).clamp(0.0, 1.0);
    let base = 14.0 + (3.2 - 14.0) * k - ((depth - 500.0).max(0.0) / 3000.0).min(0.8);
    base + lamp
}

fn monster_sprite(size: Size) -> &'static str {
    match size {
        Size::Small => "monster_small",
        Size::Medium => "monster_medium",
        Size::Large | Size::Huge => "monster_large",
    }
}

impl Renderer {
    pub fn new() -> Renderer {
        Renderer {
            particles: Vec::new(),
            toasts: Vec::new(),
            cam: vec2(crate::game::SPAWN.0 * TILE, 0.0),
            view: Vec2::ZERO,
            light: radial(256, |r| {
                let k = ((r - 0.3) / 0.7).clamp(0.0, 1.0);
                k * k * (3.0 - 2.0 * k)
            }),
            glow: radial(64, |r| (1.0 - r).powi(2)),
            t: 0.0,
            flash: 0.0,
            birds: Vec::new(),
            effects: Vec::new(),
            next_flock: 4.0,
        }
    }

    /// Jumps the camera onto the pod instead of easing over from the spawn.
    pub fn snap_to(&mut self, game: &Game) {
        self.cam = vec2(game.pod.x, game.pod.y) * TILE;
    }

    /// Sends a flock across right away rather than after the usual wait.
    pub fn prime_birds(&mut self) {
        self.next_flock = 0.0;
    }

    #[allow(clippy::too_many_arguments)] // Mirrors Effect's fields.
    fn effect(
        &mut self,
        prefix: &'static str,
        x: f32,
        y: f32,
        dur: f32,
        size: f32,
        rise: f32,
        animate: bool,
    ) {
        self.effects.push(Effect {
            prefix,
            x,
            y,
            t: 0.0,
            dur,
            size,
            rise,
            animate,
            variant: gen_range(0, 16),
        });
    }

    fn draw_effects(&self, art: &Art) {
        for e in &self.effects {
            let n = (0..16)
                .take_while(|i| art.get(&format!("{}_{i}", e.prefix)).is_some())
                .count();
            if n == 0 {
                continue;
            }
            let k = e.t / e.dur;
            let frame = if e.animate {
                ((k * n as f32) as usize).min(n - 1)
            } else {
                e.variant % n
            };
            let Some(s) = art.get(&format!("{}_{frame}", e.prefix)) else {
                continue;
            };
            let size = e.size
                * TILE
                * if e.animate {
                    0.7 + 0.5 * k
                } else {
                    0.8 + 0.4 * k
                };
            let a = if e.animate {
                1.0 - k * 0.5
            } else {
                0.8 * (1.0 - k)
            };
            let r = (size / s.size().x).min(size / s.size().y);
            let (w, h) = (s.size().x * r, s.size().y * r);
            let p = self.screen(e.x, e.y);
            s.draw(p.x - w / 2.0, p.y - h / 2.0, w, h, with_alpha(WHITE, a));
        }
    }

    pub fn toast(&mut self, text: impl Into<String>) {
        let text = text.into();
        // The same message again just bumps a counter instead of stacking.
        if let Some(last) = self.toasts.last_mut()
            && last.text == text
        {
            last.count += 1;
            last.age = 0.0;
            return;
        }
        self.toasts.push(Toast {
            text,
            age: 0.0,
            count: 1,
        });
        if self.toasts.len() > 4 {
            self.toasts.remove(0);
        }
    }

    #[allow(clippy::too_many_arguments)] // Private one-liner spray helper; a struct would just rename the args.
    fn burst(
        &mut self,
        x: f32,
        y: f32,
        n: usize,
        color: Color,
        speed: f32,
        gravity: f32,
        glow: bool,
    ) {
        for _ in 0..n {
            let a = gen_range(0.0, TAU);
            let s = gen_range(0.3, 1.0) * speed;
            let life = gen_range(0.4, 0.9);
            self.particles.push(Particle {
                x,
                y,
                vx: a.cos() * s,
                vy: a.sin() * s,
                life,
                max: life,
                size: gen_range(2.0, 5.0),
                color,
                gravity,
                glow,
            });
        }
    }

    pub fn update(&mut self, dt: f32, game: &Game, events: &[Event]) {
        self.t += dt;
        self.flash = (self.flash - dt * 1.5).max(0.0);
        let mut target = vec2(game.pod.x, game.pod.y) * TILE;
        // NOTE: the shop panel covers the right ~720 px, so while shopping the
        // camera slides right to keep the pod and building in the open space
        // to its left instead of hidden under the menu.
        if game.at_shop() {
            target.x += 360.0;
        }
        self.cam += (target - self.cam) * (1.0 - (-dt * 8.0).exp());

        for e in events {
            self.react(e, game);
        }

        let p = &game.pod;
        if p.thrusting {
            for _ in 0..3 {
                let life = gen_range(0.15, 0.35);
                self.particles.push(Particle {
                    x: p.x - p.facing * 0.25 + gen_range(-0.1, 0.1),
                    y: p.y + POD_HALF,
                    vx: gen_range(-0.8, 0.8),
                    vy: gen_range(3.0, 6.0),
                    life,
                    max: life,
                    size: gen_range(3.0, 6.0),
                    color: mix(
                        Color::new(1.0, 0.9, 0.4, 1.0),
                        Color::new(1.0, 0.3, 0.1, 1.0),
                        gen_range(0.0, 1.0),
                    ),
                    gravity: 0.0,
                    glow: true,
                });
            }
        }
        if let Some(d) = p.dig
            && gen_range(0.0, 1.0) < 0.6
        {
            let (tx, ty) = (d.tx as f32 + 0.5, d.ty as f32 + 0.5);
            let color = tile_color(game.world.get(d.tx, d.ty), d.ty);
            self.burst(
                tx + gen_range(-0.3, 0.3),
                ty + gen_range(-0.3, 0.3),
                1,
                color,
                3.0,
                12.0,
                false,
            );
        }
        for m in &game.monsters {
            if matches!(m.phase, Phase::Warning(_)) && gen_range(0.0, 1.0) < 0.4 {
                let (w, h) = m.size.dims();
                let color = tile_color(Tile::Dirt, m.y as i32);
                self.burst(
                    m.x + gen_range(-w, w) / 2.0,
                    m.y - h / 2.0,
                    1,
                    color,
                    0.5,
                    10.0,
                    false,
                );
            }
        }

        for pt in &mut self.particles {
            pt.vy += pt.gravity * dt;
            pt.x += pt.vx * dt;
            pt.y += pt.vy * dt;
            pt.life -= dt;
        }
        self.particles.retain(|pt| pt.life > 0.0);
        for toast in &mut self.toasts {
            toast.age += dt;
        }
        self.toasts.retain(|t| t.age < 3.5);

        // Every so often a small flock crosses the sky, in a loose V.
        let sw = screen_width();
        self.next_flock -= dt;
        if self.next_flock <= 0.0 {
            self.next_flock = gen_range(10.0, 24.0);
            let from_left = gen_range(0.0, 1.0) < 0.5;
            let vx: f32 = gen_range(45.0, 75.0) * if from_left { 1.0 } else { -1.0 };
            let (x0, h0) = (
                if from_left { -40.0 } else { sw + 40.0 },
                gen_range(180.0, 330.0),
            );
            for i in 0..gen_range(3, 7) {
                let rank = ((i + 1) / 2) as f32;
                let side = if i % 2 == 0 { 1.0 } else { -1.0 };
                self.birds.push(Bird {
                    x: x0 - vx.signum() * rank * 22.0,
                    height: h0 + side * rank * 12.0,
                    vx: vx * gen_range(0.95, 1.05),
                    phase: gen_range(0.0, TAU),
                    size: gen_range(0.8, 1.2),
                });
            }
        }
        for b in &mut self.birds {
            b.x += b.vx * dt;
        }
        self.birds.retain(|b| b.x > -120.0 && b.x < sw + 120.0);

        for e in &mut self.effects {
            e.t += dt;
            e.y -= e.rise * dt;
        }
        self.effects.retain(|e| e.t < e.dur);
    }

    fn react(&mut self, e: &Event, game: &Game) {
        match e {
            Event::DigStarted { x, y } => {
                if gen_range(0.0, 1.0) < 0.35 {
                    self.effect(
                        "fx_dust",
                        *x as f32 + 0.5,
                        *y as f32 + 0.3,
                        0.6,
                        1.0,
                        0.6,
                        false,
                    );
                }
                let color = tile_color(game.world.get(*x, *y), *y);
                self.burst(*x as f32 + 0.5, *y as f32 + 0.5, 8, color, 2.5, 8.0, false);
            }
            Event::Dug { x, y, tile } => {
                self.burst(
                    *x as f32 + 0.5,
                    *y as f32 + 0.5,
                    14,
                    tile_color(*tile, *y),
                    4.0,
                    14.0,
                    false,
                );
            }
            Event::Pickup { item, x, y } => {
                let extra = if let Item::Gem(t) = item {
                    4 * (*t as usize).min(5)
                } else {
                    0
                };
                self.burst(*x, *y, 18 + extra, item_color(*item), 5.0, 0.0, true);
                match item {
                    Item::Gem(_) | Item::Meat => {
                        self.toast(format!("+1 {}  ({})", item.name(), money(item.value())))
                    }
                    Item::Mineral(_) => self.toast(format!("+1 {}  (for bombs)", item.name())),
                }
            }
            Event::LeftBehind(items) => {
                let names: Vec<&str> = items.iter().take(5).map(|i| i.name()).collect();
                let more = if items.len() > 5 { ", ..." } else { "" };
                self.toast(format!(
                    "Hold full. Left behind: {}{more}",
                    names.join(", ")
                ));
            }
            Event::CantBomb(why) => self.toast(*why),
            Event::Towed => {
                self.cam = vec2(game.pod.x, game.pod.y) * TILE;
            }
            Event::Landed { .. } => {
                let p = &game.pod;
                self.burst(
                    p.x,
                    p.y + POD_HALF,
                    10,
                    tile_color(Tile::Dirt, p.y as i32 + 1),
                    2.5,
                    6.0,
                    false,
                );
            }
            Event::Hurt { .. } => {
                let p = &game.pod;
                self.burst(p.x, p.y, 16, Color::new(1.0, 0.3, 0.2, 1.0), 6.0, 6.0, true);
            }
            Event::Died {
                y, cause, dropped, ..
            } => {
                self.flash = 1.0;
                if *dropped > 0 {
                    self.toast(format!(
                        "{cause}! Your {dropped} gems are waiting at {} m.",
                        y.max(0.0).round()
                    ));
                } else {
                    self.toast(format!("{cause}!"));
                }
            }
            Event::Respawned => {
                self.cam = vec2(game.pod.x, game.pod.y) * TILE;
                self.burst(
                    game.pod.x,
                    game.pod.y,
                    30,
                    Color::new(0.6, 0.9, 1.0, 1.0),
                    4.0,
                    0.0,
                    true,
                );
            }
            Event::Sold { amount } => {
                self.toast(format!("Sold for {}!", money(*amount)));
                self.burst(game.pod.x, game.pod.y - 0.5, 30, GOLD, 5.0, 6.0, true);
            }
            Event::Refueled => self.toast("Fuel tank filled"),
            Event::Repaired => self.toast("Hull repaired"),
            Event::Bought(u) => {
                self.toast(format!("{} upgraded!", u.name()));
                self.burst(
                    game.pod.x,
                    game.pod.y,
                    30,
                    Color::new(0.5, 1.0, 0.7, 1.0),
                    5.0,
                    0.0,
                    true,
                );
            }
            Event::CantAfford => self.toast("Not enough money yet"),
            Event::LowFuel => self.toast("Low fuel! Fly up to the shop to refuel."),
            Event::MonsterWarning { .. } => self.toast("The ground rumbles..."),
            Event::MonsterEmerged { size, x, y } => {
                let (w, _) = size.dims();
                self.burst(
                    *x,
                    *y,
                    (12.0 * w) as usize,
                    tile_color(Tile::Dirt, *y as i32),
                    3.0 + w,
                    10.0,
                    false,
                );
                match size {
                    Size::Huge => self.toast("RUN! Fly straight up!"),
                    Size::Large => self.toast("Something big is coming... fly up!"),
                    _ => {}
                }
            }
            Event::StashRecovered { x, y, count } => {
                self.toast(format!("Recovered {count} lost items!"));
                self.burst(*x, *y, 40, Color::new(0.8, 0.5, 1.0, 1.0), 6.0, 0.0, true);
            }
            Event::Crafted => {
                self.toast("Bomb crafted! Press B underground to drop one.");
                self.burst(
                    game.pod.x,
                    game.pod.y - 0.5,
                    30,
                    Color::new(1.0, 0.6, 0.2, 1.0),
                    4.0,
                    0.0,
                    true,
                );
            }
            Event::CantCraft => {
                if game.bombs >= BOMB_POUCH {
                    self.toast("Bomb pouch is full");
                } else {
                    self.toast("A bomb needs 1 Coal, 1 Sulfur and 1 Potassium");
                }
            }
            Event::BombPlaced { .. } => self.toast("Fuse lit! Get clear!"),
            Event::Exploded { x, y } => {
                self.flash = self.flash.max(0.35);
                self.effect("fx_explosion", *x, *y, 0.45, 3.2, 0.0, true);
                for i in 0..3 {
                    let k = i as f32;
                    self.effect(
                        "fx_smoke",
                        *x + (k - 1.0) * 0.6,
                        *y - 0.3,
                        1.6 + 0.3 * k,
                        1.6,
                        0.9,
                        false,
                    );
                }
                self.burst(*x, *y, 60, Color::new(1.0, 0.65, 0.2, 1.0), 9.0, 0.0, true);
                self.burst(*x, *y, 30, Color::new(1.0, 0.95, 0.7, 1.0), 4.0, 0.0, true);
                self.burst(
                    *x,
                    *y,
                    40,
                    tile_color(Tile::Dirt, *y as i32),
                    7.0,
                    14.0,
                    false,
                );
            }
            Event::MonsterKilled { x, y, .. } => {
                self.toast("Got it!");
                self.burst(*x, *y, 30, Color::new(0.6, 0.3, 0.8, 1.0), 6.0, 8.0, true);
            }
            Event::MonsterStunned { .. } => self.toast("Stunned it! Fly up!"),
            Event::BoughtGun(g) => {
                self.toast(format!("{} bought!", g.name()));
                self.burst(
                    game.pod.x,
                    game.pod.y,
                    30,
                    Color::new(0.5, 0.9, 1.0, 1.0),
                    5.0,
                    0.0,
                    true,
                );
            }
            Event::RocketFired => {}
            Event::BaseBuilt { x, y } => {
                self.toast("Base built! You'll restart here, and it's a full shop.");
                self.burst(*x, *y, 40, Color::new(1.0, 0.75, 0.35, 1.0), 5.0, 0.0, true);
            }
            Event::BoughtBaseKit => self.toast("Base kit bought. Press P underground to build it."),
            Event::TeleportArmed => {
                self.toast("Press T again to sacrifice your cargo and teleport")
            }
            Event::Teleported { sacrificed } => {
                self.toast(format!("Teleported! {sacrificed} items sacrificed."));
                self.flash = self.flash.max(0.6);
                self.cam = vec2(game.pod.x, game.pod.y) * TILE;
                self.burst(
                    game.pod.x,
                    game.pod.y,
                    50,
                    Color::new(0.7, 0.5, 1.0, 1.0),
                    6.0,
                    0.0,
                    true,
                );
            }
            Event::CantTeleport(why) => self.toast(*why),
            Event::Pinged => {}
            Event::CantBuild(why) => self.toast(*why),
            Event::RelicDropped { x, y } => {
                self.toast("Something dropped a relic!");
                self.burst(*x, *y, 30, GOLD, 5.0, 0.0, true);
            }
            Event::RelicFound(r) => {
                self.toast(format!("Relic found! {}", r.name()));
                self.flash = self.flash.max(0.3);
                self.burst(game.pod.x, game.pod.y, 50, GOLD, 6.0, 0.0, true);
            }
            Event::IonFired { x0, x1, y } => {
                self.flash = self.flash.max(0.12);
                let n = ((x1 - x0).abs() * 3.0) as usize;
                for i in 0..n {
                    let x = x0 + (x1 - x0) * i as f32 / n.max(1) as f32;
                    self.burst(x, *y, 1, Color::new(0.6, 0.9, 1.0, 1.0), 3.0, 0.0, true);
                }
            }
            Event::CritterKilled { x, y } => {
                self.burst(*x, *y, 14, Color::new(0.8, 0.25, 0.2, 1.0), 4.0, 8.0, false);
            }
            Event::CrateOpened { x, y, loot } => {
                self.toast(match loot {
                    Loot::Bomb => "Crate opened: +1 bomb!".to_string(),
                    Loot::Gun(g) => format!("Crate opened: {} found!", g.name()),
                    Loot::Fuel => "Crate opened: fuel tank filled!".to_string(),
                });
                self.effect("fx_dust", *x, *y, 0.6, 1.2, 0.6, false);
                self.burst(*x, *y, 30, GOLD, 5.0, 0.0, true);
            }
            Event::CrateKeptShut => self.toast("Bomb pouch is full: this crate stays shut"),
        }
    }

    fn draw_critters(&self, game: &Game, art: &Art) {
        for (i, c) in game.critters.iter().enumerate() {
            let frame = ((game.time * 3.0) as usize + i) % 2;
            let Some(s) = art.get(&format!("{}_{frame}", c.kind.sprite())) else {
                continue;
            };
            let size = TILE * 0.7;
            let k = (size / s.size().x).min(size / s.size().y);
            let (w, h) = (s.size().x * k, s.size().y * k);
            let feet = self.screen(c.x, c.y + critters::HALF);
            s.draw_ex(feet.x - w / 2.0, feet.y - h, w, h, WHITE, c.dir < 0.0, 0.0);
        }
    }

    /// Unopened supply crates, each on the floor of its own hollow.
    fn draw_crates(&self, game: &Game, art: &Art, sw: f32, sh: f32) {
        let mid = (self.view + vec2(sw, sh) / 2.0) / TILE;
        let reach = sw.max(sh) / TILE / 2.0 + 1.0;
        for (tx, ty) in game.crates_near(mid.x, mid.y, reach) {
            let p = self.screen(tx as f32, ty as f32);
            let size = TILE * 0.85;
            let Some(s) = art.get("crate") else {
                let pad = (TILE - size) / 2.0;
                let wood = Color::new(0.55, 0.38, 0.2, 1.0);
                art.rect(p.x + pad, p.y + TILE - size, size, size, wood);
                continue;
            };
            let k = (size / s.size().x).min(size / s.size().y);
            let (w, h) = (s.size().x * k, s.size().y * k);
            s.draw(p.x + (TILE - w) / 2.0, p.y + TILE - h, w, h, WHITE);
        }
    }

    fn screen(&self, wx: f32, wy: f32) -> Vec2 {
        vec2(wx * TILE - self.view.x, wy * TILE - self.view.y)
    }

    pub fn draw(&mut self, game: &Game, art: &Art, shake: bool) {
        let (sw, sh) = (screen_width(), screen_height());
        let world_w = WIDTH as f32 * TILE;
        let mut cam = self.cam;
        cam.x = if sw >= world_w + 2.0 * TILE {
            world_w / 2.0
        } else {
            cam.x.clamp(sw / 2.0 - TILE, world_w + TILE - sw / 2.0)
        };
        cam.y = cam.y.max(-8.0 * TILE + sh / 2.0);
        let amp = if shake {
            game.shake.min(1.2).powi(2) * 14.0
        } else {
            0.0
        };
        let shake = vec2(gen_range(-amp, amp), gen_range(-amp, amp));
        // Whole pixels only: text snaps to the pixel grid while sprites don't, so a
        // fractional camera makes labels jitter against the art they sit on.
        self.view = (cam - vec2(sw / 2.0, sh / 2.0) + shake).round();

        clear_background(Color::new(0.02, 0.02, 0.04, 1.0));
        self.draw_sky(art, sw);
        self.draw_surface_trees(game, art, sw);
        self.draw_tiles(game, art, sw, sh);
        self.draw_shop(art);
        self.draw_sites(game, art, sh);
        self.draw_stashes(game, art);
        self.draw_crates(game, art, sw, sh);
        self.draw_critters(game, art);
        self.draw_bombs(game);
        self.draw_rockets(game);
        for m in &game.monsters {
            self.draw_monster(m, art);
        }
        self.draw_pod(game, art);
        self.draw_particles(false);
        self.draw_effects(art);
        self.draw_darkness(game, sw, sh);
        self.draw_glints(game, art, sw, sh);
        self.draw_particles(true);
        self.draw_stash_arrows(game, sw, sh);
        self.draw_radar(game, sw, sh);
        if self.flash > 0.0 {
            draw_rectangle(
                0.0,
                0.0,
                sw,
                sh,
                Color::new(1.0, 0.95, 0.9, self.flash * 0.6),
            );
        }
        self.draw_hud(game, art, sw, sh);
        if game.at_shop() {
            draw_shop_panel(game, sw);
        }
        self.draw_toasts(sw, sh, game.at_shop());
    }

    fn draw_sky(&self, art: &Art, sw: f32) {
        if self.view.y >= 0.0 {
            return;
        }
        let ground = self.screen(0.0, 0.0).y;
        if ground <= 0.0 {
            return;
        }
        // Soft vertical gradient: clear blue overhead, warm haze at the horizon.
        let top = Color::new(0.33, 0.58, 0.9, 1.0);
        let haze = Color::new(0.93, 0.9, 0.82, 1.0);
        let bands = 32;
        let span = ground.max(1.0);
        for i in 0..bands {
            let y = span * i as f32 / bands as f32;
            let k = (i as f32 / bands as f32).powf(1.6);
            draw_rectangle(0.0, y, sw, span / bands as f32 + 1.0, mix(top, haze, k));
        }
        let sun = vec2(sw * 0.78 - self.view.x * 0.01, ground - 300.0);
        for (r, a) in [(260.0, 0.18), (130.0, 0.35), (60.0, 0.7)] {
            draw_texture_ex(
                &self.glow,
                sun.x - r,
                sun.y - r,
                Color::new(1.0, 0.95, 0.8, a),
                DrawTextureParams {
                    dest_size: Some(vec2(2.0 * r, 2.0 * r)),
                    ..Default::default()
                },
            );
        }
        draw_circle(sun.x, sun.y, 26.0, Color::new(1.0, 0.98, 0.9, 0.95));

        self.draw_clouds(art, sw, ground, 0.04, 5.0, 0.9, 7, 11);
        // Far to near: each layer moves a little more with the camera than the
        // last. Painted mountains when the art is there, drawn ridges otherwise.
        // NOTE: the ranges stand just under the lowest dip of the green ridge
        // (RIDGE * 0.23): any lower and the hill hides their foothills, any
        // higher and sky shows under their flat bases.
        let base = ground - RIDGE * 0.23 + 4.0;
        if !self.draw_strip(art, "mountains_far", sw, base, 200.0, 0.08) {
            self.draw_ridge(
                art,
                sw,
                ground,
                0.08,
                230.0,
                Color::new(0.62, 0.72, 0.86, 1.0),
                1,
                false,
            );
        }
        self.draw_clouds(art, sw, ground, 0.12, 11.0, 0.6, 5, 12);
        if !self.draw_strip(art, "mountains_near", sw, base, 150.0, 0.22) {
            self.draw_ridge(
                art,
                sw,
                ground,
                0.22,
                150.0,
                Color::new(0.5, 0.7, 0.66, 1.0),
                2,
                false,
            );
        }
        self.draw_ridge(
            art,
            sw,
            ground,
            0.45,
            RIDGE,
            Color::new(0.43, 0.66, 0.38, 1.0),
            3,
            true,
        );
        self.draw_birds(ground);
    }

    /// A painted strip (a mountain range) repeated across the sky, standing on
    /// `bottom`, `height` pixels tall. False if the sprite isn't there.
    fn draw_strip(
        &self,
        art: &Art,
        name: &str,
        sw: f32,
        bottom: f32,
        height: f32,
        parallax: f32,
    ) -> bool {
        let Some(s) = art.get(name) else {
            return false;
        };
        let w = height * s.size().x / s.size().y;
        // NOTE: the painting isn't made to repeat, so every other copy is
        // mirrored: each edge then meets its own reflection, with no seam.
        let scroll = self.view.x * parallax;
        let mut i = (scroll / w).floor() as i64;
        let mut x = i as f32 * w - scroll;
        while x < sw {
            s.draw_ex(
                x,
                bottom - height,
                w + 1.0,
                height,
                WHITE,
                i.rem_euclid(2) == 1,
                0.0,
            );
            x += w;
            i += 1;
        }
        true
    }

    /// Names that exist for `prefix_0`, `prefix_1`, ... in the atlas.
    fn variants<'a>(art: &'a Art, prefix: &str) -> Vec<crate::art::Sprite<'a>> {
        (0..16)
            .map_while(|i| art.get(&format!("{prefix}_{i}")))
            .collect()
    }

    /// A rolling ridge line made of a few slow sine waves, drawn as thin columns.
    #[allow(clippy::too_many_arguments)] // Private layer helper; the args are the layer.
    fn draw_ridge(
        &self,
        art: &Art,
        sw: f32,
        ground: f32,
        parallax: f32,
        height: f32,
        color: Color,
        seed: u64,
        trees: bool,
    ) {
        let s = seed as f32;
        let ridge = |u: f32| {
            height
                * (0.62
                    + 0.22 * (u * 0.0021 + s).sin()
                    + 0.12 * (u * 0.0057 + 2.0 * s).sin()
                    + 0.05 * (u * 0.013 + 3.0 * s).sin())
        };
        let step = 4.0;
        let offset = self.view.x * parallax;
        let mut x = 0.0;
        while x < sw {
            let h = ridge(x + offset);
            draw_rectangle(x, ground - h, step + 0.5, h, color);
            // A lighter rim where the light catches the crest.
            draw_rectangle(x, ground - h, step + 0.5, 2.0, mix(color, WHITE, 0.25));
            x += step;
        }
        if !trees {
            return;
        }
        // A loose woodland along the nearest ridge: leafy rounds, conifers,
        // poplars and bushes, each standing on the hill where it's drawn.
        let spacing = 30.0;
        let first = (offset / spacing).floor() as i32 - 1;
        let mut painted = Self::variants(art, "tree");
        painted.extend(Self::variants(art, "bush"));
        for i in first..first + (sw / spacing) as i32 + 3 {
            let h = world::hash(seed, i, 0, 21);
            if world::unit(h) < 0.35 {
                continue;
            }
            let u = i as f32 * spacing + world::unit(h >> 8) * 18.0;
            let base = ground - ridge(u) + 3.0;
            let s = 0.7 + 0.7 * world::unit(h >> 16);
            if painted.is_empty() {
                draw_tree(u - offset, base, s, (h >> 24) % 4, world::unit(h >> 32));
                continue;
            }
            let sprite = painted[((h >> 24) as usize) % painted.len()];
            let th = 44.0 * s;
            let tw = th * sprite.size().x / sprite.size().y;
            sprite.draw(u - offset - tw / 2.0, base - th, tw, th, WHITE);
        }
    }

    /// A layer of fat, flat-bottomed cumulus clouds drifting with the wind.
    #[allow(clippy::too_many_arguments)] // Private layer helper; the args are the layer.
    fn draw_clouds(
        &self,
        art: &Art,
        sw: f32,
        ground: f32,
        parallax: f32,
        wind: f32,
        scale: f32,
        count: i32,
        seed: u64,
    ) {
        let margin = 260.0 * scale;
        let span = sw + 2.0 * margin;
        let shade = Color::new(0.84, 0.85, 0.95, 0.95);
        let white = Color::new(1.0, 1.0, 0.99, 0.97);
        let painted = Self::variants(art, "cloud");
        for i in 0..count {
            let h1 = world::unit(world::hash(seed, i, 1, 31));
            let h2 = world::unit(world::hash(seed, i, 2, 31));
            let r = (38.0 + 34.0 * h2) * scale;
            let travel = h1 * span - self.t * wind - self.view.x * parallax;
            let x = travel.rem_euclid(span) - margin;
            let y = ground - (170.0 + 230.0 * h2) * (0.6 + scale * 0.5);
            if !painted.is_empty() {
                let c = painted[i as usize % painted.len()];
                let cw = 4.2 * r;
                let ch = cw * c.size().y / c.size().x;
                c.draw(x - cw / 2.0, y - ch / 2.0, cw, ch, with_alpha(WHITE, 0.95));
                continue;
            }
            let puffs = [
                (-1.25, 0.15, 0.6),
                (-0.6, -0.15, 0.82),
                (0.0, -0.38, 1.0),
                (0.65, -0.12, 0.78),
                (1.2, 0.15, 0.55),
            ];
            draw_ellipse(x, y + 0.22 * r, 1.9 * r, 0.42 * r, 0.0, shade);
            for (dx, dy, k) in puffs {
                draw_circle(x + dx * r, y + dy * r + 0.12 * r, k * r, shade);
            }
            for (dx, dy, k) in puffs {
                draw_circle(x + dx * r, y + dy * r, k * r * 0.92, white);
            }
        }
    }

    fn draw_birds(&self, ground: f32) {
        let ink = Color::new(0.18, 0.2, 0.28, 0.85);
        for b in &self.birds {
            let flap = (self.t * 9.0 + b.phase).sin();
            let (x, y) = (
                b.x,
                ground - b.height + (self.t * 1.3 + b.phase).sin() * 3.0,
            );
            let span = 7.0 * b.size;
            let lift = flap * 4.0 * b.size;
            draw_line(x - span, y - lift, x, y, 1.6, ink);
            draw_line(x, y, x + span, y - lift, 1.6, ink);
        }
    }

    /// The odd tree or bush standing on the surface itself, behind the pod.
    fn draw_surface_trees(&self, game: &Game, art: &Art, sw: f32) {
        let ground = self.screen(0.0, 0.0).y;
        if ground <= 0.0 {
            return;
        }
        let trees = Self::variants(art, "tree");
        let bushes = Self::variants(art, "bush");
        if trees.is_empty() {
            return;
        }
        let seed = game.world.seed;
        let x0 = (self.view.x / TILE).floor() as i32 - 3;
        let x1 = ((self.view.x + sw) / TILE).ceil() as i32 + 3;
        // Clear of the shop, with room to park beside it.
        let shop = SHOP_COLUMNS.0 as i32 - 2..SHOP_COLUMNS.1 as i32 + 2;
        for tx in x0.max(0)..=x1.min(world::WIDTH - 1) {
            let h = world::hash(seed, tx, 0, 77);
            // A tree dug out from under would hang in the air.
            if world::unit(h) > 0.13 || shop.contains(&tx) || !game.world.get(tx, 0).is_solid() {
                continue;
            }
            let pick = (h >> 8) as usize % (trees.len() * 3 + bushes.len());
            let size = world::unit(h >> 16);
            let (sprite, th) = match trees.get(pick % trees.len()) {
                Some(t) if pick < trees.len() * 3 => (*t, TILE * (1.5 + 1.1 * size)),
                _ => (bushes[pick - trees.len() * 3], TILE * (0.6 + 0.3 * size)),
            };
            let tw = th * sprite.size().x / sprite.size().y;
            let cx = self.screen(tx as f32 + 0.5, 0.0).x;
            sprite.draw_ex(
                cx - tw / 2.0,
                ground + 4.0 - th,
                tw,
                th,
                WHITE,
                h >> 32 & 1 == 1,
                0.0,
            );
        }
    }

    fn draw_tiles(&self, game: &Game, art: &Art, sw: f32, sh: f32) {
        let x0 = (self.view.x / TILE).floor() as i32 - 1;
        let y0 = ((self.view.y / TILE).floor() as i32 - 1).max(-1);
        let x1 = ((self.view.x + sw) / TILE).ceil() as i32 + 1;
        let y1 = ((self.view.y + sh) / TILE).ceil() as i32 + 1;
        let seed = game.world.seed;
        let mut minerals = Vec::new();
        for ty in y0..=y1 {
            for tx in x0..=x1 {
                let tile = game.world.get(tx, ty);
                if ty < 0 && tile == Tile::Air {
                    continue;
                }
                let p = self.screen(tx as f32, ty as f32);
                let band = world::band(ty);
                let base = BAND_COLORS[band];
                let variant = world::hash(seed, tx, ty, 5) % 3;
                let sprite = match tile {
                    Tile::Air => {
                        art.rect(p.x, p.y, TILE, TILE, mix(base, BLACK, 0.72));
                        continue;
                    }
                    Tile::Wall => {
                        art.rect(p.x, p.y, TILE, TILE, Color::new(0.09, 0.08, 0.11, 1.0));
                        art.rect(
                            p.x + 4.0,
                            p.y + 4.0,
                            TILE - 8.0,
                            TILE - 8.0,
                            Color::new(0.12, 0.11, 0.15, 1.0),
                        );
                        continue;
                    }
                    Tile::Dirt if ty == 0 => format!("grass_{variant}"),
                    Tile::Dirt => format!("dirt_{band}_{variant}"),
                    Tile::Boulder => "boulder".to_string(),
                    Tile::Ore(t) => format!("ore_{t}"),
                    Tile::Mineral(_) => format!("dirt_{band}_{variant}"),
                };
                if ty > 0 || tile != Tile::Dirt {
                    art.rect(p.x, p.y, TILE, TILE, base);
                }
                match art.get(&sprite) {
                    Some(sprite) => {
                        // Slight overlap hides the ragged painted edges between tiles.
                        let pad = TILE * 0.06;
                        let size = TILE + 2.0 * pad;
                        sprite.draw(p.x - pad, p.y - pad, size, size, WHITE);
                    }
                    None => art.rect(p.x, p.y, TILE, TILE, tile_color(tile, ty)),
                }
                if let Tile::Mineral(m) = tile {
                    minerals.push((p, m, world::hash(seed, tx, ty, 6)));
                }
            }
        }
        // Mineral chunks are plain shapes on a different texture, so they go in
        // their own pass rather than breaking the tile batch at every mineral.
        for (p, m, h) in minerals {
            draw_mineral(p, m, h);
        }
    }

    /// Deep outposts and her bases. The surface shop has its own drawing.
    fn draw_sites(&self, game: &Game, art: &Art, sh: f32) {
        let mid = (self.view.y + sh / 2.0) / TILE;
        for site in game.sites_near(mid) {
            let base = self.screen(site.x, site.floor as f32);
            let (name, w) = match site.kind {
                SiteKind::Surface => continue,
                SiteKind::Outpost => ("cottage", 2.0 * site.half * TILE * 0.9),
                SiteKind::Base => ("tent", 2.0 * site.half * TILE),
            };
            if base.y < -4.0 * TILE || base.y > sh + 4.0 * TILE {
                continue;
            }
            // Lantern light so the place reads as safe in the dark.
            let r = w * 0.9;
            draw_texture_ex(
                &self.glow,
                base.x - r,
                base.y - r * 1.2,
                Color::new(1.0, 0.75, 0.4, 0.35),
                DrawTextureParams {
                    dest_size: Some(vec2(2.0 * r, 2.0 * r)),
                    ..Default::default()
                },
            );
            let sprite = art.get(name).or_else(|| art.get("shop"));
            if let Some(s) = sprite {
                let h = w * s.size().y / s.size().x;
                s.draw(base.x - w / 2.0, base.y - h, w, h, WHITE);
            }
            if site.kind == SiteKind::Base
                && let Some(fire) = art.get("campfire")
            {
                let fw = TILE * 0.8;
                fire.draw(base.x + w / 2.0, base.y - fw, fw, fw, WHITE);
            }
        }
    }

    fn draw_shop(&self, art: &Art) {
        let left = self.screen(SHOP_COLUMNS.0, 0.0);
        let w = (SHOP_COLUMNS.1 - SHOP_COLUMNS.0) * TILE;
        let Some(tex) = art.get("shop") else {
            draw_rectangle(
                left.x,
                left.y - 3.0 * TILE,
                w,
                3.0 * TILE,
                Color::new(0.5, 0.3, 0.2, 1.0),
            );
            text_centered("SHOP", left.x + w / 2.0, left.y - 1.5 * TILE, 32.0, WHITE);
            return;
        };
        let h = w * tex.size().y / tex.size().x;
        // The painted grass island under the shop sits on the surface row.
        let top = left.y - h * 0.9;
        tex.draw(left.x, top, w, h, WHITE);
        text_centered(
            "SHOP",
            left.x + w * 0.349,
            top + h * 0.43,
            30.0,
            Color::new(0.35, 0.2, 0.1, 1.0),
        );
    }

    fn draw_rockets(&self, game: &Game) {
        for r in &game.rockets {
            let p = self.screen(r.x, r.y);
            let d = r.vx.signum();
            draw_rectangle(
                p.x - 9.0,
                p.y - 3.0,
                18.0,
                6.0,
                Color::new(0.75, 0.75, 0.8, 1.0),
            );
            draw_triangle(
                vec2(p.x + d * 9.0, p.y - 3.0),
                vec2(p.x + d * 9.0, p.y + 3.0),
                vec2(p.x + d * 15.0, p.y),
                Color::new(0.9, 0.25, 0.2, 1.0),
            );
        }
    }

    fn draw_bombs(&self, game: &Game) {
        for b in &game.live_bombs {
            let p = self.screen(b.x, b.y);
            draw_circle(p.x, p.y, 11.0, Color::new(0.1, 0.1, 0.12, 1.0));
            draw_circle(p.x - 3.5, p.y - 3.5, 3.5, Color::new(0.45, 0.45, 0.5, 1.0));
            draw_rectangle(
                p.x + 2.0,
                p.y - 14.0,
                5.0,
                5.0,
                Color::new(0.3, 0.3, 0.32, 1.0),
            );
            draw_line(
                p.x + 4.5,
                p.y - 14.0,
                p.x + 7.0,
                p.y - 19.0,
                2.0,
                Color::new(0.6, 0.5, 0.3, 1.0),
            );
        }
    }

    fn draw_stashes(&self, game: &Game, art: &Art) {
        for s in &game.stashes {
            let bob = (self.t * 2.5 + s.x).sin() * 3.0;
            let p = self.screen(s.x, s.y) + vec2(0.0, bob);
            let size = TILE * 0.9;
            match art.get("stash") {
                Some(tex) => tex.draw(p.x - size / 2.0, p.y - size / 2.0, size, size, WHITE),
                None => draw_rectangle(
                    p.x - size / 2.0,
                    p.y - size / 2.0,
                    size,
                    size,
                    Color::new(0.5, 0.3, 0.6, 1.0),
                ),
            }
        }
    }

    fn draw_monster(&self, m: &Monster, art: &Art) {
        let (w, h) = m.size.dims();
        let p = self.screen(m.x, m.y);
        if let Phase::Warning(_) = m.phase {
            // Cracks spreading through the rock where it's about to break out.
            let k = 0.5 + 0.5 * (self.t * 9.0).sin();
            for i in 0..6 {
                let a = i as f32 * TAU / 6.0 + m.x;
                let r = (w.max(h) * TILE * 0.5) * (0.6 + 0.4 * k);
                draw_line(
                    p.x,
                    p.y,
                    p.x + a.cos() * r,
                    p.y + a.sin() * r * 0.7,
                    2.0,
                    Color::new(0.05, 0.03, 0.03, 0.8),
                );
            }
            return;
        }
        let dest = vec2(w * TILE * 1.15, h * TILE * 1.15);
        // The new 8-frame creature art, or the older 2-frame sprites until it's in.
        let frame8 = (self.t * 8.0 + m.x) as i32 % 8;
        let frame2 = (self.t * 4.0 + m.x) as i32 % 2;
        let (sprite, tint, bob) = match art.get(&format!("{}_{frame8}", m.kind.sprite())) {
            Some(s) => (Some(s), WHITE, 0.0),
            None => {
                let tint = if m.size == Size::Huge {
                    Color::new(0.55, 0.35, 0.75, 1.0)
                } else {
                    WHITE
                };
                let bob = if m.size == Size::Medium {
                    (self.t * 6.0).sin() * 4.0
                } else {
                    0.0
                };
                let old = format!("{}_{frame2}", monster_sprite(m.size));
                (art.get(&old), tint, bob)
            }
        };
        match sprite {
            Some(tex) => {
                // Fit inside the body box without stretching the art.
                let k = (dest.x / tex.size().x).min(dest.y / tex.size().y);
                let (dw, dh) = (tex.size().x * k, tex.size().y * k);
                tex.draw_ex(
                    p.x - dw / 2.0,
                    p.y + dest.y / 2.0 - dh + bob,
                    dw,
                    dh,
                    tint,
                    m.dir < 0.0,
                    0.0,
                )
            }
            None => draw_ellipse(
                p.x,
                p.y,
                dest.x / 2.0,
                dest.y / 2.0,
                0.0,
                Color::new(0.3, 0.1, 0.3, 1.0),
            ),
        }
    }

    fn draw_pod(&self, game: &Game, art: &Art) {
        let pod = &game.pod;
        if pod.invuln > 0.0 && (self.t * 12.0) as i32 % 2 == 0 {
            return;
        }
        let p = self.screen(pod.x, pod.y);
        let digging_down = pod.dig.is_some_and(|d| d.to.1 > d.from.1 + 0.2);
        let jitter = if pod.dig.is_some() {
            vec2(gen_range(-1.5, 1.5), gen_range(-1.5, 1.5))
        } else {
            Vec2::ZERO
        };
        let frame = if pod.dig.is_some() {
            (self.t * 14.0) as i32 % 2
        } else {
            0
        };
        let w = TILE * 1.2;
        let Some(tex) = art.get(&format!("pod_{frame}")) else {
            draw_rectangle(
                p.x - TILE * 0.4,
                p.y - TILE * 0.4,
                TILE * 0.8,
                TILE * 0.8,
                Color::new(0.95, 0.55, 0.15, 1.0),
            );
            return;
        };
        let h = w * tex.size().y / tex.size().x;
        let rotation = if digging_down {
            FRAC_PI_2 * pod.facing
        } else {
            0.0
        };
        // Sit the sprite on the hitbox floor when level; center it when drilling down.
        let y = if digging_down {
            p.y - h / 2.0
        } else {
            p.y + POD_HALF * TILE - h
        };
        tex.draw_ex(
            p.x - w / 2.0 + jitter.x,
            y + jitter.y,
            w,
            h,
            WHITE,
            pod.facing < 0.0,
            rotation,
        );
    }

    fn draw_particles(&self, glow: bool) {
        for pt in self.particles.iter().filter(|p| p.glow == glow) {
            let p = self.screen(pt.x, pt.y);
            let a = pt.life / pt.max;
            if glow {
                let r = pt.size * 3.0;
                draw_texture_ex(
                    &self.glow,
                    p.x - r,
                    p.y - r,
                    with_alpha(pt.color, a),
                    DrawTextureParams {
                        dest_size: Some(vec2(2.0 * r, 2.0 * r)),
                        ..Default::default()
                    },
                );
                draw_circle(p.x, p.y, pt.size * 0.5 * a, with_alpha(WHITE, a));
            } else {
                draw_rectangle(
                    p.x - pt.size / 2.0,
                    p.y - pt.size / 2.0,
                    pt.size,
                    pt.size,
                    with_alpha(pt.color, a),
                );
            }
        }
    }

    fn light(&self, game: &Game) -> (Vec2, f32) {
        let depth = game.depth();
        let flicker = 1.0
            + world::band_position(depth) / 4.0
                * (0.03 * (self.t * 11.0).sin() + 0.02 * (self.t * 23.0).sin());
        let r = light_radius(depth, game.levels.lamp_bonus()) * flicker * TILE;
        (self.screen(game.pod.x, game.pod.y), r)
    }

    fn draw_darkness(&self, game: &Game, sw: f32, sh: f32) {
        let depth = game.depth();
        let pos = world::band_position(depth);
        // Colors cool and shift toward purple the deeper she goes.
        let tint = mix(
            Color::new(0.1, 0.2, 0.5, 0.0),
            Color::new(0.25, 0.05, 0.35, 0.0),
            (pos / 4.0).min(1.0),
        );
        draw_rectangle(
            0.0,
            0.0,
            sw,
            sh,
            with_alpha(
                Color::new(tint.r, tint.g, tint.b, 1.0),
                (pos / 4.0).min(1.0) * 0.22,
            ),
        );

        let d = darkness(depth);
        if d < 0.01 {
            return;
        }
        let (c, r) = self.light(game);
        let shade = Color::new(0.0, 0.0, 0.01, d);
        draw_texture_ex(
            &self.light,
            c.x - r,
            c.y - r,
            shade,
            DrawTextureParams {
                dest_size: Some(vec2(2.0 * r, 2.0 * r)),
                ..Default::default()
            },
        );
        let (l, t, rt, b) = (c.x - r, c.y - r, c.x + r, c.y + r);
        draw_rectangle(0.0, 0.0, sw, t.max(0.0), shade);
        draw_rectangle(0.0, b, sw, (sh - b).max(0.0), shade);
        draw_rectangle(0.0, t, l.max(0.0), 2.0 * r, shade);
        draw_rectangle(rt, t, (sw - rt).max(0.0), 2.0 * r, shade);
    }

    fn draw_glints(&self, game: &Game, art: &Art, sw: f32, sh: f32) {
        let (c, r) = self.light(game);
        let seed = game.world.seed;
        let x0 = (self.view.x / TILE).floor() as i32;
        let y0 = (self.view.y / TILE).floor() as i32;
        let x1 = ((self.view.x + sw) / TILE).ceil() as i32;
        let y1 = ((self.view.y + sh) / TILE).ceil() as i32;
        for ty in y0.max(0)..=y1 {
            for tx in x0..=x1 {
                let Tile::Ore(tier) = game.world.get(tx, ty) else {
                    continue;
                };
                let phase = world::unit(world::hash(seed, tx, ty, 9)) * TAU;
                let a = (self.t * 1.8 + phase).sin().max(0.0).powi(14);
                if a < 0.02 {
                    continue;
                }
                let ox = 0.2 + 0.6 * world::unit(world::hash(seed, tx, ty, 10));
                let oy = 0.2 + 0.6 * world::unit(world::hash(seed, tx, ty, 11));
                let p = self.screen(tx as f32 + ox, ty as f32 + oy);
                // Gems twinkle even a little past the edge of the light.
                let vis = 1.0 - ((p.distance(c) - r) / (r * 0.6)).clamp(0.0, 1.0);
                if vis <= 0.0 {
                    continue;
                }
                let color = mix(ORE_COLORS[(tier as usize).min(5)], WHITE, 0.5);
                let s = (5.0 + tier as f32) * a;
                let col = with_alpha(color, a * vis);
                draw_line(p.x - s * 1.6, p.y, p.x + s * 1.6, p.y, 2.0, col);
                draw_line(p.x, p.y - s * 1.6, p.x, p.y + s * 1.6, 2.0, col);
                draw_texture_ex(
                    &self.glow,
                    p.x - s * 2.0,
                    p.y - s * 2.0,
                    col,
                    DrawTextureParams {
                        dest_size: Some(vec2(s * 4.0, s * 4.0)),
                        ..Default::default()
                    },
                );
            }
        }

        // Monster eyes glow through the dark: the scary part.
        for m in &game.monsters {
            let (w, h) = m.size.dims();
            let pulse = match m.phase {
                Phase::Warning(_) => 0.5 + 0.5 * (self.t * 8.0).sin(),
                Phase::Active => 0.9,
            };
            let eye = Color::new(
                1.0,
                if m.size == Size::Huge { 0.15 } else { 0.75 },
                0.2,
                pulse,
            );
            let p = self.screen(m.x + m.dir * w * 0.22, m.y - h * 0.12);
            let gap = w * TILE * 0.12;
            let r = h * TILE * 0.18;
            for dx in [-gap, gap] {
                draw_texture_ex(
                    &self.glow,
                    p.x + dx - r,
                    p.y - r,
                    eye,
                    DrawTextureParams {
                        dest_size: Some(vec2(2.0 * r, 2.0 * r)),
                        ..Default::default()
                    },
                );
                draw_circle(p.x + dx, p.y, r * 0.18, with_alpha(WHITE, pulse));
            }
        }

        self.draw_beams(game);
        self.draw_loose(game, art);
        self.draw_relic_drops(game, art);
        self.draw_health_bars(game);

        // The fuse spark blinks faster as it burns down.
        for b in &game.live_bombs {
            let p = self.screen(b.x, b.y);
            let urgency = 1.0 - (b.fuse / FUSE).clamp(0.0, 1.0);
            let blink = 0.5 + 0.5 * (self.t * (6.0 + 22.0 * urgency)).sin();
            let r = 14.0 + 10.0 * blink;
            draw_texture_ex(
                &self.glow,
                p.x + 7.0 - r,
                p.y - 19.0 - r,
                Color::new(1.0, 0.5 + 0.3 * blink, 0.2, 0.9),
                DrawTextureParams {
                    dest_size: Some(vec2(2.0 * r, 2.0 * r)),
                    ..Default::default()
                },
            );
        }

        for s in &game.stashes {
            let p = self.screen(s.x, s.y);
            let r = TILE * (1.4 + 0.3 * (self.t * 3.0).sin());
            draw_texture_ex(
                &self.glow,
                p.x - r,
                p.y - r,
                Color::new(0.7, 0.4, 1.0, 0.8),
                DrawTextureParams {
                    dest_size: Some(vec2(2.0 * r, 2.0 * r)),
                    ..Default::default()
                },
            );
        }
    }

    /// Relics glow over the dark so a drop is never lost in it.
    /// Things the hold had no room for, glinting where they were dug out.
    fn draw_loose(&self, game: &Game, art: &Art) {
        for (i, l) in game.loose.iter().enumerate() {
            let p = self.screen(l.x, l.y);
            let pulse = 0.7 + 0.3 * (self.t * 3.0 + i as f32).sin();
            let c = item_color(l.item);
            let r = TILE * 0.5 * pulse;
            draw_texture_ex(
                &self.glow,
                p.x - r,
                p.y - r,
                with_alpha(c, 0.7),
                DrawTextureParams {
                    dest_size: Some(vec2(2.0 * r, 2.0 * r)),
                    ..Default::default()
                },
            );
            if l.item == Item::Meat
                && let Some(s) = art.get("meat")
            {
                let size = TILE * 0.6;
                s.draw(p.x - size / 2.0, p.y - size / 2.0, size, size, WHITE);
                continue;
            }
            draw_poly(p.x, p.y, 6, TILE * 0.18, self.t * 40.0, c);
            draw_poly(
                p.x,
                p.y,
                6,
                TILE * 0.08,
                self.t * 40.0,
                with_alpha(WHITE, 0.9),
            );
        }
    }

    fn draw_relic_drops(&self, game: &Game, art: &Art) {
        for (i, d) in game.drops.iter().enumerate() {
            let bob = (self.t * 3.0 + i as f32).sin() * 4.0;
            let p = self.screen(d.x, d.y) + vec2(0.0, bob);
            let r = TILE * (0.9 + 0.2 * (self.t * 4.0).sin());
            draw_texture_ex(
                &self.glow,
                p.x - r,
                p.y - r,
                Color::new(1.0, 0.85, 0.35, 0.8),
                DrawTextureParams {
                    dest_size: Some(vec2(2.0 * r, 2.0 * r)),
                    ..Default::default()
                },
            );
            let size = TILE * 0.7;
            match art.get("relic_coin") {
                Some(s) => s.draw(p.x - size / 2.0, p.y - size / 2.0, size, size, WHITE),
                None => {
                    draw_poly(p.x, p.y, 6, size * 0.45, self.t * 60.0, GOLD);
                    draw_poly(
                        p.x,
                        p.y,
                        6,
                        size * 0.25,
                        self.t * 60.0,
                        Color::new(1.0, 0.95, 0.7, 1.0),
                    );
                }
            }
        }
    }

    /// Laser, ion shot and the ion cannon's charge glow. Drawn over the dark,
    /// since they're light sources themselves.
    fn draw_beams(&self, game: &Game) {
        if let Some(b) = game.laser {
            let (a, c) = (self.screen(b.x0, b.y), self.screen(b.x1, b.y));
            let flicker = 0.75 + 0.25 * (self.t * 40.0).sin();
            draw_line(
                a.x,
                a.y,
                c.x,
                c.y,
                7.0,
                Color::new(1.0, 0.2, 0.25, 0.35 * flicker),
            );
            draw_line(
                a.x,
                a.y,
                c.x,
                c.y,
                2.5,
                Color::new(1.0, 0.85, 0.85, flicker),
            );
            let r = 14.0;
            draw_texture_ex(
                &self.glow,
                c.x - r,
                c.y - r,
                Color::new(1.0, 0.4, 0.3, 0.9),
                DrawTextureParams {
                    dest_size: Some(vec2(2.0 * r, 2.0 * r)),
                    ..Default::default()
                },
            );
        }
        if let Some(b) = game.ion_beam {
            let (a, c) = (self.screen(b.x0, b.y), self.screen(b.x1, b.y));
            let k = (b.t / 0.35).clamp(0.0, 1.0);
            draw_line(
                a.x,
                a.y,
                c.x,
                c.y,
                26.0 * k,
                Color::new(0.4, 0.8, 1.0, 0.45 * k),
            );
            draw_line(a.x, a.y, c.x, c.y, 9.0 * k, Color::new(0.9, 1.0, 1.0, k));
        }
        if game.ion_charge > 0.0 {
            let k = (game.ion_charge / crate::game::ION_CHARGE).min(1.0);
            let p = self.screen(game.pod.x + game.pod.facing * POD_HALF, game.pod.y);
            let r = 8.0
                + 26.0 * k
                + if k >= 1.0 {
                    (self.t * 20.0).sin() * 3.0
                } else {
                    0.0
                };
            draw_texture_ex(
                &self.glow,
                p.x - r,
                p.y - r,
                Color::new(0.5, 0.85, 1.0, 0.5 + 0.5 * k),
                DrawTextureParams {
                    dest_size: Some(vec2(2.0 * r, 2.0 * r)),
                    ..Default::default()
                },
            );
        }
    }

    /// A bar over each monster in or near the light, so you can see a fight
    /// going your way, or that this weapon can't hurt it at all.
    fn draw_health_bars(&self, game: &Game) {
        let (c, r) = self.light(game);
        for m in &game.monsters {
            if !m.is_active() {
                continue;
            }
            let (w, h) = m.size.dims();
            let top = self.screen(m.x, m.y - h * 0.6);
            if top.distance(c) > r * 1.3 + w * TILE {
                continue;
            }
            let bw = (w * TILE * 0.8).clamp(30.0, 140.0);
            let (x, y) = (top.x - bw / 2.0, top.y - 10.0);
            draw_rectangle(
                x - 1.0,
                y - 1.0,
                bw + 2.0,
                7.0,
                Color::new(0.0, 0.0, 0.0, 0.7),
            );
            if m.immune_flash > 0.0 {
                draw_rectangle(x, y, bw, 5.0, Color::new(0.55, 0.55, 0.6, 1.0));
                text_centered(
                    "bombs or ion only",
                    top.x,
                    y - 6.0,
                    18.0,
                    Color::new(0.85, 0.85, 0.9, 1.0),
                );
            } else {
                let hp = m.health();
                let fill = mix(
                    Color::new(0.9, 0.2, 0.2, 1.0),
                    Color::new(0.35, 0.9, 0.4, 1.0),
                    hp,
                );
                draw_rectangle(x, y, bw * hp, 5.0, fill);
            }
        }
    }

    /// An arrow on the screen edge pointing at something off screen, labelled.
    fn edge_arrow(&self, game: &Game, target: Vec2, color: Color, label: &str, sw: f32, sh: f32) {
        let from = self.screen(game.pod.x, game.pod.y);
        let margin = 56.0;
        let to = self.screen(target.x, target.y);
        if to.x > margin && to.x < sw - margin && to.y > margin && to.y < sh - margin {
            return;
        }
        let dir = (to - from).normalize_or_zero();
        if dir == Vec2::ZERO {
            return;
        }
        // Where the line from the pod to the target crosses the inset screen edge.
        let tx = if dir.x > 0.0 {
            (sw - margin - from.x) / dir.x
        } else if dir.x < 0.0 {
            (margin - from.x) / dir.x
        } else {
            f32::INFINITY
        };
        let ty = if dir.y > 0.0 {
            (sh - margin - from.y) / dir.y
        } else if dir.y < 0.0 {
            (margin - from.y) / dir.y
        } else {
            f32::INFINITY
        };
        let at = from + dir * tx.min(ty).max(0.0);
        let pulse = 0.75 + 0.25 * (self.t * 4.0).sin();
        let size = 22.0 * pulse.max(0.8);
        let perp = vec2(-dir.y, dir.x);
        draw_texture_ex(
            &self.glow,
            at.x - 40.0,
            at.y - 40.0,
            with_alpha(color, 0.5 * pulse),
            DrawTextureParams {
                dest_size: Some(vec2(80.0, 80.0)),
                ..Default::default()
            },
        );
        draw_triangle(
            at + dir * size,
            at - dir * size * 0.6 + perp * size * 0.7,
            at - dir * size * 0.6 - perp * size * 0.7,
            color,
        );
        let dist = (target.x - game.pod.x).hypot(target.y - game.pod.y).round();
        let label = format!("{label} {dist} m");
        let lw = measure_text(&label, None, 20, 1.0).width;
        let lx = (at.x - lw / 2.0).clamp(8.0, sw - lw - 8.0);
        let ly = if dir.y > 0.0 {
            at.y - 30.0
        } else {
            at.y + 40.0
        };
        text(&label, lx, ly, 20.0, WHITE);
    }

    fn draw_stash_arrows(&self, game: &Game, sw: f32, sh: f32) {
        for s in &game.stashes {
            self.edge_arrow(
                game,
                vec2(s.x, s.y),
                Color::new(0.85, 0.55, 1.0, 1.0),
                "Lost cargo",
                sw,
                sh,
            );
        }
        // And the nearest place to refuel, once she's underground.
        if game.depth() > 2.0 {
            let site = game.nearest_site(game.pod.x, game.pod.y);
            let (x, y) = site.landing();
            let label = match site.kind {
                SiteKind::Surface => "Shop",
                SiteKind::Outpost => "Outpost",
                SiteKind::Base => "Base",
            };
            self.edge_arrow(
                game,
                vec2(x, y),
                Color::new(0.45, 1.0, 0.55, 1.0),
                label,
                sw,
                sh,
            );
        }
    }

    /// Bottom-right mini-map: ground around her, shops, crates, monsters and
    /// relics. A ping (Q) lights up the ore for a few seconds.
    fn draw_radar(&self, game: &Game, sw: f32, sh: f32) {
        const CELL: f32 = 3.0;
        let (w, h) = (222.0, 159.0);
        let (x0, y0) = (sw - w - 14.0, sh - h - 14.0);
        draw_rectangle(x0 - 4.0, y0 - 24.0, w + 8.0, h + 28.0, PANEL);
        let label = if game.ping_cooldown > 0.0 {
            format!("RADAR   ping in {:.0}s", game.ping_cooldown.ceil())
        } else {
            "RADAR   Q to ping".to_string()
        };
        text(&label, x0, y0 - 6.0, 18.0, Color::new(0.6, 1.0, 0.7, 1.0));
        let (cols, rows) = ((w / CELL) as i32, (h / CELL) as i32);
        let (px, py) = (game.pod.x.floor() as i32, game.pod.y.floor() as i32);
        let (tx0, ty0) = (px - cols / 2, py - rows / 2);
        let pinging = game.ping > 0.0;
        for r in 0..rows {
            for c in 0..cols {
                let (tx, ty) = (tx0 + c, ty0 + r);
                let tile = game.world.get(tx, ty);
                let near = ((tx - px).pow(2) + (ty - py).pow(2)) <= 25 * 25;
                let color = match tile {
                    Tile::Air if ty < 0 => Color::new(0.2, 0.3, 0.45, 1.0),
                    Tile::Air => Color::new(0.03, 0.03, 0.05, 1.0),
                    Tile::Wall => Color::new(0.3, 0.26, 0.36, 1.0),
                    Tile::Boulder => Color::new(0.45, 0.45, 0.48, 1.0),
                    Tile::Ore(t) if pinging && near => ORE_COLORS[(t as usize).min(5)],
                    Tile::Mineral(m) if pinging && near => mineral_color(m),
                    _ => mix(BAND_COLORS[world::band(ty)], BLACK, 0.45),
                };
                draw_rectangle(
                    x0 + c as f32 * CELL,
                    y0 + r as f32 * CELL,
                    CELL,
                    CELL,
                    color,
                );
            }
        }
        // Markers; anything off the map is pinned to its edge.
        let to_map = |x: f32, y: f32| {
            vec2(
                (x0 + (x - tx0 as f32) * CELL).clamp(x0 + 3.0, x0 + w - 3.0),
                (y0 + (y - ty0 as f32) * CELL).clamp(y0 + 3.0, y0 + h - 3.0),
            )
        };
        for site in game.sites_near(game.pod.y) {
            let (sx, sy) = site.landing();
            let p = to_map(sx, sy);
            draw_rectangle(p.x - 3.0, p.y - 3.0, 7.0, 7.0, GOLD);
        }
        for s in &game.stashes {
            let p = to_map(s.x, s.y);
            draw_circle(p.x, p.y, 3.0, Color::new(0.85, 0.55, 1.0, 1.0));
        }
        for d in &game.drops {
            let p = to_map(d.x, d.y);
            draw_circle(p.x, p.y, 3.0, GOLD);
        }
        for m in &game.monsters {
            let p = to_map(m.x, m.y);
            draw_circle(
                p.x,
                p.y,
                2.0 + m.size.dims().0,
                Color::new(1.0, 0.25, 0.2, 1.0),
            );
        }
        let p = to_map(game.pod.x, game.pod.y);
        // An arrow off the pod toward the nearest place to refuel, since the
        // gold squares pinned to the edge don't say which one is closest.
        let (sx, sy) = game.nearest_site(game.pod.x, game.pod.y).landing();
        let to_site = vec2(sx - game.pod.x, sy - game.pod.y);
        if to_site.length() > 2.0 {
            let d = to_site.normalize();
            let side = vec2(-d.y, d.x) * 5.0;
            let green = Color::new(0.45, 1.0, 0.55, 1.0);
            draw_line(p.x, p.y, p.x + d.x * 10.0, p.y + d.y * 10.0, 2.0, green);
            draw_triangle(
                p + d * 19.0,
                p + d * 10.0 + side,
                p + d * 10.0 - side,
                green,
            );
        }
        let blink = 0.6 + 0.4 * (self.t * 6.0).sin();
        draw_circle(p.x, p.y, 3.5, with_alpha(WHITE, blink));
        if pinging {
            // The sonar ring sweeping outward.
            let k = 1.0 - game.ping / crate::game::PING_REVEAL;
            draw_circle_lines(
                p.x,
                p.y,
                25.0 * CELL * k.min(1.0),
                1.5,
                Color::new(0.6, 1.0, 0.7, 1.0 - k),
            );
        }
    }

    fn draw_hud(&self, game: &Game, art: &Art, sw: f32, sh: f32) {
        let (x, y) = (14.0, 14.0);
        let minerals: Vec<(Mineral, String)> = Mineral::ALL
            .iter()
            .map(|&m| (m, format!("{} {}", m.name(), game.count(Item::Mineral(m)))))
            .collect();
        let gap = 24.0;
        let minerals_w: f32 = minerals
            .iter()
            .map(|(_, l)| 16.0 + measure_text(l, None, 20, 1.0).width + gap)
            .sum::<f32>()
            - gap;
        // Grow to fit the widest row, so counts in the tens and hundreds never spill.
        let width = 300.0f32.max(12.0 + minerals_w + 14.0);
        let has_kit = game.arsenal.laser
            || game.arsenal.rockets
            || game.arsenal.ion
            || game.levels.relics() > 0;
        let height = if has_kit { 286.0 } else { 254.0 };
        draw_rectangle(x, y, width, height, PANEL);
        let icon = |name: &str, row: f32| {
            if let Some(tex) = art.get(name) {
                tex.draw(x + 10.0, y + 8.0 + row * 33.0, 28.0, 28.0, WHITE);
            }
        };
        let tx = x + 48.0;
        let row_y = |row: f32| y + 30.0 + row * 33.0;

        icon("icon_depth", 0.0);
        text(
            &format!("{} m", game.depth().round()),
            tx,
            row_y(0.0),
            28.0,
            WHITE,
        );
        text(
            &format!("best {} m", game.max_depth.round()),
            tx + 110.0,
            row_y(0.0),
            18.0,
            Color::new(0.7, 0.7, 0.8, 1.0),
        );

        icon("icon_money", 1.0);
        text(&money(game.money), tx, row_y(1.0), 28.0, GOLD);

        let bar = |row: f32, frac: f32, color: Color| {
            let by = row_y(row) - 18.0;
            draw_rectangle(tx, by, 200.0, 18.0, Color::new(0.2, 0.2, 0.25, 1.0));
            draw_rectangle(tx, by, 200.0 * frac.clamp(0.0, 1.0), 18.0, color);
            draw_rectangle_lines(tx, by, 200.0, 18.0, 2.0, Color::new(0.0, 0.0, 0.0, 0.6));
        };
        let fuel = game.pod.fuel / game.levels.fuel_max();
        icon("icon_fuel", 2.0);
        bar(2.0, fuel, self.fuel_color(game));
        icon("icon_hull", 3.0);
        let hull = game.pod.hull / game.levels.hull_max();
        bar(
            3.0,
            hull,
            mix(
                Color::new(0.9, 0.2, 0.2, 1.0),
                Color::new(0.3, 0.85, 0.4, 1.0),
                hull,
            ),
        );

        icon("icon_cargo", 4.0);
        let cap = game.levels.cargo_cap();
        let full = game.cargo.len() >= cap;
        text(
            &format!(
                "{}/{}   worth {}",
                game.cargo.len(),
                cap,
                money(game.cargo_value())
            ),
            tx,
            row_y(4.0),
            22.0,
            if full {
                Color::new(1.0, 0.5, 0.4, 1.0)
            } else {
                WHITE
            },
        );

        text(
            &format!("Bombs {}/{BOMB_POUCH}   (B to drop)", game.bombs),
            x + 12.0,
            row_y(5.0),
            22.0,
            Color::new(1.0, 0.7, 0.4, 1.0),
        );
        let mut mx = x + 12.0;
        for (m, label) in &minerals {
            draw_circle(mx + 6.0, row_y(6.0) - 7.0, 6.0, mineral_color(*m));
            text(label, mx + 16.0, row_y(6.0), 20.0, WHITE);
            mx += 16.0 + measure_text(label, None, 20, 1.0).width + gap;
        }

        // Guns she owns with their keys, and relics found.
        let mut kit: Vec<&str> = Vec::new();
        if game.arsenal.laser {
            kit.push("F laser");
        }
        if game.arsenal.rockets {
            kit.push("G rockets");
        }
        if game.arsenal.ion {
            kit.push("H ion");
        }
        let relics = game.levels.relics();
        if !kit.is_empty() || relics > 0 {
            let line = if relics > 0 {
                format!("{}   relics {relics}", kit.join("  "))
            } else {
                kit.join("  ")
            };
            text(
                line.trim(),
                x + 12.0,
                y + 274.0,
                20.0,
                Color::new(0.6, 0.85, 1.0, 1.0),
            );
        }

        if game.depth() < 1.0 && !game.at_shop() {
            text_centered(
                "Arrow keys or WASD to drive and dig.  Up to fly.  Sell gems at the shop.",
                sw / 2.0,
                sh - 24.0,
                22.0,
                WHITE,
            );
        }
    }

    /// Messages along the top, or, while a shop menu fills the top right,
    /// along the bottom (clear of the radar) so they never print over it.
    /// The fuel bar warms to red and flashes ever faster as the tank nears the
    /// point of no return: the fuel it takes to reach the nearest shop.
    fn fuel_color(&self, game: &Game) -> Color {
        let orange = Color::new(1.0, 0.6, 0.15, 1.0);
        let red = Color::new(0.95, 0.12, 0.1, 1.0);
        if game.depth() < 1.0 {
            return orange;
        }
        // margin 1.0 is the point of no return.
        let margin = game.pod.fuel / game.return_fuel().max(0.1);
        let color = mix(orange, red, (2.0 - margin).clamp(0.0, 1.0));
        if margin >= 1.5 {
            return color;
        }
        let hz = 1.5 + 8.5 * ((1.5 - margin) / 0.5).clamp(0.0, 1.0);
        if (self.t * hz * TAU).sin() > 0.0 {
            color
        } else {
            mix(red, BLACK, 0.55)
        }
    }

    fn draw_toasts(&self, sw: f32, sh: f32, shop_open: bool) {
        let (cx, top) = if shop_open {
            (
                (sw - 240.0) / 2.0,
                sh - 40.0 - self.toasts.len() as f32 * 34.0,
            )
        } else {
            (sw / 2.0, 60.0)
        };
        for (i, t) in self.toasts.iter().enumerate() {
            let a = (1.0 - (t.age - 2.5).max(0.0)).clamp(0.0, 1.0);
            let rise = (t.age * 6.0).min(6.0);
            let y = top + i as f32 * 34.0 - rise;
            if shop_open {
                let w = measure_text(&t.text, None, 28, 1.0).width
                    + if t.count > 1 { 50.0 } else { 0.0 };
                draw_rectangle(
                    cx - w / 2.0 - 10.0,
                    y - 24.0,
                    w + 20.0,
                    32.0,
                    with_alpha(PANEL, a),
                );
            }
            let line = if t.count > 1 {
                format!("{}  x{}", t.text, t.count)
            } else {
                t.text.clone()
            };
            text_centered(&line, cx, y, 28.0, with_alpha(WHITE, a));
        }
    }
}

/// The shop's rows, laid out for the current screen; shared by drawing and input.
pub fn shop_buttons(game: &Game, sw: f32) -> Vec<ShopButton> {
    // Two columns, services on the left, upgrades and weapons on the right,
    // so the panel fits a 720-pixel-tall window. Number keys cover the first ten.
    let (w, h, gap) = (340.0, 44.0, 6.0);
    let right = sw - w - 16.0;
    let left = right - w - 12.0;
    let keys = [
        KeyCode::Key1,
        KeyCode::Key2,
        KeyCode::Key3,
        KeyCode::Key4,
        KeyCode::Key5,
        KeyCode::Key6,
        KeyCode::Key7,
        KeyCode::Key8,
        KeyCode::Key9,
        KeyCode::Key0,
    ];
    let mut rows: Vec<ShopButton> = Vec::new();
    let mut ys = [64.0f32, 64.0f32];
    let mut push = |col: usize, action, label: String, detail: String, enabled: bool| {
        let x = if col == 0 { left } else { right };
        rows.push(ShopButton {
            rect: Rect::new(x, ys[col], w, h),
            action,
            key: None,
            label,
            detail,
            enabled,
        });
        ys[col] += h + gap;
    };
    let value = game.cargo_value();
    push(
        0,
        ShopAction::Sell,
        "Sell gems and meat".into(),
        format!("+{}", money(value)),
        value > 0,
    );
    let minerals = game.mineral_value();
    push(
        0,
        ShopAction::SellMinerals,
        "Sell spare minerals".into(),
        format!("+{}", money(minerals)),
        minerals > 0,
    );
    push(
        0,
        ShopAction::Craft,
        format!("Craft bomb  ({}/{BOMB_POUCH})", game.bombs),
        "C+S+P".into(),
        game.can_craft(),
    );
    let fuel = game.refuel_cost();
    push(
        0,
        ShopAction::Refuel,
        "Refuel".into(),
        money(fuel),
        fuel > 0 && game.money > 0,
    );
    let repair = game.repair_cost();
    push(
        0,
        ShopAction::Repair,
        "Repair hull".into(),
        money(repair),
        repair > 0 && game.money > 0,
    );
    push(
        0,
        ShopAction::BuyBaseKit,
        format!("Base kit  ({}/{MAX_BASE_KITS})", game.base_kits),
        money(BASE_KIT_PRICE),
        game.base_kits < MAX_BASE_KITS && game.money >= BASE_KIT_PRICE,
    );
    for u in Upgrade::ALL {
        let level = game.levels.get(u);
        let label = format!("{}  Lv {level}/{MAX_LEVEL}", u.name());
        if level >= MAX_LEVEL {
            push(1, ShopAction::Buy(u), label, "MAX".into(), false);
        } else {
            let cost = u.cost(level);
            push(
                1,
                ShopAction::Buy(u),
                label,
                money(cost),
                game.money >= cost,
            );
        }
    }
    for g in Gun::ALL {
        if game.arsenal.has(g) {
            push(
                1,
                ShopAction::BuyGun(g),
                g.name().into(),
                "OWNED".into(),
                false,
            );
        } else {
            push(
                1,
                ShopAction::BuyGun(g),
                g.name().into(),
                money(g.price()),
                game.money >= g.price(),
            );
        }
    }
    // Keys follow the old single-column order: services, then upgrades.
    let keyed = rows
        .iter()
        .enumerate()
        .filter(|(_, b)| !matches!(b.action, ShopAction::BuyGun(_) | ShopAction::BuyBaseKit))
        .map(|(i, _)| i)
        .take(keys.len())
        .collect::<Vec<_>>();
    for (k, i) in keyed.into_iter().enumerate() {
        rows[i].key = Some(keys[k]);
    }
    rows
}

fn key_digit(k: KeyCode) -> char {
    match k {
        KeyCode::Key1 => '1',
        KeyCode::Key2 => '2',
        KeyCode::Key3 => '3',
        KeyCode::Key4 => '4',
        KeyCode::Key5 => '5',
        KeyCode::Key6 => '6',
        KeyCode::Key7 => '7',
        KeyCode::Key8 => '8',
        KeyCode::Key9 => '9',
        _ => '0',
    }
}

fn draw_shop_panel(game: &Game, sw: f32) {
    let rows = shop_buttons(game, sw);
    if rows.is_empty() {
        return;
    }
    let x0 = rows.iter().map(|b| b.rect.x).fold(f32::MAX, f32::min);
    let x1 = rows.iter().map(|b| b.rect.right()).fold(0.0, f32::max);
    let y1 = rows.iter().map(|b| b.rect.bottom()).fold(0.0, f32::max);
    draw_rectangle(x0 - 12.0, 14.0, x1 - x0 + 24.0, y1 + 12.0 - 14.0, PANEL);
    let title = match game.current_site().map(|s| s.kind) {
        Some(SiteKind::Outpost) => "OUTPOST  (click, or press 1-0)",
        Some(SiteKind::Base) => "YOUR BASE  (click, or press 1-0)",
        _ => "SHOP  (click, or press 1-0)",
    };
    text(title, x0, 46.0, 26.0, GOLD);
    let mouse = Vec2::from(mouse_position());
    for b in &rows {
        let hover = b.rect.contains(mouse) && b.enabled;
        let bg = match (b.enabled, hover) {
            (true, true) => Color::new(0.35, 0.3, 0.15, 1.0),
            (true, false) => Color::new(0.2, 0.18, 0.12, 1.0),
            _ => Color::new(0.12, 0.12, 0.14, 1.0),
        };
        draw_rectangle(b.rect.x, b.rect.y, b.rect.w, b.rect.h, bg);
        let fg = if b.enabled {
            WHITE
        } else {
            Color::new(0.5, 0.5, 0.55, 1.0)
        };
        text(
            &match b.key {
                Some(k) => format!("{}  {}", key_digit(k), b.label),
                None => format!("   {}", b.label),
            },
            b.rect.x + 12.0,
            b.rect.y + 29.0,
            24.0,
            fg,
        );
        let dw = measure_text(&b.detail, None, 24, 1.0).width;
        text(
            &b.detail,
            b.rect.x + b.rect.w - dw - 12.0,
            b.rect.y + 29.0,
            24.0,
            if b.enabled { GOLD } else { fg },
        );
    }
}
