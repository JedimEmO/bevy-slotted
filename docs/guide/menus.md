# Menus

The screens a game is made of that are not inventories: a main menu, a pause
screen, settings that persist, a confirm dialog, a toast, a page of text, a
conversation ([dialogue.md](dialogue.md)), and the hint bar under all of
them. `slotted-menu` ships every one of those as a
screen template, and a game gets the lot by adding `SlottedPlugins::default()`
and two resources. Nothing in the crate calls into game code; it speaks through
messages, and a game answers the ones it cares about.

This may look like the crate is deciding what your menus look like. It is not:
a template is a `.screen.ron` like any of yours, registered only when nobody
registered that kind first, and a game overrides a template by registering its
own, extends one with `inherits`, and restyles all of them through the theme
([themes.md](themes.md)). The crate is a consumer of the same extension API a
mod uses.

## The smallest game

```rust
use bevy::prelude::*;
use slotted::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(SlottedPlugins::default())
        .insert_resource(MenuConfig {
            settings_kind: ScreenKind::new("game:settings"),
            title: LocKey("game.title".into()),
            version: env!("CARGO_PKG_VERSION").into(),
            ..default()
        })
        .insert_resource(Settings { spec: settings_spec() })
        .insert_resource(SettingsStorage::file("settings.ron"))
        .add_systems(Update, answer_menu)
        .run();
}

fn answer_menu(mut choices: MessageReader<MenuChoice>, mut commands: Commands) {
    for choice in choices.read() {
        match choice.id.as_str() {
            "play" => { /* pop the main menu, start the game */ }
            "quit" => confirm(&mut commands, ConfirmSpec::new("quit", "game.quit.message").danger()),
            _ => {}
        }
    }
}
```

With that, `Escape` (or `Start` on a pad) with nothing open pushes the pause
screen; its Settings button pushes `game:settings`, generated from the spec
with every value saved to `settings.ron` as it changes; its Quit button
arrives as a `MenuChoice` the game turns into a confirm. `examples/menus` is
this, over the chest example's scene, with a main menu, an About page and a
toast on top; its `tests/flow.rs` walks the whole thing by gamepad headless.

## `MenuConfig`

| Field | Default | What it does |
|---|---|---|
| `pause_kind` | `slotted:pause` | The screen `Menu` pushes when no page or modal is open. |
| `settings_kind` | `slotted:settings` | The screen the `settings` button pushes. Point it at your generated settings screen; the default is the empty frame. |
| `pause_on_menu` | `true` | Whether `Menu` pauses at all. Off for a game with its own pause key. |
| `title` | `slotted.menu.title` | The main menu's title key. |
| `version` | empty | The main menu's version line, as the `{version}` argument of `slotted.menu.version`. Empty removes the node rather than drawing `Version `. |

`MenuConfig` is the opt-in. The plugin inserts none: without one the templates
still register, every `MenuChoice` is still written and `resume` still pops,
but `Menu` pauses nothing and the `settings` button pushes nothing, so a game
that only wants the templates or the toasts grows no pause screen on Escape.
Insert it before the first frame: `install_strings` reads `title` and
`version` in `PostStartup` and writes them into the registered main menu with
`ScreenDef::set_text`, touching only the crate's own nodes (a `title` whose
key is still `slotted.menu.title`), so a game's own `slotted:main_menu` keeps
its title.

Escape is both `Back` and `Menu`. When a press arrives as both, `Menu` yields
to whoever owns `Back`: the stack when a screen is open, or any consumer that
claimed it in the `Input` set (the HUD editor cancelling a drag, a key
capture). Only a pure `Menu` press, Start on a pad, pops the pause from the
pause. The `kinds` module names the five template kinds
(`kinds::pause()` and so on); it is not in the prelude because `slotted_ui`
has a `kinds` of its own.

## The templates

Every template is a file under `crates/slotted-menu/screens/`, compiled in and
registered in `PostStartup` unless a screen of that kind is already there. Each
carries anchors a mod injects at and ids a game rewrites by, and each opens
headless in the three themes with a gamepad walk reaching every button; that is
`crates/slotted-menu/tests/templates.rs`.

