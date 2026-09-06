# Phase 7 notes: package A (paper and neon themes)

The second and third themes, and what the token set had to grow to carry
them. Plan: `docs/PLAN.md` Phase 7 and section 4.4. Directions:
`docs/research/research-modern-ui.md` section 5 B and C, `docs/moodboard.html`
section 02.

The rule for the phase was "missing roles are added, not special-cased". It
held: no widget changed shape for either direction, and the chest screen's
`screen_tree()` is byte for byte the same under all three themes (tested in
`examples/chest/tests/ui.rs`). What did change is the `Material` set and the
token table, all of it additive and every addition used by a shipped theme.

## Schema additions in `slotted-theme`

### `Material` variants

| Variant | Direction | Why the existing set could not do it | Paints |
|---|---|---|---|
| `Tiled { image, scale, tint, border, radius, elevation }` | paper's dotted grid | `Solid` has one colour and `Sliced` stretches; a grid is a repeating 8 px tile | `ImageNode` with `NodeImageMode::Tiled`, plus border, radius and shadow like a `Solid` |
| `Dashed { fill, stroke, width, dash, gap, radius, elevation }` | paper's rarity stamps | `bevy_ui` has no dashed border | `BorderGradient` with hard stops alternating `stroke` and transparent every `dash`/`gap` px along a 45 degree line, so the ring reads as a hatched rule; `Node::border` when `width` is set |
| `CutCorners { fill, border, border_width, cut, corners, bar, bar_height, glow, elevation }` | neon's shape and rarity | `BorderRadius` only rounds; a chamfer needs an SDF | `MaterialNode<CutCornerMaterial>` under `blur`; square `Solid` fallback otherwise, with the bar as a bottom-up hard-stop `BackgroundGradient` |
| `Text { .., font, shadow }` | both | counts needed a font family and paper needed no drop shadow on light paper | `TextFont::font`, `TextShadow` (removed when `shadow` is `None`) |

