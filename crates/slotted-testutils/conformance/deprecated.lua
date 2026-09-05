-- STAGE: data
-- EXPECT: [
--   (type: "deprecated", call: "slotted.old_thing", since: 1, hint: "use slotted.register_item"),
--   (type: "log", level: "info", message: "once only"),
-- ]
-- The prelude reports a deprecated call once per name, whatever the caller
-- does afterwards.
slotted.__deprecated("slotted.old_thing", 1, "use slotted.register_item")
slotted.__deprecated("slotted.old_thing", 1, "use slotted.register_item")
slotted.info("once only")
