-- copper_chest/data.lua: registries are open; only register here.
--
-- Everything this mod adds to the game is in this one file: two items, two
-- tags, a recipe type, a recipe and a screen. No Rust, no rebuild.

-- The chest itself. `uncommon` tints its name in tooltips and browser cards.
slotted.register_item("copper_chest", {
    display_name = "copper_chest.item.copper_chest",
    max_stack_size = 16,
    rarity = "uncommon",
    tags = { "c:storage", "c:chests" },
})

-- What it is built from. A second item so the recipe has an ingredient this
-- mod owns rather than borrowing the demo's.
slotted.register_item("copper_ingot", {
    display_name = "copper_chest.item.copper_ingot",
    max_stack_size = 64,
    tags = { "c:ingots" },
})

-- A tag this mod owns...
slotted.register_tag("c:chests", { values = { "copper_chest:copper_chest" } })
-- ...and one it only adds to: `assets/data/demo/tags/ingots.ron` already
-- registered `c:ingots`, and tags merge across mods.
slotted.register_tag("c:ingots", { values = { "copper_chest:copper_ingot" } })

-- A 3x3 crafting grid of its own, so the browser gets a tab for it.
slotted.register_recipe_type("assembly", {
    title_key = "copper_chest.recipe_type.assembly",
    size = { 3, 3 },
})

-- Eight ingots in a ring.
slotted.register_recipe("copper_chest", {
    recipe_type = "copper_chest:assembly",
    shape = { "iii", "i i", "iii" },
    key = { i = "#c:ingots" },
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
