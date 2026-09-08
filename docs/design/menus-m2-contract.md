# Menus M2 contract: the `slotted-menu` crate

Status: v1.3 (post-review: one accept button; `MenuConfig` is the opt-in; `pause_on_menu` yields to any `Back` owner), 2026-09-08 (v1.1: skeleton facts in 1.3; v1.2: D's main menu shape in 3.1, and 5 notes the pause quit of `examples/menus` returns to the title). Bevy 0.19.1. Companion to `docs/design/menus-proposal.md` (v0.3)
section 5, and to the M0 and M1 contracts whose vocabulary this uses without restating. Four
packages: **A** (foundation changes in `slotted-ui` and `slotted-packs`), **B** (the crate's core:
templates, menu actions, pause, confirm, toast, page, hint bar) and **C** (settings: spec, generated
screen, persistence) run in parallel; **D** (facade, examples, harness, docs) runs after. The
skeleton creates the crate, its module layout, the facade feature and every public type named in
section 1; a body marked `// M2-IMPL: A|B|C|D` is that package's to fill. Signatures change only by
amending this file. Each package writes `docs/design/menus-m2-notes-{A,B,C,D}.md`.

## 0. Ground rules

- **`slotted-menu` is a consumer of the extension API.** It depends on `slotted-ui`, `slotted-theme`
  and `slotted-model`, never on `slotted-packs`, `slotted-browser` or `slotted-ecs`. Anything it
  needs from the foundation is added to `slotted-ui` publicly (package A), never reached through
  a private path. It must stay out of the `server` feature graph (`just server-check`).
- **Templates are data, embedded.** Every screen the crate ships is a `.screen.ron` under
  `crates/slotted-menu/screens/`, compiled in with `include_str!`, parsed once, and registered in
  `Screens` at startup unless a screen of that kind is already registered (so a game overrides a
  template by registering its own kind first, or extends it with `inherits`). Every template
  carries anchors (section 3.1). A parse failure of an embedded template is a test failure, never a
  runtime branch.
- **The crate speaks through messages and tags.** A template button carries a `menu` tag
  (`tags: {"menu": "play"}`); the crate turns its `Activate` into a `MenuChoice` message. It handles
  `resume`, `settings`, `back` and `close` itself and leaves every other id to the game. Nothing in
  the crate calls into game code.
- **English fallback strings, never a required `.ftl`.** Every `slotted.menu.*` key the templates
  use has an English default in Rust through the localisation fallback (section 2.1). A game or a
  mod overrides by shipping the key in its own catalogue.
- **Persistence is a port.** `SettingsStore` mirrors `HudLayoutStorage`: a trait, a file backend on
  native, a memory backend everywhere, and no resource means no persistence.
- Headless first, as always: every template opens in `UiHarness::open`, lays out, and is walked by
  gamepad in the three themes.

## 1. Public surface (done in the skeleton: types and signatures; bodies per package)

### 1.1 `slotted-ui` additions (A)

```rust
// loc.rs
impl Localization {
    pub fn set_primary(&mut self, l: impl Localizer);      // keeps fallbacks
    pub fn push_fallback(&mut self, l: impl Localizer);    // consulted in order after primary
}
// def.rs
impl UiNodeDef { pub fn find_mut(&mut self, id: &str) -> Option<&mut UiNodeDef>; }
impl ScreenDef {
    pub fn set_text(&mut self, id: &str, key: LocKey, args: LocArgs) -> bool; // text or rich_text node
    pub fn set_tag(&mut self, id: &str, tag: &str, value: &str) -> bool;
    pub fn remove_node(&mut self, id: &str) -> bool;
}
// values.rs
impl ValueStore { pub fn snapshot(&self) -> BTreeMap<String, Value>; pub fn restore(&mut self, map: BTreeMap<String, Value>); } // Serialize/Deserialize on the map
// rich.rs
#[derive(Resource)] pub enum GlyphSet { Auto, Keyboard, Xbox, PlayStation, Switch, Generic }  // Default Auto
pub fn button_glyph(button: GamepadButton, set: GlyphSet) -> String;   // Auto = resolved set
pub fn resolved_glyph_set(set: GlyphSet, mode: InputMode, gamepads: &Query<&Gamepad>) -> GlyphSet;
pub fn key_glyph_text(action, mode, bindings, set: GlyphSet) -> String;   // gains `set`
// layers.rs
pub mod zbands { pub const TOAST: i32 = 950; }
```

`GlyphSet::Auto` resolves from the first connected gamepad's `vendor_id`: `0x054C` PlayStation,
`0x057E` Switch, otherwise Xbox; `Generic` names buttons `A/B/X/Y`-free (`South`, `East`, ...).
`UiBindings::default()` changes: `Menu` is `Escape` on the keyboard (shared with `Back`) and
`Start` on a pad; `Tab` leaves the defaults (it collides with Bevy tab navigation, FOLLOWUPS).
`slotted-packs` `install` calls `Localization::set_primary` instead of replacing the resource.

### 1.2 `slotted-menu` (B and C)

```rust
pub struct MenuPlugin;                        // registers templates, strings, systems, the hint bar kind
#[derive(Resource)] pub struct MenuConfig {
    pub pause_kind: ScreenKind,               // slotted:pause
    pub settings_kind: ScreenKind,            // slotted:settings, or the game's
    pub pause_on_menu: bool,                  // true: Menu with no page/modal open pushes pause_kind
    pub title: LocKey, pub version: String,   // main menu header
}
#[derive(Message)] pub struct MenuChoice { pub id: String, pub screen: ScreenKind, pub entity: Entity }
pub mod kinds { main_menu(), pause(), settings(), confirm(), page(); }

// confirm.rs (B)
pub struct ConfirmSpec { pub id: String, pub title: LocKey, pub message: LocKey, pub args: LocArgs, pub accept: LocKey, pub cancel: LocKey, pub danger: bool }
pub fn confirm(commands: &mut Commands, spec: ConfirmSpec);
#[derive(Message)] pub struct ConfirmResult { pub id: String, pub accepted: bool }

// toast.rs (B)
pub enum ToastLevel { Info, Success, Warning, Error }
pub struct ToastSpec { pub key: LocKey, pub args: LocArgs, pub level: ToastLevel, pub duration: Option<Duration> } // None = tokens.durations.slow * 10
pub fn toast(commands: &mut Commands, spec: ToastSpec);
#[derive(Resource)] pub struct Toasts { pub max_visible: usize /* 3 */ }
#[derive(Component)] pub struct Toast { pub level: ToastLevel, pub remaining: Duration }

// page.rs (B)
pub struct PageSpec { pub title: LocKey, pub body: LocKey, pub args: LocArgs }
pub fn open_page(commands: &mut Commands, spec: PageSpec);

// hint_bar.rs (B): widget kind `slotted:hint_bar`
#[derive(Component)] pub struct HintBar;
#[derive(Component)] pub struct HintEntries(pub Vec<HintEntry>);   // what tests read
pub struct HintEntry { pub action: UiAction, pub label: LocKey }

// settings.rs (C)
pub struct SettingsSpec { pub kind: ScreenKind, pub tabs: Vec<SettingsTab> }
pub struct SettingsTab { pub id: String, pub label: LocKey, pub rows: Vec<SettingsRow> }
pub enum SettingsRow {
    Heading(LocKey), Separator,
    Toggle { key, label, default: bool, style: ToggleStyle },
    Slider { key, label, min, max, step, default: f64, format: String },
    Select { key, label, options: Vec<SelectOption>, default: String },
    Radio  { key, label, options: Vec<SelectOption>, default: String },
    Text   { key, label, default: String, placeholder: Option<LocKey>, max_len: Option<u16> },
    Binding { action: UiAction, device: InputDevice, label: Option<LocKey> },
    Custom(UiNodeDef),
}
impl SettingsSpec { builder methods; pub fn screen_def(&self) -> ScreenDef /* inherits slotted:settings */; pub fn defaults(&self) -> BTreeMap<String, Value>; pub fn rules(&self) -> ValueRules; }
pub trait SettingsStore: Send + Sync + 'static { fn load(&self) -> Option<SavedSettings>; fn save(&self, s: &SavedSettings); }
#[derive(Serialize, Deserialize, Default)] pub struct SavedSettings { pub values: BTreeMap<String, Value>, pub bindings: Option<UiBindings> }
#[derive(Resource)] pub struct SettingsStorage(pub Arc<dyn SettingsStore>);   // absent = no persistence
pub struct FileSettings { pub path: PathBuf }   // native; wasm stubs like FileHudLayout
pub struct MemorySettings(Arc<Mutex<Option<SavedSettings>>>);   // to_ron / from_ron / get / set
#[derive(Resource)] pub struct Settings { pub spec: SettingsSpec }   // inserted by the game; MenuPlugin does the rest
#[derive(Message)] pub struct SettingsReset;   // sent by the Reset button, applied by C
```

### 1.3 What the skeleton settled

- The message is `MenuChoice`, not `MenuAction`: `slotted_ecs::MenuAction` already exists and both
  reach the facade prelude. `kinds` is not in the crate's prelude for the same reason (`slotted_ui`
  has one); use `slotted_menu::kinds::pause()`.
