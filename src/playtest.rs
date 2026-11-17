//! An autopilot that plays the real game, renderer and all, through every
//! feature in one continuous run, checking the game's own events at each step
//! and saving a screenshot after it. It's the closest thing to a hands-on
//! playthrough when nobody can hold the keys. No audio, no real save.
//!
//!     gemdigger --playtest <dir>
//!
//! Shortcuts it takes, so a run lasts minutes, not hours: starting money, one
//! bomb, some cargo and the test monsters are granted rather than earned, and
//! shop purchases call the same `apply` a click does.

use std::path::Path;

use macroquad::prelude::*;

use crate::art::Art;
use crate::critters::{self, Critter};
use crate::game::{Event, Game, Gun, Input, Item, SHOP_COLUMNS, ShopAction, SiteKind, Upgrade};
use crate::menu::{Choice, Menu};
use crate::monsters::{Kind, Monster, Phase, Size, Weapon};
use crate::render::Renderer;
use crate::settings::Settings;
use crate::world::Tile;

const DT: f32 = 1.0 / 60.0;

const STEPS: [(&str, f32); 21] = [
    ("buy laser, rockets, ion cannon, base kit and upgrades", 2.0),
    ("drive off the shop", 5.0),
    ("dig down to 70 m", 150.0),
    ("radar ping (Q)", 2.0),
    ("laser cuts rock (F)", 12.0),
    ("rocket hits rock (G)", 5.0),
    ("bomb, then get clear (B)", 6.0),
    ("laser kills a medium monster (F)", 12.0),
    ("ion cannon kills a Huge monster (H)", 12.0),
    ("pick up the relic it dropped", 20.0),
    ("build a base (P)", 12.0),
    ("shop at the base", 2.0),
    ("teleport by sacrificing cargo (T T)", 60.0),
    ("die and respawn at the base", 5.0),
    ("save and reload keeps it all", 1.0),
    ("new game from the Esc menu (pick twice)", 1.0),
    ("drive off the fresh shop", 5.0),
    ("dig the fresh mine to 100 m", 120.0),
    ("laser kills a critter, collect its meat (F)", 10.0),
    ("open a supply crate", 2.0),
    ("claim a ruin's relic", 2.0),
];

#[derive(Default)]
struct Pilot {
    t: f32,
    best_depth: f32,
    stuck: f32,
    side: f32,
    sidestep: f32,
    started: bool,
    teleport_from: f32,
    note: String,
}

impl Pilot {
    /// Holds down to dig; if the depth stalls, sidesteps around whatever's
    /// in the way (a boulder) and tries again.
    fn dig_down(&mut self, g: &Game) -> Input {
        if g.depth() > self.best_depth + 0.3 {
            self.best_depth = g.depth();
            self.stuck = 0.0;
        } else {
            self.stuck += DT;
        }
        if self.sidestep > 0.0 {
            self.sidestep -= DT;
            return Input {
                left: self.side < 0.0,
                right: self.side > 0.0,
                ..Input::default()
            };
        }
        if self.stuck > 1.2 {
            self.stuck = 0.0;
            self.side = if self.side > 0.0 { -1.0 } else { 1.0 };
            self.sidestep = 0.7;
        }
        Input {
            down: true,
            ..Input::default()
        }
    }
}

/// Opens a row of tiles to the pod's right, so a monster there is in line of fire.
fn clear_row(g: &mut Game, len: i32) {
    let (x, y) = (g.pod.x.floor() as i32, g.pod.y.floor() as i32);
    for dx in 1..=len {
        g.world.set(x + dx, y, Tile::Air);
        g.world.set(x + dx, y - 1, Tile::Air);
        g.world.set(x + dx, y - 2, Tile::Air);
    }
}

fn place(g: &mut Game, kind: Kind, dx: f32) {
    let h = kind.size().dims().1;
    let floor = (g.pod.y + 0.5).floor();
    g.monsters.push(Monster {
        dir: -1.0,
        ..Monster::new(kind, g.pod.x + dx, floor - h / 2.0, Phase::Active)
    });
}

