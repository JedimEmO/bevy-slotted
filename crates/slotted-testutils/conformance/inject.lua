-- STAGE: data
-- EXPECT: [
--   (type: "inject", screen: "slotted:chest", anchor: "title_end", node: {"type": "text", "key": "test.badge"}, exclusion: false),
--   (type: "inject", screen: "slotted:chest", anchor: "left_gutter", node: {"type": "panel"}, exclusion: true),
-- ]
slotted.inject("slotted:chest", {
    anchor = "title_end",
    node = { type = "text", key = "test.badge" },
})
slotted.inject("slotted:chest", {
    anchor = "left_gutter",
    node = { type = "panel" },
    exclusion = true,
})
