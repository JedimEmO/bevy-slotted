//! The playground's own plumbing, natively.
//!
//! Everything here is what the browser exercises and a headless test can
//! reach: the bundled mods really load through `ModLoader`, an edit really
//! reaches the reloaded script, the inventory really survives the reload, and
//! a request pushed onto the bus really becomes a `ReloadMod` message. What is
//! left for the browser is the canvas and the editor.

use std::sync::Arc;

use bevy::prelude::*;
use pretty_assertions::assert_eq;
use slotted_model::{ItemStack, Namespaced};
use slotted_packs::{
    ControlScripts, ModErrors, ModFailed, ModLoader, ModReloaded, PackAssets, PendingScriptEvents,
    ReloadMod, ScriptHost, ScriptLog, ScriptLogs, route,
};
use slotted_script::{Button, LogLevel, ModId, Modifiers, ScriptCommand, ScriptEvent, StackInfo};
use slotted_ui::Screens;
use slotted_ui::def::ScreenKind;
use web_playground::bundle::EditableSource;
use web_playground::bus::{Bus, Request};

/// The control script's log line, before anything is edited.
const ORIGINAL_LINE: &str = "slot 3 clicked (left): copper_chest:copper_ingot x64";

/// A world with everything `ModLoader` reads, over the bundled mods.
fn world_with_bundle() -> (App, EditableSource) {
    let source = EditableSource::from_bundle();
    let layout = web_playground::layout(&source).expect("the bundled mods discover");
    let shared: slotted_packs::SharedSource = Arc::new(source.clone());

    let mut app = slotted_testutils::ecs_app_with(slotted_testutils::RecordingAuthority::new());
    app.init_resource::<ScriptLogs>()
        .init_resource::<ModErrors>()
        .init_resource::<ControlScripts>()
        .init_resource::<route::OpenScreens>()
        .init_resource::<PendingScriptEvents>()
        .init_resource::<route::WarnedDeprecations>()
        .init_resource::<slotted_packs::Locales>()
        .init_resource::<slotted_browser::Categories>()
        .add_message::<ScriptLog>()
        .add_message::<ModFailed>()
        .add_message::<ModReloaded>()
        .add_message::<ReloadMod>()
        .insert_resource(web_playground::runtime())
        .insert_resource(PackAssets(shared))
        .insert_resource(layout);
    (app, source)
}

fn registries(app: &App) -> Arc<slotted_registry::FrozenRegistries> {
    app.world().resource::<slotted_ecs::Registries>().0.clone()
}

fn id(text: &str) -> Namespaced {
    Namespaced::parse(text).expect("a valid id")
}

/// A `slot_click` on the copper chest, as the router would send it.
fn a_click() -> ScriptEvent {
    ScriptEvent::SlotClick {
        menu: slotted_model::MenuId(1),
        screen: "copper_chest:chest".to_owned(),
        slot: 3,
        button: Button::Left,
        modifiers: Modifiers::default(),
        stack: Some(StackInfo {
            item: "copper_chest:copper_ingot".to_owned(),
            count: 64,
            components: slotted_model::Value::Map(std::collections::BTreeMap::new()),
        }),
    }
}

/// What `copper_chest`'s control script says about `event`.
fn ask_copper_chest(app: &mut App, event: &ScriptEvent) -> Vec<String> {
    let mod_id = ModId::new("copper_chest").expect("a valid mod id");
    let script = app
        .world()
        .resource::<ControlScripts>()
        .by_mod
        .get(&mod_id)
        .expect("copper_chest has a control script")
        .id;
    let host = app.world().resource::<ScriptHost>().clone();
    let commands = host
        .lock()
        .call(script, event)
        .expect("the control script answers");
    commands
        .into_iter()
        .filter_map(|command| match command {
            ScriptCommand::Log { message, .. } => Some(message),
            _ => None,
        })
        .collect()
}

#[test]
fn the_bundled_mods_register_what_the_scene_needs() {
    let (mut app, _source) = world_with_bundle();
    ModLoader::run_all(app.world_mut()).expect("the bundled mods load");
    assert_eq!(app.world().resource::<ModErrors>().0, Vec::new());

    let registries = registries(&app);
    for item in [
        "copper_chest:copper_chest",
        "copper_chest:copper_ingot",
        "appleskin_like:apple",
    ] {
        assert!(
            registries.item_id(&id(item)).is_some(),
            "{item} was not registered by any mod's data.lua"
        );
    }
    assert!(
        app.world()
            .resource::<Screens>()
            .get(&ScreenKind::new("copper_chest:chest"))
            .is_some(),
        "the mod's screen tree did not reach the screen registry"
    );
    assert!(
        !app.world()
            .resource::<slotted_ui::Injections>()
            .0
            .is_empty(),
        "sorter's injected Sort button did not reach the injection registry"
    );
}

#[test]
fn the_control_script_answers_a_click() {
    let (mut app, _source) = world_with_bundle();
    ModLoader::run_all(app.world_mut()).expect("the bundled mods load");
    assert_eq!(ask_copper_chest(&mut app, &a_click()), [ORIGINAL_LINE]);
}

#[test]
fn an_edited_control_script_is_what_the_reload_runs() {
    let (mut app, source) = world_with_bundle();
    ModLoader::run_all(app.world_mut()).expect("the bundled mods load");
    assert_eq!(ask_copper_chest(&mut app, &a_click()), [ORIGINAL_LINE]);

    source.write(
        "scripts/copper_chest/control.lua",
        r#"
slotted.on("slot_click", function(ev)
    slotted.info("edited: slot %d holds %s", ev.slot, ev.stack.item)
end)
"#,
    );
    let mod_id = ModId::new("copper_chest").expect("a valid mod id");
    ModLoader::reload_mod(app.world_mut(), &mod_id).expect("the edited mod reloads");

    assert_eq!(
        ask_copper_chest(&mut app, &a_click()),
        ["edited: slot 3 holds copper_chest:copper_ingot"]
    );
}

