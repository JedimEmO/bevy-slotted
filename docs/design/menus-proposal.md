# Menus proposal

Status: draft v0.3, 2026-09-07. Not a contract yet; this is the document we
argue with until it becomes one. Nothing in it is implemented. Decisions
marked **decided** were made by the owner on 2026-09-07.

Owner's words: "we have a good inventory base, but we want to support other
game menus/UIs as well. should fit with the themes, look modern, feel good for
both console like as well as more text-based UIs."

## 1. What we are building

The inventory stack answers "a screen is a list of slots". This adds the second
family every game needs: screens with **no inventory behind them**. Main menu,
pause, settings, confirm dialogs, a dialogue box with choices, a quest or
codex page, a toast. Two idioms have to feel native on the same tree:

- **Console-like.** A focus ring that is always visible, d-pad or stick moves
  it, face buttons accept and back out, shoulders switch tabs, a hint bar at
  the bottom shows the glyphs. Big targets, one column of controls, values
  cycled with left/right rather than dropdowns.
- **Text-based.** Rich text that wraps, a typewriter reveal, choice lists,
  scrolling logs and pages, key or button glyphs inline in a sentence. Reads
  well at 14 px in a terminal-style theme and at 24 px on a TV.

Same principles as the rest of the workspace: screens are data, widgets are
themed by role, every screen is drivable headless, and Lua can build or inject
into any of it.

## 2. Non-goals

- A general-purpose retained UI toolkit. We add the controls a game menu
  needs, not a spreadsheet.
- Pan-zoom canvases and Markdown books stay deferred (PLAN section 1). The
  rich-text page here is a paragraph stack, not a document renderer.
- A free-floating gamepad cursor (Destiny style). **Decided:** snap focus
  only in this round. If a cursor ever comes it is a fourth `InputMode` over
  the same action vocabulary, and the ring design in 4.2 does not assume it.
- Touch. Nothing here should preclude it, but no touch-specific work.

## 3. Where the code lives

Two layers, matching the existing split between primitives and compositions.

**Foundation, in `slotted-ui` and `slotted-theme`.** Cross-cutting and needed by
the inventory screens too: the input model, focus ring, screen stack, layout
model, type scale, rich text, generic controls. Putting these in a new crate
would make the chest screen depend on it, which is backwards.

**`slotted-menu`, a new crate.** The compositions: settings model and binding,
dialog helpers, dialogue runner, the shipped screen templates, the hint bar.
Depends on `slotted-ui`, never the other way. It is the first "mod-shaped
crate built on the extension API" the plan promised: if it needs a hook the
API does not have, that is a foundation bug to fix, not a private path to
open.

**Decided:** one crate. It splits only if the dialogue runner grows a script
format of its own.

## 4. Foundation

### 4.1 Input model

Today: arrow keys drive Bevy's directional navigator, Escape is hard-coded in
two places, nothing reads a gamepad, and there is no notion of which device
the player is using.

- **`UiAction`** enum, the one vocabulary every widget listens to: `Accept`,
  `Back`, `Secondary`, `Up/Down/Left/Right`, `TabPrev/TabNext`, `PagePrev/PageNext`,
  `Menu`, `Scroll(f32)`. Emitted as a Bevy message once per frame per action
  with a source device.
- **`UiBindings`** resource maps keyboard, mouse buttons and gamepad to
  actions. Ships with a sane default (Enter/Space/South accept, Escape/East
  back, arrows and WASD and d-pad and left stick move, LB/RB tab, Q/E tab on
  keyboard). Games replace it; the settings screen's key-binding control edits
  it. `NavKeys` folds into this.
- **`InputMode`** resource: `Pointer`, `Keyboard`, `Gamepad`, set from the
  last device that produced an action or a pointer move over a dead zone.
  Widgets read it for two things only: whether to draw the focus ring, and
  which glyph set the hint bar shows. Nothing else branches on it.
- **Stick repeat.** Held direction repeats after `durations.hover_delay` then
  every `durations.fast`; same for held keys so keyboard and pad match.
- **Text entry guard** stays: while a `TextField` has focus, only `Back`
  passes through.

### 4.2 Focus and the ring

- Every interactive widget gets a `Focusable` marker and a `*.focus` role
  resolved through the existing dotted fallback, so a theme defines `focus`
  once and overrides `button.focus` or `slider.focus` only when it wants to.
- **The ring is a separate entity** drawn outside the element's rectangle in
  the tooltip z-band, following the focused entity with the theme's `Fade`
  and `Translate` presets, so moving focus slides the ring rather than
  blinking it. One `focus.ring` role (`Solid` or `Dashed` border, radius,
  elevation). The slot's existing `slot.focus` fill stays as an inner state.
