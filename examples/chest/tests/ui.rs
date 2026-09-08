//! What a consumer's own UI test suite looks like.
//!
//! These tests open `assets/screens/demo_chest.screen.ron` and the content of
//! `assets/data/demo/` — the same two files `cargo run -p chest` reads — with
//! no window, no GPU and no wall clock, then drive them through the public
//! harness. Nothing here reaches into the example's internals: it is all
//! `slotted_test` plus the three functions `chest` exposes for its own
//! `main.rs`.
//!
//! The theme is found because `examples/chest/assets` is a symlink to the
//! workspace `assets/`, which is where Bevy's `AssetPlugin` looks when it
//! resolves against `CARGO_MANIFEST_DIR`.
#![allow(clippy::unwrap_used)]

use bevy::prelude::*;
use chest::{ChestBinding, ChestDemoPlugin, ChestMenuPlugin};
use pretty_assertions::assert_eq;
use slotted_model::{InventoryRef, ToolbarAction};
use slotted_test::prelude::*;

/// The demo, headless: the RON screen over the RON content, with the demo's
/// own key bindings attached so `Esc` and `E` behave as they do on screen.
fn open_demo_chest() -> (UiHarness, Opened) {
    open_demo_chest_in("glass")
}

/// The demo under one of the three shipped themes.
fn open_demo_chest_in(theme: &str) -> (UiHarness, Opened) {
    let registries = chest::load_registries();
    let mut harness = UiHarness::builder()
        .plugins(SlottedPlugins::headless())
        .plugins((ChestDemoPlugin, ChestMenuPlugin))
        .registries(registries.clone())
        .resolution(1600.0, 900.0)
        .theme(theme)
        .build();
    let opened = harness.open_screen(
        chest::demo_screen(),
        (chest::menu_def(), chest::inventories(&registries)),
    );
    harness.settle();
    (harness, opened)
}

fn chest_slot(harness: &UiHarness, n: usize) -> Entity {
    harness.find(&by::role(SemanticRole::Slot).tag("region", "chest").index(n))
}

fn item(harness: &UiHarness, name: &str) -> ItemId {
    let id = slotted_model::Namespaced::parse(name).unwrap();
    harness
        .world()
        .resource::<Registries>()
        .item_id(&id)
        .unwrap_or_else(|| panic!("{name} is not registered"))
}

/// The whole screen, as the harness sees it. If the RON file or a widget's
/// spawned shape changes, this is the test that says so.
#[test]
fn the_screen_tree_matches_the_ron_file() {
    let (harness, _) = open_demo_chest();

    // The three grids of the screen file: 27 + 27 + 9.
    assert_eq!(harness.find_all(&by::role(SemanticRole::Slot)).len(), 63);
    assert_eq!(harness.find_all(&by::role(SemanticRole::Hotbar)).len(), 1);
    assert!(harness.try_find(&by::anchor("title_end")).is_some());
    assert_eq!(
        harness
            .text_of(harness.find(&by::test_id("title")))
            .as_deref(),
        Some("Copper Chest")
    );

    // Nine of the chest's twenty-seven slots start filled.
    assert_eq!(
        harness
            .text_of(harness.find(&by::test_id("capacity")))
            .as_deref(),
        Some("9 / 27 slots")
    );

    // The browser panel is a screen root of its own and has its own snapshot
    // in `slotted-browser`, so this one stays about the RON file: it drops the
    // panel and keeps the chest and the carried layer.
    let mut tree = harness.screen_tree();
    tree.roots
        .retain(|root| root.screen.as_deref() != Some("slotted:browser"));
    assert_tree_snapshot!(tree);
    harness.assert_conserved();
}

