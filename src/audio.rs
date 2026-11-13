//! Plays the synthesized sounds in response to game events, and crossfades the
//! five music loops as the pod goes deeper. A file named after a sound in an
//! `assets/sounds` folder next to the game replaces the synthesized one.

use std::collections::HashMap;
use std::path::Path;

use anyhow::anyhow;
use macroquad::audio::{
    PlaySoundParams, Sound, load_sound_from_bytes, play_sound, play_sound_once, set_sound_volume,
};

use crate::game::{Event, Game};
use crate::monsters::Size;
use crate::synth;
use crate::world;

const MUSIC_VOLUME: f32 = 0.35;

pub struct Audio {
    music: Vec<Sound>,
    sfx: HashMap<String, Sound>,
}

async fn load(
    dir: Option<&Path>,
    name: &str,
    make: impl FnOnce() -> Vec<f32>,
) -> anyhow::Result<Sound> {
    let custom = dir.and_then(|d| {
        ["ogg", "wav"]
            .iter()
            .find_map(|ext| std::fs::read(d.join("sounds").join(format!("{name}.{ext}"))).ok())
    });
    let bytes = custom.unwrap_or_else(|| synth::wav(&make()));
    load_sound_from_bytes(&bytes)
        .await
        .map_err(|e| anyhow!("unable to load sound \"{name}\": {e:?}"))
}

fn growl_name(size: Size) -> String {
    let i = Size::ALL.iter().position(|&s| s == size).unwrap_or(0);
    format!("growl_{i}")
}

impl Audio {
    pub async fn load(dir: Option<&Path>) -> anyhow::Result<Audio> {
        let mut music = Vec::new();
        for band in 0..5 {
            music.push(load(dir, &format!("music_{band}"), || synth::music(band)).await?);
        }
        let mut sfx = HashMap::new();
        for name in synth::sfx_names() {
            let sound = load(dir, &name, || synth::sfx(&name)).await?;
            sfx.insert(name, sound);
        }
        Ok(Audio { music, sfx })
    }

    /// Starts every loop silently; `update` fades them in and out.
    pub fn start(&self) {
        let silent = || PlaySoundParams {
            looped: true,
            volume: 0.0,
        };
        for track in &self.music {
            play_sound(track, silent());
        }
        for name in ["drill", "thrust"] {
            if let Some(s) = self.sfx.get(name) {
                play_sound(s, silent());
            }
        }
    }

    fn play(&self, name: &str, volume: f32) {
        let Some(s) = self.sfx.get(name) else {
            return;
        };
        if volume >= 1.0 {
            play_sound_once(s);
        } else {
            play_sound(
                s,
                PlaySoundParams {
                    looped: false,
                    volume,
                },
            );
        }
    }

    /// Sets loop volumes from the pod's state; call every frame.
    pub fn update(&self, game: &Game) {
        let pos = world::band_position(game.depth());
        for (i, track) in self.music.iter().enumerate() {
            let w = (1.0 - (pos - i as f32).abs()).max(0.0);
            set_sound_volume(track, w * MUSIC_VOLUME);
        }
        if let Some(s) = self.sfx.get("drill") {
            set_sound_volume(s, if game.pod.dig.is_some() { 0.35 } else { 0.0 });
        }
        if let Some(s) = self.sfx.get("thrust") {
            set_sound_volume(s, if game.pod.thrusting { 0.3 } else { 0.0 });
        }
    }

    pub fn react(&self, events: &[Event]) {
        for e in events {
            match e {
                Event::Dug { .. } => self.play("dug", 0.5),
                Event::Pickup { tier, .. } => self.play(&format!("pickup_{tier}"), 0.8),
                Event::CargoFull => self.play("full", 0.6),
                Event::Landed { .. } => self.play("land", 0.6),
                Event::Hurt { .. } => self.play("hit", 1.0),
                Event::Died { .. } => self.play("death", 1.0),
                Event::Respawned => self.play("respawn", 0.7),
                Event::Sold { .. } => self.play("sell", 0.8),
                Event::Refueled | Event::Repaired | Event::Bought(_) => self.play("buy", 0.7),
                Event::CantAfford => self.play("deny", 0.6),
                Event::LowFuel => self.play("low_fuel", 0.6),
                Event::MonsterWarning { .. } => self.play("rumble", 1.0),
                Event::MonsterEmerged { size, .. } => self.play(&growl_name(*size), 0.9),
                Event::StashRecovered { .. } => self.play("stash", 0.9),
                Event::DigStarted { .. } => {}
            }
        }
    }
}
