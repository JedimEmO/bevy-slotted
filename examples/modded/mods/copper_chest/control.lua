-- copper_chest/control.lua: registries are frozen; react to events.
--
-- A control script may not register anything. It answers events with
-- commands, and the host validates every one before applying it, so the
-- worst a broken mod can do here is log nonsense.

slotted.on("slot_click", function(ev)
    local what = ev.stack and (ev.stack.item .. " x" .. ev.stack.count) or "empty"
    slotted.info("slot %d clicked (%s): %s", ev.slot, ev.button, what)
    -- Alt+left over a slot marks it a favourite. The command becomes a
    -- `MenuAction`, so prediction and the authority see an ordinary click.
    if ev.modifiers.alt and ev.button == "left" then
        return slotted.cmd.toggle_favorite(ev.menu, ev.slot)
    end
end)

slotted.on("screen_opened", function(ev)
    slotted.info("opened %s", ev.screen)
end)
