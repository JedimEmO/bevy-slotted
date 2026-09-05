-- STAGE: control
-- EVENTS: [ (type: "control_start", api_version: 1, mods: ["test"]) ]
-- EXPECT_ERROR: budget
slotted.on("control_start", function()
    while true do end
end)
