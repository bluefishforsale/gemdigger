"""Cut the 0.3.0 sprite sheets in art/ into the named sprites the game embeds.

Name sheets to cut only those; with none, every sheet is cut again.

Like slice_atlas.py, the sheets are laid out by an image model, so the boxes
below were auto-detected by alpha (connected components) and then named by eye
from a labelled contact sheet. Swap a sheet and its boxes need re-measuring:
rerun with --boxes to print the detected boxes and render the contact sheets
to target/sheets/.

    uv run --no-project --with pillow --with numpy --with scipy python tools/slice_sheets.py [--boxes] [sheet ...]
"""

import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont
from scipy import ndimage

ROOT = Path(__file__).resolve().parent.parent
ART = ROOT / "art"
OUT = ROOT / "assets" / "sprites"
CONTACT = ROOT / "target" / "sheets"

# Pixels at or above this alpha count as part of a sprite when finding boxes.
# NOTE: the PNGs' glow fringes bridge neighbours (the fence joins the mine
# entrance) unless only fully opaque pixels count.
SOLID = {
    "surface_props.png": 255,
    "monsters.png": 255,
    "effects.jpg": 40,
    "ruins_civ_dark.png": 255,
}
FONT = ImageFont.load_default(size=16)


def from_png(path: Path) -> np.ndarray:
    # NOTE: these PNGs carry a faint coloured haze (alpha 1-40) across the
    # whole sheet and a glow fringe around each sprite; stretching alpha drops
    # the haze, or every sprite drags a green smear into the game.
    rgba = np.array(Image.open(path).convert("RGBA")).astype(np.float32)
    a = np.clip((rgba[..., 3] - 40) / (200 - 40), 0, 1)
    rgba[..., 3] = a * 255
    return rgba.astype(np.uint8)


def from_white_jpeg(path: Path, holes: bool = True) -> np.ndarray:
    # Flood the near-white background in from the border, so white enclosed
    # by an outline (the ghost's body) stays opaque.
    rgb = np.array(Image.open(path).convert("RGB")).astype(np.float32)
    lo, hi = rgb.min(axis=2), rgb.max(axis=2)
    whiteish = (lo >= 222) & (hi - lo <= 24)
    labels, count = ndimage.label(whiteish)
    edge = np.unique(
        np.concatenate([labels[0], labels[-1], labels[:, 0], labels[:, -1]])
    )
    # Enclosed holes (a lantern's ring, gaps between ribs) are paper white;
    # a white body like the ghost's is shaded, so its median sits lower.
    median = ndimage.median(lo, labels, index=np.arange(1, count + 1))
    enclosed = np.flatnonzero(np.asarray(median) >= 250) + 1 if holes else np.array([], int)
    bg = np.isin(labels, np.union1d(edge[edge > 0], enclosed))
    # 1-2 px feather, then pull the white fringe out of the edge colours.
    a = np.minimum(ndimage.gaussian_filter((~bg).astype(np.float32), 0.8) * 1.6, 1)
    a[~bg] = np.maximum(a[~bg], ndimage.binary_erosion(~bg)[~bg])
    return unblend_white(rgb, a)


def from_dark_on_white(path: Path) -> np.ndarray:
    # Glows and smoke have no hard edge for a flood fill to find; read
    # coverage straight off how far each pixel is from white.
    rgb = np.array(Image.open(path).convert("RGB")).astype(np.float32)
    a = 1 - rgb.min(axis=2) / 255
    a = np.clip((a - 0.04) / 0.96, 0, 1)  # JPEG noise in the white
    return unblend_white(rgb, a)


# Rows of the three mountain ranges on parallax.jpg; the clouds sit below.
RANGES = [(22, 181), (190, 356), (369, 528)]


def from_parallax(path: Path) -> np.ndarray:
    # NOTE: snow, mist and cloud are as white as the paper here, so a flood
    # fill from the border eats into the peaks. A range is instead everything
    # under its skyline, and a cloud everything inside its outline.
    rgb = np.array(Image.open(path).convert("RGB")).astype(np.float32)
    paper = rgb.min(axis=2) >= 249
    fg = ndimage.binary_closing(~paper, iterations=3)
    fg = ndimage.binary_fill_holes(fg)
    for top, bottom in RANGES:
        fg[top:bottom] = np.maximum.accumulate(fg[top:bottom], axis=0)
    # The outline's outer pixels are half paper; shave them or every range
    # and cloud wears a white rim.
    fg = ndimage.binary_erosion(fg, iterations=3)
    a = np.minimum(ndimage.gaussian_filter(fg.astype(np.float32), 0.8) * 1.6, 1)
    a[fg] = np.maximum(a[fg], ndimage.binary_erosion(fg)[fg])
    return unblend_white(rgb, a)


