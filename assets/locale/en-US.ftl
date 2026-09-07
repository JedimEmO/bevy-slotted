# Base pack strings. A mod's own locale/<lang>.ftl layers over this one.
#
# The machine example's screen (examples/machine/screens/furnace.screen.ron)
# is host content rather than a mod's, so its keys live here. Fluent ids use
# `-`, and `normalise_id` is what lets a screen file write `machine.title`.

machine-title = Alloy Furnace
machine-tab-redstone = Redstone control
machine-tab-sides = Side configuration
machine-redstone-ignore = Ignore redstone
machine-redstone-low = Run without a signal
machine-redstone-high = Run with a signal
machine-face-none = Face: closed
machine-face-input = Face: input
machine-face-output = Face: output

# The item browser's own chrome. `slotted-browser` resolves each of these
# through `slotted_ui::Localization` and falls back to the same English when
# no catalogue is loaded, so a game with no locale files is unchanged.
# Category chips and recipe tabs are not here: they resolve the category's own
# `RecipeTypeDef::title_key`, or `category-<ns>-<path>` when it declares none.

browser-search-placeholder = Search items
browser-search-label = Search
browser-status-indexing = indexing…
browser-status-count = { $count ->
    [one] { $count } item
   *[other] { $count } items
}
browser-status-hints = R recipes / U uses / A bookmark
browser-uses-title = Used in:
browser-uses-empty = Used in: nothing yet
browser-button-transfer = +
browser-button-back = <
browser-button-forward = >

# Action rail buttons (the widget falls back to these English labels).
slotted-rail-sort = Sort
slotted-rail-quick_stack = Quick stack
slotted-rail-deposit_all = Deposit all
slotted-rail-loot_all = Loot all

# The settings demo (assets/screens/demo_settings.screen.ron). Host content
# like the furnace's, so its keys live here too. The footer is rich text
# (docs/guide/rich-text.md): the `{key:..}` placeholders resolve to the live
# binding for the player's input mode, and a translator may use `[b]`, `[i]`
# or `[color=$accent]` in it. Braces are Fluent's own placeable syntax, so a
# rich placeholder is written as a string literal, `{"{key:back}"}`.
demo-settings-title = Settings
demo-settings-tab-display = Display
demo-settings-tab-audio = Audio
demo-settings-tab-controls = Controls
demo-settings-display-resolution = Resolution
demo-settings-resolution-hd = 1280 × 720
demo-settings-resolution-fhd = 1920 × 1080
demo-settings-resolution-qhd = 2560 × 1440
demo-settings-display-ui_scale = UI scale
demo-settings-display-reduced_motion = Reduced motion
demo-settings-display-colour_mode = Colour mode
demo-settings-colour-normal = Normal
demo-settings-colour-deuteranopia = Deuteranopia
demo-settings-colour-high_contrast = High contrast
demo-settings-audio-volume = Volume
demo-settings-audio-master = Master
demo-settings-audio-music = Music
demo-settings-audio-effects = Effects
demo-settings-audio-output = Output
demo-settings-audio-device = Device
demo-settings-device-auto = Automatic
demo-settings-device-speakers = Speakers
demo-settings-device-headphones = Headphones
demo-settings-audio-mute_in_background = Mute in the background
demo-settings-audio-subtitles = Subtitles
demo-settings-audio-accessibility = Accessibility
demo-settings-audio-mono = Mono audio
demo-settings-controls-accept = Accept
demo-settings-controls-back = Back
demo-settings-controls-tab_prev = Previous tab
demo-settings-controls-tab_next = Next tab
demo-settings-controls-player_name = Player name
demo-settings-controls-player_name_hint = Your name
demo-settings-footer = Press {"{key:back}"} to close, {"{key:tab_next}"} for the next tab
demo-settings-reset = Reset
demo-settings-done = Done
