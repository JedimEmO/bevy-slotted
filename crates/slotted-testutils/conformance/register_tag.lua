-- STAGE: data
-- EXPECT: [
--   (type: "register_tag", id: "c:foods", def: {"name": "c:foods", "values": ["demo:apple", "demo:bread"]}),
-- ]
slotted.register_tag("c:foods", { values = { "demo:apple", "demo:bread" } })
