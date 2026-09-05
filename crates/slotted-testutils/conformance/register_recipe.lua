-- STAGE: data
-- EXPECT: [
--   (type: "register_recipe", id: "test:copper_chest", def: {
--     "name": "test:copper_chest",
--     "recipe_type": "test:assembly",
--     "ingredients": [{"tag": "demo:ingots", "count": 4}],
--     "result": {"item": "test:copper_chest", "count": 1}
--   }),
-- ]
slotted.register_recipe("copper_chest", {
    recipe_type = "test:assembly",
    ingredients = { { tag = "demo:ingots", count = 4 } },
    result = { item = "test:copper_chest", count = 1 },
})
