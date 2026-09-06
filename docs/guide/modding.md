# Writing a mod

A mod is a directory with a manifest and one or two Lua files. No Rust, no
rebuild, and the same mod runs natively and in a browser tab.

The full function reference is [api/lua.md](api/lua.md), generated from the
prelude. This page is the shape of the thing.

## The layout

```
mods/
└── my_mod/
    ├── mod.toml
    ├── data.lua
    ├── control.lua
    ├── data/          RON defs, merged with what data.lua registers
    ├── locale/
    │   └── en.ftl
    ├── icons/
    └── tests/
        └── sort.lua
```

```toml
# mod.toml
id = "my_mod"
name = "My Mod"
version = "0.1.0"
api_version = 1

[entry]
data = "data.lua"
control = "control.lua"
```

`id` is your namespace. Every bare id you pass to a `register_*` call is
prefixed with it, so `register_item("copper_ingot", ...)` registers
`my_mod:copper_ingot`. Write a namespace explicitly when you mean another mod's.

## The two stages

This is the one thing to understand before writing anything.

**`data.lua` runs while the registries are open.** Register items, tags,
recipes, recipe types, fluids, screens, widgets, HUD layers, tooltip parts and
injections here. Nothing has an id yet; everything is a string.

Then the registries **freeze**. Every string id becomes a dense handle, tags
resolve, and the recipe index is built. After that nothing can be registered,
ever, for the lifetime of the load.

**`control.lua` runs after the freeze.** It subscribes to events and answers
them with commands. It cannot register anything; calling a `register_*` function
here raises.

That split is why a lookup in a running game is an array index instead of a hash
of a string, and why a screen full of slots costs nothing to search.

## data.lua

```lua
slotted.register_item("copper_ingot", {
    display_name = "my_mod.item.copper_ingot",
    max_stack_size = 64,
    tags = { "c:ingots" },
    -- A lit primitive, baked into the icon atlas. No texture to ship.
    icon = { shape = "ingot", color = "#c9793f", metallic = 0.9 },
})

slotted.register_item("copper_chest", {
    display_name = "my_mod.item.copper_chest",
    max_stack_size = 16,
    rarity = "uncommon",
    tags = { "c:storage", "c:chests" },
    -- Or a texture of your own, by path: icon = "icons/copper_chest.png"
    icon = { shape = "cube", color = "#b06a3c", accent = "#e8c49a" },
})

-- Tags merge across mods. Naming one another mod owns extends it.
slotted.register_tag("c:ingots", { values = { "my_mod:copper_ingot" } })

slotted.register_recipe_type("assembly", {
    title_key = "my_mod.recipe_type.assembly",
    size = { 3, 3 },
})

slotted.register_recipe("copper_chest", {
    recipe_type = "my_mod:assembly",
    shape = { "iii", "i i", "iii" },
    key = { i = "#c:ingots" },
    result = { item = "my_mod:copper_chest", count = 1 },
})
```

An item's `icon` is one of three things: a path string for a texture you ship, a
table with a `shape` key for a lit primitive the icon bake renders, or a table
with a `model` key for a glTF file, which parses and warns until the loader
lands. The shapes are `cube`, `slab`, `ingot`, `gem`, `rod` and `sphere`; each
takes `color`, an optional `accent` and `metallic` and `roughness` in `0..=1`.
An item with no `icon` still gets a cube in a colour derived from its id, so a
mod never shows a grid of missing textures. See
[screens](screens.md) for the whole table.

A leading `#` means "this is a tag, not an item", so the recipe accepts any
ingot any mod put in `c:ingots`.

Registering a screen is the same idea, with a tree instead of a def. The tree is
exactly the shape a `.screen.ron` file has; see [screens.md](screens.md) for
every node type.

```lua
slotted.register_screen("chest", {
    root = {
        type = "panel", role = "panel",
        layout = { direction = "column", gap = 2, padding = 2 },
        children = {
            { type = "text", key = "my_mod.chest.title", style = "title" },
            { type = "anchor", id = "title_end" },
            { type = "slot_grid", inventory = 0, cols = 9, rows = 3, first = 0,
              tags = { region = "chest" } },
            { type = "slot_grid", inventory = 1, cols = 9, rows = 3, first = 27,
              tags = { region = "player" } },
            { type = "custom", kind = "slotted:hotbar", params = { first = 54 },
              tags = { region = "hotbar" } },
        },
    },
    listring = { 0, 1 },
})
```

## control.lua

```lua
slotted.on("slot_click", function(ev)
    local what = ev.stack and (ev.stack.item .. " x" .. ev.stack.count) or "empty"
    slotted.info("slot %d clicked (%s): %s", ev.slot, ev.button, what)

    if ev.modifiers.alt and ev.button == "left" then
        return slotted.cmd.toggle_favorite(ev.menu, ev.slot)
    end
end)
```

A handler returns nothing, one command table, or an array of them. Several
handlers may share an event and run in registration order. An error in one
handler does not stop the others; it is reported after they have all run.

### The events

