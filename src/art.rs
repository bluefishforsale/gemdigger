//! Sprites, baked into the exe so the game is one file. A PNG with the same
//! name in an `assets/sprites` folder next to the game replaces the built-in
//! one, so new art can be tried without rebuilding.
//!
//! Everything is packed into one atlas texture at startup. NOTE: that's what
//! keeps a screen full of tiles to a handful of draw calls; one texture per
//! sprite broke batching on every tile and cost most of the frame at 4K.

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
    "pod_drill_0",
    "pod_drill_1",
    "pod_thrust_0",
    "pod_thrust_1",
    "pod_thrust_2",
    "shop",
    "stash",
    "boulder",
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
    "mountains_far",
    "mountains_mid",
    "mountains_near",
    "cave_bg_0",
    "cave_bg_1",
    "cave_bg_2",
    "cave_bg_3",
    "cave_bg_4",
    "cave_fg_1",
    "cave_fg_2",
    "cave_fg_3",
    "cave_fg_4",
    "topsoil_bg_0",
    "topsoil_bg_1",
    "topsoil_bg_2",
    "topsoil_bg_3",
    "topsoil_fg_0",
    "topsoil_fg_1",
    "topsoil_fg_2",
    "topsoil_fg_3",
    "turf_lip_0",
    "turf_lip_1",
    "turf_lip_2",
    "tree_0",
    "tree_1",
    "tree_2",
    "tree_3",
    "tree_4",
    "tree_5",
    "tree_6",
    "tree_7",
    "tree_8",
    "tree_9",
    "tree_10",
    "bush_0",
    "bush_1",
    "bush_2",
    "bush_3",
    "bush_4",
    "bush_5",
    "bush_6",
    "bush_7",
    "bush_8",
    "flower_0",
    "flower_1",
    "flower_2",
    "flower_3",
    "flower_4",
    "flower_5",
    "flower_6",
    "flower_7",
    "cloud_0",
    "cloud_1",
    "cloud_2",
    "cloud_3",
    "cloud_4",
    "cloud_5",
    "cloud_6",
    "cloud_7",
    "cloud_8",
    "cottage",
    "windmill",
    "mine_entrance",
    "fence",
    "lamp_post",
    "cart",
    "crate",
    "tent",
    "campfire",
    "jerrycan",
    "workbench",
    "chest_closed",
    "chest_open",
    "lantern_0",
    "lantern_1",
    "lantern_2",
    "energy_cell_0",
    "energy_cell_1",
    "energy_cell_2",
    "meat",
    "relic_coin",
    "relic_rune",
    "relic_idol_0",
    "relic_idol_1",
    "relic_idol_2",
    "relic_idol_3",
    "ruin_0",
    "ruin_1",
    "ruin_2",
    "ruin_3",
    "ruin_4",
    "ruin_5",
    "ruin_6",
    "ruin_7",
    "ruin_8",
    "ruin_9",
    "ruin_10",
    "ruin_11",
    "ruin_12",
    "ruin_13",
    "ruin_14",
    "ruin_15",
    "ruin_16",
    "ruin_17",
    "ruin_18",
    "ruin_19",
    "ruin_20",
    "ruin_21",
    "ruin_22",
    "ruin_23",
    "ruin_24",
    "ruin_25",
    "ruin_26",
    "ruin_27",
    "ruin_28",
    "ruin_29",
    "ruin_30",
    "ruin_31",
    "ruin_32",
    "ruin_33",
    "ruin_34",
    "ruin_35",
    "ruin_36",
    "pickaxe_0",
    "pickaxe_1",
    "pickaxe_2",
    "pickaxe_3",
    "drill_icon_0",
    "drill_icon_1",
    "waterfall_cave",
    "glow_mushroom",
    "glow_plant",
    "roots",
    "stalactite_0",
    "stalactite_1",
    "stalagmite_0",
    "crystal_pool",
    "bones_0",
    "bones_1",
    "crystal_0",
    "crystal_1",
    "crystal_2",
    "crystal_3",
    "crystal_4",
    "pillar_0",
    "pillar_1",
    "pillar_2",
    "pillar_3",
    "mine_support",
    "ruins_0",
    "ruins_1",
    "ruins_2",
    "critter_leaf_0",
    "critter_leaf_1",
    "critter_mushroom_0",
    "critter_mushroom_1",
    "critter_firefly_0",
    "critter_firefly_1",
    "critter_ghost_0",
    "critter_ghost_1",
    "critter_beetle_0",
    "critter_beetle_1",
    "critter_fish_0",
    "critter_fish_1",
    "critter_spider_0",
    "critter_spider_1",
    "critter_golem_0",
    "critter_golem_1",
    "critter_mole_0",
    "critter_mole_1",
    "monster_ancient_worm_0",
    "monster_ancient_worm_1",
    "monster_ancient_worm_2",
    "monster_ancient_worm_3",
    "monster_ancient_worm_4",
    "monster_ancient_worm_5",
    "monster_ancient_worm_6",
    "monster_ancient_worm_7",
    "monster_crystal_golem_0",
    "monster_crystal_golem_1",
    "monster_crystal_golem_2",
    "monster_crystal_golem_3",
    "monster_crystal_golem_4",
    "monster_crystal_golem_5",
    "monster_crystal_golem_6",
    "monster_crystal_golem_7",
    "monster_moss_titan_0",
    "monster_moss_titan_1",
    "monster_moss_titan_2",
    "monster_moss_titan_3",
    "monster_moss_titan_4",
    "monster_moss_titan_5",
    "monster_moss_titan_6",
    "monster_moss_titan_7",
    "monster_shadow_leviathan_0",
    "monster_shadow_leviathan_1",
    "monster_shadow_leviathan_2",
    "monster_shadow_leviathan_3",
    "monster_shadow_leviathan_4",
    "monster_shadow_leviathan_5",
    "monster_shadow_leviathan_6",
    "monster_shadow_leviathan_7",
    "monster_deep_one_0",
    "monster_deep_one_1",
    "monster_deep_one_2",
    "monster_deep_one_3",
    "monster_deep_one_4",
    "monster_deep_one_5",
    "monster_deep_one_6",
    "monster_deep_one_7",
    "monster_void_spider_0",
    "monster_void_spider_1",
    "monster_void_spider_2",
    "monster_void_spider_3",
    "monster_void_spider_4",
    "monster_void_spider_5",
    "monster_void_spider_6",
    "monster_void_spider_7",
    "fx_leaf_0",
    "fx_leaf_1",
    "fx_leaf_2",
    "fx_leaf_3",
    "fx_leaf_4",
    "fx_leaf_5",
    "fx_leaf_6",
    "fx_leaf_7",
    "fx_leaf_8",
    "fx_wisp_0",
    "fx_wisp_1",
    "fx_wisp_2",
    "fx_wisp_3",
    "fx_wisp_4",
    "fx_splash_0",
    "fx_splash_1",
    "fx_splash_2",
    "fx_smoke_0",
    "fx_smoke_1",
    "fx_smoke_2",
    "fx_smoke_3",
    "fx_sparkle_0",
    "fx_sparkle_1",
    "fx_sparkle_2",
    "fx_sparkle_3",
    "fx_sparkle_4",
    "fx_sparkle_5",
    "fx_sparkle_6",
    "fx_sparkle_7",
    "fx_sparkle_8",
    "fx_dust_0",
    "fx_dust_1",
    "fx_dust_2",
    "fx_dust_3",
    "fx_dust_4",
    "fx_dust_5",
    "fx_debris_dirt",
    "fx_debris_stone",
    "fx_explosion_0",
    "fx_explosion_1",
    "fx_explosion_2",
    "fx_explosion_3",
);

