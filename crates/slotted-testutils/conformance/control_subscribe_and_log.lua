-- STAGE: control
-- EVENTS: [
--   (type: "control_start", api_version: 1, mods: ["test"]),
--   (type: "slot_click", menu: 1, screen: "test:chest", slot: 3, button: "left", modifiers: (shift: false, ctrl: false, alt: true), stack: None),
-- ]
-- EXPECT: [
--   (type: "log", level: "info", message: "clicked 3"),
--   (type: "toggle_favorite", menu: 1, slot: 3),
-- ]
slotted.on("slot_click", function(ev)
    slotted.info("clicked %d", ev.slot)
    if ev.modifiers.alt then
        return slotted.cmd.toggle_favorite(ev.menu, ev.slot)
    end
end)
