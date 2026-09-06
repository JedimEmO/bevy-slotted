#!/usr/bin/env python3
"""Writes assets/models/pickaxe.gltf, the workspace's one demo glTF.

The repository ships no third-party 3D assets, so the model `IconDef::Model`
is demonstrated with is generated here: a stylised pickaxe built from three
boxes, flat shaded, with two materials. Everything is authored in this file,
so the model carries the repository's own licence and nothing has to be
attributed.

The output is a single self-contained `.gltf`: one buffer, embedded as a
`data:` URI, so the asset is one file and `AssetServer::load` needs no
sidecar. It is a few kilobytes, which is the right size for a demo and small
enough to read in a diff.

Run from the workspace root:

    python3 tools/gen-demo-model.py

The file it writes is committed. Regenerate it only when the geometry here
changes; the output is deterministic, so an unchanged run is an empty diff.
"""

from __future__ import annotations

import base64
import json
import math
import pathlib
import struct

# ---------------------------------------------------------------------------
# Geometry
# ---------------------------------------------------------------------------

Vec3 = tuple[float, float, float]


def box(half: Vec3, centre: Vec3) -> list[tuple[Vec3, Vec3]]:
    """A flat-shaded axis-aligned box as 12 triangles of (position, normal).

    Flat shaded means every face gets its own four vertices, so no normal is
    averaged across an edge. That is what keeps the silhouette crisp at 64
    pixels, which is the only size these icons are ever seen at.
    """
    hx, hy, hz = half
    cx, cy, cz = centre
    # Each face: its normal, then its four corners counter-clockwise seen
    # from outside.
    faces: list[tuple[Vec3, list[Vec3]]] = [
        ((0, 0, 1), [(-hx, -hy, hz), (hx, -hy, hz), (hx, hy, hz), (-hx, hy, hz)]),
        ((0, 0, -1), [(hx, -hy, -hz), (-hx, -hy, -hz), (-hx, hy, -hz), (hx, hy, -hz)]),
        ((1, 0, 0), [(hx, -hy, hz), (hx, -hy, -hz), (hx, hy, -hz), (hx, hy, hz)]),
        ((-1, 0, 0), [(-hx, -hy, -hz), (-hx, -hy, hz), (-hx, hy, hz), (-hx, hy, -hz)]),
        ((0, 1, 0), [(-hx, hy, hz), (hx, hy, hz), (hx, hy, -hz), (-hx, hy, -hz)]),
        ((0, -1, 0), [(-hx, -hy, -hz), (hx, -hy, -hz), (hx, -hy, hz), (-hx, -hy, hz)]),
    ]
    out: list[tuple[Vec3, Vec3]] = []
    for normal, corners in faces:
        for corner in corners:
            out.append(((corner[0] + cx, corner[1] + cy, corner[2] + cz), normal))
    return out


def rotate_z(verts: list[tuple[Vec3, Vec3]], radians: float) -> list[tuple[Vec3, Vec3]]:
    """Rotates positions and normals about Z, so a box can lie on a diagonal."""
    cos, sin = math.cos(radians), math.sin(radians)

    def turn(v: Vec3) -> Vec3:
        return (v[0] * cos - v[1] * sin, v[0] * sin + v[1] * cos, v[2])

    return [(turn(p), turn(n)) for p, n in verts]


def indices_for(quad_count: int) -> list[int]:
    """Two triangles per quad, in the order `box` emits its corners."""
    out: list[int] = []
    for q in range(quad_count):
        base = q * 4
        out += [base, base + 1, base + 2, base, base + 2, base + 3]
    return out


# The pickaxe, authored lying on its working diagonal: the head is up and to
# the right, the haft runs down to the left. Drawn that way in model space
# rather than rotated by the rig, because a rod-shaped icon is read along its
# length and the bake's three-quarter view is applied on top of this.
SHAFT = rotate_z(box((0.045, 0.42, 0.045), (0.0, -0.04, 0.0)), math.radians(-38))
# The head: a wide wedge across the top of the haft, plus the spike behind it.
HEAD = rotate_z(box((0.30, 0.075, 0.075), (0.0, 0.40, 0.0)), math.radians(-38))
COLLAR = rotate_z(box((0.075, 0.055, 0.075), (0.0, 0.31, 0.0)), math.radians(-38))

