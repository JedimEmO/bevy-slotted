# Screens

A screen is data. `ScreenDef` holds a tree of `UiNodeDef`s, and `spawn_screen`
is the one place that turns that tree into entities. The same tree comes from a
Rust struct literal, a `.screen.ron` file or a Lua table, so a mod can build a
screen the engine has never heard of without a rebuild. A game opens one
through the [screen stack](#the-screen-stack), which is what makes `Back`
close it.

## The file

```ron
#![enable(implicit_some)]
(
    kind: "demo:chest",
    presentation: (mode: "page"),
    initial_focus: "chest_grid",
    root: (
        type: "panel",
        role: "panel",
        layout: (direction: "column", gap: 2.0, padding: 2.0),
        children: [
            (type: "text", key: "demo.chest.title", style: "title"),
            (type: "anchor", id: "title_end"),
            (type: "slot_grid", inventory: 0, cols: 9, rows: 3, first: 0,
             tags: {"region": "chest", "test_id": "chest_grid"}),
            (type: "slot_grid", inventory: 1, cols: 9, rows: 3, first: 27,
             tags: {"region": "player"}),
            (type: "custom", kind: "slotted:hotbar", params: (first: 54),
             tags: {"region": "hotbar"}),
        ],
    ),
    listring: [0, 1],
)
```

`ScreenDef` has seven fields.

| Field | Type | Default | |
|---|---|---|---|
| `kind` | `"namespace:path"` | required | The screen's id. A mod injects by naming it. |
| `root` | `UiNodeDef` | required | The tree. |
| `inherits` | `"namespace:path"` | none | A base screen to start from; see [Inheritance](#inheritance). |
| `listring` | array of inventory indices | `[]` | The order quick-move walks. Mirrors `MenuDef::listring`. |
| `remove` | array of node ids | `[]` | Nodes to delete from the inherited tree. Only meaningful with `inherits`. |
| `presentation` | `Presentation` | `(mode: "page")` | How the screen sits on the stack; see [Presentation](#presentation). |
| `initial_focus` | node id | none | Where keyboard and gamepad focus starts; see [Focus](#focus-and-nav-links). |

### Presentation

```ron
presentation: (mode: "modal", scrim: true, transition: "slide_up", back: "pop"),
```

| Field | Values | Default | |
|---|---|---|---|
| `mode` | `page`, `modal`, `overlay` | `page` | A `page` hides every stack entry below it and takes focus. A `modal` keeps the entries below visible, draws a scrim, traps focus and takes it. An `overlay` takes no focus, blocks no input and is not counted by `Back`. |
| `scrim` | bool | `true` for `modal`, else `false` | Draw the themed `scrim` role under the screen. |
| `transition` | `fade`, `slide_up`, `slide_left`, `none` | `fade` | The arrival motion, on the theme's motion tokens. Reduced motion collapses all of them to a fade. |
| `back` | `pop`, `ignore` | `pop` | What an unclaimed `Back` does to this screen when it is on top. |

### Focus and nav links

`initial_focus` names the node (by its `test_id`) that takes focus when the
screen opens: the node itself when it is focusable, else its first focusable
descendant, so naming a grid focuses its first slot. Without it, the first
focusable node in tree order takes focus. Only a `page` or a `modal` takes
focus, and only when it is the top of the stack.

Inside a container Bevy's directional navigator picks the neighbour. Between
containers, four reserved tags say where focus goes:

```ron
(type: "slot_grid", inventory: 0, cols: 9, rows: 3, first: 0,
 tags: {"test_id": "chest_grid", "nav.down": "player_grid"}),
```

`nav.up`, `nav.down`, `nav.left` and `nav.right` name a node id; on a
directional action the focused node and its ancestors up to the screen root
are searched for the first link in that direction, and focus lands on the
named node's first focusable descendant. A link whose id resolves to nothing
is logged once per spawn and falls through to the navigator. The whole input
side is in [input.md](input.md).

`ScreenDef::from_ron` parses through the untyped `Value` form, so `Option` fields
accept both `Some(x)` and a bare `x`. Files conventionally start with
`#![enable(implicit_some)]` so they can write the bare form.

## How a node is written

Every node is a map with a `type` key. There are no RON enum variants here: the
tree has to survive both an untyped `Value` and a Lua table, so it is internally
tagged. In Lua the same node is

```lua
{ type = "slot_grid", inventory = 0, cols = 9, rows = 3, first = 0,
  tags = { region = "chest" } }
```

`tags` is on every node except `anchor`. It is a string-to-string map, it is how
`slotted-test` and a mod's Lua tests find a node, and the reserved key `test_id`
becomes a `TestId` component. Give anything you will want to assert on a
`test_id`.

## Node types

### `panel`

A container. Everything else nests inside one.

| Field | Type | Default | |
|---|---|---|---|
| `role` | string | required | Theme role: `panel`, `invisible`, `tab.rail`, or one of your own. |
| `layout` | `Layout` | all defaults | See below. |
| `children` | array | `[]` | |

`Layout` maps onto Bevy's flex `Node`:

| Field | Type | Default | Node mapping |
|---|---|---|---|
| `direction` | `row`, `column` | `column` | `flex_direction` |
| `gap` | number, in steps | `0` | `row_gap`, `column_gap` |
| `padding` | number, or `(top, right, bottom, left)`, in steps | `0` | `padding` |
| `width`, `height` | `Length` | fit content | `width`, `height` |
| `min_width`, `max_width`, `min_height`, `max_height` | `Length` | none | `min_*`, `max_*` |
| `align` | `start`, `center`, `end`, `stretch` | `start` | `align_items` (the cross axis) |
| `justify` | `start`, `center`, `end`, `space_between` | `start` | `justify_content` (the main axis) |
| `grow` | number | `0` | `flex_grow` |
| `wrap` | bool | `false` | `flex_wrap` |
| `overflow` | `visible`, `scroll` | `visible` | `overflow`; `scroll` clips and adds a `ScrollPosition` |
| `place` | `(anchor: ..., offset: (x, y))` | none | absolute placement: `top_left` ... `bottom_right` or `center`, the nine HUD anchors, offset in pixels |
| `center` | bool | `false` | deprecated alias: `center: true` with `align` unset means `align: center` |

A `Length` is a bare number for pixels, or a string: `"50%"` of the parent,
`"fill"` (100%), `"auto"`, or `"3s"` for three spacing steps. `"12px"` is
rejected with a message naming the accepted forms.

Gaps, padding and step lengths are in spacing steps (`1.0` is `spacing.sm`)
rather than pixels, so a theme with a different spacing scale rescales a screen
it has never seen. `place` on a screen root's child is how a menu sits bottom
left with a logo top right without nesting three panels.

### `slot_grid`

The workhorse: a rectangle of slots backed by one inventory.

| Field | Type | |
|---|---|---|
| `inventory` | integer | Which inventory of the menu backs it. |
| `cols`, `rows` | integer | The shape. |
| `first` | integer | The first menu-wide slot index. The grid covers `first .. first + cols * rows`. |

With no `region` tag of its own the grid publishes one from `inventory`.

### `slot`

One slot, by menu-wide index. `slot` is the only field.

### `virtual_grid`

A grid whose contents come from a data source rather than an inventory, and
where only the visible rows exist as entities. For a list of thousands.

| Field | Type | Default | |
|---|---|---|---|
| `source` | `"namespace:path"` | required | A registered `VirtualGridSource`. |
| `cols` | integer | required | |
| `rows` | integer | `3` | Visible rows. |

### `text`

| Field | Type | Default | |
|---|---|---|---|
| `key` | string | required | A locale key. Shown verbatim when no locale resolves it. |
| `style` | string | required | `display`, `title`, `heading`, `body`, `muted`, `label`, `caption` or `count`, mapping to the theme roles `text.display`, `panel.title`, `text.heading`, `text`, `text.muted`, `text.label`, `text.caption` and `count`. |
| `args` | map of name to value | `{}` | Fluent arguments: `args: {"count": 3}` fills `{ $count }` in the string. A value is a bool, a number or a string. |
| `wrap` | bool | `true` | Wrap at the node's width. `false` is one line, however wide. |
| `align` | `left`, `center`, `right` | `left` | Line alignment. |
| `max_lines` | integer | none | Truncate with `…` once the laid-out paragraph passes this many lines. |

A `text` node is plain: markup tags in the string are shown as written. For
bold runs and key glyphs use `rich_text`.

### `rich_text`

A paragraph in the [rich-text markup](rich-text.md): `[b]`, `[i]`, `[color=..]`,
`[size=..]`, `{key:accept}`, `{icon:demo:chest}`.

| Field | Type | Default | |
|---|---|---|---|
| `key` | string | required | A locale key. The resolved string is the markup, so `.ftl` files may carry tags. |
| `style` | string | `body` | The base style, as for `text`. |
| `args`, `wrap`, `align` | | as for `text` | `max_lines` is accepted but ignored on a `rich_text`. |
| `inline` | bool | `false` | A single-line row whose `{icon:..}` runs are real images beside the text rather than item names in it. |

```ron
(type: "rich_text", key: "settings.footer", style: "caption",
 tags: {"test_id": "footer"}),
```

with `settings-footer = {key:back} back · {key:tab_next} next tab` in the
locale file renders `Esc back · E next tab` on a keyboard and `B back · RB next
tab` on a pad, re-rendering when the input mode flips or a binding changes.

### `button`

| Field | Type | Default | |
|---|---|---|---|
| `widget` | `"namespace:path"` | none | The behaviour: `slotted:sort` on a rail, `slotted:close` to pop the screen stack. Without one the button is a plain `Activate` source whose `tags` reach a mod's `widget_activate`. |
| `label` | locale key | none | The text. |
| `icon` | `IconDef` | none | An icon before the label. |
| `variant` | `primary`, `secondary`, `danger` | `secondary` | Which of `button`, `button.primary`, `button.danger` it paints in. |
| `disabled` | bool | `false` | Inert, and painted `.disabled`. |
| `compact` | bool | `false` | `control_height_compact` tall rather than `control_height`. |

A focused button activates on `Accept` from any device, and on a click. It does
not carry `bevy_ui_widgets::Button`, so a rebound `Accept` key reaches it.

### Value controls

The five controls below and `text_field`, `list` and `tabs` are views of a
value: each carries a binding, paints from it, and writes a `SetValue` when the
player acts. The store validates the write and the control repaints from
whatever the store decided, so a refused write snaps back with no code in the
control. [values.md](values.md) is the store's page.

Every one of them takes the same three binding fields:

| Field | Type | |
|---|---|---|
| `bind` | string | A `ValueStore` key, `"settings.ui_scale"`. |
| `property` | integer | A menu property instead. Only `Int` travels this way. |
| `disabled` | bool | Inert, and painted `.disabled`. |

A node with both `bind` and `property` is a load error naming the node. A node
with neither keeps its own state. Each control writes one `Value` type:

| Control | Writes |
|---|---|
| `toggle` | `Bool` |
| `slider` | `Float` |
| `select`, `radio_group` | `Text`, the option's `id` |
| `key_binding` | nothing; it edits `UiBindings` |
| `text_field` | `Text` |
| `list` | `Int`, the selected row |
| `tabs` | `Text`, the active tab's `id` |

A screen never writes a control's height. The theme's `sizes.control_height`
(44 in glass, 36 in paper, 40 in neon) and `control_height_compact` decide it,
so the same settings screen sits right in all three ([themes.md](themes.md)).

### `toggle`

A switch or a checkbox with a label on its left. `Accept` flips it; so does a
click anywhere on the row.

| Field | Type | Default | |
|---|---|---|---|
| `label` | locale key | none | |
| `style` | `switch`, `checkbox` | `switch` | A sliding thumb in `toggle` / `toggle.on`, or a square with a tick in `checkbox` / `checkbox.on`. |

### `slider`

A track with a fill, a thumb and a readout.

| Field | Type | Default | |
|---|---|---|---|
| `label` | locale key | none | |
| `min`, `max` | number | required | The range. |
| `step` | number | `0` | The grid. `0` is one percent of the range. |
| `format` | string | `"{value}"` | The readout: `{value}`, `{min}`, `{max}`, and `{value:.1}` for one decimal. |

`Left` and `Right` step, repeating while held; `PagePrev` and `PageNext` move a
tenth of the range. A press on the track jumps, a drag scrubs, and every move
is its own `SetValue`, so a guard sees each one.

### `select`

A pill showing the current option between two chevrons.

| Field | Type | Default | |
|---|---|---|---|
| `label` | locale key | none | |
| `options` | array of `(id: ..., label: ...)` | required | `id` is what is written; `label` is a locale key. |

`Left` and `Right` cycle in place, with wrap. In pointer mode `Accept` (or a
click on the pill) opens a popup of the options under the pill: `Up` and `Down`
move, `Accept` picks, `Back` closes the popup and is claimed, so the screen
stays open.

### `radio_group`

The same data as `select`, laid out as a row of segments (`radio`,
`radio.active`). `Left` and `Right` move and write at once, and stop at the
ends. A click on a segment picks it.

### `key_binding`

A row that rebinds one action on one device.

| Field | Type | Default | |
|---|---|---|---|
| `label` | locale key | the action's name | |
| `action` | `UiAction` name | required | `accept`, `back`, `tab_next`, ... |
| `device` | `keyboard`, `gamepad` | required | Which binding of the action. |
| `disabled` | bool | `false` | |

The cell shows the current binding as the same glyph text `{key:..}` uses.
`Accept` enters capture (the cell shows `…` in `key_binding.capturing`); the
next key or button replaces the action's first binding in `UiBindings` and
writes `BindingChanged`. `Back` cancels; Escape can never be bound. This node
has no `bind`: `UiBindings` is what it edits.

### `text_field`

| Field | Type | Default | |
|---|---|---|---|
| `label` | locale key | none | To the left of the frame. |
| `placeholder` | locale key | none | Shown while empty. |
| `filter` | `any`, `numeric`, `integer` | `any` | What typing accepts. |
| `max_len` | integer | none | Character cap. |

Focus does not edit: a focused field shows the ring, and `Accept` or a click
starts editing. Enter commits and writes; `Back` reverts to the bound value
and is claimed. A filtered field writes on every change, so a guard's clamp
shows live. A gamepad `Accept` writes `TextEntryRequested` for a game's own
on-screen keyboard and nothing else.

### `list`

A one-column `virtual_grid` whose rows are focusable.

| Field | Type | Default | |
|---|---|---|---|
| `source` | `"namespace:path"` | required | A registered `VirtualGridSource`. |
| `rows` | integer | `6` | Visible rows, each `control_height_compact` tall. |

`Up` and `Down` walk row to row and scroll the window. `Accept` (or a click)
selects the row, writes its index, and triggers `Activate` on the row with a
`row` tag carrying the index.

### `scroll`

A panel that clips and scrolls, with the wheel, with `PagePrev` / `PageNext`
from a focused descendant, and by following focus into view.

| Field | Type | Default | |
|---|---|---|---|
| `layout` | `Layout` | all defaults | `overflow` is forced to `scroll`. |
| `scrollbar` | bool | `true` | Draw `scroll.bar` and `scroll.thumb` beside the content. |
| `children` | array | `[]` | Direct children never shrink. |

### `tabs`

A tab bar over one page per tab. Exactly one page is visible; the others are
hidden and take no focus.

| Field | Type | Default | |
|---|---|---|---|
| `tabs` | array of `(id: ..., label: ..., icon: ...)` | required | `icon` is optional. |
| `children` | array | `[]` | One page per tab, in the same order. |

`TabPrev` and `TabNext` switch from anywhere on the screen and wrap; a click on
a tab, or `Accept` on a focused one, switches too. Focus moves into the new
page. `bind` carries the active tab's `id`.

### `separator`, `spacer`, `image`

Decoration: none is focusable, none is picked, and the harness ignores their
motion when settling.

| Node | Fields | |
|---|---|---|
| `separator` | `direction`: `row` (default) or `column` | A 1 px hairline in `separator` across the cross axis. |
| `spacer` | `size`: a `Length`, default `"fill"` | Empty space; `fill` grows to take what is left. |
| `image` | `path`, `width`, `height` | An image asset at a fixed size. |

### `tank`

A fluid tank bound to two menu properties.

| Field | Type | Default | |
|---|---|---|---|
| `property` | integer | required | The current amount. |
| `capacity` | integer | required | The capacity. |
| `orientation` | string | required | `"vertical"` fills upward, `"horizontal"` rightward. |
| `fluid` | id | none | A fixed fluid, by registry name. |
| `fluid_property` | integer | none | A property holding the frozen fluid id. Wins over `fluid`. |
| `unit` | string | `"mB"` | The unit in the label and tooltip. |

### `bar` and `progress`

Both bind a value and a maximum to a fill. `bar` is a resource meter; `progress`
is an arrow, using the `progress` theme roles instead of the `bar` ones.

| Field | Type | Default | |
|---|---|---|---|
| `property`, `max` | integer | required | The two menu properties. |
| `direction` | string | required | `right`, `left`, `up`, `down`. |
| `text` | bool | `false` | `bar` only: draw `value / max` on it. |

### `side_tab`

A tab that opens sideways out of a panel, for redstone modes and side config.

| Field | Type | Default | |
|---|---|---|---|
| `icon` | `IconDef` | required | `(image: "icons/redstone.png")` or `(item: "demo:chest")`. |
| `side` | string | required | `"left"` or `"right"`: which way it opens. |
| `label` | locale key | none | The header label. Falls back to the icon path. |
| `open` | bool | `false` | Start expanded. |
| `children` | array | `[]` | The contents. |

### `icon_button`

A button that cycles through named states on click.

| Field | Type | Default | |
|---|---|---|---|
| `states` | array | required, at least one | Each is `(id: ..., icon: ..., label: ...)`. |
| `property` | integer | none | Mirrors the state index into a menu property. |

The current state's `id` is published as a `state` tag and arrives in the
`widget_activate` event's `tags`, so a script reads it without counting clicks.

### `viewport`

A 3D view of something.

| Field | Type | Default | |
|---|---|---|---|
| `subject` | `ViewSubject` | required | `(player: ())`, `(item: "demo:chest")` or `(block: "demo:furnace")`. |
| `size` | number | `96.0` | Edge length in pixels. |

Needs the `viewport` feature on `slotted-ui` to spawn a real camera. Without it,
it lays out as an empty box, which is what the headless harness wants.

### Item icons

Two different things are called `IconDef`, and a screen file touches both.

The one above, on `side_tab` and `icon_button`, is `slotted_ui::IconDef`: what a
piece of chrome draws, either `(image: "icons/redstone.png")` or
`(item: "demo:chest")`, the second going through the `IconSource` like any
slot.

The other is `slotted_registry::icon::IconDef`, the `icon` field of an item, and
it is what the icon bake reads. It has three forms, told apart by their shape so
that RON, JSON and a Lua table all write the same thing:

| Written as | Means | Drawn as |
|---|---|---|
| `"icons/sword.png"` | an image path | the texture, loaded through the asset server |
| `(shape: "ingot", color: "#c9793f", metallic: 0.9)` | a lit primitive | a cell of the baked atlas |
| `(model: "models/anvil.gltf")` | a glTF model | nothing yet: the missing glyph, with one warning |
| the field absent | nothing declared | a cube in the item's own hash colour |

A shape is one of `cube`, `slab`, `ingot`, `gem`, `rod` or `sphere`, with
`color`, an optional second `accent` colour for a head or an inlay band, and
`metallic` and `roughness` in `0..=1` (out-of-range values clamp). Every shape
is lit by the same fixed three-point rig, so the icons in a screen read as one
set whoever declared them. A hovered item's tooltip shows the same shape turning
in a live viewport when the `live-icons` feature is on.

### `anchor`

An empty node that other mods splice into. `id` is its only field, and it is the
one node type with no `tags`.

An anchor costs nothing and is the difference between a screen a mod can extend
and one it cannot. Put one in every header and beside every grid.

`title_end` is the conventional name for the anchor at the end of a screen's
title row; the demo mods inject there. Anchor names are free strings, so pick
descriptive ones and document them with the screen. The menu templates name
theirs in [menus.md](menus.md#the-templates).

An injected node is a child of the anchor node, which is a plain flex row, so
it sizes to its content. A button that should be as wide as the column it
joins says so itself: wrap it in a `panel` with `width: "100%"` and a column
layout, as `examples/menus` does for its About button.

### `custom`

A registered widget kind.

| Field | Type | Default | |
|---|---|---|---|
| `kind` | `"namespace:path"` | required | The widget. |
| `params` | any value | unit | Interpreted by the widget. |
| `children` | array | `[]` | Handed to the widget. |

The built-in kinds are `slotted:panel`, `slotted:text`, `slotted:rich_text`,
`slotted:slot`, `slotted:slot_grid`, `slotted:button`, `slotted:close`,
`slotted:action_rail`, `slotted:hotbar`, `slotted:tooltip`, `slotted:tank`,
`slotted:bar`, `slotted:progress`, `slotted:side_tab`, `slotted:icon_button`,
`slotted:virtual_grid`, `slotted:viewport`, and one per M1 control
(`slotted:toggle`, `slotted:slider`, `slotted:select`, `slotted:radio_group`,
`slotted:key_binding`, `slotted:text_field`, `slotted:list`, `slotted:scroll`,
`slotted:tabs`, `slotted:separator`, `slotted:spacer`, `slotted:image`), whose
`params` are the typed node's fields. The two you write by hand most often are

```ron
(type: "custom", kind: "slotted:hotbar", params: (first: 54))
(type: "custom", kind: "slotted:action_rail",
 params: (actions: ["sort", "quick_stack", "deposit_all", "loot_all"],
          container: 0, player: 1))
```

A mod adds its own kinds with `slotted.register_widget`.

## Semantic roles

Every spawned node also carries a `SemanticRole`, which the widget picks; you do
not write it in the file. It is what `bevy_a11y` announces and what a locator
matches on: `Screen`, `Panel`, `Grid`, `Slot`, `Button`, `Text`, `Tooltip`,
`Rail`, `Hotbar`, `Anchor`, `Carried`, `Browser`, `TextField`, `Chip`, `Card`,
`RecipeView`, `RecipeSlot`, `Tab`, `Bookmark`, `Tank`, `Bar`, `SideTab`,
`Viewport`, `HudLayer`, the control roles `Toggle`, `Slider`, `Select`,
`RadioGroup`, `KeyBinding`, `List`, `ListItem` (a list row, or a select
popup's option), `ScrollView`, `Tabs`, `Decor` (a separator, spacer or image),
and `Custom(name)` for anything with no better fit.

## Injection

A screen a mod does not own is still extensible, because an anchor is addressed
by name:

```lua
slotted.inject("slotted:any", {
    anchor = "title_end",
    exclusion = true,
    node = { type = "custom", kind = "slotted:button",
             params = { label = "Sort" },
             tags = { my_action = "sort" } },
})
```

`"slotted:any"` matches every screen that has that anchor. `exclusion = true`
publishes the node's rectangle as an exclusion zone, so the item browser docks
clear of it rather than under it.

## Inheritance

A screen that names another in `inherits` starts from that screen's tree.
`Screens::resolve` flattens the chain when the screen spawns, so nothing else
in the crate has to know inheritance exists.

```ron
(
    kind: "copper:chest",
    inherits: "demo:chest",
    remove: ["hint"],
    root: (
        type: "panel", role: "panel", tags: {"test_id": "copper_panel"},
        children: [
            // Fills the ancestor's `(type: "anchor", id: "title_end")`,
            // wherever in its tree that anchor stands.
            (type: "text", key: "copper.badge", style: "muted",
             tags: {"test_id": "title_end"}),
            // A new id, so it is appended after the ancestor's children.
            (type: "text", key: "copper.footer", style: "muted",
             tags: {"test_id": "footer"}),
        ],
    ),
)
```

The merge identity is a node's **id**: its `test_id` tag, or, for an `anchor`,
the anchor's own id. One namespace on purpose -- that is what lets a child fill
an ancestor's anchor by naming it. The rules:

- A child node whose id names a node **anywhere** in the ancestor's tree
  replaces that node, subtree and all, in place.
- A child node with an id the ancestor does not have, or with no id at all, is
  appended under the ancestor node its own parent corresponds to.
- The child's root always wins on shape (role, layout, tags); its children merge
  onto the ancestor root's children.
- Ids in `remove` are deleted from the merged tree afterwards, at any depth.
- `kind` is always the child's. `listring` is the child's when non-empty,
  otherwise the ancestor's.

Chains are followed up to `MAX_INHERIT_DEPTH` (8). A cycle, an ancestor nobody
registered, or a chain past the limit logs one error naming the kinds involved
and falls back to the screen's own tree -- a broken data file costs you a plain
screen, never a panic.

Injections are still matched against the kind that opened, so an injection aimed
at `demo:chest` does not follow the tree into `copper:chest`; target
`slotted:any` to reach every screen with the anchor.

### Rewriting a definition by id

The same ids drive three methods on `ScreenDef` (menus M2), for code that
takes a registered screen and makes one instance of it: `set_text(id, key,
args)` rewrites a `text` or `rich_text` node's key and arguments, `set_tag(id,
tag, value)` sets a tag on any node but an anchor, and `remove_node(id)` drops
a node and its subtree (never the root). Each returns whether it found the
node. `UiNodeDef::find_mut(id)` is the lookup under them; it walks panels,
side tabs, scroll panels, tabs (a page's nodes are reachable, the `TabDef`s
are not) and custom nodes. `slotted-menu`'s `confirm` is the worked example:
it clones `slotted:confirm`, sets `title` and `message`, and removes whichever
accept button the dialog does not want.

```rust
let mut def = (**screens.get(&kinds::confirm()).unwrap()).clone();
def.set_text("message", LocKey("game.delete.message".into()), args);
def.remove_node("accept");
push_screen(&mut commands, Arc::new(def), None);
```

A button's label is not a text node; it is `ButtonOpts::label`, and
`set_text` does not reach it (`slotted_menu::confirm::set_button_label` does,
through `find_mut`). Folding that into `set_text` is on the follow-ups list.

## The screen stack

`ScreenStack` is the resource that knows what is open. A game pushes a screen
onto it and pops it off; the stack spawns and closes the tree, closes the
screen's menu on pop (the carried stack lands in `Dropped`, nothing is lost),
keeps z order, hides what a `page` covers, draws a `modal`'s scrim, keeps focus
inside the top entry and restores it when an entry regains the top.

```rust
use slotted::prelude::*;

fn open_chest(mut commands: Commands, mut ids: ResMut<MenuIdAllocator>, screens: Res<Screens>) {
    let def = screens.get(&ScreenKind::new("demo:chest")).cloned().unwrap();
    let menu = open_menu(&mut commands, &mut ids, menu_def, inventories, Actor::SURVIVAL);
    push_screen(&mut commands, def, Some(menu));
}

fn pause(mut commands: Commands, screens: Res<Screens>) {
    let def = screens.get(&ScreenKind::new("game:pause")).cloned().unwrap();
    // A menu-less screen: a modal over whatever is open.
    push_screen(&mut commands, def, None);
}

fn resume(mut commands: Commands) {
    pop_screen(&mut commands);
}
```

| | |
|---|---|
| `push_screen(commands, def, menu) -> Entity` | Resolves `def` through `Screens`, spawns it, records the entry, applies its presentation. |
| `replace_screen(commands, def, menu) -> Entity` | Pops the top non-overlay entry, then pushes. |
| `pop_screen(commands)` | Closes the top non-overlay entry and its menu. |
| `pop_to(commands, &kind)` | Pops until `kind` is on top, overlays included. A no-op when `kind` is not open. |
| `clear_screens(commands)` | Pops everything. |
| `ScreenStack::top()`, `top_any()`, `is_open(&kind)`, `kinds()`, `entries()` | What is open. `top()` skips overlays. |
| `StackChanged { kinds }` | A message written after every change, bottom to top. |

An unclaimed `Back` action (Escape, or East on a pad) pops the top entry when
its `back` policy is `pop`. That is the only Escape handling a game needs for
its screens; a widget that wants the press for itself claims it first
([input.md](input.md)).

`spawn_screen` and `close_screen` are still there and keep their signatures.
They are the low-level path the stack is built on: a screen spawned directly is
not a stack entry, `Back` leaves it alone, and the game handles its own close
as before. Mixing the two is safe in one direction: `close_screen` on a root
the stack holds removes its entry. The HUD's `hide_with_screen` layers hide
while any `page` or `modal` is on the stack, and while any screen exists
outside it.

Hot reload keeps a stacked screen stacked: when a `*.screen.ron` edit or a mod
reload respawns an open screen, the new tree goes back in at the same stack
position with the presentation the edited file declares.

## Loading one

A screen can be registered directly:

```rust
screens.register(ScreenDef::from_ron(&text)?);
```

but the usual way is the `*.screen.ron` asset loader, which is what gives a
screen file hot reload:

```rust
fn setup(assets: Res<AssetServer>, mut screens: ResMut<ScreenAssets>) {
    screens.load(&assets, "screens/demo_chest.screen.ron");
}
```

`ScreenAssets` is a list of handles and nothing more; holding the handle is what
keeps the file loaded and watched. `apply_screen_assets` does the rest: every
`Added` or `Modified` event registers the definition in `Screens` and, if a
screen of that kind is open right now, closes and re-opens it on the same menu
entity -- the same respawn path `slotted-packs` takes after a mod reload, so the
slots re-seed without an inventory write. A file that stops parsing leaves the
last good definition registered and the open screen alone.

Hot reload needs Bevy's watcher (`bevy/file_watcher`) enabled in the binary;
without it the assets still load, they just do not re-read on a save. Screens
registered by a mod's `data.lua` reload through the data stage instead; see
[hot-reload.md](hot-reload.md).
