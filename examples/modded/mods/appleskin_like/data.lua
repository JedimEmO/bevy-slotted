-- appleskin_like/data.lua: data-only. A food item, a food tag, and one
-- tooltip line for everything in the tag. No control.lua.

slotted.register_item("demo:apple", {
    display_name = "appleskin_like.item.apple",
    max_stack_size = 64,
    tags = { "c:foods" },
})

slotted.register_tag("c:foods", { values = { "demo:apple" } })

slotted.add_tooltip_part({
    id = "appleskin_like:food",
    tags = { "c:foods" },
    tier = "any",
    nodes = {
        { type = "text", key = "appleskin_like.food", style = "muted" },
    },
})