/// The one line a game writes to get an overlay: registering a
/// `ScreenHandler` for its screen kind. The panel docks beside the chest
/// rather than over it.
#[test]
fn the_browser_panel_docks_beside_the_chest() {
    let (mut harness, opened) = open_demo_chest();
    harness.browser().wait_for_index();
    harness.settle();

    assert!(harness.browser().is_attached(opened.screen));
    let layout = harness.browser().layout(opened.screen).expect("docked");
    let chest = harness.rect_of(harness.find(&by::test_id("title")));
    assert!(
        layout.rect.min.x >= chest.max.x || layout.rect.max.x <= chest.min.x,
        "the panel and the chest do not overlap"
    );
    assert!(harness.try_find(&by::test_id("browser.search")).is_some());
    assert!(!harness.find_all(&by::role(SemanticRole::Card)).is_empty());
    harness.assert_conserved();
}

/// Typing into the panel's search field narrows the cards, over the demo's
/// own content files rather than a fixture.
#[test]
fn a_search_in_the_panel_narrows_the_demo_content() {
    let (mut harness, _) = open_demo_chest();
    harness.browser().wait_for_index();
    harness.settle();

    let all = harness.browser().visible_entries().len();
    assert!(all > 5, "the demo registers thirteen items and nine tags");

    harness.browser().search("ender");
    let narrowed = harness.browser().visible_entries();
    assert!(!narrowed.is_empty() && narrowed.len() < all);
    let cards = harness.browser().visible_cards();
    assert_eq!(cards, vec!["minecraft:ender_pearl".to_owned()]);
    harness.assert_conserved();
}

/// The content files reached the widgets: the stack the data stage built is
/// the stack the slot renders.
#[test]
fn the_data_stage_content_reaches_the_slots() {
    let (harness, _) = open_demo_chest();

    let first = chest_slot(&harness, 0);
    let stack = harness.stack_at(first).expect("chest slot 0 is filled");
    assert_eq!(stack.id, item(&harness, "minecraft:cobblestone"));
    assert_eq!(stack.count, 64);
    assert_eq!(harness.displayed_stack(first), Some(stack));

    // The ender pearl caps at sixteen, which is what its item file says.
    let pearls = harness.stack_at(chest_slot(&harness, 20)).unwrap();
    assert_eq!(pearls.count, 16);
    assert_eq!(
        harness
            .world()
            .resource::<Registries>()
            .items
            .get(pearls.id)
            .unwrap()
            .max_stack_size,
        16
    );

    // An empty slot is empty, not merely unrendered.
    assert_eq!(harness.stack_at(chest_slot(&harness, 3)), None);
    harness.assert_conserved();
}

/// Shift-click sends a stack across the listring and back, and nothing is
/// created or destroyed on the way.
#[test]
fn a_shift_click_round_trip_conserves_every_item() {
    let (mut harness, opened) = open_demo_chest();

    let source = chest_slot(&harness, 0);
    let cobblestone = harness.stack_at(source).unwrap();
    assert_eq!(cobblestone.count, 64);

    harness.shift_click(source);
    harness.settle();
    assert_eq!(
        harness.stack_at(source),
        None,
        "the whole stack left the chest"
    );

    // It landed somewhere in the player's inventories.
    let moved: u32 = (0..63)
        .filter_map(|i| harness.stack_at(harness.find(&by::role(SemanticRole::Slot).index(i))))
        .filter(|s| s.id == cobblestone.id)
        .map(|s| s.count)
        .sum();
    assert_eq!(
        moved,
        64 + 23 + 9,
        "the chest's two stacks plus the player's"
    );

    // Send it back from wherever it went.
    let landed = harness.find(
        &by::role(SemanticRole::Slot)
            .tag("region", "player")
            .with_item("minecraft:cobblestone"),
    );
    harness.shift_click(landed);
    harness.settle();

    assert_eq!(opened.inventories.len(), 3);
    harness.assert_conserved();
}