- Ring visibility: always in `Gamepad` and `Keyboard` modes, hidden in
  `Pointer` mode until a key is pressed. Bevy's `InputFocus` remains the
  source of truth; we add the visual and the policy.
- **Nav graph.** Bevy's auto directional navigation works inside a container.
  Between containers we add `nav: (up: "id", down: "id", ...)` on any node in
  the screen file, and a screen-level `initial_focus: "id"`. Focus is
  restored to the last focused node when a screen regains the top of the
  stack.
- A `FocusScope` per screen so a modal traps focus and the screen under it
  keeps its last focus.

### 4.3 Screen stack

Today `spawn_screen` and `close_screen` exist and nothing knows what is open.

- **`ScreenStack`** resource: ordered list of open screen roots with a
  `Presentation` each. `push`, `replace`, `pop`, `pop_to(kind)`, `clear`.
  `spawn_screen` keeps working as the low-level call; the stack is the thing
  games and Lua use.
- **`Presentation`** is declared in the screen file:
  `presentation: (mode: "page" | "modal" | "overlay", scrim: true,
  transition: "fade" | "slide_up" | "slide_left" | "none")`. Default `page`.
  - `page` replaces focus, hides the screen below it (settings over main
    menu).
  - `modal` keeps the screen below visible, adds a scrim, traps focus,
    consumes `Back` as pop (confirm dialog, pause).
  - `overlay` does not take focus and does not block input (toast, subtitle).
- **`Back`** pops the top screen unless the top screen's root has
  `back: "ignore"` or a widget on it consumed the action (a text field, an
  open select). One place, replacing the two hard-coded Escapes.
- Z order: each stack entry gets its own z within `zbands::SCREEN`; scrim is
  a themed node (`scrim` role) between entries.
- Transitions use the motion tokens; add `Slide` and `Reveal` presets.
  Reduced motion turns every transition into a `Fade` of `durations.fast`.
- HUD visibility rule generalises: `hud_screen_visibility` hides layers when
  any `page` or `modal` is open, not when any screen is.
- **Decided:** inventory screens move onto the stack in M0. A chest opened
  through the stack is a `page`, Escape closes it, and the carried-stack
  return-on-close is unchanged.

### 4.4 Layout model

Today `Layout` has direction, gap, padding, fixed width and height, and
`center`. A settings page cannot be built with that.

Add, keeping spacing steps as the unit where a length is a length:

| Field | Values | |
|---|---|---|
| `align` | `start`, `center`, `end`, `stretch` | cross axis; replaces `center` (kept as alias) |
| `justify` | `start`, `center`, `end`, `space_between` | main axis |
| `grow` | number | flex grow |
| `width`, `height` | `Px(n)`, `Steps(n)`, `Percent(n)`, `Fill` | today's bare number stays `Px` |
| `min_width`, `max_width`, `min_height`, `max_height` | same | |
| `padding` | number or `(top, right, bottom, left)` | |
| `overflow` | `visible`, `scroll` | scroll makes the panel a scroll container, see 4.6 |
| `wrap` | bool | row wrap for chip rows and button grids |
| `place` | `(anchor: NineAnchor, offset: (x, y))` | absolute placement in the parent; lifts HUD's `NineAnchor` into the general model |

`place` on a screen root's child is how a menu sits bottom-left with a logo
top-right without nesting three panels. The HUD keeps its layer stack and
editor; it just stops owning the anchor type.

### 4.5 Typography and rich text

Today a `Text` role carries a raw size, and there are four text styles.

- **Type scale token**: `typography: { display: 40, title: 24, heading: 18,
  body: 15, label: 13, caption: 12 }` with per-role `font` and `weight`. A
  `Text` material may say `size: "$body"` like a palette reference. Existing
  raw sizes keep working. The showcase's theme scene proves three themes
  agree on the ramp.
- **`TextRole`** grows to `display`, `title`, `heading`, `body`, `muted`,
  `label`, `caption`, `count`, mapped to `text.*` roles.
- **Wrapping and alignment** on the text node: `wrap: bool`, `align:
  left|center|right`, `max_lines`. Fixes the browser status line follow-up as
  a side effect.
- **Rich text.** One inline markup, parsed once into Bevy text spans:
  `[b]bold[/b]`, `[i]`, `[color=$accent]`, `[size=heading]`,
  `{key:accept}` (renders the glyph for whatever `Accept` is bound to in the
  current input mode), `{icon:demo:chest}` (an item icon inline at
  line height), `{arg}` (Fluent argument). Square-bracket tags because Fluent
  owns braces for arguments and Lua strings are happier without HTML.
  **Decided:** brackets, not a Markdown subset; the glyph and icon tags have
  no Markdown equivalent and a second syntax would show up in mod files.
