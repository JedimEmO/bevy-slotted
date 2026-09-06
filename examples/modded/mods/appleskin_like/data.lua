-- appleskin_like/data.lua: data-only registration. A food item, a food tag,
-- and one static tooltip line for everything in the tag.
--
-- The static part is all data: no script runs while a tooltip is built. See
-- control.lua for the line that does need one.

slotted.register_item("appleskin_like:apple", {
    display_name = "appleskin_like.item.apple",
    max_stack_size = 64,
    tags = { "c:foods" },
    icon = { shape = "sphere", color = "#d2382f", accent = "#5fbf4a", roughness = 0.45 },
})

slotted.register_tag("c:foods", { values = { "appleskin_like:apple" } })

slotted.add_tooltip_part({
    id = "appleskin_like:food",
    tags = { "c:foods" },
    tier = "any",
    nodes = {
        { type = "text", key = "appleskin_like.food", style = "muted" },
    },
})
