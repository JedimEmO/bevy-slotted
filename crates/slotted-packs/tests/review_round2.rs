//! The reload seams, where two of the external review's findings meet.
//!
//! `lifecycle.rs` proves each stage. These four put two mechanisms into one
//! scenario, which is where an invalidation rule that is right on its own
//! stops being right:
//!
//! * inheritance (finding 5) against ownership (finding 6): a mod screen that
//!   inherits a game screen, and the game screen then edited through the
//!   asset loader. Both invalidation paths, one scenario.
//! * a mod removed from the mods directory entirely: every registration it
//!   owned goes, the game definition it had taken over comes back, and the
//!   screen only it provided closes.
//! * an injection retargeted from one screen to another: the screen that lost
//!   it respawns without it, the screen that gained it respawns with it.
//! * a reload that fails: the previous owned set is left exactly as it was,
//!   with nothing half-applied.

#![allow(clippy::unwrap_used)]

mod common;

use std::sync::Arc;

use bevy::prelude::*;
use pretty_assertions::assert_eq;
use slotted_model::Value;
use slotted_packs::{ModErrors, ModLoader, PackLayout, ScriptLogs};
use slotted_script::ScriptCommand;
use slotted_ui::def::{AnchorId, ScreenDef, ScreenKind, WidgetKind};
use slotted_ui::{ChangeSet, Injections, Owner, Screens, TestId, WidgetRegistry};

use common::{Harness, copy_dir, fixtures, harness, harness_at, map, mod_id, scratch, text};

// -- shared setup -----------------------------------------------------------

/// The UI registries the pack loader publishes into, seeded the way
/// `SlottedUiPlugin` seeds them.
///
/// The packs harness is built on `MinimalPlugins`, so nothing has inserted a
/// widget registry with the builtin spawners in it. `publish_ui` only
/// `init_resource`s an empty one, which would leave every `panel` in a screen
/// tree unspawnable and make a respawn look like a close.
fn with_ui_registries(harness: &mut Harness) {
    let world = harness.app.world_mut();
    world.init_resource::<Screens>();
    world.init_resource::<Injections>();
    world.init_resource::<slotted_ui::tooltip::TooltipParts>();
    let mut registry = WidgetRegistry::default();
    slotted_ui::widgets::register_builtins(&mut registry);
    world.insert_resource(registry);
    // `apply_invalidation` reads this buffer to turn a closed screen into a
    // mod-log warning; without it the close happens and nobody is told.
    world.init_resource::<Messages<slotted_ui::ScreenDropped>>();
}

/// A panel with one anchor and one tagged text node, as a script would send
/// it: `kind` is filled in by the data stage from the registration id.
fn tree(label: &str, anchor: &str) -> Value {
    map([(
        "root",
        map([
            ("type", text("panel")),
            ("role", text("panel")),
            (
                "children",
                Value::List(vec![
                    map([("type", text("anchor")), ("id", text(anchor))]),
                    map([
                        ("type", text("text")),
                        ("key", text(label)),
                        ("style", text("body")),
                        ("tags", map([("test_id", text(label))])),
                    ]),
                ]),
            ),
        ]),
    )])
}

/// The same, with an `inherits` line.
fn derived_tree(parent: &str, label: &str) -> Value {
    let Value::Map(mut fields) = tree(label, "derived_anchor") else {
        unreachable!("tree builds a map")
    };
    fields.insert("inherits".to_owned(), text(parent));
    Value::Map(fields)
}

/// A game-registered screen, the kind a Rust game writes at startup.
fn game_screen(kind: &str, label: &str) -> ScreenDef {
    ScreenDef::from_ron(&format!(
        r#"(
            kind: "{kind}",
            root: (
                type: "panel", role: "panel", tags: {{"test_id": "root"}},
                children: [
                    (type: "anchor", id: "base_anchor"),
                    (type: "text", key: "{label}", style: "body", tags: {{"test_id": "{label}"}}),
                ],
            ),
        )"#
    ))
    .expect("a valid screen")
}