# One mesh primitive per material: the two wooden parts are one primitive, the
# steel head is another. Fewer primitives is fewer draw calls, and it also
# exercises the multi-primitive path in the bake.
PRIMITIVES: list[tuple[str, list[tuple[Vec3, Vec3]], int]] = [
    ("haft", SHAFT + COLLAR, 0),
    ("head", HEAD, 1),
]

MATERIALS = [
    {
        "name": "haft",
        "pbrMetallicRoughness": {
            "baseColorFactor": [0.42, 0.30, 0.20, 1.0],
            "metallicFactor": 0.0,
            "roughnessFactor": 0.85,
        },
    },
    {
        "name": "head",
        "pbrMetallicRoughness": {
            "baseColorFactor": [0.72, 0.76, 0.80, 1.0],
            "metallicFactor": 0.9,
            "roughnessFactor": 0.35,
        },
    },
]


# ---------------------------------------------------------------------------
# glTF assembly
# ---------------------------------------------------------------------------

COMPONENT_FLOAT = 5126
COMPONENT_USHORT = 5123
TARGET_ARRAY_BUFFER = 34962
TARGET_ELEMENT_ARRAY_BUFFER = 34963


def build() -> dict:
    buffer = bytearray()
    views: list[dict] = []
    accessors: list[dict] = []
    primitives: list[dict] = []

    def pad_to_four() -> None:
        while len(buffer) % 4:
            buffer.append(0)

    def add_view(payload: bytes, target: int) -> int:
        pad_to_four()
        views.append(
            {
                "buffer": 0,
                "byteOffset": len(buffer),
                "byteLength": len(payload),
                "target": target,
            }
        )
        buffer.extend(payload)
        return len(views) - 1

    for name, verts, material in PRIMITIVES:
        positions = [p for p, _ in verts]
        normals = [n for _, n in verts]
        idx = indices_for(len(verts) // 4)

        pos_bytes = b"".join(struct.pack("<3f", *p) for p in positions)
        nrm_bytes = b"".join(struct.pack("<3f", *n) for n in normals)
        idx_bytes = b"".join(struct.pack("<H", i) for i in idx)

        pos_view = add_view(pos_bytes, TARGET_ARRAY_BUFFER)
        nrm_view = add_view(nrm_bytes, TARGET_ARRAY_BUFFER)
        idx_view = add_view(idx_bytes, TARGET_ELEMENT_ARRAY_BUFFER)

        # POSITION is the one accessor glTF requires min/max on: a loader uses
        # it for the bounding box without touching the buffer, which is
        # exactly what the icon bake's fit step wants.
        lo = [min(p[axis] for p in positions) for axis in range(3)]
        hi = [max(p[axis] for p in positions) for axis in range(3)]

        accessors.append(
            {
                "name": f"{name}_position",
                "bufferView": pos_view,
                "componentType": COMPONENT_FLOAT,
                "count": len(positions),
                "type": "VEC3",
                "min": [round(v, 6) for v in lo],
                "max": [round(v, 6) for v in hi],
            }
        )
        accessors.append(
            {
                "name": f"{name}_normal",
                "bufferView": nrm_view,
                "componentType": COMPONENT_FLOAT,
                "count": len(normals),
                "type": "VEC3",
            }
        )
        accessors.append(
            {
                "name": f"{name}_index",
                "bufferView": idx_view,
                "componentType": COMPONENT_USHORT,
                "count": len(idx),
                "type": "SCALAR",
            }
        )
        primitives.append(
            {
                "attributes": {
                    "POSITION": len(accessors) - 3,
                    "NORMAL": len(accessors) - 2,
                },
                "indices": len(accessors) - 1,
                "material": material,
            }
        )

    pad_to_four()
    uri = "data:application/octet-stream;base64," + base64.b64encode(bytes(buffer)).decode()

    return {
        "asset": {
            "version": "2.0",
            "generator": "slotted tools/gen-demo-model.py",
            "copyright": "slotted contributors, MIT OR Apache-2.0",
        },
        "scene": 0,
        "scenes": [{"name": "pickaxe", "nodes": [0]}],
        "nodes": [{"name": "pickaxe", "mesh": 0}],
        "meshes": [{"name": "pickaxe", "primitives": primitives}],
        "materials": MATERIALS,
        "accessors": accessors,
        "bufferViews": views,
        "buffers": [{"byteLength": len(buffer), "uri": uri}],
    }


def main() -> None:
    root = pathlib.Path(__file__).resolve().parent.parent
    out = root / "assets" / "models" / "pickaxe.gltf"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(build(), indent=2) + "\n")
    print(f"wrote {out.relative_to(root)} ({out.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
