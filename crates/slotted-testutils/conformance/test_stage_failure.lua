-- STAGE: test
-- EVENTS: [
--   (type: "test_run", name: "an expectation that does not hold"),
--   (type: "test_resume", value: false),
--   (type: "test_run", name: "no such test"),
-- ]
-- EXPECT: [
--   (type: "test_step", op: {"op": "is_visible", "loc": {"test_id": "output"}}),
--   (type: "test_done", name: "an expectation that does not hold", passed: false, message: Some("the output slot is on screen")),
--   (type: "test_done", name: "no such test", passed: false, message: Some("no test named no such test")),
-- ]
-- A failing expectation is a `test_done` with the message, not a script
-- error: one bad test must not take the file's other tests down with it.
-- Asking for a test that was never registered fails the same way.
local t = slotted.test

t.test("an expectation that does not hold", function()
    t.expect(t.is_visible({ test_id = "output" }), "the output slot is on screen")
end)
