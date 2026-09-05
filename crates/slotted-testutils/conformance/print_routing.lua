-- STAGE: data
-- EXPECT: [
--   (type: "log", level: "info", message: "hello\t7"),
-- ]
-- `print` is kept but routed to a Log{Info} command.
print("hello", 7)
