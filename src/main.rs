//! GemDigger: dig down forever, sell gems, upgrade, and brave the dark.
//! This file wires input, the rules, drawing, sound and auto-save together.

// No console window behind the game on Windows.
#![cfg_attr(not(test), windows_subsystem = "windows")]

mod art;
mod audio;
mod game;
mod menu;
mod monsters;
mod render;
mod save;
mod settings;
mod synth;
mod world;

use std::path::{Path, PathBuf};

use macroquad::prelude::*;

use crate::game::{Event, Game, Input};
use crate::menu::{Choice, Menu};
use crate::render::Renderer;
use crate::settings::Settings;

const AUTOSAVE_SECS: f32 = 30.0;

fn main() {
    // NOTE: CI and install-windows.ps1 run `--version` to check which build is installed.
    if std::env::args()
        .skip(1)
        .any(|a| a == "--version" || a == "-V")
    {
        // A GUI-subsystem exe may have no stdout at all; never panic over it.
        let line = format!("{} {}\n", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        let _ = std::io::Write::write_all(&mut std::io::stdout(), line.as_bytes());
        return;
    }
    let save_path = save::default_path();
    let settings_path = save_path.as_deref().map(settings::path_beside);
    let settings = settings_path
        .as_deref()
        .map(settings::load)
        .unwrap_or_default();
    let conf = Conf {
        window_title: "GemDigger".to_string(),
        window_width: 1280,
        window_height: 720,
        high_dpi: true,
        fullscreen: settings.fullscreen,
        ..Default::default()
    };
    macroquad::Window::from_config(conf, run(save_path, settings_path, settings));
}

fn new_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64)
}

/// Loads the saved game, or starts a new one, plus a message for the player.
fn load_game(path: Option<&Path>) -> (Game, Option<String>) {
    let Some(path) = path else {
        return (
            Game::new(new_seed()),
            Some("No save folder found; progress won't be saved.".into()),
        );
    };
    match save::load(path) {
        Ok(save::Loaded::Resumed(data)) => (Game::from_save(data), Some("Welcome back!".into())),
        Ok(save::Loaded::Fresh) => (Game::new(new_seed()), None),
        Ok(save::Loaded::Corrupt { backup }) => (
            Game::new(new_seed()),
            Some(format!(
                "Old save was damaged; kept a copy at {}",
                backup.display()
            )),
        ),
        Err(e) => {
            eprintln!("{e:#}");
            (
                Game::new(new_seed()),
                Some("Couldn't read the save; starting fresh.".into()),
            )
        }
    }
}

fn store(path: Option<&PathBuf>, game: &Game, renderer: &mut Renderer) {
    let Some(path) = path else {
        return;
    };
    if let Err(e) = save::write(path, &game.to_save()) {
        eprintln!("{e:#}");
        renderer.toast("Couldn't save progress!");
    }
}

fn read_input() -> Input {
    let down = |keys: &[KeyCode]| keys.iter().any(|&k| is_key_down(k));
    Input {
        left: down(&[KeyCode::Left, KeyCode::A]),
        right: down(&[KeyCode::Right, KeyCode::D]),
        up: down(&[KeyCode::Up, KeyCode::W, KeyCode::Space]),
        down: down(&[KeyCode::Down, KeyCode::S]),
    }
}

fn shop_input(game: &mut Game) {
    if !game.at_shop() {
        return;
    }
    let click = is_mouse_button_pressed(MouseButton::Left).then(|| Vec2::from(mouse_position()));
    for b in render::shop_buttons(game, screen_width()) {
        if is_key_pressed(b.key) || click.is_some_and(|m| b.rect.contains(m)) {
            game.apply(b.action);
        }
    }
}

async fn run(save_path: Option<PathBuf>, settings_path: Option<PathBuf>, mut settings: Settings) {
    prevent_quit();
    clear_background(BLACK);
    draw_text("GemDigger is warming up...", 40.0, 80.0, 40.0, WHITE);
    next_frame().await;

    let dir = art::asset_dir();
    let art = art::Art::load(dir.as_deref());
    let sound = match audio::Audio::load(dir.as_deref()).await {
        Ok(a) => Some(a),
        Err(e) => {
            eprintln!("{e:#}");
            None
        }
    };
    let (mut game, notice) = load_game(save_path.as_deref());
    let mut renderer = Renderer::new();
    if let Some(n) = notice {
        renderer.toast(n);
    }
    if let Some(s) = &sound {
        s.start();
    }

    let mut menu = Menu::new();
    let mut since_save = 0.0;
    loop {
        let choice = menu.input(&settings);
        let fullscreen_key = is_key_pressed(KeyCode::F11);
        if choice == Some(Choice::Quit) {
            store(save_path.as_ref(), &game, &mut renderer);
            break;
        }
        if choice.is_some() || fullscreen_key {
            match choice {
                Some(Choice::ToggleMusic) => settings.music = !settings.music,
                Some(Choice::ToggleSound) => settings.sound = !settings.sound,
                Some(Choice::ToggleShake) => settings.shake = !settings.shake,
                Some(Choice::ToggleFullscreen) => settings.fullscreen = !settings.fullscreen,
                _ if fullscreen_key => settings.fullscreen = !settings.fullscreen,
                _ => {}
            }
            set_fullscreen(settings.fullscreen);
            if let Some(path) = &settings_path
                && let Err(e) = save::write(path, &settings)
            {
                eprintln!("{e:#}");
            }
        }

        // Clamped so a stall (window drag, sleep) can't tunnel the pod through rock.
        // Zero while paused, so the world holds still behind the menu.
        let dt = if menu.open {
            0.0
        } else {
            get_frame_time().min(0.05)
        };
        let events = if menu.open {
            Vec::new()
        } else {
            shop_input(&mut game);
            game.update(dt, read_input())
        };

        since_save += dt;
        let milestone = events.iter().any(|e| {
            matches!(
                e,
                Event::Sold { .. }
                    | Event::Bought(_)
                    | Event::Died { .. }
                    | Event::StashRecovered { .. }
            )
        });
        if since_save >= AUTOSAVE_SECS || milestone {
            store(save_path.as_ref(), &game, &mut renderer);
            since_save = 0.0;
        }

        if let Some(s) = &sound {
            s.update(&game, &settings);
            s.react(&events, &settings);
        }
        renderer.update(dt, &game, &events);
        renderer.draw(&game, &art, settings.shake);
        menu.draw(&settings);

        if is_quit_requested() {
            store(save_path.as_ref(), &game, &mut renderer);
            break;
        }
        next_frame().await;
    }
}