def from_soft_white(path: Path) -> np.ndarray:
    # NOTE: thin roots and grass blades in soft watercolour. A flood fill from
    # the border leaves the paper trapped between roots as white patches and a
    # pale rim on every root, so coverage is read off the distance from white.
    rgb = np.array(Image.open(path).convert("RGB")).astype(np.float32)
    a = np.clip((247 - rgb.min(axis=2)) / 70, 0, 1)
    return unblend_white(rgb, a)


def opaque(path: Path) -> np.ndarray:
    return np.array(Image.open(path).convert("RGBA"))


def unblend_white(rgb: np.ndarray, a: np.ndarray) -> np.ndarray:
    safe = np.maximum(a, 1e-3)[..., None]
    col = np.clip((rgb - 255 * (1 - a[..., None])) / safe, 0, 255)
    return np.dstack([col, a * 255]).round().astype(np.uint8)


LOADERS = {
    "surface_props.png": from_png,
    "monsters.png": from_png,
    "gameplay_objects.jpg": from_white_jpeg,
    "creatures.jpg": from_white_jpeg,
    "underground.jpg": from_white_jpeg,
    "effects.jpg": from_dark_on_white,
    "pod.jpg": from_white_jpeg,
    "ruins_norse.jpg": from_white_jpeg,
    "ruins_mayan.jpg": from_white_jpeg,
    "ruins_civ_dark.png": from_png,
    "parallax.jpg": from_parallax,
    # The white windows cut through these cave layers are the holes the layer
    # behind shows through.
    "cave_parallax.jpg": from_white_jpeg,
}

# NOTE: light-grey smoke and dust come out as faint black puffs under the
# darkness alpha (grey un-blended against white reads as black), invisible in
# a dark cave. Their outlines are hard, so the flood-fill loader keeps them
# white instead.
FLOOD_ON_EFFECTS = ("fx_smoke_", "fx_dust_")

# Components closer than this many pixels are found as one box, so loose
# bits (particles, a lantern's ring) stay with their sprite.
MERGE = {"effects.jpg": 6}
# Thin necks this many pixels wide are cut, so sprites the PNGs' opaque glow
# joins (the fence and the mine entrance) come apart.
SPLIT = {"surface_props.png": 4, "monsters.png": 2}


