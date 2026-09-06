-- sorter/data.lua: add a sort button to every container screen.
--
-- `slotted:any` is the wildcard injection target (Phase 6): the node lands on
-- every screen that has a `title_end` anchor, the machine's included. This
-- mod never sees those trees; it names the anchor and the host splices the
-- node in when the screen spawns. `exclusion = true` keeps the item browser
-- clear of it.

slotted.inject("slotted:any", {
    anchor = "title_end",
    exclusion = true,
    node = {
        type = "custom",
        kind = "slotted:button",
        params = { label = "Sort" },
        tags = { test_id = "sorter_sort", sorter_action = "sort" },
    },
})
