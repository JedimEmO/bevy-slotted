//! Adversarial review of the gap-closing round's UI work.
//!
//! Four questions, each about a case where the obvious implementation panics
//! or silently loses something a player was looking at.
//!
//! 1. Inheritance and injection pulling in opposite directions: a child screen
//!    deletes the anchor a mod's injection is aimed at. One of them has to
//!    give, and the answer must be a report rather than a panic or a silently
//!    dropped node.
//! 2. A `*.screen.ron` edited into something that does not parse, under a
//!    screen that is open. The screen on screen must survive, and somebody
//!    must say so.
//! 3. `UiScale` changed while a screen is open. Everything measured in another
//!    space has to be reconverted, and the stack on the cursor must not be a
//!    casualty of the relayout.
//! 4. A label whose localisation key nothing defines. It falls back to
//!    something legible, and the tree asks for it once rather than once a
//!    frame.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use pretty_assertions::assert_eq;
use slotted_test::prelude::*;
use slotted_ui::{
    AnchorId, FailedScreenAssets, Injection, Injections, LocKey, Localization, Localizer,
    ScreenAssets, ScreenDef, Screens, Tags, UiNodeDef, UnmatchedInjections,
};

// ------------------------------------------------------- warning collection

/// Collects `WARN` and `ERROR` events, so a test can assert that a data error
/// was reported rather than swallowed, and reported once.
#[derive(Clone, Default)]
struct Reports {
    lines: Arc<Mutex<Vec<String>>>,
}

impl Reports {
    fn capture<T>(&self, f: impl FnOnce() -> T) -> T {
        tracing::subscriber::with_default(Collector(self.lines.clone()), f)
    }

    fn mentioning(&self, needle: &str) -> usize {
        self.lines
            .lock()
            .expect("no panic while holding the lock")
            .iter()
            .filter(|line| line.contains(needle))
            .count()
    }

    /// Whether this collector saw anything at all.
    ///
    /// `tracing::subscriber::with_default` installs a *thread-local* default,
    /// and a callsite whose interest was first decided on another thread can
    /// stay cached as uninteresting, so a scoped collector does not reliably
    /// see an event raised inside a Bevy app while sibling tests run in
    /// parallel. Every assertion about a log line here is therefore guarded by
    /// this, and the assertion that carries the test is on state instead.
    fn saw_anything(&self) -> bool {
        !self
            .lines
            .lock()
            .expect("no panic while holding the lock")
            .is_empty()
    }
}

struct Collector(Arc<Mutex<Vec<String>>>);

impl tracing::Subscriber for Collector {
    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        *metadata.level() <= tracing::Level::WARN
    }

    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }

    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}

    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}

    fn event(&self, event: &tracing::Event<'_>) {
        let mut text = String::new();
        event.record(&mut Fields(&mut text));
        self.0
            .lock()
            .expect("no panic while holding the lock")
            .push(text);
    }

    fn enter(&self, _: &tracing::span::Id) {}

    fn exit(&self, _: &tracing::span::Id) {}
}

/// Flattens an event's fields into one string, so a report can be matched on
/// whatever it named the offending value.
struct Fields<'a>(&'a mut String);

impl tracing::field::Visit for Fields<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        use std::fmt::Write as _;
        let _ = write!(self.0, " {}={value:?}", field.name());
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        use std::fmt::Write as _;
        let _ = write!(self.0, " {}={value}", field.name());
    }
}

// ------------------------------------------ 1. inheritance versus injection

/// The base screen: a header holding an anchor a mod can reach.
const BASE: &str = r#"#![enable(implicit_some)]
(
    kind: "gaps:base",
    root: (
        type: "panel",
        role: "panel",
        layout: (direction: "column"),
        tags: {"test_id": "base_panel"},
        children: [
            (type: "text", key: "chest.title", style: "title", tags: {"test_id": "title"}),
            (type: "panel", role: "panel", tags: {"test_id": "header"}, children: [
                (type: "anchor", id: "rail"),
            ]),
        ],
    ),
)
"#;

