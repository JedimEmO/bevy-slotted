-- STAGE: data
-- EXPECT: [
--   (type: "register_widget", id: "test:framed", def: {
--     "type": "panel",
--     "style": "framed",
--     "children": [{"type": "anchor", "id": "children"}]
--   }),
-- ]
-- A widget template is a `UiNodeDef` whose `children` anchor receives the
-- caller's nodes; the prelude does not fill in a `name`.
slotted.register_widget("framed", {
    type = "panel",
    style = "framed",
    children = { { type = "anchor", id = "children" } },
})
