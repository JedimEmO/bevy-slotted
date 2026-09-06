-- sorter/tests/sort.lua: run by `cargo xtask test-mods examples/machine/mods`
-- and by examples/machine/tests/ui.rs. Phase 6 contract section 3.1.

local t = slotted.test

t.test("the injected button sorts the machine's player inventory", function()
    -- Two stacks out of order: the sort puts the larger first.
    t.open_screen("machine:furnace", {
        slots = { 3, 27, 9 },
        fill = {
            ["1:0"] = { item = "demo:cobblestone", count = 3 },
            ["1:5"] = { item = "demo:cobblestone", count = 40 },
        },
    })
    t.click({ test_id = "sorter_sort" })
    t.settle()
    t.expect(t.log_contains("sorting"), "control.lua logs the sort")
    t.expect_stack({ role = "slot", tag = { region = "player" }, index = 0 }, "demo:cobblestone", 43)
end)