The rarity bar lives inside `CutCorners` rather than as a separate `BottomBar`
material because the slot's bottom-left corner is cut: a bar drawn by a second
node would poke out of the chamfer. In the shader the bar is clipped by the
same SDF, and its glow bleeds up into the fill. `Corners` is four named flags
(default: top-right and bottom-left, the moodboard's diagonal).

`Paint` gained `tiled`, `border_gradient`, `border_width`, `font`,
`text_shadow` and `cut`. `Paint::from_material` stays a pure function; the only
runtime decision is in `apply_theme`: when `Assets<CutCornerMaterial>` exists
the square fallback is dropped from the paint so it does not show through the
chamfers.

### Tokens

- `Elevation.x` (default 0): paper's ink shadows are `2px 2px 0`, and a
  shadow token without a horizontal offset could not say so.
- `Tokens.fonts: BTreeMap<String, FontToken>` with `FontToken { family, path,
  system }`. Text roles name a token (`font: "mono"`). With `path` the TTF is
  loaded; with `system: true` the family goes to Bevy 0.19's
  `FontSource::Family`, which resolves against the OS when the game enables
  `system_font_discovery`; with neither, Bevy's default font stays and the
  family name documents which OFL file completes the look.
- `Tokens.motion: MotionTokens { easing, presets: BTreeMap<MotionPreset,
  MotionSpec { duration, easing }> }`. `Durations { fast, normal, slow }` stays
  as the tiers a preset falls back to. `Tokens::duration_ms(preset)` and
  `Tokens::easing(preset)` resolve the override-or-tier.

### Motion

`Easing` enum: `Standard` (ease-out cubic, what every tween was before),
`Linear`, `EaseInOut`, `Stamp` (ease-in cubic: gathers speed, lands dead),
`Snap` (ease-out quintic), `Overshoot` (back-out, the one curve that leaves
`0..=1`). `Tween` carries an `easing`; `Tween::new` keeps `Standard`.
`Motion::preset_duration` and `Motion::preset_tween` take `&Tokens` and honour
the per-preset override; the old `duration`/`tween` on `&Durations` are
unchanged for callers that only have tiers.

`MotionPreset` now derives `Serialize`, `Deserialize`, `Ord` so it can key the
RON map (`presets: { DropSquash: (duration: 140, easing: Stamp) }`).

### `cut.rs` and `CutCornerPlugin` (feature `blur`)

`blur` is in practice the "GPU materials" feature: it pulls `bevy_ui_render`
and `bevy_shader`, which is exactly what a cut-corner `UiMaterial` needs, so
the new material sits behind the same flag rather than a second one.
`BackdropPlugin` adds `CutCornerPlugin` if it is not already there, so the
three windowed examples needed nothing; a 2D game that wants neon without a
backdrop camera adds `CutCornerPlugin` alone. The shader is
`assets/shaders/cut_corner.wgsl` (copy in `crates/slotted-theme/assets/`).

## Changes outside `slotted-theme`

- `slotted-ui/src/motion.rs` and `widgets/side_tab.rs`: the four tween sites
  call `preset_tween`/`preset_duration` with the whole token table, so a
  theme's easing and per-preset duration reach the screen. `FlyToSlot` takes
  the theme's easing too. No behaviour change under glass.
- `assets/themes/glass.theme.ron`: `count` gains `shadow: "#000000BF"`, the
  `TextShadow` the count node used to spawn with unconditionally. Glass looks
  as it did; the difference is that paper can now turn the shadow off.
- `examples/{chest,machine,modded}`: `--theme <name>` (default `glass`).
- `justfile`: `shot-chest-themes` captures `chest-paper.png`, `chest-neon.png`
  and the recipe page in both (the browser panel is in every chest shot).
- `assets/themes/paper-dot.png`: the 8 px tile, generated, opaque paper with
  one rule-coloured pixel so draw order against a `BackgroundColor` never
  matters.

## What the screenshots show

`examples/chest/shots/chest-paper.png`: an opaque cream sheet with a visible
dot grid, a 1 px ink rule around the panel and every slot, square 2 px
corners, a hard 2 px ink shadow under each slot and button, and the rare,
epic and legendary stacks ringed in hatched orange, purple and red. The
browser is a second sheet with inked tabs and rule-coloured rarity strips. It
does not read as dark glass with beige.

`examples/chest/shots/chest-neon.png`: slate panel with 18 px cuts at top-right
and bottom-left, charcoal slots with 6 px cuts, a lime title, and rarity as a
3 px bar along the slot's bottom edge in cyan, lime, purple or magenta with a
glow rising into the slot, clipped by the chamfer. Buttons and the browser's
chips, search field and cards are cut too. Hover is a cyan rim, keyboard focus
a 2 px magenta rim.

Item icons are absent from all three shots at the time of capture because
package C's icon baking was mid-change in the working tree (the glass shot
taken the same minute has none either). Rerun `just shot-chest-themes` once C
lands.

## Fonts

Superseded. At the time none of IBM Plex Sans, IBM Plex Mono, Manrope,
Rajdhani or Barlow Condensed were installed (`fc-list` was empty for all five)
and the brief said not to fetch them, so the themes named their families in
`tokens.fonts`, every text role pointed at a token, and the shots used Bevy's
default face.

The gap pass, package C, shipped all five under `assets/fonts/<family>/` with
their `OFL.txt` and pointed the three themes' tokens at them; glass gained a
`fonts` block and `font:` on its text roles, which it had neither of. See
`docs/design/gaps-notes-C.md` section 4 and `assets/fonts/README.md`. The
`system: true` route is still there for a game that would rather ask the OS.

## Left as is, on purpose

- The browser card's rarity strip stays at the top of the card in neon. Its
  position is the card widget's layout, not a material; moving it per theme
  would be the special case the phase forbids.
- Paper's "stamp" is the hatched `Dashed` ring; a rotated text stamp would
  need a node the item view does not spawn.
- `MotionPreset::Stagger` has tokens in both themes but no widget starts a
  stagger yet; the token is there for when one does.
- `Dashed` hatches up to `DASH_SPAN` (512 px) along the diagonal; a dashed
  panel wider than that shows a solid stretch in its far corner. No shipped
  role is that large.
