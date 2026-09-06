-- appleskin_like/control.lua: the per-item half of the tooltip.
--
-- The static part in data.lua says "Food"; how much a particular food
-- restores depends on the item, so this handler picks the key. A control
-- script sees the stack, not the tag index, so the table is explicit.

local restores = {
    ["appleskin_like:apple"] = "appleskin_like.food.apple",
}

slotted.on("tooltip_build", function(ev)
    local key = restores[ev.stack.item]
    if key == nil then
        return nil
    end
    return slotted.cmd.tooltip({
        { type = "text", key = key, style = "muted" },
    })
end)