/// A variant that deletes the anchor itself. The anchor's id is its merge
/// identity, so `remove` reaches it exactly the way it reaches a `test_id`.
const DROPS_ANCHOR: &str = r#"#![enable(implicit_some)]
(
    kind: "gaps:drops_anchor",
    inherits: "gaps:base",
    remove: ["rail"],
    root: (
        type: "panel",
        role: "panel",
        layout: (direction: "column"),
        tags: {"test_id": "variant_panel"},
        children: [
            (type: "text", key: "chest.footer", style: "muted", tags: {"test_id": "footer"}),
        ],
    ),
)
"#;

/// A variant that deletes the whole node the anchor was inside, which is the
/// same loss by a different route: the anchor is not named anywhere.
const DROPS_HEADER: &str = r#"#![enable(implicit_some)]
(
    kind: "gaps:drops_header",
    inherits: "gaps:base",
    remove: ["header"],
    root: (
        type: "panel",
        role: "panel",
        layout: (direction: "column"),
        tags: {"test_id": "variant_panel"},
        children: [],
    ),
)
"#;

fn injecting_harness(screens: &[&str], injections: Vec<Injection>) -> UiHarness {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .build();
    {
        let mut registry = h.world_mut().resource_mut::<Screens>();
        for text in screens {
            registry.register(ScreenDef::from_ron(text).unwrap());
        }
    }
    h.world_mut().insert_resource(Injections(injections));
    h
}

fn injection(target: &str, anchor: &str, test_id: &str) -> Injection {
    let tags = Tags::new().with(Tags::TEST_ID, test_id);
    Injection {
        target: ScreenKind::new(target),
        anchor: AnchorId::new(anchor),
        node: UiNodeDef::Text {
            key: LocKey("mod.button".to_owned()),
            style: slotted_ui::TextRole::Body,
            tags,
        },
        exclusion: false,
        owner: slotted_ui::Owner::Game,
    }
}

/// The anchor exists: the injected node lands in it. This is the control, so
/// that the two tests below are measuring an absence rather than an injection
/// system that never worked.
#[test]
fn an_injection_lands_in_an_anchor_the_merged_tree_kept() {
    let mut h = injecting_harness(&[BASE], vec![injection("gaps:base", "rail", "injected")]);
    h.open_screen(ScreenKind::new("gaps:base"), ChestFixture::empty());
    h.settle();

    let injected = h.find(&by::test_id("injected"));
    let header = h.find(&by::test_id("header"));
    // The anchor keeps a node of its own and the injection becomes its child,
    // so the injected node is the header's grandchild rather than its child.
    let anchor = h
        .world()
        .get::<ChildOf>(injected)
        .map(ChildOf::parent)
        .expect("the injected node has a parent");
    assert_eq!(
        h.world().get::<SemanticRole>(anchor).cloned(),
        Some(SemanticRole::Anchor),
        "the injected node hangs off the anchor node"
    );
    assert_eq!(
        h.world().get::<ChildOf>(anchor).map(ChildOf::parent),
        Some(header),
        "and the anchor still stands where it was written"
    );
    let roots = h.find_all(&by::screen(ScreenKind::new("gaps:base")));
    assert!(
        roots
            .iter()
            .all(|e| h.world().get::<UnmatchedInjections>(*e).is_none()),
        "nothing was reported unmatched"
    );
}

/// A child that removes the anchor a sibling's injection targets: the screen
/// still opens, the injection is dropped, and the drop is recorded on the
/// screen root and logged.
///
/// The alternative designs are both worse. Panicking makes one mod's typo take
/// the game down. Re-adding the anchor would let an injection override a
/// screen author's explicit `remove`, which is the one thing `remove` is for.
#[test]
fn a_child_removing_an_anchor_a_sibling_injection_targets_reports_it() {
    for (kind, screens) in [
        ("gaps:drops_anchor", [BASE, DROPS_ANCHOR]),
        ("gaps:drops_header", [BASE, DROPS_HEADER]),
    ] {
        let reports = Reports::default();
        let h = reports.capture(|| {
            let mut h = injecting_harness(&screens, vec![injection(kind, "rail", "injected")]);
            h.open_screen(ScreenKind::new(kind), ChestFixture::empty());
            h.settle();
            h
        });

        assert!(
            h.try_find(&by::test_id("injected")).is_none(),
            "{kind}: the anchor is gone, so the injected node has nowhere to be"
        );
        assert!(
            h.try_find(&by::test_id("variant_panel")).is_some(),
            "{kind}: the screen still opened"
        );

        let root = h.find(&by::screen(ScreenKind::new(kind)));
        let unmatched = h
            .world()
            .get::<UnmatchedInjections>(root)
            .unwrap_or_else(|| panic!("{kind}: the dropped injection was not recorded"));
        assert_eq!(
            unmatched.0,
            vec![AnchorId::new("rail")],
            "{kind}: and it names the anchor that was missing"
        );
        assert!(
            !reports.saw_anything() || reports.mentioning("rail") >= 1,
            "{kind}: and it was logged, not only recorded"
        );
    }
}