| Kind | Presentation | Ids | Anchors |
|---|---|---|---|
| `slotted:main_menu` | page, fade, `back: ignore`; a panel placed at the left | `title`, `version`, `buttons`, `play`, `settings`, `quit`, `hints` | `title_end`, `buttons_end` (inside the button column, so an injected button lines up with the three above it), `footer` |
| `slotted:pause` | modal, scrim, slide up | `title`, `resume`, `settings`, `quit`, `hints` | `title_end`, `buttons_end` |
| `slotted:settings` | modal, scrim, fade | `title`, `settings.tabs` (empty; the generated screen replaces it), `reset`, `done`, `hints` | `title_end` |
| `slotted:confirm` | modal, scrim, fade | `title`, `message`, `cancel`, `accept`, `hints` | `message_end` |
| `slotted:page` | page, slide left | `title`, `scroll`, `body`, `done`, `hints` | `title_end`, `body_end` |
| `slotted:dialogue` | overlay that takes focus, no scrim, slide up, `back: ignore`; a panel placed at the bottom | `portrait`, `speaker`, `text`, `choices`, `hints`, `continue` | `header_end`, `text_end`, `choices_end`, `footer` ([dialogue.md](dialogue.md#the-screen)) |

Every button carries a `menu` tag (`tags: {"menu": "play"}`) and the crate
turns its `Activate` into a `MenuChoice` (below). Every string is a
`slotted.menu.*` key with an English default inside the crate, pushed as a
`Localization` fallback, so a game with no `.ftl` file at all reads `Play`,
`Settings`, `Quit`, `Resume`, `Reset`, `Done`, `Cancel`, `OK`, `Paused`, `Are
you sure?`, and the hint verbs. A catalogue that defines a key wins over the
fallback; that is how a translation lands.

### Overriding and inheriting a template

Register your own screen of the kind before `MenuPlugin`'s `PostStartup`
system runs, and the template is never registered:

```rust
app.add_systems(Startup, |mut screens: ResMut<Screens>| {
    screens.register(my_pause_screen());
});
```

Keep the `menu` tags on your buttons and the crate keeps handling them. To add
to a template rather than replace it, inherit it. The child's root restates
the template root's shape, because an inheriting root always wins on role,
layout and tags ([screens.md](screens.md#inheritance)); its nodes merge by id.
`examples/menus/screens/main.screen.ron` does exactly this:

```ron
(
    kind: "menus:main",
    inherits: "slotted:main_menu",
    initial_focus: "play",
    root: (
        type: "panel", role: "panel",
        layout: (direction: "column", gap: 3.0, padding: (5.0, 5.0, 5.0, 5.0), min_width: 340,
                 place: (anchor: "left", offset: (48.0, 0.0)), align: "start"),
        tags: {"test_id": "main_menu"},
        children: [
            (type: "text", key: "demo.menus.footer", style: "caption",
             tags: {"test_id": "footer_note"}),
        ],
    ),
)
```

A game or a mod adds a button through an `Injection` at `buttons_end`,
the same way a mod adds a Sort button to a chest. An injected node sizes to
its content, so the example wraps its About button in a full-width column
panel to take the width of the buttons above it (`menus::about_injection`).
The button carries `tags: {"menu": "about"}` and arrives as a `MenuChoice`
like the template's own.

## `MenuChoice`

```rust
#[derive(Message)]
pub struct MenuChoice { pub id: String, pub screen: ScreenKind, pub entity: Entity }
```

One per activated `menu`-tagged button under a stack screen. The crate handles
some ids before the message reaches you, and still writes the message, so a
game can log or ignore them: `resume`, `close` and `back` pop; `settings`
pushes `MenuConfig::settings_kind` (a warning names the kind when nobody
registered it); `accept` and `cancel` answer a confirm; `reset` restores the
settings defaults. Everything else is the game's, and `screen` tells you which
screen the button sat on, so `quit` on the pause can mean "back to the title"
while `quit` on the title means "leave".

## Pause

`pause_on_menu` runs in `SlottedUiSet::Input` after `UiActionEmit`. A fresh,
unclaimed `Menu` with no page or modal open (overlays do not count) pushes
`pause_kind` and claims the action; with `pause_kind` on top it pops it. `Menu`
over any other page or modal does nothing, which is what lets a chest, a
settings screen or a confirm sit on top without a pause appearing behind it.

`Escape` is both `Back` and `Menu` by default ([input.md](input.md)). With a
screen open the stack's `pop_on_back` owns the press; with nothing open only
`Menu` has anything to do; and on the pause itself the crate leaves the pop to
`pop_on_back` and only claims `Menu`, so one press pops one screen. When the
push happens the crate also claims `Back`, or the same press would pop the
pause it just opened. `Start` on a pad is `Menu` alone and pauses and resumes
the same way.

## Confirm

```rust
confirm(&mut commands, ConfirmSpec::new("delete", "game.delete.message")
    .title("game.delete.title")
    .buttons("game.delete.accept", "slotted.menu.cancel")
    .arg("world", "Ravenholm")
    .danger());
```

`confirm` clones the registered `slotted:confirm`, rewrites `title`,
`message` (with the arguments; the message is rich text, so `[b]{world}[/b]`
works) and the two button labels, turns `accept` into a danger button when
asked, pushes it, and
marks the root with `PendingConfirm(id)`. The answer is one `ConfirmResult {
id, accepted }`, written exactly once: by the accept button, by the cancel
button, or by `Back` closing the dialog. A confirm pushed over a confirm
stacks and each answers its own id. Focus starts on Cancel on purpose: a
danger button should never be the one a reflex press hits.

## Toast

```rust
toast(&mut commands, ToastSpec::new("game.saved").level(ToastLevel::Success));
toast(&mut commands, ToastSpec::new("game.moved").arg("count", 3).duration(Duration::from_secs(2)));
```

A toast is not a screen. It spawns under a lazily created full-window host in
the `zbands::TOAST` band, in a bottom-centre column that grows upward, so the
first toast sits nearest the edge and the next one stacks above it. It fades in
through the theme's `Fade` preset, counts down on virtual time (ten times
`durations.slow` unless the spec says otherwise), fades out and despawns.
More than `Toasts::max_visible` (3) queue in order. Toasts ignore the stack
entirely: they show over a page, a modal or nothing, and never move focus.
Under reduced motion they appear and vanish without a tween. The four levels
paint `toast.info`, `toast.success`, `toast.warning` and `toast.error`; the
text is `toast.text`.

## Page

```rust
open_page(&mut commands, PageSpec::new("game.credits.title", "game.credits.body"));
```

A page is `slotted:page` with `title` and `body` rewritten (the body takes
arguments too), pushed as a page that slides in from the right, with the body
in a scroll panel and a Done button that pops. The example's About page is
one.

## The hint bar

`slotted:hint_bar` is a widget kind `MenuPlugin` registers, so a template
writes `(type: "custom", kind: "slotted:hint_bar")` and so may any screen of
yours. It is a row of entries, each a key glyph and a verb, rebuilt whenever
focus, the input mode, the glyph set, the bindings, the stack or the theme
changes:

- the focused node's verb, by its `SemanticRole`: a button, tab or list row is
  `Select`, a toggle is `Toggle`, a slider is `Adjust` on `Left` and `Right`
  sharing one label, a select or radio group is `Change`, a text field is
  `Edit`, a key binding is `Rebind`, a slot is `Pick up`; a `hint.accept` tag
  on the node replaces the verb with that key;
- `Back` with `Close` on a modal and `Back` on a page, when the screen's back
  policy is `pop`; never on an overlay, which `Back` never pops;
- `Previous tab` and `Next tab` when the screen holds a `tabs` node;
- and what the bar's own tags say: `hint.accept` on the bar is the Accept
  entry when the focused node contributed none, `hint.secondary` and
  `hint.back` add those entries. The dialogue screen rewrites them per state
  (`Skip`, `Continue`, `History`, `Leave`); a screen of yours may set them in
  the file.

Accept is always first. The glyph text is `key_glyph_text` for the player's
input mode and the resolved `GlyphSet` ([input.md](input.md#glyph-sets)), so a
PlayStation pad reads `Cross` where an Xbox pad reads `A`. The bar hides in
pointer mode unless it carries `tags: {"hint.always": "true"}`.

How the glyph is drawn is the theme's call, through the kind of its
`hint.glyph` material: a `Text` material means bare bracketed text, `[Enter]`,
which is paper; anything else is a pill with the text in `hint.glyph.text`,
which is glass and neon. `HintEntries` on the bar root is what a test reads
(`UiHarness::hint_entries`).

## `SettingsSpec`

A settings screen is data: tabs of rows, each row a control bound to a store
key with a default. The crate generates the screen over the
`slotted:settings` frame, derives the store's defaults and rules from the rows,
seeds the store, saves when the changes stop and answers Reset. What you write is
the spec; what you do not write is a screen file, a rule table or a save
system.

```rust
use slotted::menu::{SettingsRow, SettingsSpec};
use slotted::ui::{InputDevice, SelectOption, ToggleStyle, UiAction};

fn key(s: &str) -> LocKey { LocKey(s.to_owned()) }

pub fn settings_spec() -> SettingsSpec {
    SettingsSpec::new(ScreenKind::new("game:settings"))
        .tab("display", "game.settings.display")
        .row(SettingsRow::Select {
            key: "settings.resolution".into(),
            label: key("game.settings.resolution"),
            options: vec![
                SelectOption { id: "1280x720".into(), label: key("game.res.hd") },
                SelectOption { id: "1920x1080".into(), label: key("game.res.fhd") },
            ],
            default: "1920x1080".into(),
        })
        .row(SettingsRow::Slider {
            key: "settings.ui_scale".into(),
            label: key("game.settings.ui_scale"),
            min: 0.5, max: 3.0, step: 0.25, default: 1.0,
            format: "{value:.2}×".into(),
        })
        .row(SettingsRow::Toggle {
            key: "settings.reduced_motion".into(),
            label: key("game.settings.reduced_motion"),
            default: false,
            style: ToggleStyle::Switch,
        })
        .tab("audio", "game.settings.audio")
        .row(SettingsRow::Heading(key("game.settings.volume")))
        .row(SettingsRow::Slider {
            key: "settings.master".into(), label: key("game.settings.master"),
            min: 0.0, max: 100.0, step: 5.0, default: 80.0, format: "{value}%".into(),
        })
        .row(SettingsRow::Separator)
        .row(SettingsRow::Radio {
            key: "settings.output".into(), label: key("game.settings.output"),
            options: vec![
                SelectOption { id: "stereo".into(), label: key("game.out.stereo") },
                SelectOption { id: "mono".into(), label: key("game.out.mono") },
            ],
            default: "stereo".into(),
        })
        .tab("controls", "game.settings.controls")
        .row(SettingsRow::Binding { action: UiAction::Accept, device: InputDevice::Keyboard, label: None })
        .row(SettingsRow::Binding { action: UiAction::Back, device: InputDevice::Keyboard, label: None })
        .row(SettingsRow::Text {
            key: "settings.player_name".into(), label: key("game.settings.player_name"),
            default: "Steve".into(), placeholder: Some(key("game.settings.name_hint")), max_len: Some(16),
        })
        .row(SettingsRow::Custom(UiNodeDef::RichText { /* a footer of your own */ }))
}
```

Then `app.insert_resource(Settings { spec: settings_spec() })` and set
`MenuConfig::settings_kind` to the spec's kind. `showcase::settings::spec` in
`examples/showcase/src/settings.rs` is the full version of this, with a
`ValueGuard` for the rule a range cannot express (a UI scale above 2 is
refused with a reason, and the slider snaps back).

What the crate makes of it:

- **The screen.** `SettingsSpec::screen_def()` is a `ScreenDef` that inherits
  `slotted:settings`, with one `tabs` node with id `settings.tabs` bound to
  `<kind path>.tab` and one page per tab: a `scroll` with id
  `settings.<tab>.page` holding an anchor `settings.<tab>.start`, the rows,
  and an anchor `settings.<tab>.end`. A value row's control has `test_id` =
  its key (`settings.ui_scale`); a binding row's is
  `bind.<device>.<action>` (`bind.keyboard.accept`). A mod injects a row of
  its own at `settings.<tab>.end`, and the row binds the same store.
- **Defaults and rules.** `defaults()` is every value row's key and default;
  `rules()` is a slider's `min`/`max`/`step` as a `ValueRule` (a `step` of
  zero means none), a select's or radio's option ids, and the tab key's tab
  ids. A toggle and a text field imply no rule.
- **Applying.** `apply_settings` runs in `PostStartup` after the templates,
  or the frame a `Settings` resource appears later: it registers the screen
  (unless the kind is registered), loads the store if there is one, installs
  the rules, then seeds every declared key with the saved value over whatever
  the game already put in the store over the default. A saved value is first
  made to fit its rule (`ValueRule::conform`: a number past a tightened range
  is clamped, an option that no longer exists or a value of the wrong kind
  falls back to the default with one warning), so a file from an older build
  or a hand edit never puts into the store what no control could have
  written. The seed goes through `ValueStore::restore`, so no guard or
  `ValueChanged` fires for it. Saved keys the spec no longer declares are
  dropped. The `UiBindings` the app started with are kept as
  `DefaultBindings` before a saved table replaces them. The tab key is seeded and ruled but never saved or reset: which tab
  you were on is UI state, not a setting.
- **Reset.** The frame's Reset button writes every default back through
  `SetValue`, so your guards still apply, and returns `UiBindings` to
  `DefaultBindings`, the table the app started with rather than the crate's;
  a game can send `SettingsReset` itself.
- **Saving.** A declared change marks the settings dirty and the save lands
  on the first frame with no new change, so a slider drag costs one write when
  it ends rather than one per frame. `AppExit` flushes a pending change.
- **Conflicts.** When a capture binds a key another action on the same device
  already has, the other action loses it and a toast
  (`slotted.menu.binding_moved`, with `action` as the argument) says so. No
  two actions share a key by accident.

The M1 control rows are the same controls a hand-written screen uses
([screens.md](screens.md#value-controls)), so everything in
[values.md](values.md) about rules, guards and bindings holds here.

## Persistence

`SettingsStore` is a port: `load() -> Option<SavedSettings>` and `save(&SavedSettings)`,
where `SavedSettings { values, bindings }` holds the declared keys and the
whole `UiBindings`. `SettingsStorage(Arc<dyn SettingsStore>)` is the
resource; no resource means no persistence, which is the right default for a
test. Two stores ship:

- `FileSettings { path }` (`SettingsStorage::file(path)`): a RON file on
  native. A missing file is `None`; an unreadable one is a warning and
  `None`; a save creates the parent directory. On wasm it is a stub that
  loads nothing and saves nowhere.
- `MemorySettings`: an `Arc<Mutex<Option<SavedSettings>>>` you can `get`,
  `set`, and read out as RON. What a test asserts on, and what a web page
  persists.

`save_settings` runs at `Last` when a `ValueChanged` named a declared key, on
any `BindingChanged`, and on `AppExit`. A tab switch and an undeclared key
never touch the file.

### The wasm bridge

A browser has no file, but it has `localStorage`. Build the store from what
the page kept, and hand the page the RON back whenever it asks:

```rust
// At startup, from a string the host page read out of localStorage.
let store = MemorySettings::from_ron(&saved_text).unwrap_or_default();
app.insert_resource(SettingsStorage::new(store.clone()));

// In a wasm-bindgen export the page calls before unload, or on a timer.
pub fn settings_ron() -> Option<String> { store.to_ron() }
```

`from_ron` fails on malformed text, and starting from an empty store is the
honest fallback there. The playground's settings scene (M4) will be this.

## In a test

`slotted-test`'s `menu` feature (on by default) adds the readers and drivers:
`toasts()` (oldest first), `hint_entries(bar)`, `confirm_accept()` and
`confirm_cancel()` (press the button on the confirm on top; a panic when the
top is not a confirm), and `menu_choices()` (every `MenuChoice` since the last
call). `stack_top()` is the kind of the top non-overlay screen, so a running
dialogue is not it; `dialogue()` and its helpers are in
[dialogue.md](dialogue.md#in-a-test).

```rust
h.gamepad(GamepadButton::Start);
h.settle();
assert_eq!(h.stack_top(), Some(kinds::pause()));
h.activate(h.find(&by::test_id("quit")));
h.settle();
assert_eq!(h.menu_choices().into_iter().map(|c| c.id).collect::<Vec<_>>(), vec!["quit"]);
h.confirm_accept();
```

`examples/menus/tests/flow.rs` and `examples/chest/tests/ui.rs` are the worked
examples: main → play → back → pause → settings → adjust → back → quit →
confirm, by gamepad, with the stack asserted at every step.