/// The rail button in the screen file is wired to the model's toolbar action.
#[test]
fn the_sort_rail_button_sorts_the_container() {
    let (mut harness, opened) = open_demo_chest();

    let sort = harness.find(&by::tag("action", "sort"));
    assert_eq!(harness.text_of(sort).as_deref(), Some("Sort"));

    // Before: the chest has gaps at 3, 7, 8 and everything after 20.
    assert_eq!(harness.stack_at(chest_slot(&harness, 3)), None);
    let filled_before = (0..27)
        .filter(|i| harness.stack_at(chest_slot(&harness, *i)).is_some())
        .count();

    harness.activate(sort);
    harness.settle();

    // After: the same number of stacks, packed against the front.
    let filled_after = (0..27)
        .filter(|i| harness.stack_at(chest_slot(&harness, *i)).is_some())
        .count();
    assert!(
        filled_after <= filled_before,
        "sorting merges partial stacks, it never adds them"
    );
    for i in 0..filled_after {
        assert!(
            harness.stack_at(chest_slot(&harness, i)).is_some(),
            "slot {i} should be packed"
        );
    }
    assert_eq!(harness.stack_at(chest_slot(&harness, filled_after)), None);

    // The two cobblestone stacks merged into one full stack.
    let cobblestone = item(&harness, "minecraft:cobblestone");
    let total: u32 = (0..27)
        .filter_map(|i| harness.stack_at(chest_slot(&harness, i)))
        .filter(|s| s.id == cobblestone)
        .map(|s| s.count)
        .sum();
    assert_eq!(total, 64 + 23);

    // The rail's action is the one the screen file named.
    let action = harness
        .world()
        .get::<slotted::ui::RailAction>(sort)
        .expect("a rail button carries its action");
    assert_eq!(
        action.0,
        ToolbarAction::Sort {
            inventory: InventoryRef::new(0)
        }
    );

    assert_eq!(
        opened.menu,
        harness
            .world()
            .resource::<ChestBinding>()
            .open
            .unwrap()
            .menu
    );
    harness.assert_conserved();
}

/// `Esc` and `E` through real key events, the same path a player's keyboard
/// takes.
#[test]
fn esc_closes_the_screen_and_e_opens_it_again() {
    let (mut harness, opened) = open_demo_chest();
    assert!(
        harness
            .try_find(&by::screen(ScreenKind::new(chest::CHEST)))
            .is_some()
    );

    harness.key(KeyCode::Escape);
    harness.settle();
    assert!(
        harness
            .try_find(&by::screen(ScreenKind::new(chest::CHEST)))
            .is_none(),
        "Esc despawns the screen"
    );
    assert!(
        harness.world().get_entity(opened.menu).is_err(),
        "and its menu"
    );
    assert!(harness.world().resource::<ChestBinding>().open.is_none());

    harness.key(KeyCode::KeyE);
    harness.settle();
    let reopened = harness.find(&by::screen(ScreenKind::new(chest::CHEST)));
    assert_ne!(reopened, opened.screen, "a fresh screen entity");
    assert_eq!(harness.find_all(&by::role(SemanticRole::Slot)).len(), 63);

    // The same chest, not a new one: the inventory entities outlived the close
    // and the stacks are still where they were.
    let binding = harness.world().resource::<ChestBinding>().clone();
    assert_eq!(binding.inventories, opened.inventories);
    assert_eq!(harness.stack_at(chest_slot(&harness, 0)).unwrap().count, 64);
    assert_eq!(
        harness
            .text_of(harness.find(&by::test_id("capacity")))
            .as_deref(),
        Some("9 / 27 slots")
    );

    harness.assert_conserved();
}

