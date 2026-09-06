-- slotted.lua: the `slotted.*` API every mod script sees.
--
-- Installed by the host into a fresh, sandboxed state before the mod's chunk
-- runs. The host sets three globals first: `__slotted_mod_id`,
-- `__slotted_stage` ("data" | "control") and `__slotted_api_version`.
-- After the chunk has run, the host delivers events by calling
-- `__slotted_dispatch(event)` and receives an array of command tables.
-- Nothing here touches the host: every function only builds tables.
-- Contract: docs/design/phase4-contract.md section 1.4.
--
-- Plain Lua 5.1-compatible Luau: `table`, `string`, `math`, `pairs`,
-- `ipairs`, `type`, `error`, `pcall`, `setmetatable` only, so a pure-Rust
-- runtime with a partial stdlib can host it unchanged.

-- Every public function carries a `---` doc block. `cargo xtask gen-docs`
-- parses them into docs/guide/api/lua.md and `cargo xtask luau-stubs` into
-- docs/guide/api/slotted.d.luau, so the reference and the editor stubs cannot
-- drift from this file. Tags: @group, @signature, @stage, @param, @return,
-- @example, @luau.

--- @group Fields
--- The API version this host speaks. Compare it against your `mod.toml`
--- `api_version` if you need to branch on host age.
--- @signature slotted.api_version
--- @luau api_version: number
--- @stage any

--- Your own mod id, as written in `mod.toml`. Bare ids you pass to any
--- `register_*` call are namespaced with it.
--- @signature slotted.mod_id
--- @luau mod_id: string
--- @stage any

--- Which stage this chunk is running in: `"data"`, `"control"` or `"test"`.
--- @signature slotted.stage
--- @luau stage: string
--- @stage any

local mod_id = __slotted_mod_id
local stage = __slotted_stage
local api_version = __slotted_api_version or 1

-- Everything emitted outside a handler -- registrations, injections, tooltip
-- parts, log lines -- lands in one buffer, in call order, so a chunk that
-- registers an item and then logs about it produces the commands in that
-- order. Two buffers would lose the interleaving.
local emitted = {}      -- commands awaiting the next dispatch, in call order
local handlers = {}     -- event name -> array of functions
local warned = {}       -- deprecated call name -> true

local slotted = {}
slotted.api_version = api_version
slotted.mod_id = mod_id
slotted.stage = stage
slotted.cmd = {}

-- ---------------------------------------------------------------------------
-- helpers
-- ---------------------------------------------------------------------------

local function namespaced(id)
    if type(id) ~= "string" or id == "" then
        error("slotted: id must be a non-empty string", 3)
    end
    if string.find(id, ":", 1, true) then
        return id
    end
    return mod_id .. ":" .. id
end

local function with_name(id, def)
    if type(def) ~= "table" then
        error("slotted: def must be a table", 3)
    end
    if def.name == nil then
        def.name = id
    end
    return def
end

local function data_only(what)
    if stage ~= "data" then
        error("slotted." .. what .. " is only available in data.lua", 3)
    end
end