/// Gap between packed sprites, filled with each sprite's own edge pixels so
/// smooth scaling never pulls in a neighbour.
const PAD: usize = 2;
const ATLAS_WIDTH: usize = 2048;
/// Name of the solid white patch used for plain rectangles.
const WHITE: &str = "__white";

/// Where one sprite lives in the atlas.
#[derive(Clone, Copy)]
pub struct Sprite<'a> {
    pub tex: &'a Texture2D,
    pub src: Rect,
}

impl Sprite<'_> {
    pub fn size(&self) -> Vec2 {
        vec2(self.src.w, self.src.h)
    }

    /// Draws the sprite stretched to `w` x `h` with its top-left at (x, y).
    pub fn draw(&self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        self.draw_ex(x, y, w, h, color, false, 0.0);
    }

    #[allow(clippy::too_many_arguments)] // Mirrors draw_texture_ex's own options.
    pub fn draw_ex(
        &self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: Color,
        flip_x: bool,
        rotation: f32,
    ) {
        draw_texture_ex(
            self.tex,
            x,
            y,
            color,
            DrawTextureParams {
                dest_size: Some(vec2(w, h)),
                source: Some(self.src),
                flip_x,
                rotation,
                ..Default::default()
            },
        );
    }
}

pub struct Art {
    atlas: Option<Texture2D>,
    rects: HashMap<&'static str, Rect>,
}

