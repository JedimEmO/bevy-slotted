# The showcase's own strings (docs/design/showcase-refresh-contract.md
# sections 4.2 and 4.3). Compiled in by `src/settings.rs` and layered under
# whatever catalogue the app already holds, so the web playground, whose
# locale comes from the pack install, and a native example, whose locale
# comes from `assets/locale/en-US.ftl`, resolve the same keys. Fluent ids use
# `-`; a screen file or a spec writes `demo.showcase.title` and
# `normalise_id` maps it.

# The Menus scene: the title, its injected About button and page, the leave
# confirm on the pause, and the toast a browser tab shows in place of an
# exit. The quit confirm on the title is the shared `demo-quit-*`.
demo-showcase-title = Slotted
demo-showcase-footer = Built on slotted-menu. Esc pauses the game; Start does on a pad.
demo-showcase-about = About
demo-showcase-about-title = About this showcase
demo-showcase-about-body = [b]slotted[/b] is a set of Bevy plugins for inventory screens and game menus. Every scene on this page is one app: the chest, the furnace, these menus and the smith's conversation share one screen stack, one theme and one value store, and the page beside the canvas talks to it through five exports. Press {"{key:back}"} to go back.
demo-showcase-leave-title = Leave the game?
demo-showcase-leave-message = The chest and everything in it stays where it is.
demo-showcase-leave-accept = Leave
demo-showcase-no_exit = A browser tab has nowhere to go; the title is back
demo-showcase-thanks = The smith nods. Come back when the furnace is hot.

# The Dialogue scene: the smith's lines (`dialogue/smith.dialogue.ron`). A
# `say` line is rich text, so `[b]`, `[i]` and `{key:..}` work here too.
demo-smith = Smith
demo-smith-hello = Mind the sparks. The furnace has been cooking since you opened this page, and it does not stop for talk.
demo-smith-ask = Need something from the forge?
demo-smith-yes = Yes, tell me what it makes
demo-smith-no = Not now
demo-smith-secret = I found the key
demo-smith-yes-line = Good, { $name }. Coal in, ingots out. The arrow fills as it cooks, and the tank on the left is water for the quench.
demo-smith-secret-line = The smith turns the key over for a long moment. That opens the chest in the first scene. Keep it.

# The chest screen's header (`assets/screens/demo_chest.screen.ron`). The
# demo overwrites all three every frame (`chest::fill_labels`), so these are
# what the nodes say for the one frame before that, and what stops the
# missing-key warning on every chest the showcase opens.
demo-chest-title = Copper Chest
demo-chest-capacity = 0 / 0 slots
demo-chest-inventory = Inventory
