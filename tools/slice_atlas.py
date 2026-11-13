"""Cut art/atlas.png into the named sprites the game embeds.

The atlas is hand-laid by an image model, so the boxes below are measured from
it (auto-detected by alpha, then named by eye). Regenerate the atlas and these
boxes need re-measuring; rerun with --boxes to print the detected boxes.

    uv run --no-project --with pillow python tools/slice_atlas.py [--boxes]
"""

import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
ATLAS = ROOT / "art" / "atlas.png"
OUT = ROOT / "assets" / "sprites"
TILE = 64

# name -> (left, top, right, bottom) in atlas pixels
SPRITES = {
    "pod_0": (28, 56, 172, 156),
    "pod_1": (192, 60, 336, 160),
    "shop": (1016, 24, 1516, 364),
    "sky": (860, 392, 1520, 640),
    "stash": (856, 864, 976, 976),
    "monster_small_0": (856, 720, 932, 816),
    "monster_small_1": (948, 712, 1028, 812),
    "monster_medium_0": (1056, 704, 1164, 816),
    "monster_medium_1": (1180, 704, 1272, 800),
    "monster_large_0": (1296, 712, 1408, 824),
    "monster_large_1": (1420, 712, 1516, 820),
    "icon_fuel": (1024, 868, 1108, 952),
    "icon_hull": (1120, 868, 1200, 952),
    "icon_cargo": (1212, 868, 1296, 952),
    "icon_money": (1304, 868, 1388, 952),
    "icon_depth": (1400, 868, 1480, 952),
}

# Square tiles, resized to TILE x TILE.
TILES = {
    "grass_0": (24, 212, 164, 368),
    "grass_1": (176, 216, 320, 368),
    "grass_2": (332, 220, 472, 368),
    "boulder": (488, 208, 652, 368),
    "dirt_1_0": (24, 388, 164, 532),
    "dirt_1_1": (176, 396, 320, 532),
    "dirt_1_2": (332, 392, 476, 532),
    "ore_0": (488, 392, 600, 532),
    "ore_1": (608, 392, 720, 528),
    "ore_2": (728, 392, 836, 528),
    "dirt_2_0": (24, 556, 164, 688),
    "dirt_2_1": (176, 556, 320, 684),
    "dirt_2_2": (332, 552, 472, 688),
    "ore_3": (488, 552, 600, 688),
    "ore_4": (608, 552, 720, 688),
    "ore_5": (728, 552, 836, 688),
    "dirt_3_0": (24, 704, 164, 840),
    "dirt_3_1": (176, 704, 320, 840),
    "dirt_3_2": (332, 704, 476, 840),
    "dirt_4_0": (488, 704, 600, 840),
    "dirt_4_1": (608, 704, 720, 840),
    "dirt_4_2": (728, 704, 836, 840),
}


def grassless(tile: Image.Image) -> Image.Image:
    # The atlas only has grass-topped topsoil; underground topsoil reuses its
    # lower half, mirrored upward over the grass.
    half = tile.crop((0, TILE // 2, TILE, TILE))
    out = tile.copy()
    out.paste(half.transpose(Image.Transpose.FLIP_TOP_BOTTOM), (0, 0))
    return out


def main() -> None:
    atlas = Image.open(ATLAS).convert("RGBA")
    OUT.mkdir(parents=True, exist_ok=True)
    for name, box in SPRITES.items():
        atlas.crop(box).save(OUT / f"{name}.png")
    for name, box in TILES.items():
        tile = atlas.crop(box).resize((TILE, TILE), Image.Resampling.LANCZOS)
        tile.save(OUT / f"{name}.png")
        if name.startswith("grass_"):
            grassless(tile).save(OUT / f"dirt_0_{name[-1]}.png")


if __name__ == "__main__":
    main()
