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
    Quit,
}

pub struct Menu {
    pub open: bool,
    cursor: usize,
}

fn on_off(on: bool) -> &'static str {
    if on { "On" } else { "Off" }
}

fn rows(s: &Settings) -> [(String, Choice); 6] {
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
        }
    }

    /// Reads this frame's input; returns a choice when one is made.
    pub fn input(&mut self, s: &Settings) -> Option<Choice> {
        if is_key_pressed(KeyCode::Escape) {
            self.open = !self.open;
            self.cursor = 0;
            return None;
        }
        if !self.open {
            return None;
        }
        let rows = rows(s);
        if is_key_pressed(KeyCode::Down) || is_key_pressed(KeyCode::S) {
            self.cursor = (self.cursor + 1) % rows.len();
        }
        if is_key_pressed(KeyCode::Up) || is_key_pressed(KeyCode::W) {
            self.cursor = (self.cursor + rows.len() - 1) % rows.len();
        }
        let mouse = Vec2::from(mouse_position());
        for i in 0..rows.len() {
            if row_rect(i).contains(mouse) {
                self.cursor = i;
            }
        }
        let clicked =
            is_mouse_button_pressed(MouseButton::Left) && row_rect(self.cursor).contains(mouse);
        if !(clicked || is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::Space)) {
            return None;
        }
        let choice = rows[self.cursor].1;
        if choice == Choice::Resume {
            self.open = false;
        }
        Some(choice)
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
        for (i, (label, _)) in rows(s).iter().enumerate() {
            let r = row_rect(i);
            let selected = i == self.cursor;
            let bg = if selected {
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
            row_rect(5).y + 100.0,
            22.0,
            Color::new(0.75, 0.75, 0.8, 1.0),
        );
    }
}