/// The chest is a stack entry, and Escape closes it *through* the stack: no
/// handler of the demo's own reads the key any more. `Back` pops the entry,
/// the stack closes the menu, the carried stack lands in `Dropped`, and the
/// demo's binding notices the root is gone. East on a pad is the same `Back`.
#[test]
fn escape_closes_the_chest_through_the_stack() {
    let (mut harness, opened) = open_demo_chest();
    let kind = ScreenKind::new(chest::CHEST);
    assert_eq!(
        harness.stack(),
        vec![kind.clone()],
        "opened as a stack entry"
    );
    assert_eq!(
        harness
            .world()
            .resource::<slotted::ui::ScreenStack>()
            .top()
            .map(|entry| (entry.root, entry.menu)),
        Some((opened.screen, Some(opened.menu)))
    );

    // Pick something up first, so the close has a carried stack to drop.
    let first = chest_slot(&harness, 0);
    harness.click_slot(first, slotted_model::Button::Left, Modifiers::default());
    harness.settle();
    assert!(harness.carried(opened.menu).is_some());

    harness.key(KeyCode::Escape);
    harness.settle();
    assert!(harness.stack().is_empty(), "Escape popped the entry");
    assert!(harness.try_find(&by::screen(kind.clone())).is_none());
    assert!(harness.world().get_entity(opened.menu).is_err());
    assert!(
        harness.world().resource::<ChestBinding>().open.is_none(),
        "the binding forgot the chest"
    );
    harness.assert_conserved();

    // `E` pushes again; East pops it the same way.
    harness.key(KeyCode::KeyE);
    harness.settle();
    assert_eq!(harness.stack(), vec![kind.clone()]);
    harness.gamepad(bevy::input::gamepad::GamepadButton::East);
    harness.settle();
    assert!(harness.stack().is_empty(), "East popped the entry");
    assert!(harness.try_find(&by::screen(kind)).is_none());
    assert!(harness.world().resource::<ChestBinding>().open.is_none());
    harness.assert_conserved();
}

/// `Esc` is `Back` and `Menu` at once (menus M2): with the chest open it pops
/// the chest, with nothing open it pauses, and on the pause it resumes. The
/// pause's Settings button opens `demo:settings` (the showcase `SettingsSpec`
/// over the `slotted:settings` frame), `Esc` pops it back to the pause, and
/// `Tab` no longer opens anything: it is Bevy's tab-navigation key.
#[test]
fn escape_pauses_with_the_chest_closed_and_the_pause_opens_settings() {
    let (mut harness, _opened) = open_demo_chest();
    let chest = ScreenKind::new(chest::CHEST);
    let settings = ScreenKind::new(chest::SETTINGS);
    let pause = slotted::menu::kinds::pause();
    let first = chest_slot(&harness, 0);
    assert_eq!(harness.focused(), Some(first));

    harness.key(KeyCode::Tab);
    harness.settle();
    assert_eq!(
        harness.stack(),
        vec![chest.clone()],
        "Tab moves focus; it opens nothing"
    );

    harness.key(KeyCode::Escape);
    harness.settle();
    assert_eq!(harness.stack(), vec![], "Escape over the chest pops it");
    assert!(harness.world().resource::<ChestBinding>().open.is_none());

    harness.key(KeyCode::Escape);
    harness.settle();
    assert_eq!(
        harness.stack(),
        vec![pause.clone()],
        "Escape with nothing open pauses"
    );
    let root = harness.find(&by::screen(pause.clone()));
    assert_eq!(
        harness.focused(),
        Some(harness.find(&by::test_id("resume").within(root))),
        "the pause's `initial_focus` is Resume"
    );

    harness.activate(harness.find(&by::test_id("settings").within(root)));
    harness.settle();
    assert_eq!(
        harness.stack(),
        vec![pause.clone(), settings.clone()],
        "Settings on the pause pushed `demo:settings`"
    );
    assert_eq!(
        harness
            .menu_choices()
            .into_iter()
            .map(|c| c.id)
            .collect::<Vec<_>>(),
        vec!["settings"]
    );
    let modal = harness.find(&by::screen(settings.clone()));
    assert!(harness.is_visible(modal));
    assert_eq!(
        harness.focused(),
        Some(harness.find(&by::role(SemanticRole::Tab).within(modal).index(0))),
        "the modal's `initial_focus` is its first tab"
    );
    assert_eq!(
        harness.find_all(&by::control("slider").within(modal)).len(),
        4,
        "the settings controls are up"
    );

    harness.key(KeyCode::Escape);
    harness.settle();
    assert_eq!(
        harness.stack(),
        vec![pause.clone()],
        "Escape popped the settings, not the pause"
    );
    assert!(harness.try_find(&by::screen(settings)).is_none());

    harness.key(KeyCode::Escape);
    harness.settle();
    assert_eq!(harness.stack(), vec![], "Escape on the pause resumes");

    harness.key(KeyCode::KeyE);
    harness.settle();
    assert_eq!(harness.stack(), vec![chest], "E reopens the chest");
    assert!(harness.world().resource::<ChestBinding>().open.is_some());
    harness.assert_conserved();
}

