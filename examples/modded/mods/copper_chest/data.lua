-- copper_chest/data.lua: registries are open; only register here.

slotted.register_item("copper_chest", {
    display_name = "copper_chest.item.copper_chest",
    max_stack_size = 16,
    tags = { "c:storage" },
})

slotted.register_tag("c:storage", { values = { "copper_chest:copper_chest" } })

slotted.register_recipe_type("assembly", {
    title_key = "copper_chest.recipe_type.assembly",
    size = { 2, 2 },
})

slotted.register_recipe("copper_chest", {
    recipe_type = "copper_chest:assembly",
    shape = { "ii", "ii" },
    key = { i = "#demo:ingots" },
    result = { item = "copper_chest:copper_chest", count = 1 },
})

-- The demo chest tree with a localised title and an anchor other mods can
-- inject into. Same shape as assets/screens/demo_chest.screen.ron.
slotted.register_screen("chest", {
    root = {
        type = "panel",
        role = "panel",
        layout = { direction = "column", gap = 2, padding = 2 },
        tags = { test_id = "chest_panel" },
        children = {
            {
                type = "panel",
                role = "invisible",
                layout = { direction = "row", gap = 2, center = true },
                tags = { test_id = "header" },
                children = {
                    { type = "text", key = "copper_chest.screen.title", style = "title", tags = { test_id = "title" } },
                    { type = "anchor", id = "title_end" },
                },
            },
            {
                type = "panel",
                role = "invisible",
                layout = { direction = "row", gap = 2 },
                tags = { test_id = "body" },
                children = {
                    {
                        type = "panel",
                        role = "invisible",
                        layout = { direction = "column", gap = 1 },
                        tags = { test_id = "grids" },
                        children = {
                            { type = "slot_grid", inventory = 0, cols = 9, rows = 3, first = 0, tags = { region = "chest", test_id = "chest_grid" } },
                            { type = "text", key = "copper_chest.screen.inventory", style = "muted", tags = { test_id = "inventory_label" } },
                            { type = "slot_grid", inventory = 1, cols = 9, rows = 3, first = 27, tags = { region = "player", test_id = "player_grid" } },
                            { type = "custom", kind = "slotted:hotbar", params = { first = 54, inventory = 2 }, tags = { test_id = "hotbar" } },
                        },
                    },
                    {
                        type = "custom",
                        kind = "slotted:action_rail",
                        params = { actions = { "sort", "quick_stack", "deposit_all", "loot_all" }, container = 0, player = 1 },
                        tags = { test_id = "rail" },
                    },
                },
            },
        },
    },
    listring = { 0, 1 },
})
