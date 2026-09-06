//! A scripted [`ScriptRuntime`] for the packs tests.
//!
//! Package A owned the script adapter; these tests must not wait for it, so the
//! runtime here simply replays commands the test wrote down, keyed by mod and
//! event name.

#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use slotted_script::{
    Limits, ModId, ScriptCommand, ScriptError, ScriptEvent, ScriptId, ScriptRuntime, Stage,
};

/// What a fake script answers, per `(mod id, event name)`.
pub type Reply = Result<Vec<ScriptCommand>, ScriptError>;

/// The replies, shared with the test so it can rewrite them between loads.
#[derive(Debug, Default)]
pub struct Replies {
    pub by_event: HashMap<(String, String), Reply>,
    /// Every `(mod, event)` the host actually asked for, in order.
    pub calls: Vec<(String, String)>,
}

impl Replies {
    pub fn set(&mut self, mod_id: &str, event: &str, reply: Reply) {
        self.by_event
            .insert((mod_id.to_owned(), event.to_owned()), reply);
    }
}

/// A [`ScriptRuntime`] that returns scripted commands.
#[derive(Debug, Clone, Default)]
pub struct FakeRuntime {
    pub replies: Arc<Mutex<Replies>>,
    loaded: Arc<Mutex<Vec<Option<(ModId, Stage)>>>>,
}

