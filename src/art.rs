//! Sprites, baked into the exe so the game is one file. A PNG with the same
//! name in an `assets/sprites` folder next to the game replaces the built-in
//! one, so new art can be tried without rebuilding.

use std::collections::HashMap;
use std::path::Path;

use macroquad::prelude::*;

macro_rules! sprites {
    ($($name:literal),* $(,)?) => {
        const SPRITES: &[(&str, &[u8])] = &[
            $(($name, include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/sprites/", $name, ".png")))),*
        ];
    };
}

sprites!(
    "pod_0",
    "pod_1",
    "shop",
    "sky",
    "stash",
    "boulder",
    "grass_0",
    "grass_1",
    "grass_2",
    "dirt_0_0",
    "dirt_0_1",
    "dirt_0_2",
    "dirt_1_0",
    "dirt_1_1",
    "dirt_1_2",
    "dirt_2_0",
    "dirt_2_1",
    "dirt_2_2",
    "dirt_3_0",
    "dirt_3_1",
    "dirt_3_2",
    "dirt_4_0",
    "dirt_4_1",
    "dirt_4_2",
    "ore_0",
    "ore_1",
    "ore_2",
    "ore_3",
    "ore_4",
    "ore_5",
    "monster_small_0",
    "monster_small_1",
    "monster_medium_0",
    "monster_medium_1",
    "monster_large_0",
    "monster_large_1",
    "icon_fuel",
    "icon_hull",
    "icon_cargo",
    "icon_money",
    "icon_depth",
);

pub struct Art {
    textures: HashMap<&'static str, Texture2D>,
}

impl Art {
    pub fn load(dir: Option<&Path>) -> Art {
        let mut textures = HashMap::new();
        for &(name, built_in) in SPRITES {
            let custom =
                dir.and_then(|d| std::fs::read(d.join("sprites").join(format!("{name}.png"))).ok());
            let image = custom
                .and_then(|bytes| Image::from_file_with_format(&bytes, None).ok())
                .or_else(|| Image::from_file_with_format(built_in, None).ok());
            // NOTE: a built-in sprite that fails to decode is skipped, and the
            // renderer falls back to drawing that thing with shapes.
            let Some(image) = image else {
                continue;
            };
            let texture = Texture2D::from_image(&image);
            texture.set_filter(FilterMode::Linear);
            textures.insert(name, texture);
        }
        Art { textures }
    }

    pub fn get(&self, name: &str) -> Option<&Texture2D> {
        self.textures.get(name)
    }
}

/// The `assets` folder next to the exe, or in the working directory when run with cargo.
pub fn asset_dir() -> Option<std::path::PathBuf> {
    let beside_exe = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("assets")));
    let in_cwd = std::env::current_dir().ok().map(|d| d.join("assets"));
    [beside_exe, in_cwd]
        .into_iter()
        .flatten()
        .find(|d| d.is_dir())
}
