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
    "shop",
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