/// Two injections aimed at the same missing anchor with a third in between.
///
/// The report is deduplicated with `Vec::dedup`, which only collapses
/// *adjacent* equal entries, so an interleaved list keeps the duplicate and
/// the same anchor is both logged twice and listed twice.
#[test]
fn a_missing_anchor_named_by_two_injections_is_reported_once() {
    let reports = Reports::default();
    let kind = "gaps:drops_anchor";
    let h = reports.capture(|| {
        let mut h = injecting_harness(
            &[BASE, DROPS_ANCHOR],
            vec![
                injection(kind, "rail", "first"),
                injection(kind, "other", "second"),
                injection(kind, "rail", "third"),
            ],
        );
        h.open_screen(ScreenKind::new(kind), ChestFixture::empty());
        h.settle();
        h
    });

    let root = h.find(&by::screen(ScreenKind::new(kind)));
    let unmatched = h
        .world()
        .get::<UnmatchedInjections>(root)
        .expect("recorded");
    let rails = unmatched
        .0
        .iter()
        .filter(|a| **a == AnchorId::new("rail"))
        .count();
    assert_eq!(
        rails, 1,
        "one missing anchor is one entry however many injections named it: {:?}",
        unmatched.0
    );
    assert!(
        !reports.saw_anything() || reports.mentioning("rail") == 1,
        "one line in the log, not one per injection"
    );
}

// ------------------------------------------------ 2. a screen edited to junk

const SCREEN_PATH: &str = "screens/gaps.screen.ron";
const GLASS: &str = include_str!("../../../assets/themes/glass.theme.ron");

fn screen_ron(label: &str) -> String {
    format!(
        r#"#![enable(implicit_some)]
(
    kind: "gaps:asset",
    root: (
        type: "panel",
        role: "panel",
        layout: (direction: "column", gap: 2.0, padding: 2.0),
        tags: {{"test_id": "asset_panel"}},
        children: [
            (type: "text", key: "chest.title", style: "title", tags: {{"test_id": "{label}"}}),
            (type: "slot_grid", inventory: 0, cols: 9, rows: 3, first: 0, tags: {{"region": "chest"}}),
        ],
    ),
)
"#
    )
}

/// A private asset root that goes away with the test.
struct AssetRoot(std::path::PathBuf);

impl AssetRoot {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("slotted-review-gaps-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("screens")).unwrap();
        std::fs::create_dir_all(dir.join("themes")).unwrap();
        std::fs::write(dir.join("themes/glass.theme.ron"), GLASS).unwrap();
        Self(dir)
    }

    fn write_screen(&self, label: &str) {
        std::fs::write(self.0.join(SCREEN_PATH), screen_ron(label)).unwrap();
    }

    fn write_raw(&self, text: &str) {
        std::fs::write(self.0.join(SCREEN_PATH), text).unwrap();
    }
}

