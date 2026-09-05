-- STAGE: data
-- EXPECT: [
--   (type: "register_item", id: "test:apple", def: {"name": "test:apple"}),
--   (type: "log", level: "info", message: "after the registrations"),
-- ]
-- A data chunk may use `slotted.on("data_stage")`: the reply is the pending
-- registrations followed by what the handlers returned.
slotted.register_item("apple", {})
slotted.on("data_stage", function(ev)
    return slotted.cmd.log("info", "after the registrations")
end)
