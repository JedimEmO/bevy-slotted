-- STAGE: test
-- EVENTS: [
--   (type: "test_list"),
--   (type: "test_run", name: "the machine cooks"),
--   (type: "test_resume", value: {"menu": 2, "screen": 3}),
--   (type: "test_resume", value: {"done": true}),
--   (type: "test_resume", value: {"item": "demo:apple", "count": 3}),
-- ]
-- EXPECT: [
--   (type: "test_list", names: ["the machine cooks"]),
--   (type: "test_step", op: {"op": "open_screen", "kind": "test:furnace", "fixture": "empty"}),
--   (type: "test_step", op: {"op": "click", "loc": {"test_id": "start"}}),
--   (type: "test_step", op: {"op": "stack_at", "loc": {"role": "slot", "index": 0}}),
--   (type: "test_done", name: "the machine cooks", passed: true, message: None),
-- ]
-- The test stage's whole protocol in one case: registration, the coroutine
-- yielding one op per action, and the host's answers coming back as the
-- action's return value. Phase 6 contract section 3.1.
local t = slotted.test

t.test("the machine cooks", function()
    local opened = t.open_screen("test:furnace")
    t.expect_eq(opened.menu, 2, "the host's menu comes back")
    t.click({ test_id = "start" })
    t.expect_stack({ role = "slot", index = 0 }, "demo:apple", 3)
end)
