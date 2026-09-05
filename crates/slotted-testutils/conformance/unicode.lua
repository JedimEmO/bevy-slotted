-- STAGE: data
-- EXPECT: [
--   (type: "register_item", id: "test:kupfer", def: {"name": "test:kupfer", "label": "Kupferkiste 家 🧰"}),
--   (type: "log", level: "info", message: "bytes 20"),
-- ]
-- Strings cross the bridge as bytes and come back byte-identical; Lua's `#`
-- counts bytes, not code points, which is why the length is 20.
local label = "Kupferkiste 家 🧰"
slotted.register_item("kupfer", { label = label })
slotted.info("bytes %d", #label)
