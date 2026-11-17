//! GemDigger: dig down forever, sell gems, upgrade, and brave the dark.
//! This file wires input, the rules, drawing, sound and auto-save together.

// No console window behind the game on Windows.
#![cfg_attr(not(test), windows_subsystem = "windows")]

mod art;
mod audio;
mod bench;
mod capture;
mod critters;
mod game;
mod log;
mod menu;
mod monsters;
mod playtest;
mod render;
mod save;
mod settings;
mod synth;
mod world;

use std::path::{Path, PathBuf};

use macroquad::miniquad::conf::{AppleGfxApi, Platform};
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
    if std::env::args().any(|a| a == "--bench") {
        let conf = Conf {
            window_title: "GemDigger bench".to_string(),
            window_width: 1280,
            window_height: 720,
            high_dpi: true,
            fullscreen: true,
            // GEMDIGGER_METAL=1 benches miniquad's Metal backend instead, for comparison.
            platform: if std::env::var_os("GEMDIGGER_METAL").is_some() {
                metal()
            } else {
                Platform::default()
            },
            ..Default::default()
        };
        macroquad::Window::from_config(conf, bench::run());
        return;
    }
    if let Some(dir) = flag_dir("--playtest") {
        let conf = Conf {
            window_title: "GemDigger playtest".to_string(),
            window_width: 1280,
            window_height: 720,
            high_dpi: false,
            ..Default::default()
        };
        macroquad::Window::from_config(conf, async move {
            if let Err(e) = playtest::run(&dir).await {
                log::line(&format!("{e:#}"));
            }
        });
        return;
    }
    if let Some(dir) = capture_dir() {
        let conf = Conf {
            window_title: "GemDigger capture".to_string(),
            window_width: 1280,
            window_height: 720,
            // NOTE: 1x framebuffer, so frames are 1280x720 on Retina screens too.
            high_dpi: false,
            // NOTE: stays on OpenGL; reading frames back (get_screen_data) is
            // unimplemented on Metal and panics.
            ..Default::default()
        };
        macroquad::Window::from_config(conf, async move {
            if let Err(e) = capture::run(&dir).await {
                log::line(&format!("{e:#}"));
            }
        });
        return;
    }
    let save_path = save::default_path();
    log::init(save_path.as_deref());
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
        // NOTE: OpenGL on macOS too. miniquad 0.4's Metal backend measured about
        // 12x slower full screen at 5K (4-6 fps against 53-67), see --bench.
        ..Default::default()
    };
    macroquad::Window::from_config(conf, run(save_path, settings_path, settings));
}

/// Metal on Apple; other platforms ignore this. Only the benchmark opts in.
fn metal() -> Platform {
    Platform {
        apple_gfx_api: AppleGfxApi::Metal,
        ..Default::default()
    }
}

/// Tells the OpenGL context to pick its drawable up again, as a resize does.
///
/// NOTE: miniquad attaches the context to the view before the window is on
/// screen, and on a Retina display the fresh window flickers at 60 Hz until
/// its first resize (going full screen and back cured it). The resize path's
/// lasting effect is this one call, so make it ourselves once the window is up.
#[cfg(target_os = "macos")]
fn refresh_gl_surface() {
    use macroquad::miniquad::native::apple::frameworks::{Object, class, msg_send, sel, sel_impl};
    // SAFETY: main thread, where the context was made current. A nil context
    // (Metal) takes the message and does nothing.
    unsafe {
        let ctx: *mut Object = msg_send![class!(NSOpenGLContext), currentContext];
        let () = msg_send![ctx, update];
    }
}

#[cfg(not(target_os = "macos"))]
fn refresh_gl_surface() {}

/// `--capture <dir>`: render the README media instead of playing.
fn capture_dir() -> Option<PathBuf> {
    flag_dir("--capture")
}

/// The path after `flag` on the command line, if the flag is there.
fn flag_dir(flag: &str) -> Option<PathBuf> {
    let args: Vec<String> = std::env::args().collect();
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1).map(PathBuf::from)
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
        Ok(save::Loaded::Resumed(data)) => (Game::from_save(*data), Some("Welcome back!".into())),
        Ok(save::Loaded::Fresh) => (Game::new(new_seed()), None),
        Ok(save::Loaded::Corrupt { backup }) => (
            Game::new(new_seed()),
            Some(format!(
                "Old save was damaged; kept a copy at {}",
                backup.display()
            )),
        ),
        Err(e) => {
            log::line(&format!("{e:#}"));
            (
                Game::new(new_seed()),
                Some("Couldn't read the save; starting fresh.".into()),
            )
        }
    }
}

