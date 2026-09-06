-- hud_clock/data.lua: a HUD layer registered from Lua.
--
-- The point of this mod is that nothing in Rust knows it exists. The host
-- registers nine built-in layers (crosshair, hotbar, health and so on); this
-- adds a tenth from a mod's data stage, on top of them, and the position
-- editor treats it exactly like the built-ins: a player can drag it and where
-- they leave it is saved with the rest of the layout.
--
-- `tree` is a `UiNodeDef`, the same shape a screen file's nodes have.
-- `test_id` is how control.lua addresses the text node it wants to write, and
-- it is what a test locates the node by, which is the same thing twice on
-- purpose.

slotted.register_hud_layer("clock", {
    -- `offset` is a `Vec2`, which is a two-element list in data, not a table
    -- of `x` and `y`.
    anchor = { anchor = "top_right", offset = { -16.0, 16.0 } },
    tree = {
        type = "panel",
        role = "hud.panel",
        layout = { direction = "column", gap = 0.25, padding = 0.75 },
        tags = { test_id = "clock_panel" },
        children = {
            {
                type = "text",
                key = "hud_clock.label",
                style = "muted",
                tags = { test_id = "clock_label" },
            },
            {
                type = "text",
                key = "hud_clock.time",
                style = "title",
                tags = { test_id = "clock_time" },
            },
        },
    },
})