/// Quit on the pause screen opens a danger confirm; Cancel returns to the
/// pause, Accept exits the app.
#[test]
fn quit_on_the_pause_confirms_with_a_danger_button_and_then_exits() {
    let (mut harness, _opened) = open_demo_chest();
    let pause = slotted::menu::kinds::pause();
    let confirm = slotted::menu::kinds::confirm();

    harness.key(KeyCode::Escape);
    harness.settle();
    harness.key(KeyCode::Escape);
    harness.settle();
    assert_eq!(harness.stack(), vec![pause.clone()]);
    let root = harness.find(&by::screen(pause.clone()));
    harness.activate(harness.find(&by::test_id("quit").within(root)));
    harness.settle();
    assert_eq!(
        harness.stack(),
        vec![pause.clone(), confirm.clone()],
        "Quit pushed the confirm over the pause"
    );
    assert_eq!(
        harness
            .menu_choices()
            .into_iter()
            .map(|c| c.id)
            .collect::<Vec<_>>(),
        vec!["quit"]
    );
    let dialog = harness.find(&by::screen(confirm.clone()));
    let accept = harness.find(&by::test_id("accept").within(dialog));
    assert_eq!(
        harness
            .world()
            .get::<slotted::ui::ButtonState>(accept)
            .expect("the accept button")
            .variant,
        slotted::ui::ButtonVariant::Danger,
        "the accept button is the danger one"
    );
    assert_eq!(
        harness
            .text_of(harness.find(&by::test_id("title").within(dialog)))
            .as_deref(),
        Some("Quit?")
    );
    assert_eq!(
        harness.focused(),
        Some(harness.find(&by::test_id("cancel").within(dialog))),
        "the confirm starts on Cancel"
    );

    harness.confirm_cancel();
    harness.settle();
    assert_eq!(
        harness.stack(),
        vec![pause.clone()],
        "Cancel is back on the pause"
    );
    assert!(
        harness.world().resource::<Messages<AppExit>>().is_empty(),
        "nothing exited"
    );

    harness.activate(harness.find(&by::test_id("quit").within(root)));
    harness.settle();
    assert_eq!(harness.stack(), vec![pause, confirm]);
    harness.confirm_accept();
    let exits: Vec<AppExit> = bevy::ecs::message::MessageCursor::default()
        .read(harness.world().resource::<Messages<AppExit>>())
        .cloned()
        .collect();
    assert_eq!(exits, vec![AppExit::Success], "Accept exits");
}

/// Steps until the active theme asset is in `Assets<Theme>`, so a test can
/// read its tokens. The file comes off disk through the asset server's own
/// task, which `settle()` does not know to wait for.
fn wait_for_theme(harness: &mut UiHarness) -> slotted::theme::Theme {
    for _ in 0..600 {
        let world = harness.world();
        let active = world.resource::<slotted::theme::ActiveTheme>().0.clone();
        if let Some(theme) = world
            .resource::<Assets<slotted::theme::Theme>>()
            .get(&active)
        {
            let theme = theme.clone();
            // One more settle so the repaint the load triggers has landed.
            harness.settle();
            return theme;
        }
        harness.step(1);
    }
    panic!("theme never loaded");
}

