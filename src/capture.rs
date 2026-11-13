//! Renders the README's screenshots and gameplay clip from scripted scenes, so
//! they can be regenerated after every gameplay change instead of hand-captured.
//! Runs the real update and renderer at a fixed timestep, with no audio and no
//! save file.
//!
//!     gemdigger --capture <dir>
//!
//! Writes `<dir>/<still>.png` for each screenshot and `<dir>/frames/*.png` at 15 fps.

use std::path::Path;

use macroquad::prelude::*;

use crate::art::Art;
use crate::game::{Game, Input, Item, POD_HALF};
use crate::monsters::{Monster, Phase, Size};
use crate::render::Renderer;
use crate::world::{Mineral, Tile};

const DT: f32 = 1.0 / 60.0;
/// Every 4th 60 Hz frame goes into the clip: 15 fps.
const CLIP_EVERY: usize = 4;
const SEED: u64 = 7;

struct Scene {
    setup: fn(&mut Game),
    input: fn(f32) -> Input,
    secs: f32,
    /// Screenshots to save, by name and time into the scene.
    stills: &'static [(&'static str, f32)],
}

fn hold(left: bool, right: bool, up: bool, down: bool) -> Input {
    Input {
        left,
        right,
        up,
        down,
        bomb: false,
    }
}

/// Clears a pocket for the pod at `depth`, standing on the row below.
fn pocket(g: &mut Game, depth: i32, x0: i32, x1: i32) {
    for x in x0..x1 {
        g.world.set(x, depth, Tile::Air);
        g.world.set(x, depth + 1, Tile::Dirt);
    }
    g.pod.x = 48.5;
    g.pod.y = depth as f32 + 1.0 - POD_HALF - 0.001;
}

fn scenes() -> Vec<Scene> {
    vec![
        // At the shop, then driving out under the sky.
        Scene {
            setup: |g| {
                g.money = 1250;
                g.cargo = vec![Item::Gem(2), Item::Gem(3), Item::Mineral(Mineral::Coal)];
            },
            input: |t| hold(t > 1.6 && t < 3.6, false, false, false),
            secs: 6.0,
            stills: &[("shop", 1.2), ("surface", 5.5)],
        },
        // Digging down through clay into a seam of gems.
        Scene {
            setup: |g| {
                pocket(g, 118, 44, 54);
                for (dy, tier) in [(2, 1), (3, 2), (4, 2), (5, 3)] {
                    g.world.set(48, 118 + dy, Tile::Ore(tier));
                }
                g.world.set(49, 122, Tile::Ore(2));
                g.world.set(47, 123, Tile::Mineral(Mineral::Sulfur));
            },
            input: |t| hold(false, false, false, t > 0.4),
            secs: 5.0,
            stills: &[("digging", 3.4)],
        },
        // The dark: something big breaks out of the rock, and the way out is up.
        Scene {
            setup: |g| {
                pocket(g, 460, 40, 56);
                for y in 446..460 {
                    g.world.set(48, y, Tile::Air);
                }
                g.levels.lamp = 2;
                g.monsters.push(Monster {
                    size: Size::Large,
                    x: 54.0,
                    y: 461.0 - Size::Large.dims().1 / 2.0,
                    dir: -1.0,
                    phase: Phase::Warning(1.2),
                    age: 0.0,
                    stun: 0.0,
                });
            },
            input: |t| hold(false, false, t > 2.6, false),
            secs: 5.0,
            stills: &[("deep", 2.3)],
        },
        // A crafted bomb opening up boulders.
        Scene {
            setup: |g| {
                pocket(g, 210, 38, 56);
                for x in 46..52 {
                    g.world.set(x, 209, Tile::Boulder);
                    g.world.set(x, 211, Tile::Boulder);
                }
                g.bombs = 2;
            },
            input: |t| Input {
                left: t > 0.3 && t < 1.2,
                bomb: (t / DT).round() as i32 == 6,
                ..Input::default()
            },
            secs: 4.5,
            stills: &[("bomb", 2.65)],
        },
    ]
}

fn save(path: &Path) {
    get_screen_data().export_png(&path.to_string_lossy());
}

pub async fn run(dir: &Path) -> anyhow::Result<()> {
    let frames = dir.join("frames");
    std::fs::create_dir_all(&frames)?;
    let art = Art::load(None);
    let mut frame = 0usize;
    for scene in scenes() {
        let mut game = Game::new(SEED);
        (scene.setup)(&mut game);
        let mut renderer = Renderer::new();
        renderer.snap_to(&game);
        renderer.prime_birds();
        let steps = (scene.secs / DT) as usize;
        for step in 0..steps {
            let t = step as f32 * DT;
            let events = game.update(DT, (scene.input)(t));
            renderer.update(DT, &game, &events);
            renderer.draw(&game, &art, true);
            if step % CLIP_EVERY == 0 {
                save(&frames.join(format!("{frame:05}.png")));
                frame += 1;
            }
            for &(name, at) in scene.stills {
                if step == (at / DT) as usize {
                    save(&dir.join(format!("{name}.png")));
                }
            }
            next_frame().await;
        }
    }
    Ok(())
}