def detect(rgba: np.ndarray, sheet: str) -> tuple[np.ndarray, list[tuple]]:
    """Labels the solid pixels and returns (labels, boxes), boxes in reading order."""
    solid = rgba[..., 3] >= SOLID.get(sheet, 64)
    merge = MERGE.get(sheet, 0)
    grown = ndimage.binary_dilation(solid, iterations=merge) if merge else solid
    cut = SPLIT.get(sheet, 0)
    core = ndimage.binary_erosion(grown, iterations=cut) if cut else grown
    labels, _ = ndimage.label(core)
    # Hand every solid pixel the label of its nearest core pixel.
    _, (iy, ix) = ndimage.distance_transform_edt(labels == 0, return_indices=True)
    labels = labels[iy, ix]
    labels[~solid] = 0
    boxes = []
    for i, sl in enumerate(ndimage.find_objects(labels), start=1):
        if sl is None:
            continue
        box = (sl[1].start, sl[0].start, sl[1].stop, sl[0].stop)
        if (box[2] - box[0]) * (box[3] - box[1]) >= 120:
            boxes.append((i, box))
    boxes.sort(key=lambda b: (b[1][1] // 40, b[1][0]))
    return labels, boxes


def contact_sheet(sheet: str, rgba: np.ndarray, boxes: list[tuple]) -> None:
    h, w = rgba.shape[:2]
    check = (np.indices((h, w)).sum(axis=0) // 16 % 2)[..., None]
    bg = np.where(check, [255, 0, 255], [40, 40, 40]).astype(np.float32)
    if sheet == "effects.jpg":
        bg[:] = 40
    a = rgba[..., 3:4] / 255
    img = Image.fromarray((rgba[..., :3] * a + bg * (1 - a)).astype(np.uint8))
    draw = ImageDraw.Draw(img)
    for n, (_, box) in enumerate(boxes):
        draw.rectangle(box, outline=(255, 255, 0))
        draw.text(
            (box[0] + 2, box[1] + 1),
            str(n),
            fill=(255, 255, 0),
            font=FONT,
            stroke_width=2,
            stroke_fill=(0, 0, 0),
        )
    CONTACT.mkdir(parents=True, exist_ok=True)
    img.save(CONTACT / f"{Path(sheet).stem}.png")


def print_boxes(only: list[str]) -> None:
    for sheet, load in LOADERS.items():
        if only and sheet not in only:
            continue
        rgba = load(ART / sheet)
        _, boxes = detect(rgba, sheet)
        contact_sheet(sheet, rgba, boxes)
        print(f"# {sheet}")
        for n, (_, box) in enumerate(boxes):
            print(f"{n:3d} {box}")


# Each sprite is every component whose pixels mostly fall inside its region,
# so a region can be loose; (left, top, right, bottom) in sheet pixels.
SHEETS = {
    "surface_props.png": {
        "tree_0": (690, 20, 915, 258),
        "tree_1": (915, 28, 1090, 252),
        "tree_2": (1092, 28, 1240, 252),
        "tree_3": (1236, 25, 1386, 252),
        "tree_4": (1388, 110, 1512, 237),
        "tree_5": (688, 270, 795, 415),
        "tree_6": (826, 238, 980, 416),
        "tree_7": (1000, 248, 1135, 423),
        "tree_8": (1150, 256, 1303, 423),
        "tree_9": (1310, 294, 1402, 423),
        "tree_10": (1390, 236, 1515, 424),
        "bush_0": (776, 356, 848, 416),
        "bush_1": (955, 380, 1015, 420),
        "bush_2": (1105, 384, 1174, 423),
        "bush_3": (12, 870, 140, 1000),
        "bush_4": (142, 886, 296, 998),
        "bush_5": (316, 905, 386, 995),
        "bush_6": (476, 872, 578, 956),
        "bush_7": (600, 902, 678, 964),
        "bush_8": (938, 830, 1058, 894),
        "flower_0": (895, 432, 945, 478),
        "flower_1": (970, 435, 1022, 478),
        "flower_2": (1040, 436, 1093, 478),
        "flower_3": (1115, 440, 1160, 478),
        "flower_4": (1178, 433, 1213, 478),
        "flower_5": (380, 885, 470, 998),
        "flower_6": (700, 875, 785, 952),
        "flower_7": (808, 875, 875, 950),
        "cottage": (12, 620, 315, 862),
        "windmill": (320, 630, 452, 864),
        "mine_entrance": (454, 655, 718, 860),
        "fence": (697, 732, 846, 796),
        "lamp_post": (855, 704, 942, 856),
        "cart": (957, 743, 1059, 821),
        "crate": (1198, 795, 1266, 865),
        "tent": (1158, 893, 1336, 999),
        "campfire": (1352, 912, 1422, 994),
    },
    "gameplay_objects.jpg": {
        "jerrycan": (10, 30, 172, 245),
        "workbench": (340, 17, 574, 283),
        "chest_closed": (887, 40, 1059, 189),
        "chest_open": (1058, 19, 1247, 205),
        "lantern_0": (1091, 209, 1161, 353),
        "lantern_1": (1174, 210, 1250, 353),
        "lantern_2": (962, 375, 1067, 558),
        "energy_cell_0": (604, 374, 704, 557),
        "energy_cell_1": (712, 379, 828, 565),
        "energy_cell_2": (846, 384, 932, 559),
        "relic_coin": (1104, 382, 1235, 524),
        "relic_rune": (244, 524, 387, 717),
        "pickaxe_0": (6, 265, 120, 386),
        "pickaxe_1": (118, 268, 206, 386),
        "pickaxe_2": (166, 265, 272, 388),
        "pickaxe_3": (7, 392, 118, 515),
        "drill_icon_0": (314, 371, 454, 515),
        "drill_icon_1": (456, 366, 599, 515),
    },
    "underground.jpg": {
        "waterfall_cave": (15, 24, 288, 268),
        "glow_mushroom": (286, 44, 462, 246),
        "glow_plant": (475, 34, 618, 240),
        "roots": (630, 18, 787, 273),
        "stalactite_0": (812, 18, 1020, 178),
        "stalactite_1": (1036, 18, 1255, 167),
        "stalagmite_0": (825, 150, 1023, 321),
        "crystal_pool": (1028, 169, 1261, 343),
        "bones_0": (20, 285, 287, 472),
        "bones_1": (312, 281, 495, 458),
        "crystal_0": (514, 311, 624, 447),
        "crystal_1": (644, 296, 777, 461),
        "crystal_2": (805, 318, 908, 462),
        "crystal_3": (931, 328, 1042, 461),
        "crystal_4": (1104, 368, 1217, 515),
        "pillar_0": (16, 507, 113, 824),
        "pillar_1": (131, 610, 229, 824),
        "pillar_2": (244, 579, 342, 708),
        "pillar_3": (238, 737, 364, 825),
        "mine_support": (559, 494, 803, 708),
        "ruins_0": (975, 473, 1074, 642),
        "ruins_1": (1043, 539, 1257, 749),
        "ruins_2": (955, 706, 1145, 836),
    },
    "effects.jpg": {
        "fx_leaf_0": (465, 40, 510, 103),
        "fx_leaf_1": (570, 44, 629, 98),
        "fx_leaf_2": (641, 73, 697, 139),
        "fx_leaf_3": (724, 43, 786, 95),
        "fx_leaf_4": (517, 80, 562, 141),
        "fx_leaf_5": (707, 110, 768, 151),
        "fx_leaf_6": (452, 129, 513, 178),
        "fx_leaf_7": (572, 117, 628, 182),
        "fx_leaf_8": (769, 123, 834, 178),
        "fx_wisp_0": (27, 239, 98, 341),
        "fx_wisp_1": (98, 272, 157, 343),
        "fx_wisp_2": (175, 215, 253, 300),
        "fx_wisp_3": (198, 270, 306, 344),
        "fx_wisp_4": (305, 242, 416, 359),
        "fx_splash_0": (470, 235, 565, 365),
        "fx_splash_1": (575, 235, 695, 366),
        "fx_splash_2": (700, 255, 815, 368),
        "fx_smoke_0": (26, 420, 126, 584),
        "fx_smoke_1": (138, 469, 208, 586),
        "fx_smoke_2": (232, 414, 359, 581),
        "fx_smoke_3": (360, 438, 460, 584),
        "fx_sparkle_0": (974, 431, 1015, 488),
        "fx_sparkle_1": (1051, 451, 1072, 475),
        "fx_sparkle_2": (1116, 438, 1155, 485),
        "fx_sparkle_3": (1192, 453, 1218, 483),
        "fx_sparkle_4": (1019, 479, 1049, 520),
        "fx_sparkle_5": (1159, 487, 1209, 551),
        "fx_sparkle_6": (1059, 516, 1089, 552),
        "fx_sparkle_7": (983, 526, 1008, 554),
        "fx_sparkle_8": (1128, 534, 1153, 563),
        "fx_debris_dirt": (545, 648, 670, 805),
        "fx_debris_stone": (830, 640, 945, 810),
        "fx_explosion_0": (983, 614, 1110, 713),
        "fx_explosion_1": (1126, 616, 1236, 714),
        "fx_explosion_2": (979, 733, 1113, 823),
        "fx_explosion_3": (1124, 726, 1244, 828),
    },
}

MONSTER_COLS = {
    "left": [(0, 203), (203, 386), (386, 566), (566, 768)],
    "right": [(768, 978), (978, 1166), (1166, 1345), (1345, 1536)],
}
# name -> (side, row bands); frames run left to right along each band.
MONSTER_ROWS = {
    "ancient_worm": ("left", [(42, 186), (186, 345)]),
    "crystal_golem": ("left", [(382, 508), (508, 660)]),
    "moss_titan": ("left", [(699, 845), (845, 1024)]),
    "shadow_leviathan": ("right", [(42, 190), (190, 345)]),
    "deep_one": ("right", [(388, 522), (522, 660)]),
    "void_spider": ("right", [(699, 850), (850, 1024)]),
}
SHEETS["monsters.png"] = {
    f"monster_{name}_{row * 4 + col}": (x0, y0, x1, y1)
    for name, (side, rows) in MONSTER_ROWS.items()
    for row, (y0, y1) in enumerate(rows)
    for col, (x0, x1) in enumerate(MONSTER_COLS[side])
}

CRITTERS = [
    "leaf",
    "mushroom",
    "firefly",
    "ghost",
    "beetle",
    "fish",
    "spider",
    "golem",
    "mole",
]
SHEETS["creatures.jpg"] = dict(
    zip(
        (f"critter_{c}_{f}" for c in CRITTERS for f in (0, 1)),
        [
            (23, 19, 200, 233),
            (218, 34, 373, 233),
            (448, 29, 615, 233),
            (643, 51, 811, 232),
            (879, 35, 1039, 222),
            (1067, 62, 1233, 230),
            (42, 279, 180, 501),
            (203, 320, 371, 484),
            (454, 288, 617, 501),
            (631, 356, 833, 496),
            (887, 318, 1045, 482),
            (1062, 360, 1239, 477),
            (22, 598, 195, 735),
            (206, 605, 385, 734),
            (420, 563, 631, 780),
            (643, 556, 852, 782),
            (883, 584, 1055, 762),
            (1069, 623, 1240, 767),
        ],
    )
)

SHEETS["parallax.jpg"] = {
    "cloud_0": (8, 533, 400, 689),
    "cloud_1": (407, 537, 777, 699),
    "cloud_2": (744, 546, 939, 617),
    "cloud_3": (949, 543, 1249, 657),
    "cloud_4": (840, 658, 1024, 736),
    "cloud_5": (9, 694, 425, 844),
    "cloud_6": (960, 713, 1219, 832),
    "cloud_7": (414, 725, 591, 784),
    "cloud_8": (604, 735, 912, 827),
}
# The ranges fill their rows edge to edge, so they are cropped, not detected.
STRIPS = {
    "parallax.jpg": {
        "mountains_far": (2, 22, 1262, 181),
        "mountains_mid": (2, 190, 1262, 356),
        # Stops short of the bottom rows, which fade back to paper and show as
        # a pale line along the grass.
        "mountains_near": (2, 369, 1262, 520),
    },
    # One painted cave wall per depth band, shown behind dug-out tiles.
    "cave_backgrounds.jpg": {
        "cave_bg_0": (34, 30, 1166, 183),
        "cave_bg_1": (34, 205, 1166, 357),
        "cave_bg_3": (34, 553, 1166, 704),
        "cave_bg_4": (34, 726, 1166, 875),
    },
    # The flooded-ruins band's wall with its ruins lifted off onto a front
    # layer, and front layers of rock, trunks and crystal for three more bands.
    "cave_parallax.jpg": {
        "cave_bg_2": (34, 30, 1166, 183),
        "cave_fg_2": (34, 210, 1166, 363),
        "cave_fg_1": (34, 384, 1166, 534),
        "cave_fg_3": (34, 553, 1166, 704),
        "cave_fg_4": (34, 726, 1166, 875),
    },
}

# What sits right under the grass, one painting per file and several variants
# of each, which the game shuffles along the surface: lit soil darkening into
# the root wall, loose roots to hang in front of it, and the grass and sod.
#
# Every variant of a set comes out the same size, whatever was cut from it, so
# the game can lay them at one pitch.
# set -> (loader, output size, sides that fade, [(file, crop box), ...])
TOPSOIL = {
    # Cut inside the blurred, featureless ends each wall was painted with.
    "topsoil_bg": (
        opaque,
        (1000, 300),
        "left",
        [
            ("topsoil_wall_1.jpg", (196, 0, 1862, 512)),
            ("topsoil_wall_2.jpg", (196, 0, 1866, 512)),
            ("topsoil_wall_3.jpg", (328, 0, 1702, 512)),
            ("topsoil_wall_4.jpg", (193, 0, 1861, 512)),
        ],
    ),
    # NOTE: three of these stop on a ruled line at each side, roots sliced
    # mid-air, so both ends fade.
    "topsoil_fg": (
        from_soft_white,
        (1000, 230),
        "both",
        [
            ("topsoil_roots_1.jpg", (0, 0, 2064, 445)),
            ("topsoil_roots_2.jpg", (156, 0, 1912, 445)),
            ("topsoil_roots_3.jpg", (114, 0, 1949, 445)),
            ("topsoil_roots_4.jpg", (82, 0, 1983, 445)),
        ],
    ),
    # NOTE: the grass sits at a different height in each painting. Every box
    # is 155 rows above its ground line and 210 below (running off the image
    # where it has to), so the ground is at the same place in all of them:
    # the game's GROUND is 155 / 365.
    "turf_lip": (
        from_soft_white,
        (1400, 175),
        "left",
        [
            ("turf_lip_1.jpg", (0, 128 - 155, 2928, 128 + 210)),
            ("turf_lip_2.jpg", (0, 216 - 155, 2928, 216 + 210)),
            ("turf_lip_3.jpg", (0, 226 - 155, 2928, 226 + 210)),
        ],
    ),
}
# Share of a variant's width over which a fading side goes from clear to
# solid. NOTE: the game overlaps neighbours by exactly this much (SEAM in
# render.rs), so two different paintings cross-fade instead of meeting on a
# line. Change one without the other and the surface shows gaps or doubles.
SEAM = 0.06
# Share of the soil wall's height, from the bottom, over which it fades out.
# NOTE: the soil ends dark but not as dark as the root wall under it; cut
# square, the join shows as a line across every pit.
SOIL_FADE_OUT = 0.35


def topsoil_variant(rgba: np.ndarray, box: tuple, size: tuple, sides: str) -> Image.Image:
    l, t, r, b = box
    canvas = Image.new("RGBA", (r - l, b - t))
    canvas.paste(Image.fromarray(rgba), (-l, -t))
    # NOTE: resize premultiplied, or the transparent black around roots and
    # blades bleeds into them as a dark fringe.
    out = np.array(canvas.convert("RGBa").resize(size, Image.Resampling.LANCZOS).convert("RGBA"))
    alpha = out[..., 3].astype(np.float32)
    ramp = np.linspace(0, 1, round(size[0] * SEAM))
    alpha[:, : len(ramp)] *= ramp
    if sides == "both":
        alpha[:, -len(ramp) :] *= ramp[::-1]
    out[..., 3] = alpha.round()
    return Image.fromarray(out)


def cut_topsoil(only: list[str]) -> None:
    for name, (load, size, sides, variants) in TOPSOIL.items():
        for n, (sheet, box) in enumerate(variants):
            if only and sheet not in only:
                continue
            img = topsoil_variant(load(ART / sheet), box, size, sides)
            if name == "topsoil_bg":
                px = np.array(img)
                rows = round(px.shape[0] * SOIL_FADE_OUT)
                fade = np.linspace(1, 0, rows)[:, None]
                px[-rows:, :, 3] = (px[-rows:, :, 3] * fade).round()
                img = Image.fromarray(px)
            img.save(OUT / f"{name}_{n}.png")

SHEETS["pod.jpg"] = {
    "pod_0": (15, 3, 312, 212),
    "pod_1": (330, 3, 628, 212),
    "pod_drill_0": (652, 3, 945, 212),
    "pod_drill_1": (970, 3, 1262, 212),
    "pod_thrust_0": (0, 214, 318, 420),
    "pod_thrust_1": (318, 220, 634, 436),
    "pod_thrust_2": (628, 220, 952, 432),
    "fx_dust_0": (30, 690, 180, 818),
    "fx_dust_1": (228, 685, 440, 820),
    "fx_dust_2": (485, 705, 580, 805),
    "fx_dust_3": (655, 690, 782, 818),
    "fx_dust_4": (838, 665, 1018, 835),
    "fx_dust_5": (1055, 642, 1245, 840),
}
# Every pod frame goes on one canvas with the hull in its top-right corner,
# so flames out the back and underneath never shift the hull between frames.
# name -> (x of the drill tip, y of the antenna top) on the sheet.
POD_CANVAS = (326, 220)
POD_HULL = (286, 200)
POD_ANCHOR = {
    "pod_0": (308, 8),
    "pod_1": (622, 8),
    "pod_drill_0": (940, 8),
    "pod_drill_1": (1257, 8),
    "pod_thrust_0": (316, 219),
    "pod_thrust_1": (633, 224),
    "pod_thrust_2": (950, 224),
}

RUINS = {
    "ruins_norse.jpg": [
        (12, 215, 234, 519),
        (245, 358, 565, 519),
        (574, 227, 905, 519),
        (920, 223, 1177, 513),
        (9, 548, 316, 880),
        (325, 577, 633, 876),
        (637, 548, 904, 880),
        (915, 547, 1187, 877),
    ],
    "ruins_mayan.jpg": [
        (29, 194, 281, 438),
        (313, 241, 615, 427),
        (649, 182, 906, 426),
        (935, 231, 1177, 431),
        (13, 461, 511, 871),
        (395, 442, 889, 633),
        (520, 646, 872, 876),
        (889, 530, 1187, 857),
    ],
    # The two or three largest pieces of each civilization.
    "ruins_civ_dark.png": [
        (492, 15, 752, 219),
        (341, 43, 475, 141),
        (1119, 50, 1289, 223),
        (1297, 15, 1518, 221),
        (331, 284, 507, 450),
        (509, 247, 761, 453),
        (1111, 261, 1296, 455),
        (1302, 236, 1523, 453),
        (335, 502, 519, 664),
        (530, 466, 758, 665),
        (1120, 503, 1295, 671),
        (1302, 469, 1523, 668),
        (333, 696, 503, 835),
        (515, 679, 760, 839),
        (1115, 701, 1292, 843),
        (1307, 680, 1521, 841),
        (341, 866, 503, 996),
        (507, 843, 747, 1003),
        (971, 846, 1117, 1004),
        (1126, 857, 1295, 1002),
        (1303, 846, 1524, 1004),
    ],
}
_ruin = 0
for _sheet, _boxes in RUINS.items():
    SHEETS[_sheet] = {}
    for _box in _boxes:
        SHEETS[_sheet][f"ruin_{_ruin}"] = _box
        _ruin += 1
# Small carvings that mark a ruin's treasure.
SHEETS["ruins_mayan.jpg"] |= {
    "relic_idol_0": (28, 35, 135, 180),
    "relic_idol_1": (623, 38, 725, 153),
    "relic_idol_2": (1080, 48, 1173, 142),
}
SHEETS["ruins_norse.jpg"] |= {"relic_idol_3": (181, 64, 316, 179)}

# Largest side in pixels, so everything fits one atlas no taller than 4096.
BIG = {"mountains_far", "mountains_near", "cottage", "windmill", "mine_entrance"}
MEDIUM = (
    "tree_",
    "cloud_",
    "workbench",
    "waterfall_cave",
    "crystal_pool",
    "mine_support",
    "stalactite_",
    "stalagmite_",
    "bones_",
    "ruins_",
    "roots",
    "pillar_",
)


def max_side(name: str) -> int:
    # The sky draws the mountain ranges ~200 px tall across the whole screen;
    # shrinking them made the sky a 5x-stretched blur. Keep them at full size.
    # NOTE: 2040, not 2048: the atlas is 2048 wide and pads each sprite, and
    # one that doesn't fit is silently left out of the game.
    if name.startswith(("mountains_", "cave_bg_", "cave_fg_")):
        return 2040
    if name.startswith(("ruin_", "pod_")):
        return 256
    if name in BIG:
        return 256
    if name.startswith("monster_"):
        return 160
    if name.startswith(MEDIUM):
        return 128
    return 96


def cut(rgba: np.ndarray, labels: np.ndarray, box: tuple) -> Image.Image:
    return cut_at(rgba, labels, box)[0]


def cut_at(rgba: np.ndarray, labels: np.ndarray, box: tuple) -> tuple[Image.Image, tuple]:
    """The sprite in `box`, and where its top-left corner was on the sheet."""
    l, t, r, b = box
    inside = np.bincount(labels[t:b, l:r].ravel(), minlength=labels.max() + 1)
    total = np.bincount(labels.ravel(), minlength=labels.max() + 1)
    keep = np.flatnonzero(inside * 2 > total)
    keep = keep[keep > 0]
    # Grow the kept components a little to take in their soft fringe.
    mask = ndimage.binary_dilation(np.isin(labels, keep), iterations=3)
    alpha = np.where(mask, rgba[..., 3], 0)
    ys, xs = np.nonzero(alpha)
    out = rgba[ys.min() : ys.max() + 1, xs.min() : xs.max() + 1].copy()
    out[..., 3] = alpha[ys.min() : ys.max() + 1, xs.min() : xs.max() + 1]
    out[out[..., 3] == 0] = 0
    return Image.fromarray(out), (xs.min(), ys.min())


def pod_frame(rgba: np.ndarray, labels: np.ndarray, name: str, box: tuple) -> Image.Image:
    img, (left, top) = cut_at(rgba, labels, box)
    tip, antenna = POD_ANCHOR[name]
    canvas = Image.new("RGBA", POD_CANVAS)
    canvas.paste(img, (int(left) - (tip - POD_CANVAS[0]), int(top) - antenna))
    return canvas


def shrink(img: Image.Image, side: int) -> Image.Image:
    scale = side / max(img.size)
    if scale >= 1:
        return img
    size = (max(1, round(img.width * scale)), max(1, round(img.height * scale)))
    return img.resize(size, Image.Resampling.LANCZOS)


# Columns trimmed off each end of a range before it is enlarged.
EDGE = 4


def enlarge(img: Image.Image) -> Image.Image:
    """Doubles a painting and sharpens it, for art drawn larger than it was painted.

    The mountain ranges are ~150 px tall on the sheet and ~400 px tall on a
    Retina screen. Left to the GPU that is a 2.7x bilinear stretch, which reads
    as blur; Lanczos plus an unsharp mask keeps the edges of the brushwork.
    """
    # NOTE: the sky tiles these edge to edge, and a cut edge's outermost
    # columns are half transparent: left on, they show as a line of sky
    # between two copies.
    img = img.crop((EDGE, 0, img.width - EDGE, img.height))
    size = (img.width * 2, img.height * 2)
    # NOTE: resize premultiplied, or the transparent black around the range
    # bleeds into its outline as a dark fringe.
    big = img.convert("RGBa").resize(size, Image.Resampling.LANCZOS).convert("RGBA")
    # NOTE: sharpen over colour bled out past the outline. Against transparent
    # black the mask overshoots and draws a bright rim around the whole range.
    rgba = np.array(big)
    _, (iy, ix) = ndimage.distance_transform_edt(rgba[..., 3] == 0, return_indices=True)
    rgb = Image.fromarray(rgba[iy, ix, :3]).filter(
        ImageFilter.UnsharpMask(radius=1.6, percent=140, threshold=2)
    )
    return Image.merge("RGBA", (*rgb.split(), big.getchannel("A")))


def on_shared_canvas(frames: list[Image.Image]) -> list[Image.Image]:
    """Pads animation frames to one size, bottom-centre aligned, so they don't jitter."""
    w = max(f.width for f in frames)
    h = max(f.height for f in frames)
    out = []
    for f in frames:
        canvas = Image.new("RGBA", (w, h))
        canvas.paste(f, ((w - f.width) // 2, h - f.height))
        out.append(canvas)
    return out


def animation(name: str) -> str | None:
    if name.startswith(("monster_", "critter_")):
        return name.rsplit("_", 1)[0]
    return None


def main(only: list[str]) -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    cuts: dict[str, Image.Image] = {}
    for sheet, regions in SHEETS.items():
        if only and sheet not in only:
            continue
        rgba = LOADERS[sheet](ART / sheet)
        labels, _ = detect(rgba, sheet)
        flooded = None
        for name, box in regions.items():
            if sheet == "effects.jpg" and name.startswith(FLOOD_ON_EFFECTS):
                if flooded is None:
                    # A puff's bright core is part of it, not a hole through it.
                    f = from_white_jpeg(ART / sheet, holes=False)
                    flooded = (f, detect(f, sheet)[0])
                cuts[name] = cut(flooded[0], flooded[1], box)
            elif name in POD_ANCHOR:
                cuts[name] = pod_frame(rgba, labels, name, box)
            else:
                cuts[name] = cut(rgba, labels, box)
    for sheet, strips in STRIPS.items():
        if only and sheet not in only:
            continue
        load = LOADERS.get(sheet, lambda path: np.array(Image.open(path).convert("RGBA")))
        rgba = load(ART / sheet)
        for name, (l, t, r, b) in strips.items():
            cuts[name] = Image.fromarray(rgba[t:b, l:r])
    groups: dict[str, list[str]] = {}
    for name in cuts:
        groups.setdefault(animation(name) or name, []).append(name)
    for names in groups.values():
        frames = (
            on_shared_canvas([cuts[n] for n in names])
            if len(names) > 1
            else [cuts[names[0]]]
        )
        for name, frame in zip(names, frames):
            if name.startswith("mountains_"):
                frame = enlarge(frame)
            shrink(frame, max_side(name)).save(OUT / f"{name}.png")
    cut_topsoil(only)


if __name__ == "__main__":
    if "--boxes" in sys.argv:
        print_boxes(sys.argv[sys.argv.index("--boxes") + 1 :])
    else:
        main(sys.argv[1:])
