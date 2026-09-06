-- sorter/control.lua: what the injected button does.
--
-- `widget_activate` fires for every widget in a screen, so the handler filters
-- on the tag it put on its own node. The reply is a `sort` command, which the
-- host turns into the same `MenuAction` the built-in action rail sends: one
-- code path, one prediction, one conservation rule.

slotted.on("widget_activate", function(ev)
    if ev.tags.sorter_action ~= "sort" then
        return nil
    end
    if ev.menu == nil then
        slotted.warn("sort pressed outside a menu")
        return nil
    end
    -- Inventory 1, not 0: the machine's own three slots are inventory 0 and
    -- the player's pockets are 1, which is the half a sort button is for.
    slotted.info("sorting the player inventory of menu %d", ev.menu)
    return slotted.cmd.sort(ev.menu, 1)
end)
