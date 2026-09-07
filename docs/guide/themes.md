# Themes

A theme is a RON file mapping semantic **roles** to **materials**, plus a table
of **tokens**. Widgets never name a colour. They carry a `Themed(role)` and the
apply system paints them whenever the theme loads, changes on disk or is swapped
for another one.

That indirection is the whole point: a screen written against roles renders in
any theme, and a theme that forgets a role is caught by
`Theme::missing_roles()` rather than by a player finding an unpainted panel.

## The file

```ron
#![enable(implicit_some)]
(
    name: "glass",
    tokens: (
        spacing: (xs: 2.0, sm: 6.0, md: 14.0, lg: 20.0, xl: 32.0),
        radii: (sm: 8.0, md: 12.0, lg: 16.0),
        elevation: {
            "low":  (y: 4.0,  blur: 12.0, spread: 0.0, color: "#0000008C"),
            "mid":  (y: 8.0,  blur: 24.0, spread: 1.0, color: "#0000008C"),
            "high": (y: 14.0, blur: 38.0, spread: 2.0, color: "#0000008C"),
        },
        durations: (fast: 90, normal: 180, slow: 320, hover_delay: 120),
        blur: (radius: 4.0, backdrop_divisor: 4),
        palette: {
            "base": "#0B0E14", "panel": "#141A24", "hairline": "#2A3446",
            "accent": "#7FD1FF", "warm": "#FFB454", "text": "#E6EDF3",
            "muted": "#E6EDF373", "danger": "#FF6B6B",
        },
        rarity: {
            "common": "#E6EDF3", "uncommon": "#7FD1FF", "rare": "#B48CFF",
            "epic": "#FFB454", "legendary": "#FF6B6B",
        },
    ),
    roles: {
        "panel": Glass(tint: "#141A247A", border: "#FFFFFF1A", radius: 16.0,
                       elevation: Some("high")),
        "panel.title": Text(color: "$text", size: "$title"),
        "slot": Solid(fill: "#0B0E14A6", border: "$hairline", radius: 8.0),
        "slot.hover": Solid(fill: "#7FD1FF1F", border: "$accent", radius: 8.0),
    },
)
```

The loader takes `*.theme.ron`. Three themes ship, and every example takes
`--theme <name>` to switch between them:

| Theme | The direction | Materials it leans on |
|---|---|---|
| `glass` | Dark, translucent, blurred panels over the game. The default. | `Glass`, `Solid`, `Gradient` |
| `paper` | An opaque cream sheet: a dotted grid, 1 px ink rules, square corners, hard offset shadows, hatched rarity stamps. | `Tiled`, `Dashed`, `Solid` |
| `neon` | Slate and lime with chamfered corners and a glowing rarity bar along a slot's bottom edge. | `CutCorners`, `Solid` |

The three define exactly the same set of roles, which is the property Phase 7
was built to prove: a direction that needed a role the others lacked would
have needed a different widget. `assets/themes/glass.theme.ron` is the file to
copy.

## Roles

A role is a free-form dotted string. `Theme::material` walks up the dots until
it finds a match, so a theme that defines `slot` but not `slot.hover` still
paints a hovered slot; it just does not distinguish it. Defining a state role is
how you opt into distinguishing it.

State changes are role changes. The slot widget swaps `slot` for `slot.hover`,
and only the changed node repaints.

`roles::ALL` is the 89 well-known roles. A theme that covers them covers every
shipped widget.

