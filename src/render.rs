//! Everything on screen. Draws the game, turns events into particles, toasts
//! and flashes, and makes the deep feel dark; it never changes the rules.

use std::f32::consts::{FRAC_PI_2, TAU};

use macroquad::prelude::*;
use macroquad::rand::gen_range;

use crate::art::Art;
use crate::game::{
    Event, Game, MAX_LEVEL, ORE_NAMES, POD_HALF, SHOP_COLUMNS, ShopAction, Upgrade, ore_value,
};
use crate::monsters::{Monster, Phase, Size};
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
}

/// A clickable row in the shop panel.
pub struct ShopButton {
    pub rect: Rect,
    pub action: ShopAction,
    pub key: KeyCode,
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
        Tile::Boulder => Color::new(0.6, 0.6, 0.6, 1.0),
        _ => BAND_COLORS[world::band(y)],
    }
}

/// How dark the deep is (0 = daylight) at a depth in tiles.
fn darkness(depth: f32) -> f32 {
    ((depth - 10.0) / 350.0).clamp(0.0, 1.0).powf(0.8) * 0.96
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
        }
    }

    pub fn toast(&mut self, text: impl Into<String>) {
        self.toasts.push(Toast {
            text: text.into(),
            age: 0.0,
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
        let target = vec2(game.pod.x, game.pod.y) * TILE;
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
    }

    fn react(&mut self, e: &Event, game: &Game) {
        match e {
            Event::DigStarted { x, y } => {
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
            Event::Pickup { tier, x, y } => {
                let t = (*tier as usize).min(5);
                self.burst(*x, *y, 18 + 4 * t, ORE_COLORS[t], 5.0, 0.0, true);
                self.toast(format!(
                    "+1 {}  ({})",
                    ORE_NAMES[t],
                    money(ore_value(*tier))
                ));
            }
            Event::CargoFull => self.toast("Cargo hold is full! Fly up and sell at the shop."),
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
                self.toast(format!("Recovered {count} gems!"));
                self.burst(*x, *y, 40, Color::new(0.8, 0.5, 1.0, 1.0), 6.0, 0.0, true);
            }
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
        self.view = cam - vec2(sw / 2.0, sh / 2.0) + shake;

        clear_background(Color::new(0.02, 0.02, 0.04, 1.0));
        self.draw_sky(art, sw);
        self.draw_tiles(game, art, sw, sh);
        self.draw_shop(art);
        self.draw_stashes(game, art);
        for m in &game.monsters {
            self.draw_monster(m, art);
        }
        self.draw_pod(game, art);
        self.draw_particles(false);
        self.draw_darkness(game, sw, sh);
        self.draw_glints(game, sw, sh);
        self.draw_particles(true);
        self.draw_stash_arrows(game, sw, sh);
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
        self.draw_toasts(sw);
    }

    fn draw_sky(&self, art: &Art, sw: f32) {
        if self.view.y >= 0.0 {
            return;
        }
        let top = self.screen(0.0, -9.0).y;
        let ground = self.screen(0.0, 0.0).y;
        draw_rectangle(
            0.0,
            0.0,
            sw,
            ground.max(0.0),
            Color::new(0.45, 0.7, 0.95, 1.0),
        );
        let Some(sky) = art.get("sky") else {
            return;
        };
        let h = ground - top;
        let w = h * sky.width() / sky.height();
        // Half-speed parallax so the hills drift behind the shop.
        let start = -((self.view.x * 0.5) % w + w) % w;
        let mut x = start;
        while x < sw {
            draw_texture_ex(
                sky,
                x,
                top,
                WHITE,
                DrawTextureParams {
                    dest_size: Some(vec2(w + 1.0, h)),
                    ..Default::default()
                },
            );
            x += w;
        }
    }

    fn draw_tiles(&self, game: &Game, art: &Art, sw: f32, sh: f32) {
        let x0 = (self.view.x / TILE).floor() as i32 - 1;
        let y0 = ((self.view.y / TILE).floor() as i32 - 1).max(-1);
        let x1 = ((self.view.x + sw) / TILE).ceil() as i32 + 1;
        let y1 = ((self.view.y + sh) / TILE).ceil() as i32 + 1;
        let seed = game.world.seed;
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
                        draw_rectangle(p.x, p.y, TILE, TILE, mix(base, BLACK, 0.72));
                        continue;
                    }
                    Tile::Wall => {
                        draw_rectangle(p.x, p.y, TILE, TILE, Color::new(0.09, 0.08, 0.11, 1.0));
                        draw_rectangle(
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
                };
                if ty > 0 || tile != Tile::Dirt {
                    draw_rectangle(p.x, p.y, TILE, TILE, base);
                }
                match art.get(&sprite) {
                    Some(tex) => {
                        // Slight overlap hides the ragged painted edges between tiles.
                        let pad = TILE * 0.06;
                        let dest = vec2(TILE + 2.0 * pad, TILE + 2.0 * pad);
                        draw_texture_ex(
                            tex,
                            p.x - pad,
                            p.y - pad,
                            WHITE,
                            DrawTextureParams {
                                dest_size: Some(dest),
                                ..Default::default()
                            },
                        );
                    }
                    None => {
                        draw_rectangle(p.x, p.y, TILE, TILE, tile_color(tile, ty));
                    }
                }
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
        let h = w * tex.height() / tex.width();
        // The painted grass island under the shop sits on the surface row.
        let top = left.y - h * 0.9;
        draw_texture_ex(
            tex,
            left.x,
            top,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(w, h)),
                ..Default::default()
            },
        );
        text_centered(
            "SHOP",
            left.x + w * 0.396,
            top + h * 0.43,
            30.0,
            Color::new(0.35, 0.2, 0.1, 1.0),
        );
    }

    fn draw_stashes(&self, game: &Game, art: &Art) {
        for s in &game.stashes {
            let bob = (self.t * 2.5 + s.x).sin() * 3.0;
            let p = self.screen(s.x, s.y) + vec2(0.0, bob);
            let size = TILE * 0.9;
            match art.get("stash") {
                Some(tex) => draw_texture_ex(
                    tex,
                    p.x - size / 2.0,
                    p.y - size / 2.0,
                    WHITE,
                    DrawTextureParams {
                        dest_size: Some(vec2(size, size)),
                        ..Default::default()
                    },
                ),
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
        let frame = (self.t * 4.0 + m.x) as i32 % 2;
        let bob = if m.size == Size::Medium {
            (self.t * 6.0).sin() * 4.0
        } else {
            0.0
        };
        let tint = match m.size {
            Size::Huge => Color::new(0.55, 0.35, 0.75, 1.0),
            _ => WHITE,
        };
        let dest = vec2(w * TILE * 1.15, h * TILE * 1.15);
        match art.get(&format!("{}_{frame}", monster_sprite(m.size))) {
            Some(tex) => draw_texture_ex(
                tex,
                p.x - dest.x / 2.0,
                p.y - dest.y / 2.0 + bob,
                tint,
                DrawTextureParams {
                    dest_size: Some(dest),
                    flip_x: m.dir < 0.0,
                    ..Default::default()
                },
            ),
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
        let h = w * tex.height() / tex.width();
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
        draw_texture_ex(
            tex,
            p.x - w / 2.0 + jitter.x,
            y + jitter.y,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(w, h)),
                rotation,
                flip_x: pod.facing < 0.0,
                ..Default::default()
            },
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

    fn draw_glints(&self, game: &Game, sw: f32, sh: f32) {
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

    fn draw_stash_arrows(&self, game: &Game, sw: f32, sh: f32) {
        let from = self.screen(game.pod.x, game.pod.y);
        let margin = 56.0;
        for s in &game.stashes {
            let to = self.screen(s.x, s.y);
            if to.x > margin && to.x < sw - margin && to.y > margin && to.y < sh - margin {
                continue;
            }
            let dir = (to - from).normalize_or_zero();
            if dir == Vec2::ZERO {
                continue;
            }
            // Where the line from the pod to the stash crosses the inset screen edge.
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
            let color = Color::new(0.85, 0.55, 1.0, 1.0);
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
            let dist = (s.x - game.pod.x).hypot(s.y - game.pod.y).round();
            let label = format!("Lost cargo {dist} m");
            let lw = measure_text(&label, None, 20, 1.0).width;
            let lx = (at.x - lw / 2.0).clamp(8.0, sw - lw - 8.0);
            let ly = if dir.y > 0.0 {
                at.y - 30.0
            } else {
                at.y + 40.0
            };
            text(&label, lx, ly, 20.0, WHITE);
        }
    }

    fn draw_hud(&self, game: &Game, art: &Art, sw: f32, sh: f32) {
        let (x, y) = (14.0, 14.0);
        draw_rectangle(x, y, 270.0, 176.0, PANEL);
        let icon = |name: &str, row: f32| {
            if let Some(tex) = art.get(name) {
                draw_texture_ex(
                    tex,
                    x + 10.0,
                    y + 8.0 + row * 33.0,
                    WHITE,
                    DrawTextureParams {
                        dest_size: Some(vec2(28.0, 28.0)),
                        ..Default::default()
                    },
                );
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
        let low = fuel < 0.2 && (self.t * 4.0).sin() > 0.0;
        icon("icon_fuel", 2.0);
        bar(
            2.0,
            fuel,
            if low {
                RED
            } else {
                Color::new(1.0, 0.6, 0.15, 1.0)
            },
        );
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

    fn draw_toasts(&self, sw: f32) {
        for (i, t) in self.toasts.iter().enumerate() {
            let a = (1.0 - (t.age - 2.5).max(0.0)).clamp(0.0, 1.0);
            let rise = (t.age * 6.0).min(6.0);
            text_centered(
                &t.text,
                sw / 2.0,
                60.0 + i as f32 * 34.0 - rise,
                28.0,
                with_alpha(WHITE, a),
            );
        }
    }
}

/// The shop's rows, laid out for the current screen; shared by drawing and input.
pub fn shop_buttons(game: &Game, sw: f32) -> Vec<ShopButton> {
    let (w, h) = (360.0, 44.0);
    let x = sw - w - 16.0;
    let mut y = 64.0;
    let mut rows = Vec::new();
    let keys = [
        KeyCode::Key1,
        KeyCode::Key2,
        KeyCode::Key3,
        KeyCode::Key4,
        KeyCode::Key5,
        KeyCode::Key6,
        KeyCode::Key7,
        KeyCode::Key8,
    ];
    let mut push = |action, label: String, detail: String, enabled: bool| {
        let key = keys[rows.len()];
        rows.push(ShopButton {
            rect: Rect::new(x, y, w, h),
            action,
            key,
            label,
            detail,
            enabled,
        });
        y += h + 6.0;
    };
    let value = game.cargo_value();
    push(
        ShopAction::Sell,
        "Sell cargo".into(),
        format!("+{}", money(value)),
        value > 0,
    );
    let fuel = game.refuel_cost();
    push(
        ShopAction::Refuel,
        "Refuel".into(),
        money(fuel),
        fuel > 0 && game.money > 0,
    );
    let repair = game.repair_cost();
    push(
        ShopAction::Repair,
        "Repair hull".into(),
        money(repair),
        repair > 0 && game.money > 0,
    );
    for u in Upgrade::ALL {
        let level = game.levels.get(u);
        let label = format!("{}  Lv {level}/{MAX_LEVEL}", u.name());
        if level >= MAX_LEVEL {
            push(ShopAction::Buy(u), label, "MAX".into(), false);
        } else {
            let cost = u.cost(level);
            push(ShopAction::Buy(u), label, money(cost), game.money >= cost);
        }
    }
    rows
}

fn draw_shop_panel(game: &Game, sw: f32) {
    let rows = shop_buttons(game, sw);
    let Some(first) = rows.first() else {
        return;
    };
    let last = rows.last().map_or(first.rect, |r| r.rect);
    draw_rectangle(
        first.rect.x - 12.0,
        14.0,
        first.rect.w + 24.0,
        last.y + last.h + 12.0 - 14.0,
        PANEL,
    );
    text(
        "SHOP  (click, or press 1-8)",
        first.rect.x,
        46.0,
        26.0,
        GOLD,
    );
    let mouse = Vec2::from(mouse_position());
    for (i, b) in rows.iter().enumerate() {
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
            &format!("{}  {}", i + 1, b.label),
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
