-- STAGE: data
-- EXPECT: [
--   (type: "register_recipe_type", id: "test:assembly", def: {"name": "test:assembly", "width": 2, "height": 2}),
-- ]
slotted.register_recipe_type("assembly", { width = 2, height = 2 })