| Group | Roles |
|---|---|
| Panels | `panel`, `panel.title` |
| Slots | `slot`, `slot.hover`, `slot.focus`, `slot.carried`, `count` |
| Text | `text`, `text.muted`, `text.display`, `text.heading`, `text.label`, `text.caption`, `text.key` (a `{key:..}` glyph), `text.icon` (an item name in a paragraph), `control.label` (a control's label) |
| Buttons | `button`, `button.primary`, `button.danger`, `button.hover`, `button.focus`, `button.pressed`, `button.disabled` |
| Toggles | `toggle`, `toggle.on`, `toggle.thumb`, `toggle.hover`, `toggle.focus`, `toggle.disabled`, `checkbox`, `checkbox.on` |
| Sliders | `slider`, `slider.fill`, `slider.thumb`, `slider.text`, `slider.focus`, `slider.disabled` |
| Selects | `select`, `select.hover`, `select.focus`, `select.popup`, `select.option`, `select.option.active`, `radio`, `radio.active` |
| Key bindings | `key_binding`, `key_binding.capturing` |
| Text fields | `text_field`, `text_field.focus`, `text_field.disabled`, `text_field.placeholder` |
| Scrolling and lists | `scroll.bar`, `scroll.thumb`, `list.row`, `list.row.hover`, `list.row.focus`, `list.row.selected` |
| Tabs | `tabs.bar`, `tab`, `tab.active`, `tab.hover`, `tab.focus` |
| Decor | `separator` |
| Tooltips | `tooltip`, `tooltip.frame` |
| Rails and side tabs | `rail`, `tab.rail`, `tab.side`, `tab.side.open`, `tab.side.header`, `tab.side.content` |
| Meters | `tank`, `tank.fill`, `bar`, `bar.fill`, `bar.text`, `progress`, `progress.fill` |
| Icon buttons | `icon_button`, `icon_button.hover` |
| Virtual grids | `virtual_grid`, `virtual_grid.scrollbar`, `virtual_grid.thumb` |
| Viewports | `viewport` |
| Focus and stack | `focus.ring`, `scrim` |
| HUD | `hud.panel`, `hud.crosshair`, `hud.edit.frame` |

The controls follow one convention: `<control>` at rest, then `.hover`,
`.focus`, `.active` (or `.on`, `.pressed`, `.selected`, `.capturing` where the
word fits) and `.disabled`, and the states stack, so a focused primary button
asks for `button.primary.focus` and a hovered active radio segment for
`radio.active.hover`. The dotted fallback resolves each of those through
`button.primary`, then `button`, which is why the table above lists the
states once per control rather than every combination. When two states hold at
once the control picks the role in the order `disabled`, then `active`, then
`focus`, then `hover`. A theme may define `button` once and let every state
fall through to it, or draw each one.

`carried`, the stack following the pointer, is deliberately outside `ALL`:
leaving it out is legal and the item view draws the stack unadorned.

`invisible` is a convention, not a well-known role. A panel that only groups and
spaces its children uses it, and no theme defines a material for it.

## Materials

| Material | Fields |
|---|---|
| `Solid` | `fill`, optional `border`, `radius`, `elevation` |
| `Gradient` | `angle` in degrees (0 is bottom to top), `stops` as `(position, colour)` pairs, plus the `Solid` fields |
| `Sliced` | `image`, `border` inset in image pixels, `scale` (default `1.0`), optional `tint` |
| `Glass` | `tint`, optional `blur` radius, `fallback_alpha_boost` (default `0.35`), plus the `Solid` fields |
| `Shader` | `shader` path and a map of `f32` `params`. Logged and skipped for now |
| `Text` | `color`, `size` (a number in pixels or `"$name"` into `tokens.typography`), optional `font` (a `tokens.fonts` key), `weight` (100 to 900) and `shadow` colour. See [Typography](#typography) |
| `Tiled` | `image` of a repeating tile, `scale`, optional `tint`, plus the `Solid` fields. Paper's dot grid |
| `Dashed` | `fill`, `stroke`, `width`, `dash` and `gap` in pixels, plus `radius` and `elevation`. Paper's hatched rarity stamp |
| `CutCorners` | `fill`, optional `border` and `border_width`, `cut` length, `corners`, optional `bar` with `bar_height` and `glow`, plus `elevation`. Neon's chamfer and rarity bar |

`Dashed` is a `BorderGradient` of hard stops along a 45 degree line, so it
needs no shader and draws everywhere. `CutCorners` is a `UiMaterial` behind the
`blur` feature; without it, or in a headless app, it paints a square fill with
the rarity bar as a bottom-up gradient, so a neon screen is still legible.
`Tiled` is an `ImageNode`, and its tile should carry its own opaque ground so
draw order against a background colour never matters.

`elevation` names an entry in the token table's `elevation` map rather than
giving a shadow inline, so every panel in a theme shares three shadow levels.

`Glass` without the `blur` feature degrades to a solid fill of its tint with
`fallback_alpha_boost` added to the alpha. That is why the default build still
looks like the moodboard: translucent tinted panels are the design, and blur is
the refinement. Turning `blur` on costs roughly one millisecond a frame while a
glass screen is open, measured in [ADR 0003](../adr/0003-glass-rendering.md).

## Colours

A colour is `"#RRGGBB"`, `"#RRGGBBAA"`, or `"$name"` for a palette entry. An
unresolvable colour becomes magenta rather than an error, so a typo in a theme
shows up on screen instead of failing a load in the middle of a session.
`Theme::dangling_palette_refs()` lists them all at once, over role materials,
elevation colours and rarity colours.

## Tokens

| Token | Fields | Meaning |
|---|---|---|
| `spacing` | `xs`, `sm`, `md`, `lg`, `xl` | Logical pixels. `xs` icon to count, `sm` slot gap, `md` panel padding, `lg` between sections, `xl` between panels. A screen's `gap` and `padding` are multiples of `sm`. |
| `sizes` | `slot_size`, `slot_gap`, `panel_width`, `card_width`, `card_height`, `chrome_height`, `control_height`, `control_height_compact` | Widget geometry in UI units. Every field is optional and defaults to the number the code used before it was a token: a 44 px slot, the spacing scale's `sm` step between slots, the item browser's 352 px panel of 75x94 cards above 190 px of chrome, and a 44 px control row (28 compact). See [Sizes and UI scale](#sizes-and-ui-scale). |
| `radii` | `sm`, `md`, `lg` | Slots and small buttons, buttons and tooltips, panels. |
| `elevation` | a map of `(x, y, blur, spread, color)` | Named shadow levels, `low`, `mid` and `high` by convention. `x` defaults to 0; paper's ink shadows are `2px 2px 0`. |
| `durations` | `fast`, `normal`, `slow`, `hover_delay` | Milliseconds. The first three are motion tiers before scaling: hover and press, fades, squashes and fly-to-slot, stagger. `hover_delay` is how long a slot is hovered before its compact tooltip appears; it defaults to 120 and is not scaled by `Motion`. |
| `fonts` | a map of names to `(family, path, system)` | What a text role's `font` names. With `path` the file is loaded; with `system: true` the family goes to the OS font database; with neither, Bevy's default face is used and the family name documents which file completes the look. The three shipped themes use `path`, pointing at the OFL faces under `assets/fonts/`; see `assets/fonts/README.md`. |
| `typography` | a map of names to `(size, font, weight, line_height)` | The type scale a `Text` material's `"$name"` size points at. See [Typography](#typography). |
| `motion` | `easing` and a map of `MotionPreset` to `(duration, easing)` | Per-preset overrides of the duration tiers and the curve. |
| `blur` | `radius`, `backdrop_divisor` | Read only with the `blur` feature. |
| `palette` | a map of names to colours | The `$name` targets. |
| `rarity` | a map of rarity names to colours | Tints item names in tooltips and browser cards. |

## Sizes and UI scale

`sizes` is the table a theme changes when a slot should be bigger, not just a
different colour. A widget reads it as it spawns, so a theme swap takes effect
with the next screen rather than by resizing the nodes already on the glass.

```ron
sizes: (slot_size: 52.0, slot_gap: 8.0, panel_width: 400.0),
```

The numbers are **UI units**, not pixels on the player's display. Bevy
multiplies every `Node` pixel by the render target's scale factor and by
`UiScale`, so a 44-unit slot is 88 physical pixels at `UiScale` 2 without any
of this code knowing. What does have to know is anything that plans a layout
from something measured in another space, because a window and a pointer are
in the window's logical pixels. `slotted_ui::UiUnits` converts them, and the
item browser's dock uses it: at `UiScale` 2 a 1280 px window is a 640-unit box,
so the panel re-docks into the strip that is really there and fits fewer cards
rather than overflowing the screen. Multiplying a token by `UiScale` before
writing it into a `Node` is the mistake this avoids; it scales twice.

Two numbers are deliberately not tokens yet. The recipe layouts in
`slotted-browser` (`category.rs`) place their slots from the default constants,
because `RecipeCategory::size` and `layout` are a public trait a mod
implements, and threading sizes through them changes that contract.

## Typography

A theme carries a type scale, and every text role points into it rather than
naming a pixel size:

```ron
typography: {
    "display": (size: 40.0, font: "display", weight: 600),
    "title":   (size: 24.0, font: "body", weight: 600),
    "heading": (size: 18.0, font: "body", weight: 600),
    "body":    (size: 15.0, font: "body", line_height: 1.4),
    "label":   (size: 13.0, font: "body", weight: 500),
    "caption": (size: 12.0, font: "body"),
},
roles: {
    "panel.title": Text(color: "$text", size: "$title"),
    "text":        Text(color: "$text", size: "$body"),
    "text.key":    Text(color: "$accent", size: "$label", weight: 700),
},
```

A `TypeStyle` is a `size` in pixels, an optional `font` (a `tokens.fonts`
key), an optional variable-font `weight` (100 to 900, 400 when absent) and an
optional `line_height` relative to the font size. A `Text` material's `size`
is a `ThemeSize`: a bare number, or `"$name"`. A `$` size brings the style's
font and weight with it unless the material names its own, and the style's
line height is written as Bevy's `LineHeight` on every text node the role
paints, so swapping roles never keeps the last one's metric.

The six steps, in every shipped theme, in descending size:

| Step | glass | paper | neon | `TextRole` |
|---|---|---|---|---|
| `display` | 40 Barlow Condensed | 36 IBM Plex Sans | 42 Rajdhani | `display` |
| `title` | 24 | 22 | 26 Rajdhani | `title` (`panel.title`) |
| `heading` | 18 | 17 | 19 | `heading` |
| `body` | 15, line height 1.4 | 14, 1.45 | 15, 1.3 | `body` (`text`) |
| `label` | 13 | 12 IBM Plex Mono | 13 | `label` |
| `caption` | 12 | 11 | 12 | `caption`, `muted`, `count` |

`Theme::size(&ThemeSize)` resolves a size to pixels, `Theme::type_style` hands
back the style a `$` size names, and `Theme::dangling_typography_refs()` lists
every `$name` no step defines, beside `dangling_palette_refs()`; a dangling
size paints at 13 px so the screen stays legible while the log says which
role. Anything that paints text the apply system cannot reach (a rich text
span, a slider's readout) goes through `Paint::for_role`, `Paint::text_font`
and `Paint::text_color`, so it cannot disagree with a themed node.

## Control sizes

`sizes.control_height` (44 in glass, 36 in paper, 40 in neon) and
`control_height_compact` (28, 24, 28) are the height of every M1 control row
(buttons, toggles, sliders, selects, key bindings, text fields) and of the
compact ones (rail buttons, list rows, tab buttons). A screen file never sets
a control's height; a theme that wants denser settings changes these two
numbers and every screen follows. The tokens decide the rest of a control's
geometry too: the toggle's thumb travels `spacing.md`, the slider track is
`spacing.sm + 2` tall, the corners take `radii.sm`.

## Motion

`Motion` is a resource with a `scale` and a `reduced` flag. `MotionPreset` picks
a duration token: `Hover` and `Press` take `fast`, `DropSquash`, `Fade` and
`FlyToSlot` take `normal`, and `Stagger` takes `slow`. A theme overrides either
the duration or the curve per preset under `tokens.motion.presets`.

The flight is a normal-tier motion on purpose. It decorates a model change that
has already landed, so a long one reads as lag rather than as feedback; all
three shipped themes pin it between 150 and 180 ms.

Six curves, and the theme picks one per preset:

| `Easing` | Shape | Used by |
|---|---|---|
| `Standard` | Ease-out cubic. The default. | glass and paper |
| `Linear` | No easing. | |
| `EaseInOut` | Ease-in-out cubic, for fades. | |
| `Stamp` | Ease-in cubic: gathers speed and lands hard. | paper's drop |
| `Snap` | Ease-out quintic: most of the travel up front. | neon's hover and press |
| `Overshoot` | Back-out: passes the end value by about a tenth and settles. | neon's drop |

`Overshoot` is the one curve that leaves `0..=1`, and the only one a theme with
a no-springs rule has to avoid; `Easing::overshoots()` answers that. It still
ends exactly at its end value, so a slot never settles off identity.

```rust
commands.insert_resource(Motion::REDUCED);
```

Reduced motion is not "animate faster". Every tween lands on its end value on
its first frame, so the UI is instantly correct and a reduced-motion test settles
in one frame. Wire it to your game's accessibility setting.

Tweens advance against `Time<Virtual>` and nothing else. `ActiveMotions` counts
the live ones, and `slotted-test`'s `settle()` waits for it to reach zero, which
is why a harness test never sleeps.

## Using one

```rust
commands.insert_resource(ActiveTheme(assets.load("themes/glass.theme.ron")));
```

Swapping the handle repaints every themed node. Editing the file on disk does
the same, through the ordinary asset watcher.

## Writing one

1. Copy `assets/themes/glass.theme.ron` and change `name`.
2. Replace the palette. Most roles reference `$name`, so this is the bulk of a
   reskin.
3. Run `Theme::missing_roles()` over it and fill in what it names. It reports
   only roles that resolve to nothing at all: a `tank.fill` you forgot still
   paints, in the tank's own material, which looks like a tank with no fill.
   Comparing your role list against a shipped theme's catches those.
4. Open every example against it: `cargo run -p chest -- --theme <name>`, then
   `machine`, which is the screen with the tanks, bars and side tabs.

A theme that needs a role the token set does not have is the interesting case.
Add the role to `roles::ALL` and give every shipped theme a material for it,
rather than special-casing the widget. That is what proving the token set means.
