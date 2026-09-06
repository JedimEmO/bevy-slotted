//! What the playground does when the page misuses it.
//!
//! `plumbing.rs` proves the happy path: the bundled mods load, an edit
//! reaches the reloaded script, a stack survives. This file is the other
//! half. Everything here is something a person can do in the browser in one
//! second — paste a syntax error, paste a `while true do end`, hit Run twice,
//! open a stale share link — and none of it may take the tab down, because on
//! `wasm32-unknown-unknown` a panic is an aborted module and a dead canvas.
//!
//! It runs natively against the same piccolo runtime the browser gets, so a
//! failure here is a failure there.

use std::collections::BTreeMap;
use std::sync::Arc;

use bevy::prelude::*;
use pretty_assertions::assert_eq;
use slotted_model::{ItemStack, Namespaced};
use slotted_packs::{
    ControlScripts, ModErrors, ModFailed, ModLoader, ModReloaded, PackAssets, PendingScriptEvents,
    ReloadMod, ScriptHost, ScriptLog, ScriptLogs, route,
};
use slotted_script::{Button, ModId, Modifiers, ScriptCommand, ScriptEvent, StackInfo};
use web_playground::bundle::EditableSource;
use web_playground::bus::{Bus, Line, Request};

/// The control script's log line, before anything is edited.
const ORIGINAL_LINE: &str = "slot 3 clicked (left): copper_chest:copper_ingot x64";

const COPPER_CHEST: &str = "copper_chest";
const CONTROL: &str = "scripts/copper_chest/control.lua";
const DATA: &str = "scripts/copper_chest/data.lua";

/// A world holding everything `ModLoader` reads, plus the page's own two
/// systems over the bundled mods.
///
/// `drain_requests` and `pump_console` are the playground's; the reload
/// between them is `slotted-packs`' `apply_reloads`, which this harness runs
/// by hand in [`frame`] because the real one lives inside `SlottedPacksPlugin`
/// and that needs a renderer.
fn harness() -> (App, EditableSource, Bus) {
    let source = EditableSource::from_bundle();
    let layout = web_playground::layout(&source).expect("the bundled mods discover");
    let shared: slotted_packs::SharedSource = Arc::new(source.clone());
    let bus = Bus::new();

    let mut app = slotted_testutils::ecs_app_with(slotted_testutils::RecordingAuthority::new());
    app.init_resource::<ScriptLogs>()
        .init_resource::<ModErrors>()
        .init_resource::<ControlScripts>()
        .init_resource::<route::OpenScreens>()
        .init_resource::<PendingScriptEvents>()
        .init_resource::<route::WarnedDeprecations>()
        .init_resource::<slotted_packs::Locales>()
        .init_resource::<slotted_browser::Categories>()
        .init_resource::<web_playground::scene::ConsoleErrors>()
        .init_resource::<web_playground::scene::ConsoleVisible>()
        .add_message::<ScriptLog>()
        .add_message::<ModFailed>()
        .add_message::<ModReloaded>()
        .add_message::<ReloadMod>()
        // Phase 6: `Request::RunTests` becomes a `StartTests` message.
        .add_message::<web_playground::StartTests>()
        .insert_resource(web_playground::runtime())
        .insert_resource(PackAssets(shared))
        .insert_resource(layout)
        .insert_resource(source.clone())
        .insert_resource(bus.clone())
        .add_systems(
            Update,
            (
                web_playground::drain_requests_for_test,
                apply_reloads,
                web_playground::pump_console_for_test,
            )
                .chain(),
        );
    (app, source, bus)
}

/// `apply_reloads` from `slotted-packs`, transcribed.
///
/// The real one lives inside `SlottedPacksPlugin`, which needs a renderer this
/// test has no use for; it does exactly this, and the point of running it in
/// the same `Update` as the playground's own two systems is that the ordering
/// under test is the ordering the browser gets.
fn apply_reloads(world: &mut World) {
    let requested: Vec<ModId> = {
        let mut messages = world.resource_mut::<bevy::ecs::message::Messages<ReloadMod>>();
        messages.drain().map(|message| message.mod_id).collect()
    };
    for mod_id in requested {
        let _ = ModLoader::reload_mod(world, &mod_id);
    }
}

/// One frame of the page's loop.
fn frame(app: &mut App) {
    app.update();
}

fn registries(app: &App) -> Arc<slotted_registry::FrozenRegistries> {
    app.world().resource::<slotted_ecs::Registries>().0.clone()
}

fn id(text: &str) -> Namespaced {
    Namespaced::parse(text).expect("a valid id")
}

fn copper_chest() -> ModId {
    ModId::new(COPPER_CHEST).expect("a valid mod id")
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
            components: slotted_model::Value::Map(BTreeMap::new()),
        }),
    }
}