/// One frame of the autopilot for `step`: the input to send, and whether the
/// step is done given the events this frame produced.
fn drive(step: usize, p: &mut Pilot, g: &mut Game, events: &[Event]) -> (Input, bool) {
    let first = !p.started;
    p.started = true;
    let saw = |f: &dyn Fn(&Event) -> bool| events.iter().any(f);
    match step {
        0 => {
            if first {
                g.money = 20_000;
                for a in [
                    ShopAction::BuyGun(Gun::Laser),
                    ShopAction::BuyGun(Gun::Rockets),
                    ShopAction::BuyGun(Gun::Ion),
                    ShopAction::BuyBaseKit,
                    ShopAction::Buy(Upgrade::Drill),
                    ShopAction::Buy(Upgrade::Tank),
                    ShopAction::Buy(Upgrade::Lamp),
                ] {
                    g.apply(a);
                }
            }
            let a = g.arsenal;
            (
                Input::default(),
                a.laser && a.rockets && a.ion && g.base_kits == 1 && g.levels.drill == 1,
            )
        }
        1 => (
            Input {
                left: true,
                ..Input::default()
            },
            g.pod.x < SHOP_COLUMNS.0 - 3.0,
        ),
        2 => (p.dig_down(g), g.depth() >= 70.0),
        3 => (
            Input {
                ping: first,
                ..Input::default()
            },
            saw(&|e| *e == Event::Pinged) && g.ping > 0.0,
        ),
        4 => {
            if first {
                g.pod.facing = 1.0;
            }
            let cut = g.laser.is_some() && saw(&|e| matches!(e, Event::Dug { .. }));
            (
                Input {
                    laser: true,
                    ..Input::default()
                },
                cut,
            )
        }
        5 => (
            Input {
                rocket: first,
                ..Input::default()
            },
            saw(&|e| matches!(e, Event::Exploded { .. })),
        ),
        6 => {
            if first {
                g.bombs = 1;
                p.note = format!("hull before {:.0}", g.pod.hull);
            }
            let input = Input {
                bomb: first,
                left: !first && p.t < 1.4,
                up: !first && p.t < 1.4,
                ..Input::default()
            };
            let boom = saw(&|e| matches!(e, Event::Exploded { .. }));
            if boom {
                p.note = format!("{}, after {:.0}", p.note, g.pod.hull);
            }
            (input, boom)
        }
        7 => {
            if first {
                g.pod.facing = 1.0;
                g.pod.hull = g.levels.hull_max();
                clear_row(g, 9);
                place(g, Kind::AncientWorm, 5.0);
            }
            let killed = saw(&|e| {
                matches!(
                    e,
                    Event::MonsterKilled {
                        by: Weapon::Laser,
                        ..
                    }
                )
            });
            (
                Input {
                    laser: true,
                    ..Input::default()
                },
                killed,
            )
        }
        8 => {
            if first {
                g.pod.facing = 1.0;
                g.pod.fuel = g.levels.fuel_max();
                clear_row(g, 16);
                place(g, Kind::ShadowLeviathan, 10.0);
            }
            if saw(&|e| matches!(e, Event::RelicDropped { .. })) {
                p.note = "relic dropped".into();
            }
            // Charge past full, let go for a frame, repeat.
            let charging = g.ion_charge < crate::game::ION_CHARGE + 0.1;
            let killed = saw(&|e| {
                matches!(
                    e,
                    Event::MonsterKilled {
                        size: Size::Huge,
                        by: Weapon::Ion,
                        ..
                    }
                )
            });
            (
                Input {
                    ion: charging,
                    ..Input::default()
                },
                killed,
            )
        }
        9 => {
            let Some(d) = g.drops.first().copied() else {
                return (
                    Input::default(),
                    saw(&|e| matches!(e, Event::RelicFound(_))),
                );
            };
            let dx = d.x - g.pod.x;
            let input = Input {
                left: dx < -0.3,
                right: dx > 0.3,
                up: d.y < g.pod.y - 0.6,
                ..Input::default()
            };
            (input, saw(&|e| matches!(e, Event::RelicFound(_))))
        }
        10 => {
            let press = (p.t / DT) as i32 % 30 == 0;
            if saw(&|e| matches!(e, Event::CantBuild(_))) {
                p.side = -1.0;
                p.sidestep = 0.4;
            }
            if p.sidestep > 0.0 {
                p.sidestep -= DT;
                return (
                    Input {
                        left: true,
                        ..Input::default()
                    },
                    false,
                );
            }
            (
                Input {
                    base: press,
                    ..Input::default()
                },
                saw(&|e| matches!(e, Event::BaseBuilt { .. })),
            )
        }
        11 => (
            Input::default(),
            g.current_site().map(|s| s.kind) == Some(SiteKind::Base),
        ),
        12 => {
            if first {
                g.cargo = vec![Item::Gem(2), Item::Gem(3), Item::Gem(3)];
                p.teleport_from = g.depth();
                p.best_depth = g.depth();
            }
            // Dig well away from the base first, so the jump is a real one.
            if g.depth() < p.teleport_from + 15.0 && p.note.is_empty() {
                return (p.dig_down(g), false);
            }
            if p.note.is_empty() {
                p.note = format!("teleported from {:.0} m", g.depth());
                p.t = 0.0;
            }
            let tap = (p.t / DT) as i32 == 2 || (p.t / DT) as i32 == 12;
            let done = saw(&|e| matches!(e, Event::Teleported { .. }));
            if done {
                p.note = format!(
                    "{}, landed at a {:?}",
                    p.note,
                    g.current_site().map(|s| s.kind)
                );
            }
            (
                Input {
                    teleport: tap,
                    ..Input::default()
                },
                done,
            )
        }
        13 => {
            if first {
                g.pod.y += 20.0;
                g.pod.fuel = 0.0;
            }
            let back = saw(&|e| *e == Event::Respawned);
            (
                Input::default(),
                back && g.current_site().map(|s| s.kind) == Some(SiteKind::Base),
            )
        }
        16 => (
            Input {
                left: true,
                ..Input::default()
            },
            g.pod.x < SHOP_COLUMNS.0 - 3.0,
        ),
        17 => (p.dig_down(g), g.depth() >= 100.0),
        18 => {
            if first {
                g.arsenal.laser = true;
                g.cargo.clear();
                g.pod.facing = 1.0;
                g.pod.fuel = g.levels.fuel_max();
                g.pod.hull = g.levels.hull_max();
                clear_row(g, 6);
                let (x, floor) = (g.pod.x.floor() as i32, (g.pod.y + 0.5).floor());
                for dx in 1..=6 {
                    g.world.set(x + dx, floor as i32, Tile::Dirt);
                }
                g.critters.push(Critter {
                    kind: critters::Kind::Mole,
                    x: g.pod.x + 4.0,
                    y: floor - critters::HALF,
                    vy: 0.0,
                    dir: -1.0,
                });
            }
            if saw(&|e| matches!(e, Event::CritterKilled { .. })) {
                p.note = "critter shot".into();
            }
            let alive = !g.critters.is_empty();
            (
                Input {
                    laser: alive,
                    right: !alive,
                    ..Input::default()
                },
                g.cargo.contains(&Item::Meat),
            )
        }
        19 => {
            if first {
                g.bombs = 0;
                let (px, py) = (g.pod.x, g.pod.y);
                let near = |&(x, y): &(i32, i32)| (x as f32 - px).hypot(y as f32 - py);
                let crates = g.crates_near(px, py, 48.0);
                if let Some(&(x, y)) = crates.iter().min_by(|a, b| near(a).total_cmp(&near(b))) {
                    g.pod.x = x as f32 + 0.5;
                    g.pod.y = y as f32 + 0.5;
                    p.note = format!("crate at {x}, {y}");
                }
            }
            (
                Input::default(),
                saw(&|e| matches!(e, Event::CrateOpened { .. })),
            )
        }
        20 => {
            if first {
                let (px, py) = (g.pod.x, g.pod.y);
                let near = |&(x, y, _): &(i32, i32, bool)| (x as f32 - px).hypot(y as f32 - py);
                let ruins = g.ruins_near(px, py, 400.0);
                if let Some(&(x, y, _)) = ruins.iter().min_by(|a, b| near(a).total_cmp(&near(b))) {
                    let (rx, ry) = Game::ruin_relic_at(x, y);
                    // One tile to the side, so the first frame shows the relic waiting.
                    g.pod.x = rx - 1.5;
                    g.pod.y = ry;
                    p.note = format!("ruin at {x}, {y}");
                }
            }
            (
                Input {
                    right: p.t > 0.5,
                    ..Input::default()
                },
                saw(&|e| matches!(e, Event::RelicFound(_))),
            )
        }
        _ => {
            let json = serde_json::to_string(&g.to_save()).unwrap_or_default();
            let ok = serde_json::from_str(&json)
                .ok()
                .map(Game::from_save)
                .is_some_and(|b| {
                    b.arsenal == g.arsenal
                        && b.bases == g.bases
                        && b.levels.relics() == g.levels.relics()
                });
            (Input::default(), ok)
        }
    }
}

