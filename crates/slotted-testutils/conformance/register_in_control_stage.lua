-- STAGE: control
-- EXPECT_ERROR: runtime
-- `register_*` outside the data stage raises rather than silently building a
-- command the host would have to reject.
slotted.register_item("apple", {})