/// What `copper_chest`'s control script logs about `event`.
fn ask_copper_chest(app: &mut App, event: &ScriptEvent) -> Vec<String> {
    let script = app
        .world()
        .resource::<ControlScripts>()
        .by_mod
        .get(&copper_chest())
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

/// Everything the page would have seen since it last polled, as one string.
fn drained(bus: &Bus) -> String {
    use std::fmt::Write as _;
    bus.drain_console()
        .iter()
        .fold(String::new(), |mut out, line: &Line| {
            let _ = writeln!(out, "[{}/{}] {}", line.level, line.who, line.text);
            out
        })
}

/// Queues the page's own Run: new text for a file, then the reload.
fn press_run(bus: &Bus, path: &str, contents: &str) {
    bus.request(Request::Write {
        path: path.to_owned(),
        contents: contents.to_owned(),
    });
    bus.request(Request::Reload {
        mod_id: COPPER_CHEST.to_owned(),
    });
}

// ---------------------------------------------------------------------------
// A chunk that will not compile
// ---------------------------------------------------------------------------

#[test]
fn a_compile_error_keeps_the_old_control_script_and_reaches_the_page() {
    let (mut app, _source, bus) = harness();
    ModLoader::run_all(app.world_mut()).expect("the bundled mods load");
    assert_eq!(ask_copper_chest(&mut app, &a_click()), [ORIGINAL_LINE]);
    let _ = drained(&bus);

    press_run(&bus, CONTROL, "slotted.on(\"slot_click\", function(ev)\n");
    frame(&mut app);

    assert_eq!(
        ask_copper_chest(&mut app, &a_click()),
        [ORIGINAL_LINE],
        "a chunk that will not compile replaced the working control script"
    );
    let console = drained(&bus);
    assert!(
        console.contains("[error/"),
        "the compile error never reached the page's console: {console}"
    );
}

// ---------------------------------------------------------------------------
// A chunk that will not stop
// ---------------------------------------------------------------------------

#[test]
fn an_infinite_loop_in_data_lua_is_stopped_by_the_budget_and_the_game_runs_on() {
    let (mut app, _source, bus) = harness();
    ModLoader::run_all(app.world_mut()).expect("the bundled mods load");
    let before = registries(&app);
    let _ = drained(&bus);

    // Not a hang: `Limits::default().budget` is 1,000,000 ticks, and the
    // piccolo adapter charges 8 fuel to the tick, so this runs out and comes
    // back as a value.
    press_run(&bus, DATA, "while true do end\n");
    frame(&mut app);

    let console = drained(&bus);
    assert!(
        console.contains("[error/"),
        "the runaway data chunk was never reported: {console}"
    );
    assert!(
        console.to_lowercase().contains("budget"),
        "the report did not name the budget, so the page cannot tell a loop \
         from a typo: {console}"
    );

    // The game keeps running: the registries are the ones the last good data
    // stage built, and the control script still answers a click.
    let after = registries(&app);
    assert_eq!(before.items.len(), after.items.len());
    assert!(after.item_id(&id("copper_chest:copper_ingot")).is_some());
    assert_eq!(ask_copper_chest(&mut app, &a_click()), [ORIGINAL_LINE]);
}

// ---------------------------------------------------------------------------
// Two reloads in one frame
// ---------------------------------------------------------------------------

#[test]
fn two_reloads_queued_together_both_run_and_in_order() {
    let (mut app, source, bus) = harness();
    ModLoader::run_all(app.world_mut()).expect("the bundled mods load");
    let _ = drained(&bus);

    let first = "slotted.info(\"first edit\")\n";
    let second = "slotted.info(\"second edit\")\n";
    press_run(&bus, CONTROL, first);
    press_run(&bus, CONTROL, second);

    // Frame one takes the first write and its reload; the second pair is put
    // back, because applying its write here would mean the first reload read
    // text it was never asked to run.
    frame(&mut app);
    let after_one = drained(&bus);
    assert!(
        after_one.contains("first edit"),
        "the first reload was swallowed by the second: {after_one}"
    );
    assert_eq!(
        source.read_text(CONTROL).as_deref(),
        Some(first),
        "the second write reached the source before the first reload read it"
    );

    frame(&mut app);
    let after_two = drained(&bus);
    assert!(
        after_two.contains("second edit"),
        "the second reload never ran: {after_two}"
    );
    assert_eq!(source.read_text(CONTROL).as_deref(), Some(second));
}

// ---------------------------------------------------------------------------
// Names the page made up
// ---------------------------------------------------------------------------

#[test]
fn an_unknown_mod_or_file_is_an_error_and_not_a_panic() {
    assert!(
        web_playground::bundle::read_mod_file("no_such_mod", "control.lua")
            .is_err_and(|error| error.contains("no_such_mod")),
        "an unknown mod id must come back as an error naming it"
    );
    assert!(
        web_playground::bundle::read_mod_file(COPPER_CHEST, "no_such_file.lua")
            .is_err_and(|error| error.contains("no_such_file.lua")),
        "an unknown file must come back as an error naming it"
    );
    assert!(
        web_playground::bundle::read_mod_file(COPPER_CHEST, "control.lua")
            .is_ok_and(|text| text.contains("slotted.on")),
        "the real file still reads"
    );
}

#[test]
fn a_bad_mod_id_does_not_eat_the_requests_behind_it() {
    let (mut app, source, bus) = harness();
    ModLoader::run_all(app.world_mut()).expect("the bundled mods load");

    bus.request(Request::Reload {
        mod_id: "Not A Mod Id".to_owned(),
    });
    press_run(&bus, CONTROL, "slotted.info(\"after the bad id\")\n");
    frame(&mut app);

    let console = drained(&bus);
    assert!(
        console.contains("is not a mod id"),
        "the rejected id was never reported: {console}"
    );
    assert!(
        console.contains("after the bad id"),
        "a rejected id stopped the good reload behind it: {console}"
    );
    assert_eq!(
        source.read_text(CONTROL).as_deref(),
        Some("slotted.info(\"after the bad id\")\n")
    );
}

// ---------------------------------------------------------------------------
// The mod list
// ---------------------------------------------------------------------------

#[test]
fn the_mod_list_is_the_same_after_a_failed_reload() {
    let (mut app, _source, bus) = harness();
    ModLoader::run_all(app.world_mut()).expect("the bundled mods load");
    let before_ids = web_playground::bundle::mod_ids();
    let before_json = web_playground::bundle::mods_json();
    assert_eq!(before_ids, ["appleskin_like", COPPER_CHEST, "sorter"]);

    press_run(&bus, DATA, "this is not lua ===\n");
    frame(&mut app);
    press_run(&bus, CONTROL, "while true do end\n");
    frame(&mut app);

    assert_eq!(
        web_playground::bundle::mod_ids(),
        before_ids,
        "a failed reload changed what the editor lists"
    );
    assert_eq!(web_playground::bundle::mods_json(), before_json);
    assert!(
        web_playground::bundle::read_mod_file(COPPER_CHEST, "control.lua")
            .is_ok_and(|text| text.contains("slotted.on")),
        "the editor's Reset text was rewritten by a failed run"
    );
}

// ---------------------------------------------------------------------------
// Conservation
// ---------------------------------------------------------------------------

/// Every stack in the world, summed by item **name** rather than by id.
///
/// The name is the point: a reload re-interns every id, so a count keyed on
/// the raw id would compare two different numbering schemes.
fn totals_by_name(app: &mut App) -> BTreeMap<String, u32> {
    let registries = registries(app);
    let mut totals: BTreeMap<String, u32> = BTreeMap::new();
    let mut query = app.world_mut().query::<&slotted_ecs::menu::Inventory>();
    let stacks: Vec<ItemStack> = query
        .iter(app.world())
        .flat_map(|inventory| {
            (0..inventory.0.len())
                .filter_map(|slot| inventory.0.get(slot).cloned())
                .collect::<Vec<ItemStack>>()
        })
        .collect();
    for stack in stacks {
        let name = registries
            .items
            .name_of(stack.id)
            .map_or_else(|| format!("<unnamed {:?}>", stack.id), ToString::to_string);
        *totals.entry(name).or_default() += stack.count;
    }
    totals
}

#[test]
fn the_chest_holds_the_same_items_across_five_reloads() {
    let (mut app, source, bus) = harness();
    ModLoader::run_all(app.world_mut()).expect("the bundled mods load");
    let bundled_data = source.read_text(DATA).expect("the bundle has data.lua");

    for inventory in web_playground::scene::inventories(&registries(&app)) {
        app.world_mut()
            .spawn(slotted_ecs::menu::Inventory(inventory));
    }
    let before = totals_by_name(&mut app);
    assert!(
        before.get("copper_chest:copper_ingot").copied() == Some(64 + 31 + 8),
        "the demo chest did not start with the ingots the scene table names: {before:?}"
    );

    // Five rounds, each adding an item definition and then taking it away
    // again. Every add and every remove shifts the interned ids of everything
    // registered after it, so a reload that did not remap by name would leave
    // the ingots pointing at the chest, or at nothing.
    for round in 0..5 {
        let with_extra = format!(
            "{bundled_data}\nslotted.register_item(\"aaa_round_{round}\", {{ max_stack_size = 1 }})\n"
        );
        press_run(&bus, DATA, &with_extra);
        frame(&mut app);
        assert!(
            registries(&app)
                .item_id(&id(&format!("copper_chest:aaa_round_{round}")))
                .is_some(),
            "round {round}'s added item never registered"
        );
        assert_eq!(
            totals_by_name(&mut app),
            before,
            "round {round}'s add lost or moved a stack"
        );

        press_run(&bus, DATA, &bundled_data);
        frame(&mut app);
        assert!(
            registries(&app)
                .item_id(&id(&format!("copper_chest:aaa_round_{round}")))
                .is_none(),
            "round {round}'s added item outlived the reload that removed it"
        );
        assert_eq!(
            totals_by_name(&mut app),
            before,
            "round {round}'s remove lost or moved a stack"
        );
    }
}
