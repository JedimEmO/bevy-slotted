#!/usr/bin/env python3
"""Draws the five 16 px hint glyphs into assets/icons/.

The glyphs are white with an alpha mask; the ui tints them through
`ImageNode.color` from the theme's `slot.hint` role, so they suit every
theme without a second asset. Re-run after editing a shape:

    python3 tools/gen-hint-glyphs.py
"""

import math
import os
import struct
import zlib

SIZE = 16
OUT = os.path.join(os.path.dirname(__file__), "..", "assets", "icons")


def blank():
    return [[0.0] * SIZE for _ in range(SIZE)]


def png(path, mask):
    raw = b""
    for y in range(SIZE):
        raw += b"\x00"
        for x in range(SIZE):
            a = max(0, min(255, round(mask[y][x] * 255)))
            raw += bytes((255, 255, 255, a))

    def chunk(tag, data):
        c = struct.pack(">I", len(data)) + tag + data
        return c + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

    header = struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0)
    body = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )
    with open(path, "wb") as f:
        f.write(body)


def disc(mask, cx, cy, r, inner=0.0, value=1.0):
    for y in range(SIZE):
        for x in range(SIZE):
            d = math.hypot(x + 0.5 - cx, y + 0.5 - cy)
            if inner <= d <= r:
                edge = min(r - d, d - inner if inner else 1.0)
                mask[y][x] = max(mask[y][x], value * min(1.0, edge + 0.5))


def rect(mask, x0, y0, x1, y1, value=1.0):
    for y in range(int(y0), int(y1)):
        for x in range(int(x0), int(x1)):
            if 0 <= x < SIZE and 0 <= y < SIZE:
                mask[y][x] = max(mask[y][x], value)


def output():
    """An arrow leaving the slot."""
    m = blank()
    rect(m, 3, 7, 9, 9)
    for i in range(5):
        rect(m, 8 + i, 7 - (4 - i), 9 + i, 9 + (4 - i))
    return m


def locked():
    """A padlock."""
    m = blank()
    rect(m, 4, 8, 12, 14)
    disc(m, 8, 8, 4.0, 2.6)
    rect(m, 4, 9, 12, 16, 0.0)
    rect(m, 4, 8, 12, 14)
    for y in range(10, 13):
        for x in range(7, 9):
            m[y][x] = 0.0
    return m


def polygon(mask, points, value=1.0):
    for y in range(SIZE):
        for x in range(SIZE):
            px, py = x + 0.5, y + 0.5
            inside = False
            j = len(points) - 1
            for i, (xi, yi) in enumerate(points):
                xj, yj = points[j]
                if (yi > py) != (yj > py) and px < (xj - xi) * (py - yi) / (yj - yi) + xi:
                    inside = not inside
                j = i
            if inside:
                mask[y][x] = max(mask[y][x], value)


def tag():
    """A luggage tag pointing right, with its eyelet punched out."""
    m = blank()
    polygon(m, [(2, 3), (10, 3), (14, 8), (10, 13), (2, 13)])
    for y in range(SIZE):
        for x in range(SIZE):
            if math.hypot(x + 0.5 - 5.5, y + 0.5 - 8.0) <= 1.8:
                m[y][x] = 0.0
    return m


def any_item():
    """A dashed square: anything at all goes here."""
    m = blank()
    for i in range(0, 12, 4):
        rect(m, 2 + i, 2, 4 + i, 3)
        rect(m, 2 + i, 13, 4 + i, 14)
        rect(m, 2, 2 + i, 3, 4 + i)
        rect(m, 13, 2 + i, 14, 4 + i)
    return m


def info():
    """A filled dot over a bar: the corner mark."""
    m = blank()
    disc(m, 8, 8, 7.0, 5.4)
    rect(m, 7, 4, 9, 6)
    rect(m, 7, 7, 9, 12)
    return m


def main():
    os.makedirs(OUT, exist_ok=True)
    for name, fn in [
        ("hint_output", output),
        ("hint_locked", locked),
        ("hint_tag", tag),
        ("hint_any", any_item),
        ("hint_info", info),
    ]:
        png(os.path.join(OUT, name + ".png"), fn())
        print(name)


if __name__ == "__main__":
    main()