/// Throws away the current mine for a freshly rolled one, keeping the old
/// save beside the new one in case she regrets it.
fn start_over(path: Option<&PathBuf>, game: &mut Game, renderer: &mut Renderer) {
    if let Some(path) = path
        && path.exists()
        && let Err(e) = std::fs::copy(path, path.with_extension("previous.json"))
    {
        log::line(&format!("keeping the old save {}: {e}", path.display()));
    }
    *game = Game::new(new_seed());
    *renderer = Renderer::new();
    renderer.toast("A fresh mine. Happy digging!");
    store(path, game, renderer);
    log::line(&format!("new game, seed {}", game.world.seed));
}

fn store(path: Option<&PathBuf>, game: &Game, renderer: &mut Renderer) {
    let Some(path) = path else {
        return;
    };
    if let Err(e) = save::write(path, &game.to_save()) {
        log::line(&format!("{e:#}"));
        renderer.toast("Couldn't save progress!");
    }
}

/// Hides the mouse pointer while she's playing with the keyboard, and brings
/// it back the moment the mouse moves or a menu needs it.
struct Cursor {
    last: Vec2,
    shown: bool,
}

impl Cursor {
    fn new() -> Cursor {
        Cursor {
            last: Vec2::from(mouse_position()),
            shown: true,
        }
    }

    fn update(&mut self, menu_open: bool) {
        let now = Vec2::from(mouse_position());
        let moved = now.distance(self.last) > 0.5;
        self.last = now;
        let want = if moved || menu_open {
            true
        } else if !get_keys_down().is_empty() {
            false
        } else {
            self.shown
        };
        if want != self.shown {
            show_mouse(want);
            self.shown = want;
        }
    }
}

fn read_input() -> Input {
    let down = |keys: &[KeyCode]| keys.iter().any(|&k| is_key_down(k));
    Input {
        left: down(&[KeyCode::Left, KeyCode::A]),
        right: down(&[KeyCode::Right, KeyCode::D]),
        up: down(&[KeyCode::Up, KeyCode::W, KeyCode::Space]),
        down: down(&[KeyCode::Down, KeyCode::S]),
        bomb: is_key_pressed(KeyCode::B),
        laser: is_key_down(KeyCode::F),
        rocket: is_key_pressed(KeyCode::G),
        ion: is_key_down(KeyCode::H),
        base: is_key_pressed(KeyCode::P),
        teleport: is_key_pressed(KeyCode::T),
        ping: is_key_pressed(KeyCode::Q),
    }
}

fn shop_input(game: &mut Game) {
    if !game.at_shop() {
        return;
    }
    let click = is_mouse_button_pressed(MouseButton::Left).then(|| Vec2::from(mouse_position()));
    for b in render::shop_buttons(game, screen_width()) {
        if b.key.is_some_and(is_key_pressed) || click.is_some_and(|m| b.rect.contains(m)) {
            game.apply(b.action);
        }
    }
}

async fn run(save_path: Option<PathBuf>, settings_path: Option<PathBuf>, mut settings: Settings) {
    prevent_quit();
    clear_background(BLACK);
    draw_text("GemDigger is warming up...", 40.0, 80.0, 40.0, WHITE);
    next_frame().await;

    log::graphics();
    log::line(&format!(
        "screen {}x{} at dpi scale {}",
        screen_width(),
        screen_height(),
        screen_dpi_scale()
    ));
    log::gl_errors("window setup");
    let dir = art::asset_dir();
    let art = art::Art::load(dir.as_deref());
    log::line(&format!("sprites loaded: {}", art.count()));
    log::gl_errors("loading sprites");
    let sound = match audio::Audio::load(dir.as_deref()).await {
        Ok(a) => Some(a),
        Err(e) => {
            log::line(&format!("{e:#}"));
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
    let mut frames = 0u32;
    let mut cursor = Cursor::new();
    loop {
        cursor.update(menu.open);
        let choice = menu.input(&settings);
        let fullscreen_key = is_key_pressed(KeyCode::F11);
        if choice == Some(Choice::Quit) {
            store(save_path.as_ref(), &game, &mut renderer);
            break;
        }
        if choice == Some(Choice::NewGame) {
            start_over(save_path.as_ref(), &mut game, &mut renderer);
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
                log::line(&format!("{e:#}"));
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

        if events.contains(&Event::Towed) {
            log::line("stranded on the surface with nothing to trade; towed to the shop");
        }
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
            log::line("quit cleanly");
            break;
        }
        next_frame().await;
        frames += 1;
        if frames == 1 || frames == 30 {
            refresh_gl_surface();
        }
        if frames == 1 || frames == 120 {
            log::gl_errors(&format!("frame {frames}"));
        }
        if frames == 120 {
            log::line(&format!(
                "screen at frame 120: {}x{} at dpi scale {}",
                screen_width(),
                screen_height(),
                screen_dpi_scale()
            ));
        }
    }
}