/// Opens a screen of `kind` on its own menu-less root, the way a game opens
/// one, and returns the root entity.
fn open(harness: &mut Harness, kind: &str) -> Entity {
    let kind = ScreenKind::new(kind);
    let def = harness
        .app
        .world()
        .resource::<Screens>()
        .get(&kind)
        .unwrap_or_else(|| panic!("`{}` is registered", kind.0))
        .clone();
    let world = harness.app.world_mut();
    let root = {
        let mut commands = world.commands();
        slotted_ui::spawn_screen(&mut commands, def, None)
    };
    world.flush();
    root
}

/// Every open screen root, as `(entity, kind)`.
fn roots(harness: &Harness) -> Vec<(Entity, String)> {
    let mut out: Vec<(Entity, String)> = harness
        .app
        .world()
        .iter_entities()
        .filter_map(|entity| {
            entity
                .get::<slotted_ui::ScreenRoot>()
                .map(|root| (entity.id(), root.kind.0.to_string()))
        })
        .collect();
    out.sort_by_key(|(_, kind)| kind.clone());
    out
}

/// The root entity currently drawing `kind`, if any.
fn root_of(harness: &Harness, kind: &str) -> Option<Entity> {
    roots(harness)
        .into_iter()
        .find(|(_, k)| k == kind)
        .map(|(entity, _)| entity)
}

/// The `test_id` tags under `root`, so a test can say what a respawned tree
/// actually drew rather than only that it respawned.
fn test_ids(harness: &Harness, root: Entity) -> Vec<String> {
    let world = harness.app.world();
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(entity) = stack.pop() {
        if let Some(id) = world.get::<TestId>(entity) {
            out.push(id.0.clone());
        }
        if let Some(children) = world.get::<Children>(entity) {
            stack.extend(children.iter());
        }
    }
    out.sort();
    out
}

fn owner_of(harness: &Harness, kind: &str) -> Option<Owner> {
    harness
        .app
        .world()
        .resource::<Screens>()
        .owner(&ScreenKind::new(kind))
        .cloned()
}

// -- 5 meets 6: inheritance, ownership and the asset loader -----------------

/// A mod screen inherits a game screen; then the game screen is edited on
/// disk and comes back through the asset loader.
///
/// Two invalidation paths in one scenario, and they are the two that used to
/// miss each other. Respawning by name equality would have moved the game
/// screen and left the mod screen that draws *from* it untouched, showing the
/// old base for the rest of the process. And an asset-owned entry must
/// survive a mod reload: `reconcile_mods` replaces the mod-owned set, and an
/// `Owner::Asset` screen is not part of it.
#[test]
fn an_edit_to_an_inherited_game_screen_reaches_the_mod_screen_through_both_paths() {
    let mut harness = harness();
    with_ui_registries(&mut harness);
    harness
        .app
        .world_mut()
        .resource_mut::<Screens>()
        .register(game_screen("game:base", "base_v1"));

    harness.reply(
        "beta",
        "data_stage",
        Ok(vec![ScriptCommand::RegisterScreen {
            id: "beta:derived".to_owned(),
            def: derived_tree("game:base", "derived_v1"),
        }]),
    );
    harness.run_all();

    assert_eq!(
        owner_of(&harness, "beta:derived"),
        Some(Owner::Mod("beta".to_owned())),
        "the mod owns the screen it registered"
    );
    assert_eq!(
        owner_of(&harness, "game:base"),
        Some(Owner::Game),
        "and the game keeps the one it registered"
    );

    let first = open(&mut harness, "beta:derived");
    assert_eq!(
        test_ids(&harness, first),
        vec!["base_v1".to_owned(), "derived_v1".to_owned()],
        "the derived screen draws the base's nodes and its own; the child's \
         root wins on shape, so the base root's tag is not among them"
    );

    // Path one: the asset loader re-registers the base from a changed file.
    // This is exactly what `apply_screen_assets` does, minus the `AssetPlugin`
    // the packs harness does not run.
    {
        let world = harness.app.world_mut();
        world.resource_mut::<Screens>().register_owned(
            game_screen("game:base", "base_v2"),
            Owner::Asset("screens/base.screen.ron".to_owned()),
        );
        let affected = slotted_ui::invalidate_and_respawn(
            world,
            &ChangeSet::screens([ScreenKind::new("game:base")]),
        );
        let affected: Vec<String> = affected.into_iter().map(|k| k.0.to_string()).collect();
        assert!(
            affected.contains(&"beta:derived".to_owned()),
            "an edit to a base reaches what inherits it: {affected:?}"
        );
    }

    let second = root_of(&harness, "beta:derived").expect("still open");
    assert_ne!(second, first, "the screen really respawned");
    assert_eq!(
        test_ids(&harness, second),
        vec!["base_v2".to_owned(), "derived_v1".to_owned()],
        "and now draws the edited base"
    );

    // Path two: the mod changes its own screen and reloads. The asset-owned
    // base must come through untouched -- a mod reconcile replaces the
    // mod-owned set and nothing else.
    harness.reply(
        "beta",
        "data_stage",
        Ok(vec![ScriptCommand::RegisterScreen {
            id: "beta:derived".to_owned(),
            def: derived_tree("game:base", "derived_v2"),
        }]),
    );
    ModLoader::reload_mod(harness.app.world_mut(), &mod_id("beta")).expect("the reload succeeds");

    let third = root_of(&harness, "beta:derived").expect("still open");
    assert_ne!(third, second, "the reload respawned it too");
    assert_eq!(
        test_ids(&harness, third),
        vec!["base_v2".to_owned(), "derived_v2".to_owned()],
        "the mod's new tree over the asset loader's base"
    );
    assert_eq!(
        owner_of(&harness, "game:base"),
        Some(Owner::Asset("screens/base.screen.ron".to_owned())),
        "a mod reload does not take an asset-owned screen away"
    );
}

