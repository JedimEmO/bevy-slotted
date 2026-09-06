-- hud_clock/control.lua: what keeps the clock moving.
--
-- `hud_tick` is the host's periodic event, fired only when the game asked for
-- it (`PacksConfig::hud_tick`). The handler answers with a `hud_update`
-- command naming the layer, the node's `test_id` and the new text, which the
-- host turns into a `slotted_ui::HudUpdate` message applied on the next render
-- pass. The mod never touches an entity.
--
-- The clock is a game clock, not the wall clock: `elapsed_ms` is virtual time
-- since the world started, so a paused game has a stopped clock and a test
-- that steps twenty frames sees a number it can predict.

local MINUTES_PER_DAY = 24 * 60
--- How many game minutes one real second is worth. Sixty, so a game day is
--- twenty-four real seconds and the clock visibly moves while you watch it.
local MINUTES_PER_SECOND = 60

slotted.on("hud_tick", function(ev)
    local minutes = math.floor(ev.elapsed_ms / 1000 * MINUTES_PER_SECOND) % MINUTES_PER_DAY
    local text = string.format("%02d:%02d", math.floor(minutes / 60), minutes % 60)
    return slotted.cmd.hud_update("hud_clock:clock", "clock_time", text)
end)
