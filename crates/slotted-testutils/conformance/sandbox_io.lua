-- STAGE: data
-- EXPECT_ERROR: runtime
-- `io` is nil in the sandbox, so indexing it is a runtime error.
io.write("nope")
