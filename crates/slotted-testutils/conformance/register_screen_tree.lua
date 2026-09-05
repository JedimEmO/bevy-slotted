-- STAGE: data
-- EXPECT: [
--   (type: "register_screen", id: "test:chest", def: {
--     "kind": "test:chest",
--     "root": {
--       "type": "panel",
--       "children": [
--         {"type": "text", "key": "test.screen.title"},
--         {"type": "anchor", "id": "title_end"},
--         {"type": "grid", "inventory": 0, "columns": 9, "rows": 3}
--       ]
--     }
--   }),
-- ]
-- A nested tree survives the bridge with its arrays as arrays and its
-- string-keyed tables as maps.
slotted.register_screen("chest", {
    root = {
        type = "panel",
        children = {
            { type = "text", key = "test.screen.title" },
            { type = "anchor", id = "title_end" },
            { type = "grid", inventory = 0, columns = 9, rows = 3 },
        },
    },
})