const NEW_GAME_STEP: usize = 15;

/// Picks New game twice in the real Esc menu, with a screenshot of the
/// confirm, then runs the same reset the game does and checks the old mine
/// is gone (and kept as a backup) and a different one took its place.
async fn new_game(
    dir: &Path,
    game: &mut Game,
    renderer: &mut Renderer,
    art: &Art,
) -> anyhow::Result<(bool, String)> {
    let save = dir.join("save.json");
    crate::save::write(&save, &game.to_save())?;
    let old_seed = game.world.seed;
    let mut menu = Menu::new();
    menu.open = true;
    let first = menu.pick(Choice::NewGame);
    renderer.draw(game, art, true);
    menu.draw(&Settings::default());
    get_screen_data().export_png(&dir.join("15a_new_game_confirm.png").to_string_lossy());
    next_frame().await;
    let second = menu.pick(Choice::NewGame);
    if second == Some(Choice::NewGame) {
        crate::start_over(Some(&save), game, renderer);
    }
    let reloaded = match crate::save::load(&save)? {
        crate::save::Loaded::Resumed(d) => Game::from_save(*d).world.seed,
        _ => 0,
    };
    let ok = first.is_none()
        && second == Some(Choice::NewGame)
        && !menu.open
        && game.world.seed != old_seed
        && reloaded == game.world.seed
        && game.deaths == 0
        && game.bases.is_empty()
        && game.depth() < 1.0
        && save.with_extension("previous.json").exists();
    Ok((ok, format!("seed {old_seed} -> {}", game.world.seed)))
}

