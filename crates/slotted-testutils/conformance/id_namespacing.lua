-- STAGE: data
-- EXPECT: [
--   (type: "register_item", id: "test:copper", def: {"name": "test:copper"}),
--   (type: "register_item", id: "demo:apple", def: {"name": "demo:apple"}),
--   (type: "register_item", id: "test:named", def: {"name": "kept"}),
-- ]
-- A bare id gets `mod_id ..":"`; one that already has a namespace is left
-- alone; an explicit `name` is not overwritten.
slotted.register_item("copper", {})
slotted.register_item("demo:apple", {})
slotted.register_item("named", { name = "kept" })