local function emit(cmd)
    emitted[#emitted + 1] = cmd
end

-- ---------------------------------------------------------------------------
-- data stage
-- ---------------------------------------------------------------------------

local function register(kind, what)
    return function(id, def)
        data_only(what)
        local full = namespaced(id)
        emit({ type = kind, id = full, def = with_name(full, def or {}) })
    end
end

--- @group Data stage
--- Add an item to the registry, or replace one another mod registered.
---
--- @signature slotted.register_item(id, def)
--- @stage data
--- @param id string An item id. A bare `"copper_ingot"` is namespaced with your mod id; `"other:thing"` is taken as written.
--- @param def table The item def. `display_name` is a locale key, `max_stack_size` a number, `rarity` one of the theme's rarity names, `tags` an array of tag ids, `components` a map of default component values.
--- @return nil
--- @luau register_item: (id: string, def: { [string]: any }) -> ()
--- @example
--- slotted.register_item("copper_ingot", {
---     display_name = "mymod.item.copper_ingot",
---     max_stack_size = 64,
---     tags = { "c:ingots" },
--- })
slotted.register_item = register("register_item", "register_item")

--- Add a tag, or add entries to one that already exists. Tags merge across
--- mods, so naming a tag another mod owns extends it rather than replacing it.
---
--- @signature slotted.register_tag(id, def)
--- @stage data
--- @param id string A tag id, conventionally in the `c:` namespace for cross-mod tags.
--- @param def table `values` is an array of item or tag ids; a leading `#` marks a nested tag. `replace = true` discards what other mods put in the tag first.
--- @return nil
--- @luau register_tag: (id: string, def: { [string]: any }) -> ()
--- @example
--- slotted.register_tag("c:ingots", { values = { "mymod:copper_ingot" } })
slotted.register_tag = register("register_tag", "register_tag")

--- Add a recipe type. The item browser gives each type its own tab.
---
--- @signature slotted.register_recipe_type(id, def)
--- @stage data
--- @param id string The recipe type id.
--- @param def table `title_key` is the locale key for the browser tab, `size` the `{ cols, rows }` of the crafting grid it draws.
--- @return nil
--- @luau register_recipe_type: (id: string, def: { [string]: any }) -> ()
--- @example
--- slotted.register_recipe_type("assembly", {
---     title_key = "mymod.recipe_type.assembly",
---     size = { 3, 3 },
--- })
slotted.register_recipe_type = register("register_recipe_type", "register_recipe_type")

--- Add a recipe.
---
--- @signature slotted.register_recipe(id, def)
--- @stage data
--- @param id string The recipe id. It does not have to match the result item.
--- @param def table `recipe_type` names a registered type. A shaped recipe carries `shape` (an array of row strings) and `key` (a map from shape character to an item id or `#tag`); a shapeless one carries `ingredients`. `result` is `{ item = ..., count = ... }`.
--- @return nil
--- @luau register_recipe: (id: string, def: { [string]: any }) -> ()
--- @example
--- slotted.register_recipe("copper_chest", {
---     recipe_type = "mymod:assembly",
---     shape = { "iii", "i i", "iii" },
---     key = { i = "#c:ingots" },
---     result = { item = "mymod:copper_chest", count = 1 },
--- })
slotted.register_recipe = register("register_recipe", "register_recipe")
-- Phase 6: fluids for tanks and HUD layers (contract 1.1, 2.1).

--- Add a fluid, so a `tank` node can name it and paint itself in its colour.
---
--- @signature slotted.register_fluid(id, def)
--- @stage data
--- @param id string The fluid id.
--- @param def table `display_name` is a locale key, `color` a `"#RRGGBB"` string, `density` a number (a negative one floats).
--- @return nil
--- @luau register_fluid: (id: string, def: { [string]: any }) -> ()
--- @example
--- slotted.register_fluid("molten_copper", {
---     display_name = "mymod.fluid.molten_copper",
---     color = "#C87137",
--- })
slotted.register_fluid = register("register_fluid", "register_fluid")


--- Add a HUD layer: a small tree drawn over the world rather than inside a
--- screen. The player can move it with the position editor in dev builds.
---
--- @signature slotted.register_hud_layer(id, def)
--- @stage data
--- @param id string The layer id.
--- @param def table `tree` is a `UiNodeDef` (required). `anchor` is a corner or edge name, `offset` a `{ x, y }` pair, `scale` a number, `visible` a boolean.
--- @return nil
--- @luau register_hud_layer: (id: string, def: { [string]: any }) -> ()
--- @example
--- slotted.register_hud_layer("mana", {
---     anchor = "bottom_left",
---     tree = { type = "bar", property = 0, max = 1, direction = "right",
---              tags = { test_id = "mana_bar" } },
--- })
function slotted.register_hud_layer(id, def)
    data_only("register_hud_layer")
    if type(def) ~= "table" or type(def.tree) ~= "table" then
        error("slotted.register_hud_layer: def must be a table with a tree", 2)
    end
    emit({ type = "register_hud_layer", id = namespaced(id), def = def })
end


--- Add a screen. `tree` is a `ScreenDef` body: the same shape a `.screen.ron`
--- file has, minus the `kind`, which is filled in from `id`.
---
--- @signature slotted.register_screen(id, tree)
--- @stage data
--- @param id string The screen kind.
--- @param tree table `root` is the `UiNodeDef` tree; `listring` is the array of inventory indices quick-move walks. See docs/guide/screens.md for every node type.
--- @return nil
--- @luau register_screen: (id: string, tree: { [string]: any }) -> ()
--- @example
--- slotted.register_screen("chest", {
---     root = {
---         type = "panel", role = "panel",
---         layout = { direction = "column", gap = 2, padding = 2 },
---         children = {
---             { type = "text", key = "mymod.chest.title", style = "title" },
---             { type = "anchor", id = "title_end" },
---             { type = "slot_grid", inventory = 0, cols = 9, rows = 3, first = 0 },
---         },
---     },
---     listring = { 0, 1 },
--- })
function slotted.register_screen(id, tree)
    data_only("register_screen")
    local full = namespaced(id)
    if type(tree) ~= "table" then
        error("slotted.register_screen: tree must be a table", 2)
    end
    if tree.kind == nil then
        tree.kind = full
    end
    emit({ type = "register_screen", id = full, def = tree })
end


--- Add a reusable node template. A `custom` node naming this id is replaced by
--- the template, with its own children spliced into the template's `children`
--- anchor.
---
--- @signature slotted.register_widget(id, template)
--- @stage data
--- @param id string The widget kind other screens name.
--- @param template table A `UiNodeDef`. Put an `{ type = "anchor", id = "children" }` node where the caller's children should land.
--- @return nil
--- @luau register_widget: (id: string, template: { [string]: any }) -> ()
--- @example
--- slotted.register_widget("framed", {
---     type = "panel", role = "panel",
---     layout = { direction = "column", padding = 1 },
---     children = { { type = "anchor", id = "children" } },
--- })
function slotted.register_widget(id, template)
    data_only("register_widget")
    if type(template) ~= "table" then
        error("slotted.register_widget: template must be a table", 2)
    end
    emit({ type = "register_widget", id = namespaced(id), def = template })
end


--- Splice a node into a screen at a named anchor, including a screen your mod
--- does not own. This is the whole extension story: name a screen kind and an
--- anchor, and the host does the splicing when that screen spawns.
---
--- @signature slotted.inject(screen_kind, spec)
--- @stage data
--- @param screen_kind string The screen to patch, or `"slotted:any"` for every screen that has the anchor.
--- @param spec table `anchor` is the anchor id (required), `node` the `UiNodeDef` to splice in (required), `exclusion = true` publishes the node as an exclusion zone so the item browser keeps clear of it.
--- @return nil
--- @luau inject: (screen_kind: string, spec: { [string]: any }) -> ()
--- @example
--- slotted.inject("slotted:any", {
---     anchor = "title_end",
---     exclusion = true,
---     node = { type = "custom", kind = "slotted:button",
---              params = { label = "Sort" },
---              tags = { my_action = "sort" } },
--- })
function slotted.inject(screen_kind, spec)
    data_only("inject")
    if type(spec) ~= "table" or type(spec.anchor) ~= "string" or type(spec.node) ~= "table" then
        error("slotted.inject: expected { anchor = <string>, node = <table>, exclusion = <bool?> }", 2)
    end
    emit({
        type = "inject",
        screen = screen_kind,
        anchor = spec.anchor,
        node = spec.node,
        exclusion = spec.exclusion == true,
    })
end

local function tooltip_part(spec)
    if type(spec) ~= "table" or type(spec.nodes) ~= "table" then
        error("slotted.add_tooltip_part: expected { nodes = {...}, items = {...}?, tags = {...}?, tier = <string>?, id = <string>? }", 3)
    end
    return {
        type = "add_tooltip_part",
        id = spec.id,
        when = { items = spec.items or {}, tags = spec.tags or {} },
        tier = spec.tier or "any",
        nodes = spec.nodes,
    }
end


--- Add nodes to the tooltip of every stack that matches a filter. Both filters
--- empty means every stack.
---
--- @signature slotted.add_tooltip_part(spec)
--- @stage data
--- @param spec table `nodes` is the array of `UiNodeDef`s to append (required). `items` and `tags` are the filters, `tier` is `"any"`, `"compact"` or `"expanded"`, `id` is a stable id so a hot reload replaces the part rather than adding a second one.
--- @return nil
--- @luau add_tooltip_part: (spec: { [string]: any }) -> ()
--- @example
--- slotted.add_tooltip_part({
---     id = "hunger",
---     tags = { "c:foods" },
---     tier = "expanded",
---     nodes = { { type = "text", key = "mymod.tooltip.hunger", style = "muted" } },
--- })
function slotted.add_tooltip_part(spec)
    data_only("add_tooltip_part")
    emit(tooltip_part(spec))
end

-- ---------------------------------------------------------------------------
-- control stage
-- ---------------------------------------------------------------------------


--- @group Control stage
--- Subscribe to a host event. The handler is called with the event table and
--- returns nothing, one command table, or an array of them. Several handlers
--- may share an event; they run in registration order.
---
--- A handler must not register anything. Registries are frozen by the time the
--- control stage runs.
---
--- @signature slotted.on(event_name, handler)
--- @stage control
--- @param event_name string One of `slot_click`, `widget_activate`, `tooltip_build`, `recipe_lookup`, `screen_opened`, `screen_closed`, `hud_tick`, `search_changed`, `property_changed`, `control_start`.
--- @param handler function Called as `handler(event)`.
--- @return nil
--- @luau on: (event_name: string, handler: (event: { [string]: any }) -> any) -> ()
--- @example
--- slotted.on("slot_click", function(ev)
---     if ev.modifiers.alt and ev.button == "left" then
---         return slotted.cmd.toggle_favorite(ev.menu, ev.slot)
---     end
--- end)
function slotted.on(event_name, handler)
    if type(event_name) ~= "string" then
        error("slotted.on: event name must be a string", 2)
    end
    if type(handler) ~= "function" then
        error("slotted.on: handler must be a function", 2)
    end
    local list = handlers[event_name]
    if not list then
        list = {}
        handlers[event_name] = list
    end
    list[#list + 1] = handler
end

local cmd = slotted.cmd


--- @group Commands
--- Sort one inventory of a menu. Becomes the same `MenuAction` the built-in
--- action rail sends, so prediction and the authority see an ordinary click.
---
--- @signature slotted.cmd.sort(menu, inventory)
--- @stage control
--- @param menu number The menu id from the event.
--- @param inventory number Which inventory within the menu. Defaults to `0`, the container.
--- @return table A command table.
--- @luau sort: (menu: number, inventory: number?) -> { [string]: any }
--- @example
--- return slotted.cmd.sort(ev.menu, 1)
function cmd.sort(menu, inventory)
    return { type = "sort", menu = menu, inventory = inventory or 0 }
end


--- Move every stack in `from` that already has a home in `to` across.
---
--- @signature slotted.cmd.quick_stack(menu, from, to)
--- @stage control
--- @param menu number The menu id.
--- @param from number Source inventory index.
--- @param to number Target inventory index.
--- @return table A command table.
--- @luau quick_stack: (menu: number, from: number, to: number) -> { [string]: any }
--- @example
--- return slotted.cmd.quick_stack(ev.menu, 1, 0)
function cmd.quick_stack(menu, from, to)
    return { type = "quick_stack", menu = menu, from = from, to = to }
end


--- Move a stack between two slots, as a plan of ordinary clicks.
---
--- @signature slotted.cmd.move(menu, from, to)
--- @stage control
--- @param menu number The menu id.
--- @param from number Source slot index, menu-wide.
--- @param to number Target slot index, menu-wide.
--- @return table A command table.
--- @luau move: (menu: number, from: number, to: number) -> { [string]: any }
--- @example
--- return slotted.cmd.move(ev.menu, ev.slot, 0)
function cmd.move(menu, from, to)
    return { type = "move", menu = menu, from = from, to = to }
end


--- Mark or unmark a slot as a favourite, which sorting keeps in place.
---
--- @signature slotted.cmd.toggle_favorite(menu, slot)
--- @stage control
--- @param menu number The menu id.
--- @param slot number Menu-wide slot index.
--- @return table A command table.
--- @luau toggle_favorite: (menu: number, slot: number) -> { [string]: any }
--- @example
--- return slotted.cmd.toggle_favorite(ev.menu, ev.slot)
function cmd.toggle_favorite(menu, slot)
    return { type = "toggle_favorite", menu = menu, slot = slot }
end


--- Send any click action verbatim. The escape hatch, for the modes the other
--- commands do not cover.
---
--- @signature slotted.cmd.click(menu, action)
--- @stage control
--- @param menu number The menu id.
--- @param action table A `ClickAction`, tagged the way the model serialises it.
--- @return table A command table.
--- @luau click: (menu: number, action: { [string]: any }) -> { [string]: any }
--- @example
--- return slotted.cmd.click(ev.menu, { type = "quick_move", slot = ev.slot })
function cmd.click(menu, action)
    return { type = "click", menu = menu, action = action }
end


--- Replace parts of a HUD layer. An unknown layer id creates one on top.
---
--- @signature slotted.cmd.set_hud(layer, value)
--- @stage control
--- @param layer string The layer id.
--- @param value table Any of `tree`, `anchor`, `offset`, `scale`, `visible`. Whatever you leave out keeps its current value.
--- @return table A command table.
--- @luau set_hud: (layer: string, value: { [string]: any }) -> { [string]: any }
--- @example
--- return slotted.cmd.set_hud("mymod:mana", { visible = false })
function cmd.set_hud(layer, value)
    return { type = "set_hud", layer = layer, value = value }
end

-- Phase 6: write one value into a HUD node by its test_id.

--- Write one value into a HUD layer's node, found by its `test_id`. A string
--- sets text, a boolean sets visibility, a number or a `{ value, max }` pair
--- sets a fill. Cheaper than replacing the tree every tick.
---
--- @signature slotted.cmd.hud_update(layer, path, value)
--- @stage control
--- @param layer string The layer id.
--- @param path string The target node's `test_id`.
--- @param value any A string, a boolean, a number, or `{ value = n, max = m }`.
--- @return table A command table.
--- @luau hud_update: (layer: string, path: string, value: any) -> { [string]: any }
--- @example
--- return slotted.cmd.hud_update("mymod:mana", "mana_bar", { value = 30, max = 100 })
function cmd.hud_update(layer, path, value)
    return { type = "hud_update", layer = layer, path = path, value = value }
end


--- Append nodes to the tooltip being composed. Only valid in reply to a
--- `tooltip_build` event, where the filters of a static part make no sense.
---
--- @signature slotted.cmd.tooltip(nodes)
--- @stage control
--- @param nodes table An array of `UiNodeDef`s.
--- @return table A command table.
--- @luau tooltip: (nodes: { any }) -> { [string]: any }
--- @example
--- slotted.on("tooltip_build", function(ev)
---     if ev.tier ~= "expanded" then return nil end
---     return slotted.cmd.tooltip({
---         { type = "text", key = "mymod.tooltip.detail", style = "muted" },
---     })
--- end)
function cmd.tooltip(nodes)
    return tooltip_part({ nodes = nodes })
end


--- Build a console line as a command, for when you want it ordered with the
--- rest of a handler's reply rather than emitted immediately.
---
--- @signature slotted.cmd.log(level, message)
--- @stage any
--- @param level string `"trace"`, `"debug"`, `"info"`, `"warn"` or `"error"`.
--- @param message any Anything; it is passed through `tostring`.
--- @return table A command table.
--- @luau log: (level: string, message: any) -> { [string]: any }
--- @example
--- return { slotted.cmd.log("warn", "nothing to sort"), slotted.cmd.sort(ev.menu, 1) }
function cmd.log(level, message)
    return { type = "log", level = level, message = tostring(message) }
end

-- ---------------------------------------------------------------------------
-- logging
-- ---------------------------------------------------------------------------

local function emit_log(level, fmt, ...)
    local message = fmt
    if select("#", ...) > 0 then
        message = string.format(fmt, ...)
    end
    emit(cmd.log(level, message))
end


--- @group Logging
--- Write a console line immediately. Extra arguments are formatted into `fmt`
--- with `string.format`.
---
--- @signature slotted.log(level, fmt, ...)
--- @stage any
--- @param level string `"trace"`, `"debug"`, `"info"`, `"warn"` or `"error"`.
--- @param fmt string A message, or a `string.format` pattern.
--- @param ... any Format arguments.
--- @return nil
--- @luau log: (level: string, fmt: string, ...any) -> ()
--- @example
--- slotted.log("debug", "menu %d has %d slots", ev.menu, 63)
function slotted.log(level, fmt, ...)
    emit_log(level, fmt, ...)
end


--- `slotted.log("info", ...)`.
---
--- @signature slotted.info(fmt, ...)
--- @stage any
--- @param fmt string A message, or a `string.format` pattern.
--- @param ... any Format arguments.
--- @return nil
--- @luau info: (fmt: string, ...any) -> ()
--- @example
--- slotted.info("opened %s", ev.screen)

--- `slotted.log("warn", ...)`.
---
--- @signature slotted.warn(fmt, ...)
--- @stage any
--- @param fmt string A message, or a `string.format` pattern.
--- @param ... any Format arguments.
--- @return nil
--- @luau warn: (fmt: string, ...any) -> ()
--- @example
--- slotted.warn("sort pressed outside a menu")

--- `slotted.log("error", ...)`. Logging an error does not stop the script; to
--- fail, call Lua's `error`.
---
--- @signature slotted.error(fmt, ...)
--- @stage any
--- @param fmt string A message, or a `string.format` pattern.
--- @param ... any Format arguments.
--- @return nil
--- @luau error: (fmt: string, ...any) -> ()
--- @example
--- slotted.error("recipe %s has no result", id)
function slotted.info(fmt, ...) emit_log("info", fmt, ...) end
function slotted.warn(fmt, ...) emit_log("warn", fmt, ...) end
function slotted.error(fmt, ...) emit_log("error", fmt, ...) end

-- `print` goes to the console at info level.

--- Print to the script console at info level. Arguments are joined with tabs.
--- There is no `io` and no real stdout in a sandboxed script; this is it.
---
--- @signature print(...)
--- @stage any
--- @param ... any Anything; each argument is passed through `tostring`.
--- @return nil
--- @luau print: (...any) -> ()
--- @example
--- print("loaded", slotted.mod_id)
function print(...)
    local parts = {}
    for i = 1, select("#", ...) do
        parts[i] = tostring((select(i, ...)))
    end
    emit_log("info", table.concat(parts, "\t"))
end

-- Called by the prelude itself when a shimmed call is used.
local function deprecated(call, since, hint)
    if not warned[call] then
        warned[call] = true
        emit({ type = "deprecated", call = call, since = since, hint = hint })
    end
end
slotted.__deprecated = deprecated

-- ---------------------------------------------------------------------------
-- dispatch
-- ---------------------------------------------------------------------------

local function append_result(out, result)
    if result == nil then
        return
    end
    if type(result) ~= "table" then
        error("slotted: a handler must return nil, a command table or an array of them", 0)
    end
    if result.type ~= nil then
        out[#out + 1] = result
        return
    end
    for _, c in ipairs(result) do
        out[#out + 1] = c
    end
end

local function drain(out)
    for _, c in ipairs(emitted) do
        out[#out + 1] = c
    end
    emitted = {}
end

local function subscriptions()
    local names = {}
    for name in pairs(handlers) do
        names[#names + 1] = name
    end
    table.sort(names)
    return names
end

function __slotted_dispatch(event)
    local out = {}
    local name = event.type
    -- Phase 6: the test stage answers its three events itself (slotted_test.lua).
    local test = rawget(slotted, "test")
    if test ~= nil then
        if name == "test_list" then
            return { test.__list() }
        elseif name == "test_run" then
            return { test.__run(event.name) }
        elseif name == "test_resume" then
            return { test.__resume(event.value, event.error) }
        end
    end
    if name == "control_start" then
        out[#out + 1] = { type = "subscribe", events = subscriptions() }
    end
    -- Whatever the chunk itself emitted, in the order it emitted it. At the
    -- data stage that is the registration list the contract asks for.
    drain(out)
    local list = handlers[name]
    if list then
        local first_error
        for _, handler in ipairs(list) do
            local ok, result = pcall(handler, event)
            -- A handler logs before it returns, so its log lines come first.
            drain(out)
            if ok then
                append_result(out, result)
            elseif first_error == nil then
                first_error = result
            end
        end
        if first_error ~= nil then
            error(first_error, 0)
        end
    end
    drain(out)
    return out
end

-- Freeze the API table: a mod that assigns into `slotted` gets an error
-- rather than a silently different API for the next mod.
setmetatable(slotted, {
    __newindex = function(_, key)
        error("slotted." .. tostring(key) .. " is read-only", 2)
    end,
})

_G.slotted = slotted
