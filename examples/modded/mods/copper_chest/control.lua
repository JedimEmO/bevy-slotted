-- copper_chest/control.lua: registries are frozen; react to events.

slotted.on("slot_click", function(ev)
    local what = ev.stack and (ev.stack.item .. " x" .. ev.stack.count) or "empty"
    slotted.info("slot %d clicked (%s): %s", ev.slot, ev.button, what)
    if ev.modifiers.alt and ev.button == "left" then
        return slotted.cmd.toggle_favorite(ev.menu, ev.slot)
    end
end)

slotted.on("screen_opened", function(ev)
    slotted.info("opened %s", ev.screen)
end)
