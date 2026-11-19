//! The ESC menu: pauses the game and toggles settings. Works with the
//! keyboard (arrows and Enter) or the mouse, whichever she reaches for.

use macroquad::prelude::*;

use crate::settings::Settings;

/// What picking a menu row asks the main loop to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Resume,
    ToggleMusic,
    ToggleSound,
    ToggleShake,
    ToggleFullscreen,
    /// Throw away this mine and dig a freshly rolled one.
    NewGame,
    Quit,
}

pub struct Menu {
    pub open: bool,
    cursor: usize,
    /// New game was picked once; picking it again confirms.
    armed: bool,
}

fn on_off(on: bool) -> &'static str {
    if on { "On" } else { "Off" }
}

fn rows(s: &Settings, armed: bool) -> [(String, Choice); 7] {
    [
        ("Resume".to_string(), Choice::Resume),
        (format!("Music: {}", on_off(s.music)), Choice::ToggleMusic),
        (
            format!("Sound effects: {}", on_off(s.sound)),
            Choice::ToggleSound,
        ),
        (
            format!("Screen shake: {}", on_off(s.shake)),
            Choice::ToggleShake,
        ),
        (
            format!("Full screen: {}", on_off(s.fullscreen)),
            Choice::ToggleFullscreen,
        ),
        (
            if armed {
                "Erase this mine? Press again".to_string()
            } else {
                "New game (new map)".to_string()
            },
            Choice::NewGame,
        ),
        ("Save and quit".to_string(), Choice::Quit),
    ]
}

fn row_rect(i: usize) -> Rect {
    let (w, h) = (420.0, 54.0);
    let top = screen_height() / 2.0 - 3.0 * (h + 10.0) + 30.0;
    Rect::new(
        screen_width() / 2.0 - w / 2.0,
        top + i as f32 * (h + 10.0),
        w,
        h,
    )
}

impl Menu {
    pub fn new() -> Menu {
        Menu {
            open: false,
            cursor: 0,
            armed: false,
        }
    }

    /// Turns a picked row into what the main loop should do. NOTE: New game
    /// wipes her progress, so it takes two picks in a row; anything else in
    /// between (moving off it, closing the menu) disarms it.
    pub fn pick(&mut self, choice: Choice) -> Option<Choice> {
        if choice == Choice::NewGame && !self.armed {
            self.armed = true;
            return None;
        }
        self.armed = false;
        if matches!(choice, Choice::Resume | Choice::NewGame) {
            self.open = false;
        }
        Some(choice)
    }

    fn move_to(&mut self, cursor: usize) {
        if cursor != self.cursor {
            self.armed = false;
        }
        self.cursor = cursor;
    }

    /// Reads this frame's input; returns a choice when one is made.
    pub fn input(&mut self, s: &Settings) -> Option<Choice> {
        if is_key_pressed(KeyCode::Escape) {
            self.open = !self.open;
            self.cursor = 0;
            self.armed = false;
            return None;
        }
        if !self.open {
            return None;
        }
        let rows = rows(s, self.armed);
        if is_key_pressed(KeyCode::Down) || is_key_pressed(KeyCode::S) {
            self.move_to((self.cursor + 1) % rows.len());
        }
        if is_key_pressed(KeyCode::Up) || is_key_pressed(KeyCode::W) {
            self.move_to((self.cursor + rows.len() - 1) % rows.len());
        }
        let mouse = Vec2::from(mouse_position());
        for i in 0..rows.len() {
            if row_rect(i).contains(mouse) {
                self.move_to(i);
            }
        }
        let clicked =
            is_mouse_button_pressed(MouseButton::Left) && row_rect(self.cursor).contains(mouse);
        if !(clicked || is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Space)) {
            return None;
        }
        self.pick(rows[self.cursor].1)
    }

    pub fn draw(&self, s: &Settings) {
        if !self.open {
            return;
        }
        let (sw, sh) = (screen_width(), screen_height());
        draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.03, 0.6));
        let title = "Paused";
        let tw = measure_text(title, None, 56, 1.0).width;
        draw_text(
            title,
            sw / 2.0 - tw / 2.0,
            row_rect(0).y - 40.0,
            56.0,
            WHITE,
        );
        let rows = rows(s, self.armed);
        for (i, (label, choice)) in rows.iter().enumerate() {
            let r = row_rect(i);
            let selected = i == self.cursor;
            let bg = if *choice == Choice::NewGame && self.armed {
                Color::new(0.5, 0.12, 0.1, 0.95)
            } else if selected {
                Color::new(0.35, 0.3, 0.15, 0.95)
            } else {
                Color::new(0.1, 0.09, 0.14, 0.9)
            };
            draw_rectangle(r.x, r.y, r.w, r.h, bg);
            if selected {
                draw_rectangle_lines(r.x, r.y, r.w, r.h, 3.0, Color::new(1.0, 0.82, 0.3, 1.0));
            }
            let lw = measure_text(label, None, 30, 1.0).width;
            draw_text(label, r.x + r.w / 2.0 - lw / 2.0, r.y + 36.0, 30.0, WHITE);
        }
        let hint = "Esc to resume.  F11 toggles full screen.";
        let hw = measure_text(hint, None, 22, 1.0).width;
        draw_text(
            hint,
            sw / 2.0 - hw / 2.0,
            row_rect(rows.len() - 1).y + 100.0,
            22.0,
            Color::new(0.75, 0.75, 0.8, 1.0),
        );
    }
}

