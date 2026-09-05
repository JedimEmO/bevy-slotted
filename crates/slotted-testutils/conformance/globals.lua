-- STAGE: data
-- EXPECT: [
--   (type: "log", level: "info", message: "test data 1"),
-- ]
slotted.info("%s %s %d", slotted.mod_id, slotted.stage, slotted.api_version)
