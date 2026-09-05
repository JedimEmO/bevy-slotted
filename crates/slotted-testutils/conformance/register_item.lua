-- STAGE: data
-- EXPECT: [
--   (type: "register_item", id: "test:apple", def: {"name": "test:apple", "max_stack_size": 16}),
-- ]
slotted.register_item("apple", { max_stack_size = 16 })
