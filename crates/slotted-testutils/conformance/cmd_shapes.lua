-- STAGE: control
-- EVENTS: [
--   (type: "control_start", api_version: 1, mods: ["test"]),
--   (type: "search_changed", text: "go"),
-- ]
-- EXPECT: [
--   (type: "sort", menu: 1, inventory: 0),
--   (type: "quick_stack", menu: 1, from: 0, to: 1),
--   (type: "move", menu: 1, from: 3, to: 4),
--   (type: "toggle_favorite", menu: 1, slot: 5),
--   (type: "click", menu: 1, action: Pickup(slot: 3, button: Left)),
--   (type: "set_hud", layer: "test:counter", value: {"n": 3}),
--   (type: "add_tooltip_part", id: None, when: (items: [], tags: []), tier: "any", nodes: [{"type": "text", "key": "test.tip"}]),
--   (type: "log", level: "debug", message: "done"),
-- ]
slotted.on("search_changed", function()
    return {
        slotted.cmd.sort(1),
        slotted.cmd.quick_stack(1, 0, 1),
        slotted.cmd.move(1, 3, 4),
        slotted.cmd.toggle_favorite(1, 5),
        slotted.cmd.click(1, { Pickup = { slot = 3, button = "Left" } }),
        slotted.cmd.set_hud("test:counter", { n = 3 }),
        slotted.cmd.tooltip({ { type = "text", key = "test.tip" } }),
        slotted.cmd.log("debug", "done"),
    }
end)
