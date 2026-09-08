//! The data/control lifecycle, event routing, reload and localisation.
//! Contract sections 2.3 to 2.7.

mod common;

use std::sync::Arc;

use bevy::prelude::*;
use pretty_assertions::assert_eq;
use slotted_ecs::SlotClicked;
use slotted_model::{Button, ClickAction, InventoryRef, ItemStack, ToolbarAction, Value};
use slotted_packs::{ControlScripts, ModLoader, ScriptHost, ScriptLogs};
use slotted_script::{LogLevel, ModId, ScriptCommand, ScriptError};
use slotted_ui::def::{LocKey, ScreenKind};
use slotted_ui::{LocText, Screens};

use common::{
    Harness, copy_dir, fixtures, harness, harness_at, id, map, mod_id, scratch, screen_tree, text,
};

// -- the data and freeze stages ---------------------------------------------

#[test]
fn run_all_freezes_ron_and_script_registrations_together() {
    let mut harness = harness();
    harness.reply(
        "beta",
        "data_stage",
        Ok(vec![
            ScriptCommand::RegisterItem {
                id: "beta:widget".to_owned(),
                def: map([("max_stack_size", Value::Int(4))]),
            },
            ScriptCommand::RegisterScreen {
                id: "beta:chest".to_owned(),
                def: screen_tree(),
            },
            ScriptCommand::Log {
                level: LogLevel::Info,
                message: "beta loaded".to_owned(),
            },
        ]),
    );

    let report = harness.run_all();
    assert_eq!(
        report
            .mods
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["zeta", "alpha", "beta"]
    );

    let registries = harness.registries();
    // The RON file and the script both landed in the same frozen set.
    assert!(registries.items.contains(&id("beta:gem")), "gem from RON");
    assert!(
        registries.items.contains(&id("beta:widget")),
        "widget from data.lua"
    );
    assert_eq!(
        registries
            .items
            .get_by_name(&id("beta:widget"))
            .expect("registered")
            .max_stack_size,
        4
    );

    // The screen reached `slotted-ui` through the registry payload.
    let screens = harness.app.world().resource::<Screens>();
    assert!(screens.get(&ScreenKind::new("beta:chest")).is_some());

    let logs = harness.app.world().resource::<ScriptLogs>();
    assert!(
        logs.entries.iter().any(|e| e.message == "beta loaded"),
        "{:?}",
        logs.entries
    );
    assert!(harness.errors().is_empty(), "{:?}", harness.errors());
}

#[test]
fn a_control_command_from_a_data_script_is_the_wrong_stage() {
    let mut harness = harness();
    harness.reply(
        "beta",
        "data_stage",
        Ok(vec![ScriptCommand::Sort {
            menu: slotted_model::MenuId(1),
            inventory: 0,
        }]),
    );
    harness.run_all();

    let errors = harness.errors();
    assert!(
        matches!(errors.as_slice(), [slotted_packs::ModError::WrongStage { command, .. }] if command == "sort"),
        "{errors:?}"
    );
}

#[test]
fn a_data_script_that_fails_keeps_the_mod_s_ron() {
    let mut harness = harness();
    harness.reply(
        "beta",
        "data_stage",
        Err(ScriptError::Runtime {
            name: "data.lua".to_owned(),
            message: "boom".to_owned(),
            traceback: String::new(),
        }),
    );
    harness.run_all();

    assert!(harness.registries().items.contains(&id("beta:gem")));
    assert!(matches!(
        harness.errors().as_slice(),
        [slotted_packs::ModError::Script { .. }]
    ));
}

// -- routing ----------------------------------------------------------------

#[test]
fn a_slot_click_reaches_the_script_and_its_sort_reaches_the_authority() {
    let mut harness = harness();
    harness.reply(
        "beta",
        "control_start",
        Ok(vec![ScriptCommand::Subscribe {
            events: vec!["slot_click".to_owned()],
        }]),
    );
    harness.run_all();

    let (_, slot, menu_id) = harness.open_menu_with(two_partial_stacks(&harness));
    harness.reply(
        "beta",
        "slot_click",
        Ok(vec![ScriptCommand::Sort {
            menu: slotted_model::MenuId(menu_id),
            inventory: 0,
        }]),
    );

    harness.app.world_mut().trigger(SlotClicked {
        entity: slot,
        button: Button::Left,
        modifiers: slotted_ecs::Modifiers::NONE,
    });
    harness.app.update();

    let submitted = harness.authority.submitted();
    assert!(
        submitted.iter().any(|s| s.action
            == ClickAction::Toolbar(ToolbarAction::Sort {
                inventory: InventoryRef::new(0)
            })),
        "{submitted:?}"
    );
}

