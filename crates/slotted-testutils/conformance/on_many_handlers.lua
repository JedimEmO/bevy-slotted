-- STAGE: control
-- EVENTS: [
--   (type: "control_start", api_version: 1, mods: ["test"]),
--   (type: "search_changed", text: "x"),
-- ]
-- EXPECT: [
--   (type: "log", level: "info", message: "first"),
--   (type: "log", level: "warn", message: "second"),
--   (type: "log", level: "error", message: "third-a"),
--   (type: "log", level: "error", message: "third-b"),
-- ]
-- Handlers run in registration order; nil, one command and an array of them
-- are all valid replies.
slotted.on("search_changed", function()
    return slotted.cmd.log("info", "first")
end)
slotted.on("search_changed", function()
    return { slotted.cmd.log("warn", "second") }
end)
slotted.on("search_changed", function()
    return nil
end)
slotted.on("search_changed", function()
    return {
        slotted.cmd.log("error", "third-a"),
        slotted.cmd.log("error", "third-b"),
    }
end)