// -- 6: a mod that is gone --------------------------------------------------

/// A mod is deleted from the mods directory and the game reloads.
///
/// Everything the mod owned goes: the screen only it provided is unregistered
/// and the open instance is closed rather than left drawing from a definition
/// nothing holds, with the close reported to the mod log. The game screen the
/// mod had taken over comes back as the game's own, and its open instance
/// respawns showing the game's tree again. Before ownership, none of this
/// happened: a mod's registrations outlived the mod for the life of the
/// process.
#[test]
fn a_mod_removed_from_the_dir_loses_its_registrations_and_gives_the_game_screen_back() {
    let root = scratch("review-round2-removed");
    copy_dir(&fixtures(), &root);
    let mut harness = harness_at(&root);
    with_ui_registries(&mut harness);
    harness
        .app
        .world_mut()
        .resource_mut::<Screens>()
        .register(game_screen("game:chest", "game_chest"));

    // The mod ships one screen of its own and takes over the game's.
    harness.reply(
        "beta",
        "data_stage",
        Ok(vec![
            ScriptCommand::RegisterScreen {
                id: "beta:panel".to_owned(),
                def: tree("beta_panel", "panel_anchor"),
            },
            ScriptCommand::RegisterScreen {
                id: "game:chest".to_owned(),
                def: tree("mod_chest", "chest_anchor"),
            },
        ]),
    );
    harness.run_all();

    assert_eq!(
        owner_of(&harness, "game:chest"),
        Some(Owner::Mod("game".to_owned())),
        "the takeover is allowed, and recorded"
    );
    let panel = open(&mut harness, "beta:panel");
    let chest = open(&mut harness, "game:chest");
    assert_eq!(test_ids(&harness, chest), vec!["mod_chest".to_owned()]);
    assert!(harness.app.world().get_entity(panel).is_ok());

    // The player deletes the mod.
    std::fs::remove_dir_all(root.join("mods/beta")).expect("the mod goes");
    let layout = PackLayout::new(root.join("base"))
        .with_mods(&root.join("mods"))
        .expect("the remaining mods discover");
    harness.app.world_mut().insert_resource(layout);
    ModLoader::run_all(harness.app.world_mut()).expect("the smaller set loads");

    let screens = harness.app.world().resource::<Screens>();
    assert!(
        screens.get(&ScreenKind::new("beta:panel")).is_none(),
        "the mod-only screen is unregistered"
    );
    assert_eq!(
        screens.owner(&ScreenKind::new("game:chest")),
        Some(&Owner::Game),
        "and the game's definition came back out of the shadow"
    );

    assert!(
        harness.app.world().get_entity(panel).is_err(),
        "the open instance of a screen nothing registers any more is closed"
    );
    let chest_now = root_of(&harness, "game:chest").expect("the game screen is still open");
    assert_ne!(chest_now, chest, "and the taken-over one respawned");
    assert_eq!(
        test_ids(&harness, chest_now),
        vec!["game_chest".to_owned(), "root".to_owned()],
        "drawing the game's tree again"
    );

    let logs = harness.app.world().resource::<ScriptLogs>();
    assert!(
        logs.entries
            .iter()
            .any(|entry| entry.message.contains("beta:panel")
                && entry.message.contains("no longer registered")),
        "the close is reported to the mod log: {:?}",
        logs.entries
    );

    std::fs::remove_dir_all(&root).ok();
}

