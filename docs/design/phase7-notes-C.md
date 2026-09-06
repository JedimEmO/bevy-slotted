# Phase 7 package C: item icons and the polish list

Status: done, 2026-09-06. Covers PLAN 4.6 (`slotted-icons`), the icon half of
`docs/research/research-modern-ui.md` sections 2 and 4, and the three
"Phase 7 polish" entries in `docs/FOLLOWUPS.md`.

## 1. The icon format

`ItemDef.icon` is now an `IconDef` (`slotted-registry/src/icon.rs`) rather than
an `Option<String>`:

| Written as | Parses to | Rendered as |
| --- | --- | --- |
| `"icons/sword.png"` | `Image(path)` | the texture, loaded through the asset server |
| `(shape: "ingot", color: "#c9793f", metallic: 0.9)` | `Shape(ShapeIcon)` | a lit primitive in the atlas |
| `(model: "models/anvil.gltf")` | `Model(path)` | nothing yet: `IconRef::Missing`, one warning per item |
| absent | — | a cube in the item's hash colour |

Three shapes, told apart by their **shape** rather than by a tag: a string is
an image, a map with a `shape` key is a shape, a map with a `model` key is a
model. That is what lets RON, JSON and a Lua table write the same thing, so a
mod declares `icon = { shape = "cube", color = "#b06a3c" }` in `data.lua` and
gets the same picture the base game's RON does. Phase 2's bare string still
parses, which is why no existing data file had to change.

`ShapeKind` is `cube | slab | ingot | gem | rod | sphere`, serialised as a
lowercase string for the same reason `Rarity` is: it has to survive the
`Value` round trip the patch stage does, and a Lua table can only write a
string. `IconColor` is `#rgb`, `#rrggbb` or `#rrggbbaa`; the registry has no
Bevy dependency, so it is four `f32`s and the bake converts once.

## 2. Two bakes, one picture

**GPU (`slotted-icons/src/gpu.rs`, `gpu` feature, on by default in the facade).**
One orthographic `Camera3d` looks down `-Z` at a grid of meshes laid out
exactly on the atlas cells, and its render target *is* the atlas image. There
is no readback: nothing ever leaves the GPU, which is what makes the path work
on WebGL2 where `Readback` and texture-to-buffer copies are not available.
Because the camera is orthographic and the three lights are directional, every
cell is lit identically wherever it sits in the grid. The rig is the fixed
three-point one section 2 of the research asks for: key from the upper left,
warm fill from the lower right, cool rim (`#8CBCFF`-ish) from behind.

**CPU (`slotted-icons/src/atlas.rs` + `shape.rs`).** The same silhouettes as
flat polygons from the same viewpoint, with the same rim light, supersampled
2x2. It is what a headless app and the test harness see, it is deterministic
(two bakes of the same registry are byte-identical), and it is the *initial
content of the GPU target*, so a screen drawn before the rig's pipelines
finish compiling shows flat-shaded icons rather than a hole.

**Cache.** `IconBakeFingerprint` is an FNV-1a hash over the item ids and their
icon defs. The bake runs when the fingerprint changes, which means on a
registry freeze or a mod reload and at no other time. A theme change does not
touch the registries and therefore does not rebake: the atlas is item data,
not theme data.

`LiveIcons` (the `live` feature) is real: it returns `IconRef::Live(id)` for
every item the atlas knows and defers to the atlas for the rest. `slotted-ui`
reads that as "put a `ViewportNode` at the head of the tooltip", and the
viewport now draws the item's own `ShapeKind` mesh with the item's material,
lit by the same three-point rig, turning slowly on `Time<Virtual>`. Slots and
browser cards call `Icons::flat_icon`, which is the fallback: one camera per
tooltip is affordable, one per slot is not (ADR 0003).

## 3. Polish

- **The tank readout moved out of the fluid.** A vertical tank's root is now a
  column: the well (bordered, clipped, holding the fill) with the stacked
  three-line reading underneath it. A horizontal tank is three slots wide and
  keeps the reading inside. `render_fills` walks descendants rather than direct
  children, because a tank's fill is one level deeper than a bar's.
- **Progress arrows are 40x14 with a pill track**, per the moodboard, instead
  of 20-ish px with the theme's small radius. `BarStyle::radius` is where a
  style says which radius it wants.
- **The hovered slot's tooltip shows the live viewport** when the `live`
  feature is on; `examples/chest` turns it on through `slotted/live-icons`.

## 4. What the screenshots show

`just shot-chest`, `just shot-machine`, `just shot-modded`, all re-captured.
No magenta anywhere: cobblestone is a lit grey cube with a visible blue rim,
oak planks a tan slab, the three ingots read as metal bars at their own
roughness, diamond is a cyan gem with a bright girdle, redstone and coal are
spheres, the tools are rods with an accent head. `examples/modded` proves the
same for shapes a Lua mod declared. `chest-hover.png` shows the live tooltip
preview.

## 5. Deviations and things left

- **`Model` icons parse and warn.** No glTF loader yet; the item draws the
  missing glyph. The format carries the path so the loader is additive.
- **The rig stays active for 240 frames.** A mesh pipeline is specialised and
  compiled asynchronously, so the first frames after the rig appears draw
  nothing; switching the camera off after two or three left a permanently
  blank atlas, which is what the first Phase 7 screenshots showed. Four
  seconds at 60 Hz is a generous margin. A real fix waits on an observable
  "this view has rendered" signal.
- **The CPU and GPU pictures differ in detail**, not in reading: the CPU one is
  flat-shaded polygons, the GPU one is PBR. The accent is an inlay band on the
  CPU and a band mesh on the GPU, and a gem's accent is a girdle ring rather
  than a painted band.
- **wasm.** The whole bake compiles for `wasm32-unknown-unknown`
  (`just wasm-check` covers `slotted-icons --all-features`). The grid render
  target sidesteps the readback WebGL2 lacks, but the path has not been run in
  a browser; the playground still needs a look.