#[test]
fn a_broken_edit_leaves_the_previous_registries_standing() {
    let (mut app, source) = world_with_bundle();
    ModLoader::run_all(app.world_mut()).expect("the bundled mods load");
    let before = registries(&app);

    source.write("scripts/copper_chest/control.lua", "this is not lua ===");
    let mod_id = ModId::new("copper_chest").expect("a valid mod id");
    // A control script that will not compile is a per-mod failure, not a fatal
    // one: the data stage already succeeded, so the reload still returns Ok.
    ModLoader::reload_mod(app.world_mut(), &mod_id).expect("a bad control script is not fatal");

    assert!(
        registries(&app)
            .item_id(&id("copper_chest:copper_ingot"))
            .is_some(),
        "a broken edit took the registries with it"
    );
    assert_eq!(before.items.len(), registries(&app).items.len());
    assert!(
        app.world()
            .resource::<ScriptLogs>()
            .entries
            .iter()
            .any(|entry| entry.level == LogLevel::Error)
            || !app.world().resource::<ModErrors>().0.is_empty(),
        "the compile error was never reported"
    );
}

#[test]
fn a_stack_keeps_its_item_across_a_reload() {
    let (mut app, source) = world_with_bundle();
    ModLoader::run_all(app.world_mut()).expect("the bundled mods load");
    let ingot = registries(&app)
        .item_id(&id("copper_chest:copper_ingot"))
        .expect("the mod registered it");
    let mut inventory = slotted_model::Inventory::new(9);
    inventory.set(0, Some(ItemStack::new(ingot, 64)));
    let entity = app
        .world_mut()
        .spawn(slotted_ecs::menu::Inventory(inventory))
        .id();

    // A new item shifts every interned id, so a reload that did not remap by
    // name would leave the stack pointing at the wrong thing.
    source.write(
        "scripts/copper_chest/data.lua",
        &format!(
            "{}\nslotted.register_item(\"aaa_first\", {{ max_stack_size = 1 }})\n",
            source
                .read_text("scripts/copper_chest/data.lua")
                .expect("the bundle has it")
        ),
    );
    let mod_id = ModId::new("copper_chest").expect("a valid mod id");
    ModLoader::reload_mod(app.world_mut(), &mod_id).expect("the edited mod reloads");

    let after = registries(&app);
    let stack = app
        .world()
        .entity(entity)
        .get::<slotted_ecs::menu::Inventory>()
        .expect("the inventory is still there")
        .0
        .get(0)
        .cloned()
        .expect("the slot still holds a stack");
    assert_eq!(stack.count, 64);
    assert_eq!(
        after.items.name_of(stack.id).map(ToString::to_string),
        Some("copper_chest:copper_ingot".to_owned()),
        "the stack was not remapped by name"
    );
}

#[test]
fn a_bus_request_becomes_a_reload_message() {
    let bus = Bus::new();
    let mut app = App::new();
    app.add_message::<ReloadMod>()
        // Phase 6: `Request::RunTests` becomes one of these.
        .add_message::<web_playground::StartTests>()
        // Phase 5 restart: `Request::Restore` becomes one of these.
        .add_message::<web_playground::RestoreState>()
        .add_message::<web_playground::showcase::SwitchScene>()
        .init_resource::<web_playground::scene::ConsoleErrors>()
        .init_resource::<web_playground::scene::ConsoleVisible>()
        .insert_resource(EditableSource::from_bundle())
        .insert_resource(bus.clone());
    // The playground's own PreUpdate system, on its own.
    app.add_systems(Update, web_playground::drain_requests_for_test);

    bus.request(Request::Write {
        path: "scripts/copper_chest/control.lua".to_owned(),
        contents: "-- edited\n".to_owned(),
    });
    bus.request(Request::Reload {
        mod_id: "copper_chest".to_owned(),
    });
    app.update();

    assert_eq!(
        app.world()
            .resource::<EditableSource>()
            .read_text("scripts/copper_chest/control.lua"),
        Some("-- edited\n".to_owned()),
        "the write did not reach the source the loader reads"
    );
    let messages = app
        .world()
        .resource::<bevy::ecs::message::Messages<ReloadMod>>();
    let mut cursor = messages.get_cursor();
    let ids: Vec<String> = cursor
        .read(messages)
        .map(|message| message.mod_id.to_string())
        .collect();
    assert_eq!(ids, ["copper_chest"]);
}

#[test]
fn a_bad_mod_id_from_the_page_is_reported_and_not_a_panic() {
    let bus = Bus::new();
    let mut app = App::new();
    app.add_message::<ReloadMod>()
        // Phase 6: `Request::RunTests` becomes one of these.
        .add_message::<web_playground::StartTests>()
        // Phase 5 restart: `Request::Restore` becomes one of these.
        .add_message::<web_playground::RestoreState>()
        .add_message::<web_playground::showcase::SwitchScene>()
        .init_resource::<web_playground::scene::ConsoleErrors>()
        .init_resource::<web_playground::scene::ConsoleVisible>()
        .insert_resource(EditableSource::from_bundle())
        .insert_resource(bus.clone());
    app.add_systems(Update, web_playground::drain_requests_for_test);

    bus.request(Request::Reload {
        mod_id: "Not A Mod Id".to_owned(),
    });
    app.update();

    assert!(
        bus.console_history()
            .iter()
            .any(|line| line.level == "error"),
        "the page was never told its mod id was rejected"
    );
}