#[test]
fn a_failing_handler_in_one_mod_does_not_stop_the_next() {
    let mut harness = harness();
    for owner in ["alpha", "beta"] {
        harness.reply(
            owner,
            "control_start",
            Ok(vec![ScriptCommand::Subscribe {
                events: vec!["slot_click".to_owned()],
            }]),
        );
    }
    harness.run_all();

    let (_, slot, menu_id) = harness.open_menu_with(two_partial_stacks(&harness));
    harness.reply(
        "alpha",
        "slot_click",
        Err(ScriptError::Runtime {
            name: "control.lua".to_owned(),
            message: "boom".to_owned(),
            traceback: "stack traceback".to_owned(),
        }),
    );
    harness.reply(
        "beta",
        "slot_click",
        Ok(vec![ScriptCommand::Sort {
            menu: slotted_model::MenuId(menu_id),
            inventory: 0,
        }]),
    );

    harness.app.world_mut().trigger(SlotClicked {
        entity: slot,
        button: Button::Left,
        modifiers: slotted_ecs::Modifiers::NONE,
    });
    harness.app.update();

    // Alpha is earlier in load order and blew up; beta still ran.
    assert!(
        harness
            .errors()
            .iter()
            .any(|error| matches!(error, slotted_packs::ModError::Script { mod_id, .. } if mod_id == &mod_id_alpha())),
        "{:?}",
        harness.errors()
    );
    let submitted = harness.authority.submitted();
    assert!(
        submitted.iter().any(|s| s.action
            == ClickAction::Toolbar(ToolbarAction::Sort {
                inventory: InventoryRef::new(0)
            })),
        "{submitted:?}"
    );
}

fn mod_id_alpha() -> ModId {
    mod_id("alpha")
}

/// Two partial stacks of the same item, so a `Sort` has something to merge and
/// the authority sees a submission.
fn two_partial_stacks(harness: &Harness) -> Vec<Option<ItemStack>> {
    let gem = harness
        .registries()
        .item_id(&id("beta:gem"))
        .expect("the RON item loaded");
    vec![
        Some(ItemStack::new(gem, 1)),
        None,
        Some(ItemStack::new(gem, 1)),
        None,
    ]
}

#[test]
fn a_command_naming_a_menu_that_is_not_open_is_rejected() {
    let mut harness = harness();
    harness.reply(
        "beta",
        "control_start",
        Ok(vec![ScriptCommand::Subscribe {
            events: vec!["slot_click".to_owned()],
        }]),
    );
    harness.run_all();
    let (_, slot, _) = harness.open_menu_with(vec![None; 4]);
    harness.reply(
        "beta",
        "slot_click",
        Ok(vec![ScriptCommand::Sort {
            menu: slotted_model::MenuId(9999),
            inventory: 0,
        }]),
    );

    harness.app.world_mut().trigger(SlotClicked {
        entity: slot,
        button: Button::Left,
        modifiers: slotted_ecs::Modifiers::NONE,
    });
    harness.app.update();

    assert!(
        harness
            .errors()
            .iter()
            .any(|e| matches!(e, slotted_packs::ModError::UnknownMenu { .. })),
        "{:?}",
        harness.errors()
    );
}

// -- reload -----------------------------------------------------------------