/// Copies `img` into `atlas` at (x, y), smearing its edge pixels PAD wide.
fn blit(atlas: &mut [u8], img: &Image, x: usize, y: usize) {
    let (w, h) = (img.width as usize, img.height as usize);
    for dy in 0..h + 2 * PAD {
        let sy = dy.saturating_sub(PAD).min(h - 1);
        for dx in 0..w + 2 * PAD {
            let sx = dx.saturating_sub(PAD).min(w - 1);
            let from = (sy * w + sx) * 4;
            let to = ((y + dy) * ATLAS_WIDTH + x + dx) * 4;
            atlas[to..to + 4].copy_from_slice(&img.bytes[from..from + 4]);
        }
    }
}

impl Art {
    pub fn load(dir: Option<&Path>) -> Art {
        let mut images: Vec<(&'static str, Image)> = Vec::new();
        for &(name, built_in) in SPRITES {
            let custom =
                dir.and_then(|d| std::fs::read(d.join("sprites").join(format!("{name}.png"))).ok());
            let image = custom
                .and_then(|bytes| Image::from_file_with_format(&bytes, None).ok())
                .or_else(|| Image::from_file_with_format(built_in, None).ok());
            // NOTE: a sprite that fails to decode is skipped, and the renderer
            // falls back to drawing that thing with shapes.
            if let Some(image) = image
                && image.width > 0
                && image.height > 0
                && (image.width as usize) + 2 * PAD <= ATLAS_WIDTH
            {
                images.push((name, image));
            }
        }
        images.push((WHITE, Image::gen_image_color(8, 8, macroquad::color::WHITE)));

        // Shelf packing, tallest first, so rows waste little height.
        images.sort_by_key(|(_, img)| std::cmp::Reverse(img.height));
        let mut placed = Vec::with_capacity(images.len());
        let (mut x, mut y, mut row_h) = (0usize, 0usize, 0usize);
        for (name, img) in &images {
            let (w, h) = (img.width as usize + 2 * PAD, img.height as usize + 2 * PAD);
            if x + w > ATLAS_WIDTH {
                x = 0;
                y += row_h;
                row_h = 0;
            }
            placed.push((*name, x, y));
            x += w;
            row_h = row_h.max(h);
        }
        let height = (y + row_h).next_power_of_two();
        let mut bytes = vec![0u8; ATLAS_WIDTH * height * 4];
        let mut rects = HashMap::new();
        for ((name, img), &(_, x, y)) in images.iter().zip(&placed) {
            blit(&mut bytes, img, x, y);
            let mut r = Rect::new(
                (x + PAD) as f32,
                (y + PAD) as f32,
                img.width as f32,
                img.height as f32,
            );
            if *name == WHITE {
                // Sample the middle only, well clear of any filtering at the edges.
                r = Rect::new(r.x + 3.0, r.y + 3.0, 2.0, 2.0);
            }
            rects.insert(*name, r);
        }
        let atlas = Texture2D::from_rgba8(ATLAS_WIDTH as u16, height as u16, &bytes);
        atlas.set_filter(FilterMode::Linear);
        Art {
            atlas: Some(atlas),
            rects,
        }
    }

    /// Sprites loaded, not counting the white patch.
    pub fn count(&self) -> usize {
        self.rects.len().saturating_sub(1)
    }

    pub fn get(&self, name: &str) -> Option<Sprite<'_>> {
        let tex = self.atlas.as_ref()?;
        let src = *self.rects.get(name)?;
        Some(Sprite { tex, src })
    }

    /// Draws a solid rectangle from the atlas, so it batches with the sprites
    /// around it instead of switching textures.
    pub fn rect(&self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        match self.get(WHITE) {
            Some(white) => white.draw(x, y, w, h, color),
            None => draw_rectangle(x, y, w, h, color),
        }
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
