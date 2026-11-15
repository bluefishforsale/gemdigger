//! Frame-rate benchmark: plays scripted scenes full screen, the way the game
//! actually runs, and prints frame-time stats, so a rendering change can be
//! measured instead of eyeballed. No audio, no save.
//!
//!     gemdigger --bench

use std::time::Instant;

use macroquad::prelude::*;

use crate::art::Art;
use crate::game::{Game, Input, POD_HALF};
use crate::render::Renderer;
use crate::world::Tile;

const SECS_PER_SCENE: f32 = 6.0;

struct Scene {
    name: &'static str,
    setup: fn(&mut Game),
    input: fn(f32) -> Input,
}

fn scenes() -> [Scene; 3] {
    [
        // Busy sky: clouds, ridges, trees, birds, plus the top rows of ground.
        Scene {
            name: "surface",
            setup: |_| {},
            input: |t| Input {
                left: (t % 4.0) < 2.0,
                right: (t % 4.0) >= 2.0,
                ..Input::default()
            },
        },
        // A full screen of tiles, scrolling.
        Scene {
            name: "mid-depth",
            setup: |g| {
                for x in 20..76 {
                    g.world.set(x, 120, Tile::Air);
                }
                g.pod.x = 48.5;
                g.pod.y = 121.0 - POD_HALF - 0.001;
            },
            input: |t| Input {
                left: (t % 4.0) < 2.0,
                right: (t % 4.0) >= 2.0,
                ..Input::default()
            },
        },
        // Darkness, light falloff and twinkling gems.
        Scene {
            name: "deep",
            setup: |g| {
                for x in 20..76 {
                    g.world.set(x, 480, Tile::Air);
                }
                g.pod.x = 48.5;
                g.pod.y = 481.0 - POD_HALF - 0.001;
            },
            input: |t| Input {
                left: (t % 4.0) < 2.0,
                right: (t % 4.0) >= 2.0,
                ..Input::default()
            },
        },
    ]
}

fn report(name: &str, frames: &mut [f32], work: &mut [f32]) {
    frames.sort_by(f32::total_cmp);
    work.sort_by(f32::total_cmp);
    let avg = frames.iter().sum::<f32>() / frames.len() as f32;
    let p = |v: &[f32], q: f32| v[((v.len() - 1) as f32 * q) as usize] * 1000.0;
    println!(
        "{name:>10}: {:6.1} fps avg | frame ms p50 {:5.2} p95 {:5.2} worst {:5.2} | cpu ms p50 {:5.2} p95 {:5.2}",
        1.0 / avg,
        p(frames, 0.5),
        p(frames, 0.95),
        p(frames, 1.0),
        p(work, 0.5),
        p(work, 0.95),
    );
}

pub async fn run() {
    let art = Art::load(None);
    // Let the window settle into full screen before timing anything.
    for _ in 0..30 {
        clear_background(BLACK);
        next_frame().await;
    }
    println!(
        "screen {}x{} at dpi scale {}",
        screen_width(),
        screen_height(),
        screen_dpi_scale()
    );
    for scene in scenes() {
        let mut game = Game::new(7);
        (scene.setup)(&mut game);
        let mut renderer = Renderer::new();
        renderer.snap_to(&game);
        renderer.prime_birds();
        let (mut frames, mut work) = (Vec::new(), Vec::new());
        let mut t = 0.0;
        while t < SECS_PER_SCENE {
            let dt = get_frame_time();
            t += dt;
            let start = Instant::now();
            let events = game.update(dt.min(0.05), (scene.input)(t));
            renderer.update(dt.min(0.05), &game, &events);
            renderer.draw(&game, &art, true);
            work.push(start.elapsed().as_secs_f32());
            next_frame().await;
            // The first frames after a scene change include loading; skip them.
            if t > 0.5 {
                frames.push(get_frame_time());
            }
        }
        report(scene.name, &mut frames, &mut work);
    }
}