| Event | Fires when |
|---|---|
| `data_stage` | The data chunk has run. You normally do not subscribe; registering is enough. |
| `control_start` | The control chunk has run. The mod list is in `ev.mods`. |
| `slot_click` | A pointer click on a slot completed. Carries `menu`, `screen`, `slot`, `button`, `modifiers`, `stack`. |
| `widget_activate` | A button or custom widget fired. Carries `menu`, `screen`, `widget`, `tags`. |
| `tooltip_build` | A tooltip is being composed. Carries `stack` and `tier`. |
| `recipe_lookup` | The browser opened a recipe page. Carries `item` and `mode`. |
| `screen_opened`, `screen_closed` | A screen's tree appeared or is about to go. |
| `hud_tick` | Periodically, if the host enables it. Carries `elapsed_ms`. |
| `search_changed` | The browser's search text changed. |
| `property_changed` | A synced menu property changed. Carries `property` and `value`. |

### The commands

`slotted.cmd.sort`, `quick_stack`, `move`, `toggle_favorite` and `click` all
become the same `MenuAction` a human click produces. There is no second code
path: prediction, the conservation check and the authority see your command the
way they see a player's.

`slotted.cmd.set_hud`, `hud_update`, `tooltip` and `log` are the non-inventory
ones.

A script cannot write a slot. It can only ask, and the host decides. That is
what lets the same mod run on a dedicated server and in a browser tab without
either trusting it.

## Extending a screen you do not own

This is the part that makes a mod ecosystem, and it is four lines.

```lua
-- data.lua
slotted.inject("slotted:any", {
    anchor = "title_end",
    exclusion = true,
    node = { type = "custom", kind = "slotted:button",
             params = { label = "Sort" },
             tags = { my_action = "sort" } },
})
```

```lua
-- control.lua
slotted.on("widget_activate", function(ev)
    if ev.tags.my_action ~= "sort" then return nil end
    if ev.menu == nil then
        slotted.warn("sort pressed outside a menu")
        return nil
    end
    return slotted.cmd.sort(ev.menu, 1)
end)
```

`"slotted:any"` is the wildcard: the button lands on every screen in the game
that has a `title_end` anchor, including screens from mods loaded after yours.
`exclusion = true` publishes the button's rectangle so the item browser docks
clear of it.

`widget_activate` fires for every widget on every screen, so filter on a tag of
your own rather than on the widget kind. That is what `my_action` is for.

## Tooltips

A static part, filtered at the data stage:

```lua
slotted.add_tooltip_part({
    id = "hunger",
    tags = { "c:foods" },
    tier = "expanded",
    nodes = { { type = "text", key = "my_mod.tooltip.hunger", style = "muted" } },
})
```

Or dynamically, when the text depends on the stack:

```lua
slotted.on("tooltip_build", function(ev)
    if ev.tier ~= "expanded" then return nil end
    return slotted.cmd.tooltip({
        { type = "text", key = "my_mod.tooltip.count", style = "muted" },
    })
end)
```

Prefer the static form. It is filtered before a tooltip is built, so it costs
nothing on the stacks it does not apply to.

## Localisation

`locale/en.ftl` is Fluent, and every `key` in a `text` node and every
`display_name` is looked up in it.

```
my_mod-item-copper_ingot = Copper Ingot
my_mod-chest-title = Copper Chest
```

Locale files layer the same way data does, so a resource pack can retranslate
your mod without touching it.

## The sandbox

A script has `table`, `string`, `math`, `pairs`, `ipairs`, `type`, `error`,
`pcall` and `setmetatable`. It has no `io`, no `os`, no `require` and no
filesystem. `print` goes to the script console.

There is one script runtime, luaur, and it is the same one in a browser and on a
desktop, so the standard library is the same and your mod behaves identically on
both.

Every call is budgeted. A script that loops forever is stopped and reported
rather than hanging the game.

### One difference in a browser

In a browser the game runs as a WebAssembly module, and there a raised Lua error
ends the module rather than being caught: the host has to build a new one and
put the game back where it was. Two consequences for a mod:

- `pcall` does not protect you on the web. It works on a desktop and does
  nothing in a browser, so do not use it to recover from an error you expect;
  check the value instead. (There is a test for both halves of that: the
  desktop behaviour is pinned in `slotted-script-luaur`, and the browser
  behaviour is why its wasm test file has no error cases in it.)
- An `error(...)` you raise on purpose is a crash a visitor sees, not a line in
  the console. Use `slotted.warn` for something that went wrong but is
  survivable, and keep `error` for a bug.

[ADR 0004](../adr/0004-web-runtime-luaur.md) has the reasoning and what a host
has to do about it.

## Testing it

Put tests in `tests/*.lua` and run them headless:

```
cargo xtask test-mods mods
```

See [testing.md](testing.md).

## Editor support

`cargo xtask luau-stubs` writes
[`docs/guide/api/slotted.d.luau`](api/slotted.d.luau). Point luau-lsp at it and
you get completion and type errors over `data.lua`, `control.lua` and your
tests.

## Hot reload

Edit a file and it reloads, keeping open screens and their contents. See
[hot-reload.md](hot-reload.md) for what survives and what does not.