pub async fn run(dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    let art = Art::load(None);
    let mut game = Game::new(11);
    let mut renderer = Renderer::new();
    renderer.snap_to(&game);
    let mut events: Vec<Event> = Vec::new();
    let mut results = Vec::new();
    for (i, (name, timeout)) in STEPS.iter().enumerate() {
        let mut pilot = Pilot::default();
        let mut passed = false;
        if i == NEW_GAME_STEP {
            let (ok, note) = new_game(dir, &mut game, &mut renderer, &art).await?;
            passed = ok;
            pilot.note = note;
        }
        while !passed && i != NEW_GAME_STEP && pilot.t < *timeout {
            let (input, done) = drive(i, &mut pilot, &mut game, &events);
            if done {
                passed = true;
                break;
            }
            events = game.update(DT, input);
            renderer.update(DT, &game, &events);
            renderer.draw(&game, &art, true);
            // The pit just under the grass, where sky, turf and cave wall meet.
            if i == 2 && (pilot.t - 0.9).abs() < DT / 2.0 {
                get_screen_data().export_png(&dir.join("02a_surface_pit.png").to_string_lossy());
            }
            if i == 7 && (pilot.t - 1.0).abs() < DT / 2.0 {
                get_screen_data()
                    .export_png(&dir.join("07b_fight_health_bar.png").to_string_lossy());
            }
            next_frame().await;
            pilot.t += DT;
        }
        // One more frame so the screenshot shows the step's result.
        renderer.update(DT, &game, &[]);
        renderer.draw(&game, &art, true);
        let slug: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect();
        get_screen_data().export_png(&dir.join(format!("{i:02}_{slug}.png")).to_string_lossy());
        next_frame().await;
        let line = format!(
            "{} {i:2}. {name:<52} {:5.1}s  depth {:4.0} m  {}",
            if passed { "PASS" } else { "FAIL" },
            pilot.t,
            game.depth(),
            pilot.note
        );
        println!("{line}");
        results.push(passed);
        events.clear();
    }
    let passed = results.iter().filter(|&&p| p).count();
    println!(
        "{passed}/{} steps passed; deaths {}, relics {}, bases {}",
        results.len(),
        game.deaths,
        game.levels.relics(),
        game.bases.len()
    );
    Ok(())
}
