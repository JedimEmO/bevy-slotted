-- STAGE: control
-- EVENTS: [
--   (type: "control_start", api_version: 1, mods: ["test", "other"]),
--   (type: "slot_click", menu: 2, screen: "test:chest", slot: 7, button: "right", modifiers: (shift: true, ctrl: false, alt: false), stack: Some((item: "demo:apple", count: 3, components: {"damage": 1}))),
--   (type: "widget_activate", menu: Some(2), screen: "test:chest", widget: "test:sort_button", tags: {"role": "sort"}),
--   (type: "tooltip_build", stack: (item: "demo:apple", count: 1, components: {}), tier: "expanded"),
--   (type: "recipe_lookup", item: "demo:apple", mode: "uses"),
--   (type: "screen_opened", menu: Some(2), screen: "test:chest"),
--   (type: "screen_closed", menu: None, screen: "test:chest"),
--   (type: "hud_tick", elapsed_ms: 250),
--   (type: "search_changed", text: "app"),
-- ]
-- EXPECT: [
--   (type: "log", level: "info", message: "start test,other"),
--   (type: "log", level: "info", message: "click 7 right shift demo:apple x3"),
--   (type: "log", level: "info", message: "widget test:sort_button role=sort menu=2"),
--   (type: "add_tooltip_part", id: None, when: (items: [], tags: []), tier: "any", nodes: [{"type": "text", "key": "test.tip.expanded"}]),
--   (type: "log", level: "info", message: "lookup demo:apple uses"),
--   (type: "log", level: "info", message: "opened test:chest 2"),
--   (type: "log", level: "info", message: "closed test:chest none"),
--   (type: "log", level: "info", message: "tick 250"),
--   (type: "log", level: "info", message: "search app"),
-- ]
-- One handler per event variant, so every event shape is exercised through
-- the bridge: optional menus, an optional stack, a string map of tags.
slotted.on("control_start", function(ev)
    slotted.info("start %s", table.concat(ev.mods, ","))
end)

slotted.on("slot_click", function(ev)
    local shift = "noshift"
    if ev.modifiers.shift then shift = "shift" end
    slotted.info("click %d %s %s %s x%d", ev.slot, ev.button, shift, ev.stack.item, ev.stack.count)
end)

slotted.on("widget_activate", function(ev)
    slotted.info("widget %s role=%s menu=%d", ev.widget, ev.tags.role, ev.menu)
end)

slotted.on("tooltip_build", function(ev)
    return slotted.cmd.tooltip({ { type = "text", key = "test.tip." .. ev.tier } })
end)

slotted.on("recipe_lookup", function(ev)
    slotted.info("lookup %s %s", ev.item, ev.mode)
end)

slotted.on("screen_opened", function(ev)
    slotted.info("opened %s %d", ev.screen, ev.menu)
end)

slotted.on("screen_closed", function(ev)
    local menu = "none"
    if ev.menu ~= nil then menu = tostring(ev.menu) end
    slotted.info("closed %s %s", ev.screen, menu)
end)

slotted.on("hud_tick", function(ev)
    slotted.info("tick %d", ev.elapsed_ms)
end)

slotted.on("search_changed", function(ev)
    slotted.info("search %s", ev.text)
end)
