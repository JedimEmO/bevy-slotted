-- STAGE: data
-- EXPECT_ERROR: runtime
-- A wrong argument type raises a runtime error naming the function; the
-- adapter test asserts the message text.
slotted.register_item("apple", "not a table")
