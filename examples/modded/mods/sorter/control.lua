-- sorter/control.lua: what the injected button does.
--
-- `widget_activate` fires for every widget in a screen, so the handler filters
-- on the tag it put on its own node. The reply is a `sort` command, which the
-- host turns into the same `MenuAction` the built-in action rail sends: one
-- code path, one prediction, one conservation rule.
--
-- Which inventory to sort is the one thing that cannot be the same everywhere.
-- On a chest, inventory 0 is the chest and sorting it is the whole point. On
-- the machine, inventory 0 is the three machine slots (input, fuel, output)
-- and sorting those would be nonsense, so the button takes the player's
-- pockets instead. The mod decides from the screen it was pressed on, which is
-- the only thing it knows about a tree it has never seen.

local PLAYER_INVENTORY_SCREENS = {
    ["machine:furnace"] = 1,
}

slotted.on("widget_activate", function(ev)
    if ev.tags.sorter_action ~= "sort" then
        return nil
    end
    if ev.menu == nil then
        slotted.warn("sort pressed outside a menu")
        return nil
    end
    local inventory = PLAYER_INVENTORY_SCREENS[ev.screen] or 0
    slotted.info("sorting inventory %d of menu %d", inventory, ev.menu)
    return slotted.cmd.sort(ev.menu, inventory)
end)
