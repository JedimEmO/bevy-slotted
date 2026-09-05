-- STAGE: data
-- EXPECT: [
--   (type: "add_tooltip_part", id: Some("test:food"), when: (items: [], tags: ["c:foods"]), tier: "any", nodes: [
--     {"type": "text", "key": "test.food", "style": "muted"}
--   ]),
--   (type: "add_tooltip_part", id: None, when: (items: ["demo:apple"], tags: []), tier: "expanded", nodes: [
--     {"type": "text", "key": "test.apple"}
--   ]),
-- ]
slotted.add_tooltip_part({
    id = "test:food",
    tags = { "c:foods" },
    nodes = { { type = "text", key = "test.food", style = "muted" } },
})
slotted.add_tooltip_part({
    items = { "demo:apple" },
    tier = "expanded",
    nodes = { { type = "text", key = "test.apple" } },
})