- `Localization` is now `{ primary, fallbacks }` with `new`, `from_arc`, `set_primary`,
  `set_primary_arc`, `push_fallback`, `resolve_with` walking the chain; `slotted-packs` already
  uses `set_primary_arc` on install. `Localization(arc)` construction is gone.
- `ScreenDef::{set_text, set_tag, remove_node}`, `UiNodeDef::{find_mut, tags_mut,
  remove_descendant}`, `ValueStore::{snapshot, restore}` and `zbands::TOAST` are implemented.
- `GlyphSet` (resource, default `Auto`), `button_glyph(button, set)`, `resolved_glyph_set` and
  `key_glyph_text(.., set)` exist with the Xbox table only; A fills the other tables and the vendor
  resolution. Callers pass `GlyphSet::Auto` for now.
- Templates register in `PostStartup`, not `Startup`: the browser validates its screen handlers
  in `Startup` and skips the check while `Screens` is empty, which is how a mod's handler survives
  until the mod's screens install. `install_strings` and `apply_settings` chain after it there.
- The five templates, the toast snippet, the English fallback table (`strings.rs`), the eleven new
  roles (`roles::ALL` is 101) with first-pass materials in three themes, the facade `menu`
  feature (in the defaults) and `crates/slotted-menu/tests/skeleton.rs` exist.
