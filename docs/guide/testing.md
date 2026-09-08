# Testing

Every screen built with these crates is drivable headless: no window, no GPU, no
wall clock. Clicks go through the real `bevy_ui` layout and the real
`bevy_picking` backend, so a passing test exercises the path a player does.

There are two audiences. A game tests its screens in Rust with `slotted-test`. A
mod tests its behaviour in Lua with `slotted.test`, and the same harness runs
both.

## A game's tests

`slotted-test` is a normal published crate. Add it to `[dev-dependencies]`.

```rust
use slotted_test::prelude::*;

fn open_chest() -> (UiHarness, Opened) {
    let registries = my_game::load_registries();
    let mut harness = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .plugins(MyGamePlugin)
        .registries(registries.clone())
        .resolution(1600.0, 900.0)
        .theme("glass")
        .build();
    let opened = harness.open_screen(my_game::chest_screen(), my_game::chest_menu(&registries));
    harness.settle();
    (harness, opened)
}
```

`open_screen` opens a menu over the fixture's inventories and pushes the screen
through the screen stack, so `Opened.screen` is a stack entry: `Back` pops it
and closes the menu exactly as it does in the game. `open(screen)` does the same
for a screen with no menu (a settings page, a pause modal) and returns the root.
A test that wants a screen outside the stack calls `spawn_screen` itself.

```rust
#[test]
fn escape_closes_the_pause_menu() {
    let (mut harness, _) = open_chest();
    harness.open(my_game::pause_screen());
    assert_eq!(harness.stack().len(), 2);
    harness.action(UiAction::Back);
    harness.settle();
    assert_eq!(harness.stack().len(), 1);
}

#[test]
fn shift_clicking_a_stack_sends_it_to_the_chest() {
    let (mut harness, _) = open_chest();
    let hotbar = harness.find(&by::role(SemanticRole::Slot).tag("region", "hotbar").index(0));

    harness.shift_click(hotbar);
    harness.settle();

    assert!(harness.stack_at(hotbar).is_none());
}
```

### Locators

A locator matches on the semantic layer, never on coordinates or on a widget's
internals. That is why a theme swap, a layout change or a widget rewrite does not
break a test.

| Locator | Matches |
|---|---|
| `by::role(SemanticRole::Slot)` | A semantic role. |
| `by::test_id("title")` | The `test_id` tag from the screen file. |
| `by::tag("region", "chest")` | Any tag. |
| `by::text("Copper Chest")` | Displayed text, after localisation. |
| `by::anchor("title_end")` | An injection anchor. |
| `by::screen(kind)` | A screen root. |
| `by::hud_layer("mymod:mana")` | A HUD layer root. |
| `by::widget_kind(kind)` | A widget kind, as a `WidgetKind`. |
| `by::control("slider")` | A control by its kind's path: every M1 control carries `WidgetNode("slotted:<type>")`. |

Chain `.tag(k, v)`, `.index(n)`, `.visible()`, `.nth_visible(n)`,
`.within(entity)` and `.with_item("demo:coal")` to narrow.

`find` panics with a description of the near misses when nothing matches, which
is usually enough to see what changed. `try_find` returns an `Option` and
`find_all` returns every match in tree order.

### Actions

`click`, `right_click`, `middle_click`, `shift_click`, `hover`, `click_at`,
`pointer_move_to`, `pointer_press`, `pointer_release`, `key`,
`key_with_modifiers`, `activate`, `cycle`, `toggle_side_tab`, `menu_action`,
`click_slot`, `request_tooltip`.

Input the way a player produces it: `gamepad(button)` presses and releases a
pad button, `gamepad_hold` and `gamepad_release` split the two, `stick(Vec2)`
holds the left stick until `stick(Vec2::ZERO)`, `action(UiAction)` presses the
first keyboard key bound to an action, and `set_input_mode(InputMode)` puts the
UI in pointer, keyboard or gamepad mode directly. The harness owns one
`Gamepad` entity; a replayed recording's pad events land on it too. See
[input.md](input.md) for what the actions mean.

### Controls

The M1 controls have helpers that do what a player does, through the real
input path, and stop when the control has reacted:

| Helper | Does |
|---|---|
| `value(key)` | Reads the `ValueStore`. |
| `set_value(key, value)` | Writes a `SetValue` and steps a frame, so the key's rule and the guards apply exactly as they would to a control's write. |
| `drag_slider(slider, fraction)` | Presses on the track where the thumb is, drags to `fraction` of the track's width, releases. Every move is a write with the slider as source, so a guard sees each one. |
| `select_option(select, "high")` | Clicks the pill to open the popup, then clicks the option. |
| `type_into(field, "Steve")` | Focuses the row, `Accept` to start editing, types, Enter to commit. |
| `switch_tab(tabs, "audio")` | Clicks the tab button. |
| `capture_key(row, KeyCode::KeyK)` | Focuses the `key_binding` row, `Accept` to start capture, presses the key. `UiBindings` is rebound afterwards. |

What a test asserts on is a plain component on the control's row, never the
paint: `ToggleState`, `SliderState`, `SelectState`, `RadioState`,
`KeyBindingState`, `TextFieldState`, `ListState` and `TabsState`
([values.md](values.md) for how a value gets there).