// -- 5: an injection that moves ---------------------------------------------

/// An injection is retargeted from screen A to screen B on reload.
///
/// Both ends have to move, and the removal half is the one that used to be
/// missed entirely: matching changed screens by name reaches neither A nor B,
/// because neither definition changed. A must respawn *without* the node and
/// B *with* it.
#[test]
fn an_injection_retargeted_on_reload_leaves_one_screen_and_reaches_the_other() {
    let mut harness = harness();
    with_ui_registries(&mut harness);

    let screens = vec![
        ScriptCommand::RegisterScreen {
            id: "beta:a".to_owned(),
            def: tree("screen_a", "rail"),
        },
        ScriptCommand::RegisterScreen {
            id: "beta:b".to_owned(),
            def: tree("screen_b", "rail"),
        },
    ];
    let inject = |target: &str| ScriptCommand::Inject {
        screen: target.to_owned(),
        anchor: "rail".to_owned(),
        node: map([
            ("type", text("text")),
            ("key", text("beta.badge")),
            ("style", text("body")),
            ("tags", map([("test_id", text("badge"))])),
        ]),
        exclusion: false,
    };

    let mut first = screens.clone();
    first.push(inject("beta:a"));
    harness.reply("beta", "data_stage", Ok(first));
    harness.run_all();

    let a_before = open(&mut harness, "beta:a");
    let b_before = open(&mut harness, "beta:b");
    assert!(
        test_ids(&harness, a_before).contains(&"badge".to_owned()),
        "A has the injected node"
    );
    assert!(
        !test_ids(&harness, b_before).contains(&"badge".to_owned()),
        "and B does not"
    );

    let mut second = screens;
    second.push(inject("beta:b"));
    harness.reply("beta", "data_stage", Ok(second));
    ModLoader::reload_mod(harness.app.world_mut(), &mod_id("beta")).expect("the reload succeeds");

    let a_after = root_of(&harness, "beta:a").expect("A is still open");
    let b_after = root_of(&harness, "beta:b").expect("B is still open");
    assert_ne!(
        a_after, a_before,
        "A respawned even though its def did not change"
    );
    assert_ne!(b_after, b_before, "and so did B");
    assert!(
        !test_ids(&harness, a_after).contains(&"badge".to_owned()),
        "A lost the node: {:?}",
        test_ids(&harness, a_after)
    );
    assert!(
        test_ids(&harness, b_after).contains(&"badge".to_owned()),
        "and B gained it: {:?}",
        test_ids(&harness, b_after)
    );

    let injections = harness.app.world().resource::<Injections>();
    assert_eq!(injections.0.len(), 1, "one injection, not two");
    assert_eq!(injections.0[0].target, ScreenKind::new("beta:b"));
    assert_eq!(injections.0[0].anchor, AnchorId::new("rail"));
    assert_eq!(injections.0[0].owner, Owner::Mod("beta".to_owned()));
}

