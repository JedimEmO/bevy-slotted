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

local mod_id = __slotted_mod_id
local stage = __slotted_stage
local api_version = __slotted_api_version or 1

local pending = {}      -- data-stage registrations, in call order
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

local function push_pending(cmd)
    pending[#pending + 1] = cmd
end

-- ---------------------------------------------------------------------------
-- data stage
-- ---------------------------------------------------------------------------

local function register(kind, what)
    return function(id, def)
        data_only(what)
        local full = namespaced(id)
        push_pending({ type = kind, id = full, def = with_name(full, def or {}) })
    end
end

slotted.register_item = register("register_item", "register_item")
slotted.register_tag = register("register_tag", "register_tag")
slotted.register_recipe_type = register("register_recipe_type", "register_recipe_type")
slotted.register_recipe = register("register_recipe", "register_recipe")

function slotted.register_screen(id, tree)
    data_only("register_screen")
    local full = namespaced(id)
    if type(tree) ~= "table" then
        error("slotted.register_screen: tree must be a table", 2)
    end
    if tree.kind == nil then
        tree.kind = full
    end
    push_pending({ type = "register_screen", id = full, def = tree })
end

function slotted.register_widget(id, template)
    data_only("register_widget")
    if type(template) ~= "table" then
        error("slotted.register_widget: template must be a table", 2)
    end
    push_pending({ type = "register_widget", id = namespaced(id), def = template })
end

function slotted.inject(screen_kind, spec)
    data_only("inject")
    if type(spec) ~= "table" or type(spec.anchor) ~= "string" or type(spec.node) ~= "table" then
        error("slotted.inject: expected { anchor = <string>, node = <table>, exclusion = <bool?> }", 2)
    end
    push_pending({
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

function slotted.add_tooltip_part(spec)
    data_only("add_tooltip_part")
    push_pending(tooltip_part(spec))
end

-- ---------------------------------------------------------------------------
-- control stage
-- ---------------------------------------------------------------------------

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

function cmd.sort(menu, inventory)
    return { type = "sort", menu = menu, inventory = inventory or 0 }
end

function cmd.quick_stack(menu, from, to)
    return { type = "quick_stack", menu = menu, from = from, to = to }
end

function cmd.move(menu, from, to)
    return { type = "move", menu = menu, from = from, to = to }
end

function cmd.toggle_favorite(menu, slot)
    return { type = "toggle_favorite", menu = menu, slot = slot }
end

function cmd.click(menu, action)
    return { type = "click", menu = menu, action = action }
end

function cmd.set_hud(layer, value)
    return { type = "set_hud", layer = layer, value = value }
end

function cmd.tooltip(nodes)
    return tooltip_part({ nodes = nodes })
end

function cmd.log(level, message)
    return { type = "log", level = level, message = tostring(message) }
end

-- ---------------------------------------------------------------------------
-- logging
-- ---------------------------------------------------------------------------

local log_buffer = {}

local function emit_log(level, fmt, ...)
    local message = fmt
    if select("#", ...) > 0 then
        message = string.format(fmt, ...)
    end
    log_buffer[#log_buffer + 1] = cmd.log(level, message)
end

function slotted.log(level, fmt, ...)
    emit_log(level, fmt, ...)
end

function slotted.info(fmt, ...) emit_log("info", fmt, ...) end
function slotted.warn(fmt, ...) emit_log("warn", fmt, ...) end
function slotted.error(fmt, ...) emit_log("error", fmt, ...) end

-- `print` goes to the console at info level.
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
        log_buffer[#log_buffer + 1] = { type = "deprecated", call = call, since = since, hint = hint }
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

local function drain_logs(out)
    for _, c in ipairs(log_buffer) do
        out[#out + 1] = c
    end
    log_buffer = {}
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
    -- Anything logged while the chunk itself ran comes first.
    drain_logs(out)
    local name = event.type
    if name == "data_stage" then
        for _, c in ipairs(pending) do
            out[#out + 1] = c
        end
        pending = {}
    elseif name == "control_start" then
        out[#out + 1] = { type = "subscribe", events = subscriptions() }
    end
    local list = handlers[name]
    if list then
        local first_error
        for _, handler in ipairs(list) do
            local ok, result = pcall(handler, event)
            if ok then
                append_result(out, result)
            elseif first_error == nil then
                first_error = result
            end
        end
        drain_logs(out)
        if first_error ~= nil then
            error(first_error, 0)
        end
    end
    drain_logs(out)
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