- `SettingsSpec` has its builder (`new`, `tab`, `row`, `tab_key`, `keys`); `screen_def`,
  `defaults`, `rules` are stubs. `MemorySettings` is complete; `FileSettings` bodies are C's.

## 2. Package A: foundation (`slotted-ui`, `slotted-packs`)

- 2.1 Localisation fallback chain (1.1). `Localization::resolve` consults primary then fallbacks;
  `resolve_loc_text` unchanged. `slotted-packs/src/install.rs` uses `set_primary`. Test: a fallback
  survives a pack install.
- 2.2 `ScreenDef::{set_text, set_tag, remove_node}` and `UiNodeDef::find_mut` by node id, on a
  resolved or unresolved def. Test: confirm-style rewriting of a title and a message with args.
- 2.3 `ValueStore::{snapshot, restore}`; `restore` inserts without rules, guards or events and bumps
  the version once. Test: round trip through RON.
- 2.4 Glyph sets (1.1): tables for Xbox (`A B X Y LB RB LT RT ☰ ⧉`), PlayStation (`✕ ○ □ △ L1 R1 L2
  R2 Options Share`), Switch (`B A Y X L R ZL ZR + −`), Generic (Bevy's names), plus `Auto` from the
  vendor id. `refresh_key_glyphs` re-renders when `GlyphSet` changes or a gamepad connects. Test:
  the same `{key:accept}` reads `A`, `✕`, `B` under the three sets, and `Auto` picks PlayStation for
  a mock pad with vendor `0x054C` (Bevy's `Gamepad` mock lets a test set the vendor; if it does not,
  test `resolved_glyph_set` with an explicit vendor through a small seam).
- 2.5 `zbands::TOAST` (950), between the focus ring and tooltips.
- 2.6 (moved to D) `UiBindings::default()` `Menu` = Escape + Start, together with the chest example
  migration, since the chest opens settings on `Menu` today and the two must change at once.
- 2.7 A select popup closes on a pointer press outside it (FOLLOWUPS M1 item 8): a global
  `Pointer<Press>` observer in `select.rs` that closes any open popup whose subtree the press did
  not hit.
- 2.8 `UiHarness::open` already exists; add `UiHarness::stack_top() -> Option<ScreenKind>` and
  `toasts() -> Vec<Entity>` (D adds the second when B's marker lands; A adds the first).

## 3. Package B: templates, menu actions, pause, confirm, toast, page, hint bar

### 3.1 Templates (`crates/slotted-menu/screens/*.screen.ron`)

| Kind | Presentation | Structure and anchors |
|---|---|---|
| `slotted:main_menu` | page, fade | `place: left` `panel` column (a panel, not an invisible column, since D's paper shot: ink text on a dark scene is unreadable): `text` display title (id `title`), `text` caption version (id `version`), anchor `title_end`; button column (id `buttons`): play, settings, quit with `menu` tags, then anchor `buttons_end` *inside* the column so an injected button lines up with them; hint bar; anchor `footer`. `initial_focus: "play"` |
| `slotted:pause` | modal, scrim, slide_up | title (id `title`), buttons resume / settings / quit (`menu` tags), anchor `buttons_end`, hint bar. `initial_focus: "resume"` |
| `slotted:settings` | modal, scrim, fade | title (id `title`), anchor `title_end`, a `tabs` node with id `settings.tabs` and no tabs (C's generated screen replaces it by id), `separator`, footer row: hint bar, spacer, Reset (id `reset`), Done (`slotted:close`, id `done`). `initial_focus: "settings.tabs"` |
| `slotted:confirm` | modal, scrim, fade, `back: pop` | title (id `title`), `rich_text` message (id `message`), button row: cancel (id `cancel`, `menu: cancel`), accept (id `accept`, `menu: accept`, primary or danger), hint bar. `initial_focus: "cancel"` |
| `slotted:page` | page, slide_left | heading (id `title`), `scroll` with one `rich_text` (id `body`), hint bar, `slotted:close` Done. `initial_focus: "done"` |

Toasts are not a screen: see 3.4. Every template's strings are `slotted.menu.*` keys with English
fallbacks (`Play`, `Settings`, `Quit`, `Resume`, `Reset`, `Done`, `Cancel`, `OK`, `Close`,
`Select`, `Toggle`, `Edit`, `Adjust`, `Back`, `Next tab`, `Previous tab`). Theme roles the templates
need beyond M1: `toast`, `toast.info`, `toast.success`, `toast.warning`, `toast.error`,
`toast.text`, `hint.bar`, `hint.glyph`, `hint.label`, `menu.title`, `menu.version` (added to
`roles::ALL`, 90 becomes 101, in the three themes; B owns the materials). Paper draws `hint.glyph`
as bracketed mono text; glass and neon as an accent pill.

### 3.2 Menu actions and pause

- `route_menu_actions`: an `Activate` on a node with a `menu` tag under a stack screen writes
  `MenuChoice { id, screen, entity }`. Built-in handling, before the message reaches the game:
  `resume`/`close`/`back` pop; `settings` pushes `MenuConfig.settings_kind` (a warning if it is not
  registered); `cancel` and `accept` on `slotted:confirm` are 3.3's. Everything else is the game's.
- `pause_on_menu`: a fresh `UiAction::Menu` when `pause_on_menu` and the stack has no page or
  modal pushes `pause_kind` and claims. When the top is `pause_kind`, `Menu` pops it (Escape as
  Back already does; Start on a pad needs this).
- `MenuConfig.title`/`version` fill the main menu's `title` and `version` nodes through
  `set_text` at registration (version as a `{version}` argument to `slotted.menu.version`).

### 3.3 Confirm

`confirm` clones the registered `slotted:confirm` def, rewrites `title` and `message` (with
`args`) through `set_text`, the `accept` and `cancel` button labels through their `ButtonOpts`,
sets the `accept` button's variant to `danger` when asked (v1.3: one accept button, so a game's
own `slotted:confirm` needs only the four ids), stores `id` in a `PendingConfirm` component on
the root, and pushes. `accept`/`cancel` `MenuChoice`s and `Back`
resolve it: `ConfirmResult { id, accepted }` once, then pop. A confirm pushed over a confirm
stacks.

### 3.4 Toast

`toast` spawns (not pushes) a toast node under a lazily created host: an absolute full-window
`Pickable::IGNORE` node in `zbands::TOAST` holding a bottom-centre column (`place: bottom`,
`spacing.lg` above the edge) that stacks upward. A toast is the embedded `toast.node.ron` snippet
(`panel` in `toast.<level>` role, `rich_text` in `toast.text`) with `Toast { level, remaining }`;
it arrives with the `Fade` preset, its `remaining` counts down on virtual time, and it fades out and
despawns. More than `Toasts.max_visible` queue in order. Toasts ignore the stack entirely: they show
over a page, a modal or nothing. Reduced motion: no fades.

### 3.5 Page

`open_page` clones `slotted:page`, sets `title` and `body` (with args), pushes.

### 3.6 Hint bar

`slotted:hint_bar` is a `Widget` registered by `MenuPlugin` (so a template writes
`(type: "custom", kind: "slotted:hint_bar")`, and so does a game screen). It is a row of entries,
each a glyph (`hint.glyph`, text from `key_glyph_text` with the resolved `GlyphSet`) and a label
(`hint.label`). `update_hint_bars` (Render) rebuilds `HintEntries` when focus, `InputMode`,
`GlyphSet`, `UiBindings` or the stack changes: the focused node's verb (by `SemanticRole`:
Button → Select, Toggle → Toggle, Slider → Adjust (Left/Right glyphs), Select/RadioGroup → Change,
TextField → Edit, KeyBinding → Rebind, Slot → Pick up, Tab → Select), a `hint.accept` tag
overriding that verb, `Back` with the screen's back label (`Close` for a modal, `Back` for a page)
when the top's `BackPolicy` is `Pop`, `TabPrev`/`TabNext` when the screen contains a `tabs` node.
Hidden in `Pointer` mode unless the bar carries `tags: {"hint.always": "true"}`; entries reorder to
put Accept first.

### 3.7 Tests (B)

`crates/slotted-menu/tests/templates.rs`: every embedded template parses and registers; each opens
in three themes headless and the gamepad walk from `initial_focus` reaches every button; main menu
buttons produce `MenuChoice`s with the right ids; pause opens on Menu when nothing is open, not
over a page, and pops on Menu and on Back; `settings` pushes the configured kind; confirm
rewrites title and message with args, `accept` and `cancel` and `Back` each yield exactly one
`ConfirmResult`, danger removes the primary button; toasts stack upward, expire on virtual time,
queue past `max_visible`, and show over a page; page opens and closes; the hint bar lists Select and
Close on a pause button, Adjust with two glyphs on a slider, hides in pointer mode, and re-renders
its glyphs when `GlyphSet` flips. Role completeness at 101.

## 4. Package C: settings

- 4.1 `SettingsSpec` builder (1.2) and `screen_def()`: a `ScreenDef` with `kind = spec.kind`,
  `inherits: slotted:settings`, `initial_focus: "settings.tabs"`, and a root whose one child is a
  `tabs` node with `test_id: "settings.tabs"`, `bind: "<kind path>.tab"`, one `TabDef` per tab and one
  page per tab: a `scroll` (`max_height: "70%"`, `grow: 1`) of rows, each row the
  M1 control the `SettingsRow` names, bound to `key`, plus an anchor `settings.<tab>.end` at the end
  of every page and `settings.<tab>.start` at its top. `defaults()` and `rules()` derive from the
  rows (a slider's `min/max/step` becomes its `ValueRule`, closing FOLLOWUPS M1 item 12; a select's
  options its `options`).
- 4.2 `apply_settings` at startup when a `Settings` resource exists: register `screen_def()` (unless
  that kind is registered), load `SettingsStorage` if present, seed `ValueStore` with saved values
  over defaults (only keys the spec declares; stale saved keys are dropped), insert rules, restore
  `bindings` into `UiBindings` when saved. A `Settings` inserted after startup is applied the frame
  it appears (`Added<Settings>` through a resource-changed check).
- 4.3 Saving: `save_settings` at `Last` when any `ValueChanged` or `BindingChanged` happened this
  frame, and on `AppExit`, writes `SavedSettings { values: declared keys only, bindings: Some }`.
  `SettingsReset` (sent by the template's Reset button through a `MenuChoice` id `reset`, which C
  handles) writes every default back through `SetValue` so guards still apply, and resets
  `UiBindings` to default.
- 4.4 Key conflicts (FOLLOWUPS M1 item 9): when a capture rebinds an action to a key another
  action on the same device already has, the other action loses that key and a toast
  (`slotted.menu.binding_moved`, args `action`) says so. Lives in C's `resolve_binding_conflicts`
  observing `BindingChanged`.
- 4.5 The showcase demo (`examples/showcase/src/settings.rs`) is rebuilt on `SettingsSpec` with
  the same keys, guard, kind `demo:settings` and reset semantics, so the M1 tests keep passing
  except where the contract changes them (Tab no longer opens it; D handles the example wiring).
- 4.6 Tests: `crates/slotted-menu/tests/settings.rs`: the generated screen inherits the frame and
  carries the anchors; defaults and rules derive correctly; a `MemorySettings` round trip: change a
  slider, `save` happened, a fresh app with the same store opens with the saved value; stale keys
  dropped; bindings persist; reset restores defaults through guards; a conflict moves the key and
  toasts; a mod-style injection lands in `settings.<tab>.end`.

## 5. Package D: facade, examples, harness, docs

- `slotted` facade: feature `menu = ["ui", "dep:slotted-menu"]` in default features, `pub use
  slotted_menu as menu`, prelude line, `MenuPlugin` in `SlottedPlugins` under the feature. Note
  `Cargo.toml` and `plugins.rs` are prepared by the skeleton; D verifies `server-check`.
- `examples/menus` (native, `publish = false`): a main menu over the showcase backdrop; Play pushes
  the chest (the showcase chest lib), Escape pops it, Escape again pauses, Settings opens the
  showcase `SettingsSpec` screen with `FileSettings` under the scratch dir, Quit confirms with a
  danger button and then exits (from the pause it confirms and returns to the title, which is
  what `MenuChoice::screen` is for), quick stack shows a toast, an `About` page from the main menu.
  `--shot` flags: `--main`, `--pause`, `--confirm`, `--page`, `--toast` in the three themes;
  reference shots under `examples/menus/shots/`.
- `UiBindings::default()` `Menu` = Escape + Start (was Tab), `docs/guide/input.md` updated.
- `examples/chest`: Tab no longer opens settings; Escape with nothing open pauses (through
  `MenuPlugin`), pause → Settings opens `demo:settings`. Its M1 test changes accordingly.
- Harness: `toasts()`, `hint_entries(entity)`, `confirm_accept()` / `confirm_cancel()` (press the
  buttons by id), `menu_actions()` drained since the last call.
- Docs: `docs/guide/menus.md` (the crate: config, templates and their anchors, overriding a
  template, `MenuChoice`, confirm, toast, page, hint bar, `SettingsSpec`, persistence, wasm
  bridge); `screens.md` and `input.md` touch-ups; `themes.md` the new roles; `PLAN.md` M2 done;
  `CHANGELOG.md`; `FOLLOWUPS.md` "Menus M2" heading with every deferral.
- Tests: `examples/menus/tests/flow.rs` drives main → play → pause → settings → change → back →
  quit → confirm by gamepad, headless, and snapshots the main menu, pause and confirm trees in
  three themes.

## 6. Done when

- `just ci` green on native and wasm; `slotted-menu` checks on `wasm32-unknown-unknown`.
- A game adds `SlottedPlugins::default()`, inserts `Settings { spec }` and `MenuConfig`, and gets a
  main menu, pause, settings with persistence, confirm, toast and page with no screen file of its
  own; a game that registers `slotted:pause` first gets its own pause.
- `examples/menus` runs the whole flow by gamepad headless and the reference shots exist for
  three themes.
- FOLLOWUPS M1 items 7 (materials tuned for the templates), 8, 9, 12, 20, 21 are closed.