impl FakeRuntime {
    pub fn new(replies: Arc<Mutex<Replies>>) -> Self {
        Self {
            replies,
            loaded: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Which scripts are still loaded, oldest first.
    pub fn live(&self) -> Vec<(ModId, Stage)> {
        self.loaded
            .lock()
            .expect("not poisoned")
            .iter()
            .flatten()
            .cloned()
            .collect()
    }
}

impl ScriptRuntime for FakeRuntime {
    fn load(
        &mut self,
        mod_id: &ModId,
        _name: &str,
        _source: &str,
        stage: Stage,
    ) -> Result<ScriptId, ScriptError> {
        let mut loaded = self.loaded.lock().expect("not poisoned");
        loaded.push(Some((mod_id.clone(), stage)));
        Ok(ScriptId(
            u32::try_from(loaded.len() - 1).expect("few scripts"),
        ))
    }

    fn unload(&mut self, id: ScriptId) {
        let mut loaded = self.loaded.lock().expect("not poisoned");
        if let Some(slot) = loaded.get_mut(id.0 as usize) {
            *slot = None;
        }
    }

    fn call(
        &mut self,
        id: ScriptId,
        event: &ScriptEvent,
    ) -> Result<Vec<ScriptCommand>, ScriptError> {
        let owner = {
            let loaded = self.loaded.lock().expect("not poisoned");
            match loaded.get(id.0 as usize).and_then(Option::as_ref) {
                Some((mod_id, _)) => mod_id.clone(),
                None => return Err(ScriptError::UnknownScript(id)),
            }
        };
        let mut replies = self.replies.lock().expect("not poisoned");
        replies
            .calls
            .push((owner.to_string(), event.name().to_owned()));
        match replies
            .by_event
            .get(&(owner.to_string(), event.name().to_owned()))
        {
            Some(Ok(commands)) => Ok(commands.clone()),
            Some(Err(error)) => Err(error.clone()),
            None => Ok(Vec::new()),
        }
    }

    fn set_limits(&mut self, _limits: Limits) {}
}

// -- the shared app harness -------------------------------------------------
//
// One fixture tree, one `App` wired the way the pack plugin wires a game's,
// and a scripted runtime standing in for Lua. `lifecycle.rs` drives the
// stages against it; `review_round2.rs` drives the reload seams.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use slotted_ecs::{MenuIdAllocator, SlotRef, SlottedEcsSet, open_menu};
use slotted_model::{Actor, InventoryRef, ItemStack, MenuDef, Namespaced, SlotDef, SlotIx, Value};
use slotted_packs::{
    ControlScripts, ModErrors, ModFailed, ModLoader, ModReloaded, PackLayout, PendingScriptEvents,
    ReloadMod, ScriptHost, ScriptLog, ScriptLogs, route,
};
use slotted_testutils::RecordingAuthority;
use slotted_ui::def::ScreenKind;

// -- fixtures ---------------------------------------------------------------

pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

pub fn layout_at(root: &Path) -> PackLayout {
    PackLayout::new(root.join("base"))
        .with_mods(&root.join("mods"))
        .expect("the fixture mods discover")
}

pub fn id(text: &str) -> Namespaced {
    Namespaced::parse(text).expect("a valid id")
}

pub fn mod_id(text: &str) -> ModId {
    ModId::new(text).expect("a valid mod id")
}

pub fn map<const N: usize>(pairs: [(&str, Value); N]) -> Value {
    Value::Map(
        pairs
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect::<BTreeMap<_, _>>(),
    )
}

pub fn text(value: &str) -> Value {
    Value::Str(value.to_owned())
}

/// The smallest tree `slotted_ui::ScreenDef` accepts.
pub fn screen_tree() -> Value {
    map([(
        "root",
        map([
            ("type", text("panel")),
            ("role", text("panel")),
            ("children", Value::List(Vec::new())),
        ]),
    )])
}

// -- app --------------------------------------------------------------------

pub struct Harness {
    pub app: App,
    pub replies: Arc<Mutex<Replies>>,
    pub authority: Arc<RecordingAuthority>,
}

pub fn harness_at(root: &Path) -> Harness {
    let replies = Arc::new(Mutex::new(Replies::default()));
    let authority = RecordingAuthority::new();
    let mut app = slotted_testutils::ecs_app_with(authority.clone());
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
        .insert_resource(ScriptHost::new(FakeRuntime::new(replies.clone())))
        .insert_resource(layout_at(root))
        .add_observer(route::on_slot_clicked)
        .add_observer(route::on_screen_spawned)
        .add_observer(route::on_screen_closed)
        .add_systems(
            Update,
            route::dispatch_script_events.before(SlottedEcsSet::Input),
        );
    Harness {
        app,
        replies,
        authority,
    }
}

pub fn harness() -> Harness {
    harness_at(&fixtures())
}

impl Harness {
    pub fn reply(&self, owner: &str, event: &str, reply: Result<Vec<ScriptCommand>, ScriptError>) {
        self.replies
            .lock()
            .expect("not poisoned")
            .set(owner, event, reply);
    }

    pub fn run_all(&mut self) -> slotted_registry::LoadReport {
        ModLoader::run_all(self.app.world_mut()).expect("the fixture set loads")
    }

    pub fn registries(&self) -> Arc<slotted_registry::FrozenRegistries> {
        self.app
            .world()
            .resource::<slotted_ecs::Registries>()
            .0
            .clone()
    }

    pub fn errors(&self) -> Vec<slotted_packs::ModError> {
        self.app.world().resource::<ModErrors>().0.clone()
    }

    /// A four-slot menu over one inventory, plus a slot entity for slot 1.
    pub fn open_menu_with(&mut self, stacks: Vec<Option<ItemStack>>) -> (Entity, Entity, u32) {
        let def = MenuDef {
            slots: (0..4)
                .map(|index| SlotDef::new(InventoryRef::new(0), index))
                .collect(),
            ..MenuDef::default()
        };
        let world = self.app.world_mut();
        let inventory = world
            .spawn(slotted_ecs::Inventory(
                slotted_model::Inventory::from_slots(stacks),
            ))
            .id();
        let mut ids = world
            .remove_resource::<MenuIdAllocator>()
            .expect("the ecs plugin inserts one");
        let menu = {
            let mut commands = world.commands();
            open_menu(
                &mut commands,
                &mut ids,
                Arc::new(def),
                vec![inventory],
                Actor::SURVIVAL,
            )
        };
        world.flush();
        world.insert_resource(ids);
        let menu_id = world
            .get::<slotted_ecs::OpenMenu>(menu)
            .expect("the menu exists")
            .id;
        let slot = world
            .spawn(SlotRef {
                menu,
                slot: SlotIx(1),
            })
            .id();
        world
            .resource_mut::<route::OpenScreens>()
            .by_menu
            .insert(menu, (menu_id, ScreenKind::new("beta:chest")));
        (menu, slot, menu_id.0)
    }
}

// -- helpers ----------------------------------------------------------------

pub fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "slotted-packs-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

pub fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("destination");
    for entry in std::fs::read_dir(from).expect("readable") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy");
        }
    }
}
