-- sorter/data.lua: add a sort button to somebody else's screen.
--
-- `slotted:any` is the wildcard injection target (Phase 6): the node lands on
-- every screen that has a `title_end` anchor, the copper chest's and the
-- machine's furnace alike. This mod never sees either tree; it names the
-- anchor and the host splices the node in when the screen spawns.
-- `exclusion = true` marks the spawned node an exclusion zone, so the item
-- browser docks clear of it instead of covering it.
--
-- The node is a `slotted:button` rather than the plain `button` node, because
-- only the widget form carries a label. The tags come back on the
-- `widget_activate` event, which is how control.lua knows the press was this
-- button and not the action rail's own sort.

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