#[test]
fn reload_keeps_inventory_contents_when_item_ids_shift() {
    let root = scratch("reload");
    copy_dir(&fixtures(), &root);
    let mut harness = harness_at(&root);
    harness.run_all();

    let old = harness.registries();
    let gem = old.item_id(&id("beta:gem")).expect("the RON item loaded");
    let (_, _, _) = harness.open_menu_with(vec![Some(ItemStack::new(gem, 3)), None, None, None]);
    let inventory = harness
        .app
        .world_mut()
        .query_filtered::<Entity, With<slotted_ecs::Inventory>>()
        .iter(harness.app.world())
        .next()
        .expect("the inventory entity exists");

    // A new item file sorts before `gem.ron`, so every id after it shifts.
    std::fs::write(
        root.join("mods/beta/data/beta/items/aardvark.ron"),
        "(name: \"beta:aardvark\", max_stack_size: 1)",
    )
    .expect("write the new item");

    ModLoader::reload_mod(harness.app.world_mut(), &mod_id("beta")).expect("the reload succeeds");

    let new = harness.registries();
    let new_gem = new.item_id(&id("beta:gem")).expect("still registered");
    assert_ne!(new_gem, gem, "the new file shifted the dense ids");

    let stack = harness
        .app
        .world()
        .get::<slotted_ecs::Inventory>(inventory)
        .expect("the entity survives")
        .0
        .get(0)
        .cloned()
        .expect("the slot still holds a stack");
    assert_eq!(stack.count, 3, "the count is untouched");
    assert_eq!(
        new.items.name_of(stack.id),
        Some(&id("beta:gem")),
        "the stack was remapped by name"
    );

    std::fs::remove_dir_all(&root).ok();
}