- **Localisation with arguments.** `Localizer::resolve` gains an
  `args: &LocArgs` parameter. Plurals and selectors already work in the
  Fluent backend; the UI layer just never passed anything.

### 4.6 Controls

All in `slotted-ui/src/widgets/`, each a `Widget` kind with a `SemanticRole`,
theme roles with the `.hover`, `.focus`, `.active`, `.disabled` suffixes, a
locator-friendly `test_id`, and a `value_changed` event carrying the node's
tags. None of them knows about settings; they bind to a `Binding` (4.7).

| Kind | Role | Gamepad behaviour | Notes |
|---|---|---|---|
| `button` (extended) | Button | Accept | gains `label` as loc key, `icon`, `variant: primary|secondary|danger`, `disabled`. `slotted:close` finally exists and pops the stack. |
| `text_field` | TextField | Accept opens, Back closes; on-screen keyboard is the game's problem, we emit `text_entry_requested` | lifts the browser's `SearchField` |
| `toggle` | Toggle | Accept flips | checkbox and switch are the same widget with a theme role each |
| `slider` | Slider | Left/Right step, hold repeats | `min`, `max`, `step`, `format: "{value}%"` |
| `select` | Select | Left/Right cycle in place; Accept opens a popup list in pointer mode | options are loc keys |
| `radio_group` | RadioGroup | Left/Right or Up/Down | segmented control; same data as `select` |
| `key_binding` | KeyBinding | Accept captures next input | edits `UiBindings` |
| `list` | List | Up/Down, PagePrev/Next | virtualised, reuses `virtual_grid` with `cols: 1` and a row template |
| `scroll` | ScrollView | Scroll action, focus-follow | any panel with `overflow: scroll`; scrollbar roles from `virtual_grid` |
| `tabs` | TabBar | TabPrev/TabNext | generic version of the browser's `TabRow`; `pages` are child panels |
| `rich_text` | Text | none | the markup of 4.5 |
| `separator`, `spacer`, `image` | Decorative | none | |
| `hint_bar` | Hints | none | shows `{key:*}` glyphs for the actions the focused widget accepts; lives in `slotted-menu` but is listed here for completeness |

Every control has three sizes decided by the theme, not by the screen: the
theme's `sizes.control_height` for a menu row, `sizes.control_height_compact`
for inline use, and slot-sized for anything that sits in a slot grid.

### 4.7 Bindings: what a control is bound to

Inventory widgets bind to a menu property by index. A settings screen has no
menu. Proposal:

- **Decided:** string keys, with a typed `Settings` helper on top.
- **`ValueStore`** resource: a string-keyed map of `Value` (bool, int, float,
  string, enum id). Controls declare `bind: "audio.master"`. Reads paint the
  control; writes go through a `SetValue` command that the game observes,
  validates and commits, mirroring how clicks go through the authority. A
  value the game rejects snaps back.
- The game populates the store however it likes: from its own settings
  struct, Bevy resources, a save file. `slotted-menu` ships a `Settings`
  helper that syncs a `serde` struct both ways and persists through a
  `SettingsStore` trait with file and memory backends, the same shape as
  `HudLayoutStore`.
- A control on an inventory screen may still bind to a menu property with
  `property: n`; a control has exactly one of `bind` or `property`.
- Lua sees `value_changed` events and a `set_value` command. Values a mod
  registers live under its namespace (`my_mod.difficulty`).

## 5. `slotted-menu`

### 5.1 Screen templates

Ship as `.screen.ron` under `assets/screens/menu/`, every one meant to be
inherited from and every header carrying anchors:

- `slotted:main_menu`: title, a vertical button column bound to `menu.*`
  actions, version caption, anchors `title_end`, `buttons_end`, `footer`.
- `slotted:pause`: modal, scrim, resume, settings, quit-to-title.
- `slotted:settings`: `tabs` across the top, one `scroll` list of rows per
  tab. Rows are `label + control` pairs in a two-column grid. Anchors per
  tab so a mod can add a row to `settings.gameplay`. Sections: display,
  audio, controls (key bindings), accessibility (UI scale, reduced motion,
  text size, colour-blind palette swap), and a `mods` tab that is empty
  until mods inject into it.
- `slotted:confirm`: modal, message as rich text, two buttons, `danger`
  variant on the destructive one. A Rust helper `confirm(commands, key,
  args, on_accept)` and a Lua `open_screen("slotted:confirm", {...})`.
- `slotted:toast`: overlay, bottom-centre, auto-pops after a duration,
  stacks upward.
- `slotted:dialogue`: see 5.3.
- `slotted:page`: a scrollable rich-text page with a heading, for codex
  entries, patch notes and credits.

