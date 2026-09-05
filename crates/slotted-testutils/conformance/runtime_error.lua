-- STAGE: control
-- EVENTS: [
--   (type: "control_start", api_version: 1, mods: ["test"]),
--   (type: "search_changed", text: "x"),
-- ]
-- EXPECT_ERROR: runtime
-- An error inside a handler surfaces as a runtime error with a traceback.
slotted.on("search_changed", function()
    local t = nil
    return t.missing
end)
