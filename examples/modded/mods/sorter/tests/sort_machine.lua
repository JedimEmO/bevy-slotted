-- sorter/tests/sort_machine.lua: the same button on a screen this mod has
-- never seen. Run by `cargo xtask test-mods examples/modded/mods` and by
-- `examples/machine/tests/ui.rs`. Phase 6 contract section 3.1.
--
-- It lives beside sort.lua rather than inside it because the two need
-- different screens open, and the playground's Tests tab runs the first file
-- in the directory against whatever the canvas already shows.

local t = slotted.test

t.test("the injected button sorts the machine's player inventory", function()
    -- Two stacks out of order: the sort puts the larger first.
    t.open_screen("machine:furnace", {
        slots = { 3, 27, 9 },
        fill = {
            ["1:0"] = { item = "minecraft:cobblestone", count = 3 },
            ["1:5"] = { item = "minecraft:cobblestone", count = 40 },
        },
    })
    t.click({ test_id = "sorter_sort" })
    t.settle()
    t.expect(t.log_contains("sorting"), "control.lua logs the sort")
    t.expect_stack({ role = "slot", tag = { region = "player" }, index = 0 }, "minecraft:cobblestone", 43)
end)