// -- 6: a reload that fails -------------------------------------------------

/// A reload whose data stage throws leaves the previous owned set exactly as
/// it was.
///
/// The three stages exist for this: `prepare` guarantees that when it fails,
/// everything it produced is still a value it owns and the running game has
/// not been touched. So no registry is half-reconciled, no screen is
/// half-registered, and nothing on screen has moved -- the failure is a
/// report, not a state change.
#[test]
#[allow(clippy::too_many_lines)]
fn a_reload_that_fails_leaves_the_previous_owned_set_intact() {
    let mut harness = harness();
    with_ui_registries(&mut harness);

    harness.reply(
        "beta",
        "data_stage",
        Ok(vec![
            ScriptCommand::RegisterScreen {
                id: "beta:a".to_owned(),
                def: tree("screen_a", "rail"),
            },
            ScriptCommand::Inject {
                screen: "beta:a".to_owned(),
                anchor: "rail".to_owned(),
                node: map([
                    ("type", text("text")),
                    ("key", text("beta.badge")),
                    ("style", text("body")),
                    ("tags", map([("test_id", text("badge"))])),
                ]),
                exclusion: false,
            },
        ]),
    );
    harness.run_all();

    let before_root = open(&mut harness, "beta:a");
    let before_ids = test_ids(&harness, before_root);
    let before_def = harness
        .app
        .world()
        .resource::<Screens>()
        .get(&ScreenKind::new("beta:a"))
        .cloned()
        .expect("registered");
    let before_kinds: Vec<String> = harness
        .app
        .world()
        .resource::<Screens>()
        .kinds()
        .map(|k| k.0.to_string())
        .collect();
    let before_injections = harness.app.world().resource::<Injections>().0.clone();
    let before_widgets: Vec<WidgetKind> = harness
        .app
        .world()
        .resource::<WidgetRegistry>()
        .kinds()
        .cloned()
        .collect();
    let before_registries = harness.registries();

    // The mod's data stage throws on the way in.
    harness.reply(
        "beta",
        "data_stage",
        Err(slotted_script::ScriptError::Runtime {
            name: "beta/data.lua".to_owned(),
            message: "the data stage threw".to_owned(),
            traceback: String::new(),
        }),
    );
    let failure = ModLoader::reload_mod(harness.app.world_mut(), &mod_id("beta"))
        .expect_err("a data-stage throw is fatal to the reload");
    assert!(
        matches!(failure, slotted_packs::ModError::Script { .. }),
        "reported as the script failure it was: {failure:?}"
    );
    assert!(
        !harness.app.world().resource::<ModErrors>().0.is_empty(),
        "and recorded"
    );

    let screens = harness.app.world().resource::<Screens>();
    let after_kinds: Vec<String> = screens.kinds().map(|k| k.0.to_string()).collect();
    let mut before_sorted = before_kinds;
    let mut after_sorted = after_kinds;
    before_sorted.sort();
    after_sorted.sort();
    assert_eq!(
        before_sorted, after_sorted,
        "no screen was added or removed"
    );
    assert!(
        Arc::ptr_eq(
            &before_def,
            screens
                .get(&ScreenKind::new("beta:a"))
                .expect("still there")
        ),
        "and the definition is the very same one, not an equal rebuild"
    );

    assert_eq!(
        harness.app.world().resource::<Injections>().0,
        before_injections,
        "the injections are untouched"
    );
    let after_widgets: Vec<WidgetKind> = harness
        .app
        .world()
        .resource::<WidgetRegistry>()
        .kinds()
        .cloned()
        .collect();
    assert_eq!(after_widgets.len(), before_widgets.len());
    assert!(
        Arc::ptr_eq(&before_registries, &harness.registries()),
        "and the frozen registries were never replaced"
    );

    assert_eq!(
        root_of(&harness, "beta:a"),
        Some(before_root),
        "nothing on screen moved: the same root entity, not a respawn"
    );
    assert_eq!(test_ids(&harness, before_root), before_ids);
}
