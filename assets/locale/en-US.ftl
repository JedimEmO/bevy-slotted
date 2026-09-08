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

# The settings demo (`showcase::settings::spec`, a `SettingsSpec`). Host content
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
demo-settings-tab-demo = Demo
demo-settings-demo-found_key = Found the key (unlocks a dialogue option)
demo-settings-footer = Press {"{key:back}"} to close, {"{key:tab_next}"} for the next tab
demo-settings-reset = Reset
demo-settings-done = Done

# The examples' menus (`showcase::menus`, `examples/menus`): the quit confirm,
# the menus example's title, its About page and its toast. The template
# strings themselves (`slotted.menu.*`) fall back to English inside
# `slotted-menu` and are not repeated here.
demo-quit-title = Quit?
demo-quit-message = Unsaved progress will be lost.
demo-quit-accept = Quit
demo-menus-title = Slotted
demo-menus-about = About
demo-menus-about-title = About this example
demo-menus-about-body = [b]slotted[/b] is a set of Bevy plugins for inventory screens and game menus. This example is the main menu, the pause screen, the settings screen and the confirm dialog, all from the templates in [i]slotted-menu[/i], over the chest from the chest example. Press {"{key:back}"} to go back.
demo-menus-leave-title = Leave the game?
demo-menus-leave-message = The chest and everything in it stays where it is.
demo-menus-leave-accept = Leave
demo-menus-quick_stack = Quick stack: every stack with a match in the chest went there.
demo-menus-footer = Built on slotted-menu. Esc pauses the game; Start does on a pad.
demo-menus-talk = Talk
demo-menus-thanks = The elder nods. Come back whenever you like.

# The menus example's dialogue (assets/dialogue/greeting.dialogue.ron). A
# `say` line is rich text, so `[b]`, `[i]` and `{key:..}` work here too, and
# the typewriter counts a glyph as one unit.
demo-elder = Elder
demo-greeting-hello = Ah, a visitor. [i]Few[/i] come this far up the mountain. Will you sit a while and hear what the old stones remember?
demo-greeting-ask = Sit with the elder?
demo-yes = Yes, tell me
demo-no = Not now
demo-secret = I found the key
demo-greeting-yes = Good, { $name }. Then listen: the chest below was sealed long before the village had a name.
demo-greeting-secret = The elder says nothing, but the chest's lid stands open for the first time in years.
