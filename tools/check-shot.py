#!/usr/bin/env python3
"""Does a chest screenshot actually show its icons?

The GPU icon bake draws into the very atlas image the UI samples, so a bake
that has cleared its target and not yet drawn leaves every icon transparent.
The screen still lays out, the panel is still there, the stack counts are
still printed: only the pictures are gone. No layout assertion and no "the
file was written" check can see that, so this measures the pixels.

For each slot of the chest grid that the demo fills, it counts the pixels
inside the slot's *icon* area that differ from that area's own median colour.
An empty slot is flat and scores zero; a slot showing an icon scores in the
hundreds. The count text sits in the slot's bottom-right corner and is
deliberately outside the box, so a slot that prints "64" over a hole still
fails.

    python3 tools/check-shot.py examples/chest/shots/chest.png
    just shot-check                 # capture five times and check every one

Exit status is 1 if any slot in any shot came out empty.
"""

import sys

import numpy as np
from PIL import Image

SIZE = (1600, 900)

# Slot centres in the 1600x900 shot. These nine hold a stack in every one of
# the five captures `just shot-chest` takes, in both the chest grid and the
# player inventory below it, so one list covers all of them.
FILLED_SLOTS = (
    (536, 300),
    (586, 300),
    (636, 300),
    (736, 300),
    (786, 300),
    (836, 300),
    (536, 350),
    (636, 350),
    (636, 400),
)

# Slots that hold nothing. They are the control: unless most of them come out
# flat the measurement is reading the scene behind the translucent panel
# rather than an icon, and the floor below means nothing. Not all of them are
# empty in all five captures -- the paint capture is mid-drag and the hover
# capture has a tooltip over the grid -- which is why the rule is a majority
# rather than every one.
EMPTY_SLOTS = ((886, 300), (586, 350), (686, 400), (936, 300), (686, 300), (886, 400), (936, 400), (586, 400))
MIN_FLAT = 3

# The icon box inside a slot: up and to the left of centre, which leaves the
# stack count in the bottom-right corner outside it.
BOX = (-14, -14, 6, 6)

# A pixel counts as ink when its channels sum more than this far from the
# box's median colour.
INK_DISTANCE = 40.0
# A slot showing an icon must clear this. Across the five good captures the
# weakest icon scored 41 and the loudest empty slot scored 20 (neon draws a
# grid line inside the cell), so the floor sits between the two.
INK_FLOOR = 35
# A tooltip, a mid-drag paint or an open recipe page covers part of the grid,
# so two of the nine are allowed to be hidden. A bake that never drew scores
# zero on all nine and cannot pass this.
MIN_DRAWN = 7


def ink(pixels: np.ndarray, centre: tuple[int, int]) -> int:
    """Pixels in one slot's icon box that differ from the box's own ground."""
    cx, cy = centre
    box = pixels[cy + BOX[1] : cy + BOX[3], cx + BOX[0] : cx + BOX[2]]
    median = np.median(box.reshape(-1, 3), axis=0)
    return int((np.abs(box - median).sum(axis=2) > INK_DISTANCE).sum())


def check(path: str) -> bool:
    image = Image.open(path)
    if image.size != SIZE:
        print(f"FAIL {path}: unexpected size {image.size}", file=sys.stderr)
        return False
    pixels = np.asarray(image.convert("RGB"), dtype=np.float32)

    drawn = [s for s in FILLED_SLOTS if ink(pixels, s) >= INK_FLOOR]
    flat = [s for s in EMPTY_SLOTS if ink(pixels, s) < INK_FLOOR]

    if len(flat) < MIN_FLAT:
        print(
            f"FAIL {path}  only {len(flat)}/{len(EMPTY_SLOTS)} empty slots came out "
            "flat -- the measurement is unsound, so this says nothing about the bake",
            file=sys.stderr,
        )
        return False
    if len(drawn) < MIN_DRAWN:
        blank = [s for s in FILLED_SLOTS if s not in drawn]
        print(
            f"FAIL {path}  only {len(drawn)}/{len(FILLED_SLOTS)} slots drew an icon; "
            "blank at " + " ".join(f"{s}" for s in blank),
            file=sys.stderr,
        )
        return False
    print(
        f"ok   {path}  {len(drawn)}/{len(FILLED_SLOTS)} slots drew an icon, "
        f"weakest {min(ink(pixels, s) for s in drawn)} px"
    )
    return True


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print(__doc__, file=sys.stderr)
        raise SystemExit(2)
    raise SystemExit(0 if all([check(p) for p in sys.argv[1:]]) else 1)
