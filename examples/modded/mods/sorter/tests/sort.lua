-- sorter/tests/sort.lua: what the playground's Tests tab runs.
--
-- These run against the live app, so `open_screen` means "the screen you have
-- open" and the fixture is ignored: the playground opens the copper chest at
-- start and this checks the injected button on it. Phase 6 contract 3.2. The
-- fixture is written out anyway, so the same file passes under a headless
-- `UiHarness`, where `open_screen` really does open one.

local t = slotted.test

local CHEST = {
    slots = { 27, 27, 9 },
    fill = {
        ["0:0"] = { item = "copper_chest:copper_ingot", count = 3 },
        ["0:5"] = { item = "copper_chest:copper_ingot", count = 40 },
    },
}

t.test("the injected sort button is on the chest screen", function()
    t.open_screen("copper_chest:chest", CHEST)
    t.expect(t.is_visible({ test_id = "sorter_sort" }), "the injected button is on screen")
end)

t.test("pressing it sorts the chest", function()
    t.open_screen("copper_chest:chest", CHEST)
    t.click({ test_id = "sorter_sort" })
    t.settle()
    t.expect(t.log_contains("sorting"), "control.lua logs the sort")
end)