impl Drop for AssetRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Runs frames until `done`, or fails naming what never happened.
///
/// Asset loads finish on the IO pool, so the frame they land on is not fixed,
/// and on a machine already running the rest of the workspace's tests it is
/// not even a fixed *number* of frames: the pool is starved while a few
/// hundred frames of an otherwise empty app go by in milliseconds. Hence the
/// millisecond between frames -- it is what the wait is actually waiting for.
fn pump_until(h: &mut UiHarness, what: &str, mut done: impl FnMut(&UiHarness) -> bool) {
    for _ in 0..2000 {
        if done(h) {
            return;
        }
        h.step(1);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    panic!("{what} did not happen within 2000 frames");
}

fn registered(h: &UiHarness) -> bool {
    h.world()
        .resource::<Screens>()
        .get(&ScreenKind::new("gaps:asset"))
        .is_some()
}

/// A screen file rewritten into something that does not parse: the screen that
/// is open keeps its tree, the registered definition is untouched, and the
/// parse failure is reported.
///
/// Three separate shapes of broken, because they fail at three different
/// layers: unbalanced RON never reaches serde, a well-formed file with the
/// wrong fields fails in serde, and a file that parses but names a widget kind
/// nobody registered fails at spawn.
#[test]
fn an_unparseable_screen_edit_keeps_the_open_screen_and_reports() {
    for (name, broken) in [
        ("unbalanced", "( kind: \"gaps:asset\""),
        (
            "wrong_fields",
            "#![enable(implicit_some)]\n( kind: \"gaps:asset\", nonsense: 4 )\n",
        ),
        ("empty", ""),
    ] {
        let root = AssetRoot::new(name);
        root.write_screen("first");

        let mut h = UiHarness::builder()
            .plugins(SlottedPlugins::headless().set(AssetPlugin {
                file_path: root.0.to_string_lossy().into_owned(),
                ..default()
            }))
            .registries(TestRegistries::basic())
            .theme("glass")
            .build();
        let server = h.world().resource::<AssetServer>().clone();
        let handle = h
            .world_mut()
            .resource_mut::<ScreenAssets>()
            .load(&server, SCREEN_PATH);
        pump_until(&mut h, "the screen asset loaded", registered);
        h.open_screen(ScreenKind::new("gaps:asset"), ChestFixture::filled());
        h.settle();
        assert!(
            h.world().resource::<FailedScreenAssets>().is_empty(),
            "{name}: nothing is failing before the file is broken"
        );

        root.write_raw(broken);
        server.reload(SCREEN_PATH);
        // The report is asserted through `FailedScreenAssets` rather than
        // through the log, because a scoped `tracing` subscriber does not
        // reliably see an event raised inside a Bevy app while other tests in
        // the same binary run in parallel. The resource is the better contract
        // anyway: a game can draw it in a developer overlay, where a log line
        // only reaches a terminal.
        pump_until(&mut h, "the failed load was recorded", |h| {
            h.world()
                .resource::<FailedScreenAssets>()
                .contains(handle.id())
        });

        assert!(
            registered(&h),
            "{name}: the good definition is still registered"
        );
        assert!(
            h.try_find(&by::test_id("first")).is_some(),
            "{name}: the tree on screen is untouched"
        );
        assert!(
            !h.find_all(&by::role(SemanticRole::Slot)).is_empty(),
            "{name}: and its slots are still there"
        );
        h.step(60);
        assert_eq!(
            h.world().resource::<FailedScreenAssets>().len(),
            1,
            "{name}: one failing screen, and it stays recorded rather than \
             flickering as the system re-reads the load state"
        );

        // And a later good edit still lands, so a bad edit is not a dead end.
        let server = h.world().resource::<AssetServer>().clone();
        root.write_screen("second");
        server.reload(SCREEN_PATH);
        pump_until(&mut h, "the repaired screen respawned", |h| {
            h.try_find(&by::test_id("second")).is_some()
        });
        assert!(
            h.world().resource::<FailedScreenAssets>().is_empty(),
            "{name}: a file that loads again is no longer recorded as failing, \
             so the next break is reported too"
        );
    }
}

// ----------------------------------------------------------- 3. `UiScale`

/// A chest screen with a grid, and nothing else to distract the measurement.
fn scale_screen() -> ScreenDef {
    ScreenDef {
        kind: ScreenKind::new("gaps:scale"),
        inherits: None,
        remove: vec![],
        root: UiNodeDef::Panel {
            role: slotted_theme::roles::PANEL,
            layout: slotted_ui::Layout::default(),
            children: vec![UiNodeDef::SlotGrid {
                inventory: MenuDef::CONTAINER,
                cols: 9,
                rows: 3,
                first: 0,
                tags: Tags::default(),
            }],
            tags: Tags::default(),
        },
        listring: vec![],
    }
}

/// `UiScale` changed under an open screen: the slots and the browser are laid
/// out again against the new scale, and the stack on the cursor survives.
///
/// The carried stack is the interesting half. It is not in the screen tree --
/// it lives on a layer of its own, positioned from the pointer, which is
/// measured in logical pixels and has to be divided by the scale. A relayout
/// that dropped it would lose a real player's items, and it would look like
/// the click that picked them up had never happened.
#[test]
fn changing_ui_scale_under_an_open_screen_relays_out_and_keeps_the_carried_stack() {
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .resolution(1920.0, 1080.0)
        .ui_scale(1.0)
        .theme("glass")
        .build();
    h.world_mut()
        .resource_mut::<Screens>()
        .register(scale_screen());
    h.world_mut().resource_mut::<ScreenHandlers>().register(
        ScreenKind::new("gaps:scale"),
        Arc::new(DefaultScreenHandler),
    );

    let opened = h.open_screen(ScreenKind::new("gaps:scale"), ChestFixture::filled());
    h.browser().wait_for_index();
    h.settle();

    // Pick a stack up, so there is something to lose.
    let filled = h
        .find_all(&by::role(SemanticRole::Slot))
        .into_iter()
        .find(|e| h.stack_at(*e).is_some())
        .expect("the filled fixture put something in the chest");
    h.click(filled);
    h.settle();
    let carried = h.carried(opened.menu).expect("a stack is on the cursor");

    let slot = *h
        .find_all(&by::role(SemanticRole::Slot))
        .first()
        .expect("slots");
    let slot_at_one = h.world().get::<ComputedNode>(slot).unwrap().size();
    let cols_at_one = h.browser().layout(opened.screen).expect("docked").cols;

    // The player drags the accessibility slider while the chest is open.
    h.world_mut().insert_resource(UiScale(2.0));
    h.settle();

    let slot_at_two = h
        .world()
        .get::<ComputedNode>(*h.find_all(&by::role(SemanticRole::Slot)).first().unwrap())
        .unwrap()
        .size();
    assert!(
        (slot_at_two - slot_at_one * 2.0).abs().max_element() < 0.5,
        "a slot is drawn twice as big once the scale doubles: {slot_at_one} then {slot_at_two}"
    );

    let layout_at_two = h.browser().layout(opened.screen).expect("still docked");
    assert!(
        layout_at_two.cols < cols_at_one,
        "half as many UI units of free strip fits fewer card columns: {cols_at_one} then {}",
        layout_at_two.cols
    );

    assert_eq!(
        h.carried(opened.menu),
        Some(carried.clone()),
        "the stack on the cursor came through the relayout untouched"
    );
    let carried_node = h
        .world_mut()
        .query_filtered::<Entity, With<slotted_ui::CarriedItem>>()
        .single(h.world())
        .expect("one carried node");
    assert_eq!(
        h.displayed_stack(carried_node),
        Some(carried),
        "and is still drawn"
    );

    // And back down again, because a scale change is not a one-way door.
    h.world_mut().insert_resource(UiScale(1.0));
    h.settle();
    let back = h
        .world()
        .get::<ComputedNode>(*h.find_all(&by::role(SemanticRole::Slot)).first().unwrap())
        .unwrap()
        .size();
    assert!(
        (back - slot_at_one).abs().max_element() < 0.5,
        "the slots return to where they were: {slot_at_one} then {back}"
    );
    assert!(
        h.carried(opened.menu).is_some(),
        "and the cursor still holds its stack"
    );
}

// --------------------------------------------------- 4. a missing loc key

/// Counts what was asked for, so "falls back once" can be measured rather
/// than eyeballed.
#[derive(Default)]
struct CountingLocalizer {
    asked: AtomicUsize,
    keys: Mutex<Vec<String>>,
}

impl CountingLocalizer {
    fn asked(&self) -> usize {
        self.asked.load(Ordering::Relaxed)
    }

    /// Every key asked for since the last call, so a failure names what kept
    /// asking rather than only how often.
    fn drain(&self) -> Vec<String> {
        let mut keys = std::mem::take(&mut *self.keys.lock().expect("no panic"));
        keys.sort();
        keys.dedup();
        keys
    }
}

/// A catalogue that knows one key, for the repaint half of the test.
struct RealLocalizer;

impl Localizer for RealLocalizer {
    fn resolve(&self, key: &LocKey) -> Option<String> {
        (key.0 == "chest.title").then(|| "Copper Chest".to_owned())
    }
}

impl Localizer for CountingLocalizer {
    fn resolve(&self, key: &LocKey) -> Option<String> {
        self.asked.fetch_add(1, Ordering::Relaxed);
        self.keys.lock().expect("no panic").push(key.0.clone());
        None
    }
}

/// A label whose key nothing defines draws the key itself, once, and the tree
/// does not ask again on every frame.
///
/// Drawing the key is deliberate: an unresolved key on screen is a legible bug
/// report where an empty label is not. What must not happen is the resolve
/// running once a frame for ever, which is what a naive "rewrite every label
/// each frame" render does and what a profile would never show as a bug.
#[test]
fn a_missing_localisation_key_falls_back_to_the_key_and_is_resolved_once() {
    let counter = Arc::new(CountingLocalizer::default());
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .build();
    h.world_mut()
        .resource_mut::<Screens>()
        .register(ScreenDef::from_ron(BASE).unwrap());
    h.world_mut()
        .insert_resource(Localization(counter.clone() as Arc<dyn Localizer>));

    h.open_screen(ScreenKind::new("gaps:base"), ChestFixture::empty());
    h.settle();

    let title = h.find(&by::test_id("title"));
    assert_eq!(
        h.text_of(title).as_deref(),
        Some("chest.title"),
        "an unresolved key is drawn as written, so the bug is visible"
    );

    let after_open = counter.asked();
    assert!(after_open > 0, "the label did ask");
    counter.drain();
    h.step(30);
    assert_eq!(
        counter.asked(),
        after_open,
        "and did not ask again on any of the next thirty frames; it kept asking for {:?}",
        counter.drain()
    );
    assert_eq!(
        h.text_of(title).as_deref(),
        Some("chest.title"),
        "and still reads the same"
    );

    // Replacing the catalogue is what a locale load does, and it must repaint.
    h.world_mut()
        .insert_resource(Localization::new(RealLocalizer));
    h.settle();
    assert_eq!(
        h.text_of(title).as_deref(),
        Some("Copper Chest"),
        "a catalogue arriving after the screen opened repaints the label"
    );
}

/// The browser's chips take the same path from the other side: a category
/// whose title key nothing defines still gets a legible chip, and one chip,
/// not one per category layer.
#[test]
fn a_chip_whose_title_key_is_missing_still_draws_one_legible_chip() {
    let counter = Arc::new(CountingLocalizer::default());
    let mut h = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .registries(TestRegistries::basic())
        .resolution(1920.0, 1080.0)
        .theme("glass")
        .build();
    h.world_mut()
        .resource_mut::<Screens>()
        .register(scale_screen());
    h.world_mut().resource_mut::<ScreenHandlers>().register(
        ScreenKind::new("gaps:scale"),
        Arc::new(DefaultScreenHandler),
    );
    h.world_mut()
        .insert_resource(Localization(counter.clone() as Arc<dyn Localizer>));

    h.open_screen(ScreenKind::new("gaps:scale"), ChestFixture::filled());
    h.browser().wait_for_index();
    h.settle();

    let chips = h.find_all(&by::role(SemanticRole::Chip));
    assert!(!chips.is_empty(), "the browser drew its category chips");
    for chip in &chips {
        let text = h.text_of(*chip).unwrap_or_default();
        assert!(
            !text.trim().is_empty(),
            "a chip with no locale entry still says something: {chips:?}"
        );
    }
    let labels: Vec<String> = chips
        .iter()
        .map(|c| h.text_of(*c).unwrap_or_default())
        .collect();
    let mut unique = labels.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(
        unique.len(),
        labels.len(),
        "one chip a category, not the same fallback repeated: {labels:?}"
    );

    let after_open = counter.asked();
    counter.drain();
    h.step(30);
    assert_eq!(
        counter.asked(),
        after_open,
        "the chips do not re-resolve their missing keys every frame; kept asking for {:?}",
        counter.drain()
    );
}
