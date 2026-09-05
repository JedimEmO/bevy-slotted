-- STAGE: data
-- EXPECT_ERROR: memory
-- Each entry is a distinct 100 KB string, so the 64 MiB state limit is
-- reached in a few hundred iterations, well inside the interrupt budget.
-- Repeating one identical string does not work: Luau does not allocate a
-- fresh buffer for it, and the loop hits the budget instead.
local held = {}
for i = 1, 100000 do
    held[i] = string.rep(tostring(i) .. "y", 100000)
end