```rust
let scale = h.find(&by::test_id("ui_scale"));
h.drag_slider(scale, 0.4);                       // 0.5 + 2.5 * 0.4, on the 0.25 grid
h.settle();
assert_eq!(h.value("settings.ui_scale"), Some(Value::Float(1.5)));
assert_eq!(h.world().get::<SliderState>(scale).unwrap().value, 1.5);
h.drag_slider(scale, 1.0);                       // past the guard's limit of 2.0
h.settle();
assert!(h.value("settings.ui_scale").unwrap().as_f64().unwrap() <= 2.0);
```

`crates/slotted-test/tests/settings.rs` drives the whole settings demo this
way. One thing it does that a game's tests need not: this crate has no
`assets/` directory of its own, so it hands the headless group an
`AssetPlugin { file_path }` pointing at the workspace's, or the theme would
never load. A game's tests run from the game's crate root and find its
`assets/` as the game does.

### Queries

`stack_at`, `displayed_stack`, `carried`, `text_of`, `tooltip`, `is_visible`,
`is_focused`, `focused`, `rect_of`, `center_of`, `fill_of`, `tank_fill`,
`property_of`, `side_tab_open`, `viewport_subject`, `exclusion_zones`,
`hud_layers`, `hud_tree`, `screen_tree`.

For the input side: `focused()` is Bevy's `InputFocus`, `input_mode()` the
current `InputMode`, `stack()` the open screen kinds bottom to top,
`stack_top()` the kind of the top non-overlay screen, and `focus_ring()` the
ring's `FocusRingState` (its target and whether it shows).

For the menus (the `menu` feature, on by default): `toasts()` is every toast
on screen oldest first, `hint_entries(bar)` a hint bar's entries,
`confirm_accept()` and `confirm_cancel()` press the button on the confirm on
top, and `menu_choices()` drains every `MenuChoice` since the last call
([menus.md](menus.md#in-a-test)).

`assert_conserved()` checks that no item was created or destroyed since the
harness opened, which is the assertion worth putting at the end of anything that
moves stacks around.

### Time

There is no wall clock anywhere in `slotted-ui`, and the harness only advances
`Time<Virtual>`.

- `settle()` advances frames until layout is clean and `ActiveMotions` is zero,
  then returns how many frames it took. It panics past `max_settle_frames`.
- `try_settle()` is the same with a `Result`, for asserting that something does
  not settle.
- `step(n)` advances exactly `n` frames, for catching a transient state.
- `advance(duration)` advances virtual time.

Tests do not sleep and do not flake on a loaded CI runner. If `settle()` does not
terminate, something is animating forever, which is a real bug and worth the
panic.

### Snapshots

```rust
#[test]
fn the_screen_tree_matches_the_file() {
    let (harness, _) = open_chest();
    assert_tree_snapshot!(harness.screen_tree());
}
```

`ScreenTree` is the semantic tree: roles, labels, tags, test ids and item
summaries. It deliberately excludes colours, sizes and positions, so a snapshot
survives a theme change and fails on a structural one.

Review a diff before accepting it. `cargo insta review` shows them;
`INSTA_UPDATE=always` in a rebaseline run accepts them all, which is right after
an intentional restructure and wrong otherwise. A snapshot that changes in a pull
request that did not mean to change the tree is the whole point of having them.

## A mod's tests

A mod ships `tests/*.lua`. They are written against `slotted.test`, run in the
same headless harness, and need no Rust at all.

```lua
local t = slotted.test

t.test("the injected button sorts the player inventory", function()
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
    t.expect_stack({ role = "slot", tag = { region = "player" }, index = 0 },
                   "demo:cobblestone", 43)
end)
```

The body runs as a coroutine. Every action yields to the host, the app advances
frames, and the answer resumes the body, so the test reads as straight-line code
while real frames happen between the lines.

A locator is a table: `role` (a semantic role in snake case), `tag` (a map),
`test_id`, `text`, `widget`, `item` and `index`. Every key given must match.

A test drives the pad and the action vocabulary too: `t.gamepad("DPadRight")`
presses a button, `t.action("back")` presses whatever key `Back` is bound to,
and `t.focused()` returns the focused node's tags, so a mod can assert where
the focus ring sits without knowing which key moved it. For a settings screen,
`t.value("settings.ui_scale")` reads the value store, `t.set_value(key, v)`
writes through its rules and guards, and `t.type_into(loc, "text")` edits a
text field the way a player does.

The full list of actions, queries and expectations is in
[api/lua.md](api/lua.md#slottedtest).

Run them:

```
cargo xtask test-mods mods                    # every mod in a directory
cargo xtask test-mods mods --filter sorter    # one of them
just test-mods                                # the demo mods
```

The runner reports per test, and exits non-zero on the first failure. A failed
expectation carries its own message with no chunk-and-line prefix, so the report
reads the same whichever runtime ran it.

## Recording and replay

`cargo run -p chest -- --record session.ron` writes every pointer, key and
gamepad input to a file. `UiHarness::replay(path)` feeds it back frame by frame.

That turns a bug someone reproduced by hand into a regression test without
anyone having to describe what they clicked.

## What CI runs

| Command | |
|---|---|
| `just ci` | `fmt-check`, `lint`, `test`. The gate for any change. |
| `just wasm-check` | Every crate that has to reach a browser, on `wasm32`, with the feature set a wasm build uses. |
| `just test-mods` | The demo mods' Lua tests. |
| `just smoke` | Drives the built playground page in headless Chromium. |

The script conformance suite runs on every adapter, in both adapters' own test
suites. It is what keeps a mod behaving the same natively and in a browser.
