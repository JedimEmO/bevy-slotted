# Screens

A screen is data. `ScreenDef` holds a tree of `UiNodeDef`s, and `spawn_screen`
is the one place that turns that tree into entities. The same tree comes from a
Rust struct literal, a `.screen.ron` file or a Lua table, so a mod can build a
screen the engine has never heard of without a rebuild.

## The file

```ron
#![enable(implicit_some)]
(
    kind: "demo:chest",
    root: (
        type: "panel",
        role: "panel",
        layout: (direction: "column", gap: 2.0, padding: 2.0),
        children: [
            (type: "text", key: "demo.chest.title", style: "title"),
            (type: "anchor", id: "title_end"),
            (type: "slot_grid", inventory: 0, cols: 9, rows: 3, first: 0,
             tags: {"region": "chest"}),
            (type: "slot_grid", inventory: 1, cols: 9, rows: 3, first: 27,
             tags: {"region": "player"}),
            (type: "custom", kind: "slotted:hotbar", params: (first: 54),
             tags: {"region": "hotbar"}),
        ],
    ),
    listring: [0, 1],
)
```

`ScreenDef` has five fields.

| Field | Type | Default | |
|---|---|---|---|
| `kind` | `"namespace:path"` | required | The screen's id. A mod injects by naming it. |
| `root` | `UiNodeDef` | required | The tree. |
| `inherits` | `"namespace:path"` | none | A base screen to start from; see [Inheritance](#inheritance). |
| `listring` | array of inventory indices | `[]` | The order quick-move walks. Mirrors `MenuDef::listring`. |
| `remove` | array of node ids | `[]` | Nodes to delete from the inherited tree. Only meaningful with `inherits`. |

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

`Layout` is `direction` (`"row"` or `"column"`, default `column`), `gap` and
`padding` (in spacing steps, where `1.0` is `spacing.sm`), `width` and `height`
(fixed pixels; omit to fit content), and `center` (centre children on the cross
axis).

Sizes are in spacing steps rather than pixels so a theme with a different
spacing scale rescales a screen it has never seen.

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

| Field | Type | |
|---|---|---|
| `key` | string | A locale key. Shown verbatim when no locale resolves it. |
| `style` | string | `title`, `body`, `muted` or `count`, mapping to the theme roles `panel.title`, `text`, `text.muted` and `count`. |

### `button`

| Field | Type | |
|---|---|---|
| `widget` | `"namespace:path"` | The behaviour, `slotted:sort` or `slotted:close`. |

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
descriptive ones and document them with the screen.

### `custom`

A registered widget kind.

| Field | Type | Default | |
|---|---|---|---|
| `kind` | `"namespace:path"` | required | The widget. |
| `params` | any value | unit | Interpreted by the widget. |
| `children` | array | `[]` | Handed to the widget. |

The built-in kinds are `slotted:panel`, `slotted:text`, `slotted:slot`,
`slotted:slot_grid`, `slotted:button`, `slotted:action_rail`, `slotted:hotbar`,
`slotted:tooltip`, `slotted:tank`, `slotted:bar`, `slotted:progress`,
`slotted:side_tab`, `slotted:icon_button`, `slotted:virtual_grid` and
`slotted:viewport`. The two you write by hand most often are

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
`Viewport`, `HudLayer`, and `Custom(name)` for anything with no better fit.

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