/// The dense ids inside a `ComponentPatch` are interned by the same freeze
/// that numbers items, and a reload renumbers both. A patch that is not
/// remapped therefore reads some other component's value out of the same
/// slot, silently and with no error anywhere.
#[test]
fn reload_remaps_component_patch_keys_by_name() {
    let root = scratch("reload-patch");
    copy_dir(&fixtures(), &root);
    let mut harness = harness_at(&root);
    harness.run_all();

    let old = harness.registries();
    let gem = old.item_id(&id("beta:gem")).expect("the RON item loaded");
    let charge = old
        .components
        .get(&id("beta:charge"))
        .expect("the item's component key was interned");

    let mut stack = ItemStack::new(gem, 3);
    stack.patch.insert(charge, Value::from(7_i64));
    harness.open_menu_with(vec![Some(stack), None, None, None]);
    let inventory = harness
        .app
        .world_mut()
        .query_filtered::<Entity, With<slotted_ecs::Inventory>>()
        .iter(harness.app.world())
        .next()
        .expect("the inventory entity exists");

    // A new item declaring its own component sorts before `gem.ron`, so both
    // the item ids and the component ids after it shift.
    std::fs::write(
        root.join("mods/beta/data/beta/items/aardvark.ron"),
        "(name: \"beta:aardvark\", max_stack_size: 1, components: { \"beta:aura\": 0 })",
    )
    .expect("write the new item");

    ModLoader::reload_mod(harness.app.world_mut(), &mod_id("beta")).expect("the reload succeeds");

    let new = harness.registries();
    let new_charge = new
        .components
        .get(&id("beta:charge"))
        .expect("the key still exists");
    assert_ne!(new_charge, charge, "the new file shifted the component ids");

    let stack = harness
        .app
        .world()
        .get::<slotted_ecs::Inventory>(inventory)
        .expect("the entity survives")
        .0
        .get(0)
        .cloned()
        .expect("the slot still holds a stack");
    assert_eq!(
        stack.patch.get(new_charge),
        Some(&Value::from(7_i64)),
        "the override followed its name, not its number"
    );
    assert!(
        stack.patch.get(charge).is_none() || charge == new_charge,
        "and nothing was left behind at the old id"
    );

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_failed_reload_keeps_the_previous_registries() {
    let root = scratch("reload-fail");
    copy_dir(&fixtures(), &root);
    let mut harness = harness_at(&root);
    harness.run_all();
    let before = harness.registries();

    harness.reply(
        "beta",
        "data_stage",
        Err(ScriptError::Runtime {
            name: "data.lua".to_owned(),
            message: "boom".to_owned(),
            traceback: String::new(),
        }),
    );
    let error = ModLoader::reload_mod(harness.app.world_mut(), &mod_id("beta"))
        .expect_err("the script fails");
    assert!(matches!(error, slotted_packs::ModError::Script { .. }));
    assert!(
        Arc::ptr_eq(&before, &harness.registries()),
        "the frozen set was not replaced"
    );

    std::fs::remove_dir_all(&root).ok();
}

/// Finding 6, end to end: a mod that registers a screen and then stops has to
/// leave the runtime registry, not only stop shipping the file. Before, a
/// reload could add to `Screens` and replace entries in it, never remove one,
/// so a screen a mod dropped stayed openable forever and an open instance kept
/// drawing a definition nothing shipped.
///
/// The game's own Rust registration in the same reload is the control: it must
/// survive, because it was never the mod's to remove.
#[test]
fn a_screen_a_mod_stops_registering_is_unregistered_by_the_reload() {
    let mut harness = harness();
    harness.reply(
        "beta",
        "data_stage",
        Ok(vec![ScriptCommand::RegisterScreen {
            id: "beta:chest".to_owned(),
            def: screen_tree(),
        }]),
    );
    harness.run_all();

    // A screen the game registered itself, alongside the mod's.
    harness.app.world_mut().resource_mut::<Screens>().register(
        slotted_ui::ScreenDef::from_ron(
            r#"(kind: "game:own", root: (type: "panel", role: "panel"))"#,
        )
        .expect("the game's own screen parses"),
    );
    assert!(
        harness
            .app
            .world()
            .resource::<Screens>()
            .get(&ScreenKind::new("beta:chest"))
            .is_some(),
        "the mod's screen is registered to start with"
    );

    // The reload: beta ships no screen this time.
    harness.reply("beta", "data_stage", Ok(Vec::new()));
    ModLoader::reload_mod(harness.app.world_mut(), &mod_id("beta")).expect("the reload succeeds");

    let screens = harness.app.world().resource::<Screens>();
    assert!(
        screens.get(&ScreenKind::new("beta:chest")).is_none(),
        "the screen the mod stopped registering is gone from `Screens`"
    );
    assert!(
        screens.get(&ScreenKind::new("game:own")).is_some(),
        "reconciling the mod-owned set left the game's own registration alone"
    );
}

#[test]
fn a_control_chunk_that_fails_leaves_the_mod_answering_with_the_last_good_one() {
    let mut harness = harness();
    harness.run_all();
    let before = harness
        .app
        .world()
        .resource::<ControlScripts>()
        .by_mod
        .get(&mod_id("beta"))
        .expect("beta has a control script")
        .clone();

    // The new chunk loads and then throws out of `control_start`, which is
    // what a mod with a typo below its handler does.
    harness.reply(
        "beta",
        "control_start",
        Err(ScriptError::Runtime {
            name: "control.lua".to_owned(),
            message: "boom".to_owned(),
            traceback: String::new(),
        }),
    );
    ModLoader::reload_mod(harness.app.world_mut(), &mod_id("beta"))
        .expect("a bad control chunk is a per-mod failure, not a fatal one");

    let after = harness
        .app
        .world()
        .resource::<ControlScripts>()
        .by_mod
        .get(&mod_id("beta"))
        .cloned();
    assert_eq!(
        after.as_ref(),
        Some(&before),
        "the mod lost its control script instead of keeping the last good one"
    );
    assert!(
        !harness.errors().is_empty(),
        "the failure was never reported"
    );

    // Still live in the runtime: keeping the entry would be worthless if the
    // handle behind it had been freed.
    let host = harness.app.world().resource::<ScriptHost>().clone();
    let answer = host.lock().call(
        before.id,
        &slotted_script::ScriptEvent::ControlStart {
            api_version: slotted_script::API_VERSION,
            mods: Vec::new(),
        },
    );
    assert!(
        !matches!(answer, Err(ScriptError::UnknownScript(_))),
        "the kept script had been unloaded out from under the table"
    );
}

#[test]
fn a_control_chunk_that_loads_replaces_and_unloads_the_previous_one() {
    let mut harness = harness();
    harness.run_all();
    let before = harness
        .app
        .world()
        .resource::<ControlScripts>()
        .by_mod
        .get(&mod_id("beta"))
        .expect("beta has a control script")
        .id;

    ModLoader::reload_mod(harness.app.world_mut(), &mod_id("beta")).expect("the reload succeeds");

    let after = harness
        .app
        .world()
        .resource::<ControlScripts>()
        .by_mod
        .get(&mod_id("beta"))
        .expect("beta still has one")
        .id;
    assert_ne!(before, after, "the reload did not load a new chunk");

    let host = harness.app.world().resource::<ScriptHost>().clone();
    let answer = host.lock().call(
        before,
        &slotted_script::ScriptEvent::ControlStart {
            api_version: slotted_script::API_VERSION,
            mods: Vec::new(),
        },
    );
    assert!(
        matches!(answer, Err(ScriptError::UnknownScript(_))),
        "the replaced chunk was left loaded, so every reload leaks a script"
    );
}

// -- localisation -----------------------------------------------------------

#[test]
fn a_mod_locale_layers_over_the_base_and_a_missing_key_falls_back() {
    let mut harness = harness();
    harness.run_all();

    let locales = harness.app.world().resource::<slotted_packs::Locales>();
    assert_eq!(
        locales.resolve(&LocKey("beta-item".to_owned()), None),
        Some("Shiny gem".to_owned())
    );
    // Beta loads after base, so its layer sits above it.
    assert_eq!(
        locales.resolve(&LocKey("base-title".to_owned()), None),
        Some("Beta overrides the title".to_owned())
    );
    // Contract 2.7's convention key has dots in it, which a Fluent identifier
    // may not; `beta.screen.title` is written that way in the fixture `.ftl`.
    assert_eq!(
        locales.resolve(&LocKey("beta.screen.title".to_owned()), None),
        Some("Beta chest".to_owned())
    );
    assert_eq!(
        locales.resolve(&LocKey("nobody.defines.me".to_owned()), None),
        None
    );
}

/// A library's English defaults (menus M2 contract 2.1).
struct EnglishDefaults;

impl slotted_ui::Localizer for EnglishDefaults {
    fn resolve(&self, key: &LocKey, _args: &slotted_ui::LocArgs) -> Option<String> {
        match key.0.as_str() {
            "slotted.menu.resume" => Some("Resume".to_owned()),
            // The fixture's `.ftl` defines this too; the pack must win.
            "beta-item" => Some("Fallback gem".to_owned()),
            _ => None,
        }
    }
}

#[test]
fn a_pushed_fallback_survives_a_pack_install_and_a_reload() {
    let root = scratch("locale-fallback");
    copy_dir(&fixtures(), &root);
    let mut harness = harness_at(&root);
    // A `Localization` from before any pack loaded, with a library's
    // defaults pushed under it.
    harness
        .app
        .world_mut()
        .get_resource_or_init::<slotted_ui::Localization>()
        .push_fallback(EnglishDefaults);
    harness.run_all();

    let resolve = |harness: &Harness, key: &str| {
        harness
            .app
            .world()
            .resource::<slotted_ui::Localization>()
            .resolve(&LocKey(key.to_owned()))
    };
    // The pack's catalogue is the primary; the fallback answers what the
    // pack does not define and nothing the pack does.
    assert_eq!(
        resolve(&harness, "beta-item").as_deref(),
        Some("Shiny gem"),
        "the pack's layer sits over the fallback"
    );
    assert_eq!(
        resolve(&harness, "slotted.menu.resume").as_deref(),
        Some("Resume"),
        "the fallback survives the install"
    );
    assert_eq!(resolve(&harness, "nobody.defines.me"), None);
    assert_eq!(
        harness
            .app
            .world()
            .resource::<slotted_ui::Localization>()
            .fallbacks
            .len(),
        1
    );

    // A reload swaps the primary again and keeps the fallback.
    ModLoader::reload_mod(harness.app.world_mut(), &mod_id("beta")).expect("the reload succeeds");
    assert_eq!(resolve(&harness, "beta-item").as_deref(), Some("Shiny gem"));
    assert_eq!(
        resolve(&harness, "slotted.menu.resume").as_deref(),
        Some("Resume")
    );

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_script_registered_recipe_type_gets_a_browser_category() {
    let mut harness = harness();
    harness.reply(
        "beta",
        "data_stage",
        Ok(vec![ScriptCommand::RegisterRecipeType {
            id: "beta:assembly".to_owned(),
            def: map([("size", Value::List(vec![Value::Int(2), Value::Int(2)]))]),
        }]),
    );
    harness.run_all();

    // The browser binds defaults in its own `Startup`, which never runs again
    // for a reload and is too early for a harness that loads mods after build.
    let categories = harness
        .app
        .world()
        .resource::<slotted_browser::Categories>();
    assert!(
        categories.for_recipe_type(&id("beta:assembly")).is_some(),
        "a recipe type with no category has all its recipes dropped"
    );
}

#[test]
fn loc_text_resolves_after_the_locale_loads_and_leaves_unknown_keys_verbatim() {
    let mut harness = harness();
    harness.run_all();
    harness
        .app
        .add_systems(Update, slotted_packs::resolve_loc_text);

    let known = harness
        .app
        .world_mut()
        .spawn((
            LocText::new(LocKey("beta-item".to_owned())),
            Text::new("beta-item"),
        ))
        .id();
    let unknown = harness
        .app
        .world_mut()
        .spawn((
            LocText::new(LocKey("nobody.defines.me".to_owned())),
            Text::new("nobody.defines.me"),
        ))
        .id();
    harness.app.update();

    assert_eq!(
        harness.app.world().get::<Text>(known).expect("text").0,
        "Shiny gem"
    );
    assert_eq!(
        harness.app.world().get::<Text>(unknown).expect("text").0,
        "nobody.defines.me",
        "an unresolved key stays as written"
    );
}

// -- tooltips and injections ------------------------------------------------

#[test]
fn a_static_tooltip_part_appends_its_nodes_for_a_matching_stack() {
    let mut harness = harness();
    harness.reply(
        "beta",
        "data_stage",
        Ok(vec![
            ScriptCommand::RegisterItem {
                id: "beta:widget".to_owned(),
                def: map([
                    ("max_stack_size", Value::Int(4)),
                    ("tags", Value::List(vec![text("beta:shiny")])),
                ]),
            },
            ScriptCommand::AddTooltipPart {
                id: Some("beta:shine".to_owned()),
                when: slotted_script::TooltipFilter {
                    items: Vec::new(),
                    tags: vec!["beta:shiny".to_owned()],
                },
                tier: slotted_script::TierFilter::Any,
                nodes: vec![slotted_script::commands::UntaggedValue(map([
                    ("type", text("text")),
                    ("key", text("beta.shine")),
                    ("style", text("muted")),
                ]))],
            },
        ]),
    );
    harness.run_all();

    let registries = harness.registries();
    let widget = registries
        .item_id(&id("beta:widget"))
        .expect("the script registered it");
    let stack = ItemStack::new(widget, 1);
    let ctx = slotted_ui::tooltip::TooltipCtx {
        stack: Some(&stack),
        registries: &registries,
        tier: slotted_ui::tooltip::TooltipTier::Compact,
    };
    let parts = harness.app.world().resource::<slotted_ui::TooltipParts>();
    let nodes = parts.compose(&ctx);
    assert!(
        nodes.iter().any(|node| matches!(
            node,
            slotted_ui::def::UiNodeDef::Text { key, .. } if key.0 == "beta.shine"
        )),
        "{nodes:?}"
    );

    // A stack outside the filter gets nothing from the script part.
    let gem = ItemStack::new(registries.item_id(&id("beta:gem")).expect("from RON"), 1);
    let other = slotted_ui::tooltip::TooltipCtx {
        stack: Some(&gem),
        registries: &registries,
        tier: slotted_ui::tooltip::TooltipTier::Compact,
    };
    assert!(!parts.compose(&other).iter().any(|node| matches!(
        node,
        slotted_ui::def::UiNodeDef::Text { key, .. } if key.0 == "beta.shine"
    )));
}

#[test]
fn an_inject_command_reaches_the_ui_injections() {
    let mut harness = harness();
    harness.reply(
        "beta",
        "data_stage",
        Ok(vec![ScriptCommand::Inject {
            screen: "beta:chest".to_owned(),
            anchor: "title_end".to_owned(),
            node: map([
                ("type", text("text")),
                ("key", text("beta.badge")),
                ("style", text("muted")),
            ]),
            exclusion: false,
        }]),
    );
    harness.run_all();

    let injections = harness.app.world().resource::<slotted_ui::Injections>();
    assert_eq!(injections.0.len(), 1);
    assert_eq!(injections.0[0].target, ScreenKind::new("beta:chest"));
    assert_eq!(injections.0[0].anchor.0, "title_end");

    // A second load replaces packs' own entries instead of stacking them up.
    ModLoader::run_all(harness.app.world_mut()).expect("loads again");
    assert_eq!(
        harness
            .app
            .world()
            .resource::<slotted_ui::Injections>()
            .0
            .len(),
        1
    );
}

// -- optional fields --------------------------------------------------------

#[test]
fn optional_def_fields_survive_the_hop_from_a_lua_table() {
    let mut harness = harness();
    harness.reply(
        "beta",
        "data_stage",
        Ok(vec![
            ScriptCommand::RegisterItem {
                id: "beta:ingot".to_owned(),
                def: map([
                    ("max_stack_size", Value::Int(64)),
                    ("display_name", text("beta.item.ingot")),
                ]),
            },
            ScriptCommand::RegisterRecipeType {
                id: "beta:assembly".to_owned(),
                def: map([
                    ("title_key", text("beta.recipe_type.assembly")),
                    ("size", Value::List(vec![Value::Int(2), Value::Int(2)])),
                ]),
            },
            ScriptCommand::RegisterRecipe {
                id: "beta:widget".to_owned(),
                def: map([
                    ("recipe_type", text("beta:assembly")),
                    ("shape", Value::List(vec![text("ii"), text("ii")])),
                    ("key", map([("i", text("beta:ingot"))])),
                    (
                        "result",
                        map([("item", text("beta:ingot")), ("count", Value::Int(1))]),
                    ),
                ]),
            },
        ]),
    );
    harness.run_all();
    assert!(harness.errors().is_empty(), "{:?}", harness.errors());

    let registries = harness.registries();
    // A bare string fills an `Option<String>`; a bare list fills an
    // `Option<Vec<String>>`; an absent field is still `None`.
    let ingot = registries
        .items
        .get_by_name(&id("beta:ingot"))
        .expect("registered");
    assert_eq!(ingot.display_name.as_deref(), Some("beta.item.ingot"));
    assert_eq!(
        registries
            .items
            .get_by_name(&id("beta:gem"))
            .expect("from RON")
            .display_name,
        None
    );

    let assembly = registries
        .recipe_types
        .get_by_name(&id("beta:assembly"))
        .expect("registered");
    assert_eq!(
        assembly.title_key.as_deref(),
        Some("beta.recipe_type.assembly")
    );
    assert_eq!(assembly.size, (2, 2));

    let recipe = registries
        .recipes
        .get_by_name(&id("beta:widget"))
        .expect("registered");
    assert_eq!(
        recipe.shape.as_deref(),
        Some(["ii".to_owned(), "ii".to_owned()].as_slice())
    );
    assert_eq!(recipe.key.len(), 1);
}

#[test]
fn a_script_screen_payload_reads_back_out_of_the_registry() {
    let mut harness = harness();
    harness.reply(
        "beta",
        "data_stage",
        Ok(vec![ScriptCommand::RegisterScreen {
            id: "beta:chest".to_owned(),
            def: map([
                (
                    "root",
                    map([
                        ("type", text("panel")),
                        ("role", text("panel")),
                        (
                            "children",
                            Value::List(vec![map([
                                ("type", text("text")),
                                ("key", text("beta.chest.title")),
                                ("style", text("title")),
                            ])]),
                        ),
                    ]),
                ),
                ("listring", Value::List(vec![Value::Int(0)])),
            ]),
        }]),
    );
    harness.run_all();
    assert!(harness.errors().is_empty(), "{:?}", harness.errors());

    // The stored payload is RON-shaped, so `Screens::load_from_registry` reads
    // it back into a typed tree.
    let screens = harness.app.world().resource::<Screens>();
    let def = screens
        .get(&ScreenKind::new("beta:chest"))
        .expect("the screen is published");
    assert_eq!(def.kind, ScreenKind::new("beta:chest"));
    assert_eq!(def.inherits, None);
    assert_eq!(def.listring.len(), 1);
    assert_eq!(def.root.children().len(), 1);
}