The three themes get the new roles. The paper theme is the one that must
look right as a "text-based" UI: hairline rules, monospace labels, no
scrim blur, focus ring as an inverted row rather than a border.

### 5.2 Hint bar

**Decided:** a widget the templates place, not something automatic on every
screen, so a game that wants none deletes one node.

One widget, usually at the screen bottom, that shows the glyphs for the
actions the focused widget accepts plus the screen's own (Back, tabs). Glyph
sets: keyboard, Xbox, PlayStation, Switch, generic; chosen from `InputMode`
and the connected pad's vendor. Glyphs are an icon atlas, themed through a
`hint.glyph` role so paper can draw them as bracketed text (`[A]`).

### 5.3 Dialogue

A data-driven runner, not a scripting language. A **`Dialogue`** asset is a
list of nodes: `say { speaker, portrait, text, next }`, `choice { prompt,
options: [{ text, next, enabled_if }] }`, `end`. Conditions and side effects
go out as events (`dialogue_choice`, `dialogue_node`) and come back as
commands, so a Lua mod or the game decides what a choice does. The screen
is `slotted:dialogue`, an `overlay` by default, with:

- Typewriter reveal at a theme-tokened characters-per-second, Accept skips
  to the end of the node then advances, reduced motion shows everything at
  once.
- Choices as a `list` of buttons that appear after the reveal completes.
- A `history` page (`slotted:page`) of everything said, opened with
  `Secondary`.
- Rich text throughout, so a line can say "press {key:accept} to continue".

This is the piece that makes the text-based idiom concrete. **Decided:** it
ships in this round as M3. The runner and the screen are in; persistence and
a branching-graph editor are out.

## 6. Scripting

Data stage: nothing new; `register_screen` and `inject` already cover menus.
Add an `InjectionTarget` filter (the follow-up already noted) so a mod can say
"every screen with presentation modal" or "every screen in `settings.*`".

Control stage:

- Commands: `open_screen(kind, { args })`, `close_screen()`, `pop_to(kind)`,
  `set_value(key, value)`, `toast(key, args)`, `start_dialogue(id)`.
- Events: `value_changed`, `screen_opened` and `screen_closed` (existing)
  gain the presentation, `dialogue_node`, `dialogue_choice`, `binding_changed`.
- Lua test API: `gamepad(button)`, `action(name)`, `focused()` returning the
  focused node's tags.

## 7. Testing

- `UiHarness::open(kind)` with no fixture for menu-less screens.
- `h.gamepad(GamepadButton)`, `h.stick(dir)`, `h.action(UiAction)`, and
  `h.input_mode(mode)`.
- Assertions: `h.focused()` locator, `h.stack()` listing open kinds,
  `h.value("audio.master")`.
- Every template ships with a headless test that walks it entirely by
  gamepad: focus lands on `initial_focus`, every control is reachable, Back
  pops. That test is the console-parity gate for any theme.
- Snapshot tests of every template in all three themes at 1280x720 and
  1920x1080 with UI scale 1 and 1.5.

## 8. Showcase

Scene 9, **Menus**: a main menu over the backdrop, settings with a mod-injected
row, a confirm dialog, a dialogue with a choice. "What to try": switch to
gamepad mode and drive it with the on-page pad; open settings and change the
UI scale; watch the same screens in paper.

## 9. Phasing

Each phase ends with the workspace green, the guide updated and a notes file.

| Phase | Delivers | Proves |
|---|---|---|
| M0 Foundation | `UiAction`, `UiBindings`, `InputMode`, focus ring, nav graph, `ScreenStack`, presentations, layout model | chest screen opens through the stack, Escape closes it, gamepad walks its slots with a visible ring |
| M1 Type and controls | type scale, rich text, loc args, every control in 4.6, `ValueStore` | a settings page built from RON, driven headless by pad and mouse in three themes |
| M2 `slotted-menu` | templates, hint bar, settings helper and store, confirm, toast | main menu to settings to confirm and back, no Rust in the game beyond wiring |
| M3 Dialogue | runner, dialogue screen, history | the text-based idiom end to end |
| M4 Reach | Lua commands and events, harness gamepad, showcase scene 9, docs | a mod adds a settings row and a dialogue choice with no rebuild |

## 10. Open questions

None. Every question from v0.1 was decided on 2026-09-07 and folded into the
sections above: one crate; string-keyed `ValueStore` with a typed helper;
bracket rich-text markup; no free gamepad cursor this round; hint bar is a
placed widget; inventory screens join the stack in M0; dialogue ships as M3.

The next step is a contract for M0 (`docs/design/menus-m0-contract.md`),
written the way the phase contracts were, before any code.
