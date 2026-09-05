-- STAGE: data
-- EXPECT: [
--   (type: "register_item", id: "test:t", def: {
--     "name": "test:t",
--     "array": [1, 2, 3],
--     "map": {"a": 1, "b": 2},
--     "empty": [],
--     "nested": [[1, 2], []]
--   }),
-- ]
-- `1..n` and nothing else is a list; anything else is a map; an empty table
-- is an empty list.
slotted.register_item("t", {
    array = { 1, 2, 3 },
    map = { a = 1, b = 2 },
    empty = {},
    nested = { { 1, 2 }, {} },
})