fn update_button(yes: bool) -> Rect {
    let (w, h) = (240.0, 54.0);
    let x = if yes {
        screen_width() / 2.0 - w - 10.0
    } else {
        screen_width() / 2.0 + 10.0
    };
    Rect::new(x, screen_height() / 2.0 + 10.0, w, h)
}

/// Her answer to an offered update, if she gave one this frame: Enter or a
/// click on Update for yes, Esc or a click on Later for no.
pub fn update_answer() -> Option<bool> {
    let mouse = Vec2::from(mouse_position());
    let clicked = |yes: bool| {
        is_mouse_button_pressed(MouseButton::Left) && update_button(yes).contains(mouse)
    };
    if is_key_pressed(KeyCode::Enter) || clicked(true) {
        return Some(true);
    }
    if is_key_pressed(KeyCode::Escape) || clicked(false) {
        return Some(false);
    }
    None
}

/// The panel over a paused game: the offer, or the download in progress.
pub fn draw_update(offered: Option<&str>, installing: Option<&str>) {
    if offered.is_none() && installing.is_none() {
        return;
    }
    let (sw, sh) = (screen_width(), screen_height());
    draw_rectangle(0.0, 0.0, sw, sh, Color::new(0.0, 0.0, 0.03, 0.6));
    // A solid card, or the words are lost against whatever the game shows behind.
    let card = Rect::new(sw / 2.0 - 320.0, sh / 2.0 - 150.0, 640.0, 250.0);
    draw_rectangle(
        card.x,
        card.y,
        card.w,
        card.h,
        Color::new(0.06, 0.05, 0.09, 0.96),
    );
    draw_rectangle_lines(
        card.x,
        card.y,
        card.w,
        card.h,
        3.0,
        Color::new(1.0, 0.82, 0.3, 1.0),
    );
    let centered = |s: &str, y: f32, size: u16, color: Color| {
        let w = measure_text(s, None, size, 1.0).width;
        draw_text(s, sw / 2.0 - w / 2.0, y, f32::from(size), color);
    };
    if let Some(version) = installing {
        centered(
            &format!("Getting GemDigger {version}..."),
            sh / 2.0 - 20.0,
            44,
            WHITE,
        );
        centered(
            "The game restarts when it's ready.",
            sh / 2.0 + 30.0,
            26,
            Color::new(0.75, 0.75, 0.8, 1.0),
        );
        return;
    }
    let Some(version) = offered else {
        return;
    };
    centered(
        &format!("GemDigger {version} is out!"),
        sh / 2.0 - 80.0,
        48,
        WHITE,
    );
    centered(
        "Update now? Your mine is saved first.",
        sh / 2.0 - 30.0,
        26,
        Color::new(0.75, 0.75, 0.8, 1.0),
    );
    let mouse = Vec2::from(mouse_position());
    for (yes, label) in [(true, "Update (Enter)"), (false, "Later (Esc)")] {
        let r = update_button(yes);
        let bg = if yes {
            Color::new(0.35, 0.3, 0.15, 0.95)
        } else {
            Color::new(0.1, 0.09, 0.14, 0.9)
        };
        draw_rectangle(r.x, r.y, r.w, r.h, bg);
        if yes || r.contains(mouse) {
            draw_rectangle_lines(r.x, r.y, r.w, r.h, 3.0, Color::new(1.0, 0.82, 0.3, 1.0));
        }
        let lw = measure_text(label, None, 28, 1.0).width;
        draw_text(label, r.x + r.w / 2.0 - lw / 2.0, r.y + 36.0, 28.0, WHITE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One stray Enter must not wipe a mine she's spent hours on.
    #[test]
    fn new_game_needs_a_second_pick() {
        let mut m = Menu {
            open: true,
            cursor: 5,
            armed: false,
        };
        assert_eq!(m.pick(Choice::NewGame), None);
        assert!(m.open);
        assert_eq!(m.pick(Choice::NewGame), Some(Choice::NewGame));
        assert!(!m.open, "menu stayed open over the fresh mine");
    }

    /// Arming, then wandering off and back, starts the confirm over.
    #[test]
    fn moving_off_new_game_disarms_it() {
        let mut m = Menu {
            open: true,
            cursor: 5,
            armed: false,
        };
        m.pick(Choice::NewGame);
        m.move_to(4);
        m.move_to(5);
        assert_eq!(m.pick(Choice::NewGame), None);
    }

    /// Hovering the same row every frame (the mouse sits still) keeps it armed.
    #[test]
    fn staying_on_new_game_keeps_it_armed() {
        let mut m = Menu {
            open: true,
            cursor: 5,
            armed: false,
        };
        m.pick(Choice::NewGame);
        m.move_to(5);
        assert_eq!(m.pick(Choice::NewGame), Some(Choice::NewGame));
    }
}