/// The semantic tree is the screen's, not the theme's: paper and neon paint
/// the same entities the glass theme does, and `screen_tree()` cannot tell
/// them apart. This is the Phase 7 gate on the token set: a direction that
/// needed a different tree would have needed a different widget.
#[test]
fn the_screen_tree_is_theme_independent() {
    let (mut glass, _) = open_demo_chest();
    let glass_theme = wait_for_theme(&mut glass);
    assert_eq!(glass_theme.name, "glass");
    let reference = glass.screen_tree();

    for name in ["paper", "neon"] {
        let (mut harness, _) = open_demo_chest_in(name);
        let theme = wait_for_theme(&mut harness);
        assert_eq!(theme.name, name);
        assert_eq!(harness.screen_tree(), reference, "{name} changed the tree");
        // And the theme really is in force: the panel carries its material.
        let panel = {
            let world = harness.world_mut();
            let mut q = world.query::<(Entity, &slotted::theme::Themed)>();
            q.iter(world)
                .find(|(_, t)| t.0 == slotted::theme::roles::PANEL)
                .map(|(e, _)| e)
                .expect("a themed panel node")
        };
        let world = harness.world();
        match name {
            "paper" => assert!(
                world.get::<ImageNode>(panel).is_some(),
                "paper's panel is a tiled sheet"
            ),
            _ => assert!(
                world.get::<BackgroundColor>(panel).is_some(),
                "neon's panel degrades to a solid without the shader"
            ),
        }
    }
}

/// The theme's motion tokens decide how long a hover takes and on which
/// curve, and the harness clock sees exactly those numbers.
#[test]
fn hover_motion_follows_the_theme_tokens() {
    use slotted::theme::{Easing, MotionPreset, Tween};

    for (name, ms, easing) in [
        ("glass", 90_u64, Easing::Standard),
        ("paper", 120, Easing::Standard),
        ("neon", 90, Easing::Snap),
    ] {
        let (mut harness, _) = open_demo_chest_in(name);
        let theme = wait_for_theme(&mut harness);
        assert_eq!(
            theme.tokens.duration_ms(MotionPreset::Hover),
            u32::try_from(ms).unwrap(),
            "{name}"
        );
        assert_eq!(theme.tokens.easing(MotionPreset::Hover), easing, "{name}");

        let slot = chest_slot(&harness, 3);
        harness.hover(slot);
        let tween = harness
            .world()
            .get::<Tween>(slot)
            .unwrap_or_else(|| panic!("{name}: hover starts a scale tween"))
            .clone();
        assert_eq!(
            tween.duration,
            std::time::Duration::from_millis(ms),
            "{name}"
        );
        assert_eq!(tween.easing, easing, "{name}");

        // The tween runs on the virtual clock and is gone once it has been
        // stepped past its duration.
        harness.advance(std::time::Duration::from_millis(ms + 20));
        assert!(
            harness.world().get::<Tween>(slot).is_none(),
            "{name}: tween finished under the harness clock"
        );
    }
}

/// Paper says "no springs": no preset of its may overshoot. Neon's drop is
/// the one squash that does.
#[test]
fn paper_never_springs_and_neon_squashes_on_drop() {
    use slotted::theme::{Easing, MotionPreset};

    let presets = [
        MotionPreset::Hover,
        MotionPreset::Press,
        MotionPreset::DropSquash,
        MotionPreset::FlyToSlot,
        MotionPreset::Stagger,
        MotionPreset::Fade,
    ];
    let (mut paper, _) = open_demo_chest_in("paper");
    let paper = wait_for_theme(&mut paper);
    for preset in presets {
        assert!(!paper.tokens.easing(preset).overshoots(), "{preset:?}");
        let ms = paper.tokens.duration_ms(preset);
        assert!((120..=200).contains(&ms), "{preset:?} takes {ms} ms");
    }
    let (mut neon, _) = open_demo_chest_in("neon");
    let neon = wait_for_theme(&mut neon);
    assert_eq!(
        neon.tokens.easing(MotionPreset::DropSquash),
        Easing::Overshoot
    );
    for preset in [
        MotionPreset::Hover,
        MotionPreset::Press,
        MotionPreset::DropSquash,
    ] {
        let ms = neon.tokens.duration_ms(preset);
        assert!((90..=140).contains(&ms), "{preset:?} takes {ms} ms");
    }
}
