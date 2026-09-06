//! Run a mod's `tests/*.lua` through the harness. Phase 6 contract section
//! 3.2. The Lua side is `slotted_script::TEST_PRELUDE`; each yielded
//! [`TestOp`] is performed by [`ops`] against the harness's world.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy::prelude::*;
use slotted_model::{Inventory, MenuDef, SlotBehaviour, Value};
use slotted_packs::ScriptHost;
use slotted_script::{ModId, ScriptCommand, ScriptEvent, Stage, TestOp};

use crate::fixture::MenuFixture;
use crate::harness::UiHarness;

/// Ops one test body may yield before the runner calls it a runaway.
const MAX_STEPS: usize = 2000;

/// One test's outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaTestResult {
    /// Test name.
    pub name: String,
    /// Passed.
    pub passed: bool,
    /// Failure message.
    pub message: Option<String>,
    /// Ops performed.
    pub steps: usize,
}

/// One file's outcomes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaTestReport {
    /// Which mod.
    pub mod_id: String,
    /// Which file.
    pub file: PathBuf,
    /// In registration order.
    pub results: Vec<LuaTestResult>,
}

impl LuaTestReport {
    /// Every test passed.
    pub fn all_passed(&self) -> bool {
        self.results.iter().all(|r| r.passed)
    }

    /// The file's name as a report line shows it (`sort.lua`).
    pub fn file_name(&self) -> String {
        self.file.file_name().map_or_else(
            || self.file.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        )
    }
}

/// What performing one op produced.
#[derive(Debug, Clone, PartialEq)]
pub enum StepOutcome {
    /// Finished, with the value to hand back or the error to raise.
    Done(Result<Value, String>),
    /// Needs more frames (live driver only).
    Pending,
}

/// Something that can perform test ops against a world. The harness is one;
/// the playground's `LiveDriver` is the other.
pub trait TestDriver {
    /// Perform `op`.
    fn perform(&mut self, world: &mut World, op: &TestOp) -> StepOutcome;
}

/// Fixture aliases a host registered for `open_screen(kind, "alias")`.
#[derive(Resource, Default)]
pub struct LuaFixtures(
    pub std::collections::BTreeMap<String, std::sync::Arc<dyn MenuFixture + Send + Sync>>,
);

/// A fixture built from a Lua table: `{ slots = {27, 27, 9}, fill = { ["0:0"]
/// = { item = "ns:id", count = 8 } } }`.
#[derive(Debug, Clone, PartialEq)]
pub struct LuaFixture {
    /// Inventory sizes in order.
    pub slots: Vec<u16>,
    /// `(inventory, slot) -> (item, count)`.
    pub fill: Vec<((u16, u16), (String, u32))>,
}

impl LuaFixture {
    /// Parses the table form; `Err` names what was wrong.
    ///
    /// # Errors
    ///
    /// A value that is not a map, a missing or malformed `slots`, or a `fill`
    /// key that is not `"inventory:slot"`.
    pub fn from_value(value: &Value) -> Result<Self, String> {
        let Value::Map(map) = value else {
            return Err(format!(
                "a fixture table is a map with `slots` and an optional `fill`, not a {}",
                value.describe()
            ));
        };
        let slots = match map.get("slots") {
            Some(Value::List(items)) => items
                .iter()
                .map(|item| match item {
                    Value::Int(n) if *n >= 0 => u16::try_from(*n)
                        .map_err(|_| format!("inventory size {n} does not fit in u16")),
                    other => Err(format!(
                        "an inventory size is a non-negative integer, not {}",
                        other.describe()
                    )),
                })
                .collect::<Result<Vec<u16>, String>>()?,
            Some(other) => {
                return Err(format!(
                    "`slots` is a list of inventory sizes, not {}",
                    other.describe()
                ));
            }
            None => return Err("a fixture table needs `slots`".to_owned()),
        };
        if slots.is_empty() {
            return Err("`slots` needs at least one inventory".to_owned());
        }

        let mut fill = Vec::new();
        match map.get("fill") {
            None => {}
            Some(Value::Map(entries)) => {
                for (key, entry) in entries {
                    let position = parse_position(key)?;
                    let Value::Map(stack) = entry else {
                        return Err(format!(
                            "fill[{key:?}] is a table `{{ item = .., count = .. }}`, not a {}",
                            entry.describe()
                        ));
                    };
                    let Some(Value::Str(item)) = stack.get("item") else {
                        return Err(format!("fill[{key:?}] needs a string `item`"));
                    };
                    let count = match stack.get("count") {
                        None => 1,
                        Some(Value::Int(n)) if *n > 0 => u32::try_from(*n)
                            .map_err(|_| format!("fill[{key:?}]: count {n} does not fit"))?,
                        Some(other) => {
                            return Err(format!(
                                "fill[{key:?}]: count is a positive integer, not {}",
                                other.describe()
                            ));
                        }
                    };
                    fill.push((position, (item.clone(), count)));
                }
            }
            Some(other) => {
                return Err(format!(
                    "`fill` is a map keyed `\"inventory:slot\"`, not {}",
                    other.describe()
                ));
            }
        }
        Ok(Self { slots, fill })
    }

    /// The menu this fixture describes.
    ///
    /// `{n, 27, 9}` is [`MenuDef::generic`], so a fixture that names a
    /// container plus the player's pockets gets the quick-move routing and
    /// the hotbar every screen expects. Any other shape is a plain run of
    /// slots per inventory, in order, with no routing.
    pub fn def(&self) -> Arc<MenuDef> {
        if let [container, 27, 9] = self.slots[..] {
            return Arc::new(MenuDef::generic(container));
        }
        let mut def = MenuDef::new();
        for (index, count) in self.slots.iter().enumerate() {
            def.add_slots(
                slotted_model::InventoryRef::new(u16::try_from(index).unwrap_or(u16::MAX)),
                *count,
                SlotBehaviour::Normal,
            );
        }
        Arc::new(def)
    }

    /// The inventories, with `fill` resolved against `registries`.
    ///
    /// # Errors
    ///
    /// An item no registry holds, or a slot outside its inventory.
    pub fn inventories(
        &self,
        registries: &slotted_registry::FrozenRegistries,
    ) -> Result<Vec<Inventory>, String> {
        let mut out: Vec<Inventory> = self
            .slots
            .iter()
            .map(|n| Inventory::new(usize::from(*n)))
            .collect();
        for ((inventory, slot), (item, count)) in &self.fill {
            let target = out.get_mut(usize::from(*inventory)).ok_or_else(|| {
                format!("fill names inventory {inventory}, which the fixture has not")
            })?;
            if usize::from(*slot) >= target.len() {
                return Err(format!(
                    "fill names slot {slot} of inventory {inventory}, which holds {}",
                    target.len()
                ));
            }
            let name = slotted_model::Namespaced::parse(item)
                .map_err(|e| format!("fill item {item:?}: {e}"))?;
            let id = registries
                .item_id(&name)
                .ok_or_else(|| format!("fill item {item} is not registered"))?;
            target.set(
                usize::from(*slot),
                Some(slotted_model::ItemStack::new(id, *count)),
            );
        }
        Ok(out)
    }
}

/// `"1:5"` as `(1, 5)`.
fn parse_position(key: &str) -> Result<(u16, u16), String> {
    let (inventory, slot) = key
        .split_once(':')
        .ok_or_else(|| format!("a fill key is `\"inventory:slot\"`, not {key:?}"))?;
    let parse = |text: &str| {
        text.trim()
            .parse::<u16>()
            .map_err(|_| format!("a fill key is `\"inventory:slot\"`, not {key:?}"))
    };
    Ok((parse(inventory)?, parse(slot)?))
}

/// A [`LuaFixture`] resolved against the world's registries, ready for
/// `open_screen`.
struct BuiltFixture {
    def: Arc<MenuDef>,
    inventories: Vec<Inventory>,
}

impl MenuFixture for BuiltFixture {
    fn def(&self) -> Arc<MenuDef> {
        self.def.clone()
    }
    fn inventories(&self) -> Vec<Inventory> {
        self.inventories.clone()
    }
}

/// What `open_screen`'s `fixture` argument resolved to.
enum ResolvedFixture {
    /// A host alias.
    Alias(Arc<dyn MenuFixture + Send + Sync>),
    /// A table.
    Built(BuiltFixture),
}

impl MenuFixture for ResolvedFixture {
    fn def(&self) -> Arc<MenuDef> {
        match self {
            Self::Alias(f) => f.def(),
            Self::Built(f) => f.def(),
        }
    }
    fn inventories(&self) -> Vec<Inventory> {
        match self {
            Self::Alias(f) => f.inventories(),
            Self::Built(f) => f.inventories(),
        }
    }
    fn actor(&self) -> slotted_model::Actor {
        match self {
            Self::Alias(f) => f.actor(),
            Self::Built(f) => f.actor(),
        }
    }
}

/// The `World`-level implementation of every [`TestOp`], shared by the
/// harness driver and the playground's live driver.
pub mod ops {
    use bevy::prelude::*;
    use slotted_model::Value;
    use slotted_script::{TestLocator, TestOp};

    use crate::locator::Locator;

    /// Maps a Lua locator onto a harness [`Locator`].
    ///
    /// # Errors
    ///
    /// A malformed widget or item id, or a locator with no criterion at all,
    /// which would match the whole tree.
    pub fn to_locator(loc: &TestLocator) -> Result<Locator, String> {
        let mut out = Locator::default();
        if let Some(role) = &loc.role {
            out.role = Some(parse_role(role));
        }
        for (key, value) in &loc.tag {
            out = out.tag(key, value);
        }
        out.test_id.clone_from(&loc.test_id);
        out.text.clone_from(&loc.text);
        if let Some(widget) = &loc.widget {
            let kind = slotted_model::Namespaced::parse(widget)
                .map_err(|e| format!("widget {widget:?}: {e}"))?;
            out.widget = Some(slotted_ui::WidgetKind(kind));
        }
        if let Some(item) = &loc.item {
            out.item = Some(
                slotted_model::Namespaced::parse(item)
                    .map_err(|e| format!("item {item:?}: {e}"))?,
            );
        }
        if out.role.is_none()
            && out.tags.is_empty()
            && out.test_id.is_none()
            && out.text.is_none()
            && out.widget.is_none()
            && out.item.is_none()
        {
            return Err(
                "a locator needs at least one of role, tag, test_id, text, widget or item"
                    .to_owned(),
            );
        }
        if let Some(index) = loc.index {
            out = out.index(index);
        }
        Ok(out)
    }

    /// `SemanticRole` from its snake-case name; anything unknown is a
    /// `Custom` role, which is what a mod's own widget carries.
    fn parse_role(name: &str) -> slotted_ui::SemanticRole {
        use slotted_ui::SemanticRole as R;
        match name {
            "screen" => R::Screen,
            "panel" => R::Panel,
            "grid" => R::Grid,
            "slot" => R::Slot,
            "button" => R::Button,
            "text" => R::Text,
            "tooltip" => R::Tooltip,
            "rail" => R::Rail,
            "hotbar" => R::Hotbar,
            "anchor" => R::Anchor,
            "carried" => R::Carried,
            "browser" => R::Browser,
            "text_field" => R::TextField,
            "chip" => R::Chip,
            "card" => R::Card,
            "recipe_view" => R::RecipeView,
            "recipe_slot" => R::RecipeSlot,
            "tab" => R::Tab,
            "bookmark" => R::Bookmark,
            "tank" => R::Tank,
            "bar" => R::Bar,
            "side_tab" => R::SideTab,
            "viewport" => R::Viewport,
            "hud_layer" => R::HudLayer,
            other => R::Custom(other.to_owned()),
        }
    }

    /// Resolves to exactly one entity or a message naming near misses.
    ///
    /// # Errors
    ///
    /// Nothing matched, or several nodes did.
    pub fn resolve_one(world: &World, loc: &TestLocator) -> Result<Entity, String> {
        let locator = to_locator(loc)?;
        let found = locator.resolve(world);
        match found.as_slice() {
            [one] => Ok(*one),
            [] => Err(format!("no node matches {locator}")),
            many => Err(format!("{} nodes match {locator}", many.len())),
        }
    }

    /// A query op that needs no frames: `StackAt`, `TextOf`, `PropertyOf`,
    /// `TankFill`, `IsVisible`, `LogContains`. Returns `None` for action ops.
    pub fn query(world: &mut World, op: &TestOp) -> Option<Result<Value, String>> {
        let answer = match op {
            TestOp::StackAt { loc } => resolve_one(world, loc).map(|e| stack_at(world, e)),
            TestOp::TextOf { loc } => resolve_one(world, loc).map(|e| text_of(world, e)),
            TestOp::PropertyOf { loc } => resolve_one(world, loc).map(|e| property_of(world, e)),
            TestOp::TankFill { loc } => resolve_one(world, loc).and_then(|e| {
                world.get::<slotted_ui::FillValue>(e).map_or_else(
                    || Err(format!("{e} has no FillValue: it is not a tank or a bar")),
                    |fill| Ok(Value::Float(f64::from(fill.fraction()))),
                )
            }),
            TestOp::IsVisible { loc } => {
                resolve_one(world, loc).map(|e| Value::Bool(crate::queries::is_visible(world, e)))
            }
            TestOp::LogContains { text } => Ok(Value::Bool(log_contains(world, text))),
            _ => return None,
        };
        Some(answer)
    }

    /// `{ item, count }` of a slot entity, read from the model.
    fn stack_at(world: &World, slot: Entity) -> Value {
        let Some(stack) = read_stack(world, slot) else {
            return Value::Null;
        };
        let name = world
            .get_resource::<slotted_ecs::Registries>()
            .and_then(|r| r.items.name_of(stack.id))
            .map_or_else(|| format!("#{}", stack.id.0), ToString::to_string);
        let mut out = std::collections::BTreeMap::new();
        out.insert("item".to_owned(), Value::Str(name));
        out.insert("count".to_owned(), Value::Int(i64::from(stack.count)));
        Value::Map(out)
    }

    fn read_stack(world: &World, slot: Entity) -> Option<slotted_model::ItemStack> {
        let slot_ref = world.get::<slotted_ecs::SlotRef>(slot)?;
        let menu = world.get::<slotted_ecs::OpenMenu>(slot_ref.menu)?;
        let def = menu.def.slot(slot_ref.slot)?;
        let inventory = *menu.inventories.get(def.source.index())?;
        world
            .get::<slotted_ecs::Inventory>(inventory)?
            .get(usize::from(def.index))
            .cloned()
    }

    /// The node's `Text`, else its `SemanticLabel`.
    fn text_of(world: &World, entity: Entity) -> Value {
        world
            .get::<Text>(entity)
            .map(|t| t.0.clone())
            .or_else(|| {
                world
                    .get::<slotted_ui::SemanticLabel>(entity)
                    .map(|l| l.0.clone())
            })
            .map_or(Value::Null, Value::Str)
    }

    /// `{ id, value }` of the property a node is bound to.
    ///
    /// Read from the node's `PropertyBinding` and the menu's `MenuProperty`
    /// child rather than through the harness, so the live driver gets the
    /// same answer without one.
    fn property_of(world: &World, entity: Entity) -> Value {
        let Some(binding) = world.get::<slotted_ui::PropertyBinding>(entity) else {
            return Value::Null;
        };
        let Some(children) = world.get::<Children>(binding.menu) else {
            return Value::Null;
        };
        for child in children.iter() {
            if let Some(property) = world.get::<slotted_ecs::MenuProperty>(child)
                && property.id == binding.value
            {
                let mut out = std::collections::BTreeMap::new();
                out.insert("id".to_owned(), Value::Int(i64::from(property.id.0)));
                out.insert("value".to_owned(), Value::Int(i64::from(property.value)));
                return Value::Map(out);
            }
        }
        Value::Null
    }

    /// Whether any script console line contains `needle`.
    fn log_contains(world: &World, needle: &str) -> bool {
        world
            .get_resource::<slotted_packs::ScriptLogs>()
            .is_some_and(|logs| logs.entries.iter().any(|e| e.message.contains(needle)))
    }

    /// A `KeyCode` by the name a test writes (`Enter`, `KeyR`, `escape`).
    pub fn key_code(name: &str) -> Option<KeyCode> {
        KEYS.iter()
            .find(|code| format!("{code:?}").eq_ignore_ascii_case(name))
            .copied()
    }

    /// The keys a test may name. Matching on the `Debug` form keeps this a
    /// list rather than a second spelling of every name.
    const KEYS: &[KeyCode] = &[
        KeyCode::KeyA,
        KeyCode::KeyB,
        KeyCode::KeyC,
        KeyCode::KeyD,
        KeyCode::KeyE,
        KeyCode::KeyF,
        KeyCode::KeyG,
        KeyCode::KeyH,
        KeyCode::KeyI,
        KeyCode::KeyJ,
        KeyCode::KeyK,
        KeyCode::KeyL,
        KeyCode::KeyM,
        KeyCode::KeyN,
        KeyCode::KeyO,
        KeyCode::KeyP,
        KeyCode::KeyQ,
        KeyCode::KeyR,
        KeyCode::KeyS,
        KeyCode::KeyT,
        KeyCode::KeyU,
        KeyCode::KeyV,
        KeyCode::KeyW,
        KeyCode::KeyX,
        KeyCode::KeyY,
        KeyCode::KeyZ,
        KeyCode::Digit0,
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
        KeyCode::Enter,
        KeyCode::Escape,
        KeyCode::Space,
        KeyCode::Tab,
        KeyCode::Backspace,
        KeyCode::Delete,
        KeyCode::Home,
        KeyCode::End,
        KeyCode::PageUp,
        KeyCode::PageDown,
        KeyCode::ArrowUp,
        KeyCode::ArrowDown,
        KeyCode::ArrowLeft,
        KeyCode::ArrowRight,
        KeyCode::ShiftLeft,
        KeyCode::ControlLeft,
        KeyCode::AltLeft,
        KeyCode::F1,
        KeyCode::F3,
        KeyCode::F7,
        KeyCode::F8,
    ];
}

impl UiHarness {
    /// Registers a fixture alias for Lua `open_screen(kind, "alias")`.
    pub fn register_fixture(
        &mut self,
        alias: &str,
        fixture: impl MenuFixture + Send + Sync + 'static,
    ) {
        let world = self.world_mut();
        world.init_resource::<LuaFixtures>();
        world
            .resource_mut::<LuaFixtures>()
            .0
            .insert(alias.to_owned(), std::sync::Arc::new(fixture));
    }

    /// Loads `source` as a test-stage script of `mod_id` and runs every test
    /// it registers, performing each op through this harness.
    ///
    /// A failure inside one test is that test's message; a failure in the
    /// runtime or the protocol is one result named after the file, so a
    /// report never silently loses a file.
    pub fn run_lua_tests(&mut self, mod_id: &str, file: &Path, source: &str) -> LuaTestReport {
        let name = file.file_name().map_or_else(
            || file.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let results = match self.run_lua_tests_inner(mod_id, &name, source) {
            Ok(results) => results,
            Err(message) => vec![LuaTestResult {
                name: name.clone(),
                passed: false,
                message: Some(message),
                steps: 0,
            }],
        };
        LuaTestReport {
            mod_id: mod_id.to_owned(),
            file: file.to_path_buf(),
            results,
        }
    }

    fn run_lua_tests_inner(
        &mut self,
        mod_id: &str,
        file: &str,
        source: &str,
    ) -> Result<Vec<LuaTestResult>, String> {
        let host = self
            .world()
            .get_resource::<ScriptHost>()
            .cloned()
            .ok_or_else(|| {
                "no ScriptHost: build the harness with a facade that has a script runtime"
                    .to_owned()
            })?;
        let id = ModId::new(mod_id).map_err(|e| e.to_string())?;
        let script = host
            .lock()
            .load(&id, file, source, Stage::Test)
            .map_err(|e| e.to_string())?;

        let outcome = self.drive_tests(&host, script);
        host.lock().unload(script);
        outcome
    }

    /// The `test_list` / `test_run` / `test_resume` loop of contract 3.1.
    fn drive_tests(
        &mut self,
        host: &ScriptHost,
        script: slotted_script::ScriptId,
    ) -> Result<Vec<LuaTestResult>, String> {
        let call = |event: ScriptEvent| -> Result<ScriptCommand, String> {
            let commands = host
                .lock()
                .call(script, &event)
                .map_err(|e| e.to_string())?;
            commands
                .into_iter()
                .find(|c| {
                    matches!(
                        c,
                        ScriptCommand::TestList { .. }
                            | ScriptCommand::TestStep { .. }
                            | ScriptCommand::TestDone { .. }
                    )
                })
                .ok_or_else(|| "the test file answered with no test command".to_owned())
        };

        let names = match call(ScriptEvent::TestList)? {
            ScriptCommand::TestList { names } => names,
            other => return Err(format!("expected a test list, got {other:?}")),
        };

        let mut results = Vec::with_capacity(names.len());
        for name in names {
            let mut steps = 0usize;
            let mut command = call(ScriptEvent::TestRun { name: name.clone() })?;
            let result = loop {
                match command {
                    ScriptCommand::TestDone {
                        name,
                        passed,
                        message,
                    } => {
                        break LuaTestResult {
                            name,
                            passed,
                            message,
                            steps,
                        };
                    }
                    ScriptCommand::TestStep { op } => {
                        steps += 1;
                        if steps > MAX_STEPS {
                            break LuaTestResult {
                                name: name.clone(),
                                passed: false,
                                message: Some(format!("more than {MAX_STEPS} steps: runaway test")),
                                steps,
                            };
                        }
                        let (value, error) = match self.perform_op(&op) {
                            Ok(value) => (value, None),
                            Err(message) => (Value::Null, Some(message)),
                        };
                        command = call(ScriptEvent::TestResume { value, error })?;
                    }
                    other => return Err(format!("expected a test step, got {other:?}")),
                }
            };
            results.push(result);
        }
        Ok(results)
    }

    /// [`mod_layout`](UiHarness::mod_layout) plus the base pack's
    /// `data/<namespace>/` directories, each as a script-less mod.
    ///
    /// A running game registers its own content before any mod does. A
    /// harness has no game, so `assets/data/demo` is never read and a mod
    /// test's fixture cannot name a single item; this is how a test, and
    /// `test-mods`, plays the host. Nothing is added for a namespace a real
    /// mod already owns.
    ///
    /// # Panics
    ///
    /// When no `mods_dir` was set on the builder, or the synthetic manifests
    /// cannot be ordered, which would mean a mod depends on a base namespace
    /// in a way that contradicts itself.
    pub fn mod_layout_with_base(&mut self) -> slotted_packs::PackLayout {
        let mut layout = self.mod_layout();
        let base = layout.base.clone();
        let Ok(entries) = std::fs::read_dir(base.join("data")) else {
            return layout;
        };
        let mut namespaces: Vec<String> = entries
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        namespaces.sort();

        let mut manifests = Vec::new();
        for id in namespaces {
            if layout.mods.iter().any(|m| m.id().as_str() == id) {
                continue;
            }
            // Parsed from TOML rather than built field by field: this crate
            // has no `semver` dependency of its own.
            let text = format!(
                "id = \"{id}\"\nname = \"{id} (base pack)\"\nversion = \"0.0.0\"\napi_version = 1\n"
            );
            match slotted_registry::manifest::ModManifest::parse(
                &format!("assets/data/{id}"),
                &text,
            ) {
                Ok(manifest) => manifests.push((base.clone(), manifest)),
                Err(error) => tracing::warn!(%error, "base namespace {id}"),
            }
        }
        for entry in layout.mods.iter() {
            manifests.push((entry.root.clone(), entry.manifest.clone()));
        }
        layout.mods = slotted_packs::ModSet::from_manifests(manifests)
            .unwrap_or_else(|e| panic!("ordering the base namespaces: {e}"));
        layout
    }

    /// Runs `tests/*.lua` (sorted) of `mod_id` under the mods directory the
    /// harness was built with.
    ///
    /// # Panics
    ///
    /// When the harness was built without
    /// [`mods_dir`](crate::UiHarnessBuilder::mods_dir).
    pub fn run_mod_tests(&mut self, mod_id: &str) -> Vec<LuaTestReport> {
        let dir = self
            .world()
            .get_resource::<crate::mods::ModsUnderTest>()
            .expect("run_mod_tests needs UiHarness::builder().mods_dir(..)")
            .dir()
            .join(mod_id)
            .join("tests");
        let mut out = Vec::new();
        for file in test_files(&dir) {
            match std::fs::read_to_string(&file) {
                Ok(source) => out.push(self.run_lua_tests(mod_id, &file, &source)),
                Err(error) => out.push(LuaTestReport {
                    mod_id: mod_id.to_owned(),
                    file: file.clone(),
                    results: vec![LuaTestResult {
                        name: file.display().to_string(),
                        passed: false,
                        message: Some(format!("unreadable: {error}")),
                        steps: 0,
                    }],
                }),
            }
        }
        out
    }

    /// Performs one op against this harness, answering what the body asked
    /// for. The harness owns its world, so this is the inherent form
    /// [`TestDriver::perform`] delegates to.
    ///
    /// # Errors
    ///
    /// The message the test body raises: a locator that matched nothing, a
    /// fixture that did not type, a `settle` that timed out.
    pub fn perform_op(&mut self, op: &TestOp) -> Result<Value, String> {
        if let Some(answer) = ops::query(self.world_mut(), op) {
            return answer;
        }
        match op {
            TestOp::OpenScreen { kind, fixture } => self.open_for_test(kind, fixture),
            TestOp::Click { loc } => self.act(loc, UiHarness::click),
            TestOp::ShiftClick { loc } => self.act(loc, UiHarness::shift_click),
            TestOp::RightClick { loc } => self.act(loc, UiHarness::right_click),
            TestOp::Hover { loc } => self.act(loc, UiHarness::hover),
            TestOp::Cycle { loc, forward } => {
                let entity = ops::resolve_one(self.world(), loc)?;
                let forward = *forward;
                self.cycle(entity, forward);
                Ok(Value::Null)
            }
            TestOp::Key { key } => {
                let code = ops::key_code(key).ok_or_else(|| format!("no key named {key:?}"))?;
                self.key(code);
                Ok(Value::Null)
            }
            TestOp::TypeText { text } => {
                self.type_text(text);
                Ok(Value::Null)
            }
            TestOp::Settle => self
                .try_settle()
                .map(|_| Value::Null)
                .map_err(|e| e.to_string()),
            TestOp::Step { frames } => {
                self.step(*frames as usize);
                Ok(Value::Null)
            }
            // `ops::query` answered every other variant above.
            other => Err(format!("{other:?} is not an action the harness performs")),
        }
    }

    /// Resolve one node, then do something to it.
    fn act(
        &mut self,
        loc: &slotted_script::TestLocator,
        action: fn(&mut Self, Entity),
    ) -> Result<Value, String> {
        // Let layout stop moving before the pointer aims at anything. A
        // pointer gesture is delivered at a position, so a node that is still
        // settling -- a font that has just loaded and re-measured every label
        // is the usual reason -- would be clicked where it used to be.
        let _ = self.try_settle();
        let entity = ops::resolve_one(self.world(), loc)?;
        action(self, entity);
        Ok(Value::Null)
    }

    /// `open_screen` with the fixture the test named.
    fn open_for_test(&mut self, kind: &str, fixture: &Value) -> Result<Value, String> {
        let fixture = self.resolve_fixture(fixture)?;
        let kind = slotted_ui::ScreenKind(
            slotted_model::Namespaced::parse(kind).map_err(|e| format!("screen {kind:?}: {e}"))?,
        );
        if self
            .world()
            .resource::<slotted_ui::Screens>()
            .get(&kind)
            .is_none()
        {
            let mut known: Vec<String> = self
                .world()
                .resource::<slotted_ui::Screens>()
                .0
                .keys()
                .map(|k| k.0.to_string())
                .collect();
            known.sort();
            return Err(format!(
                "screen {} is not registered; known: {}",
                kind.0,
                if known.is_empty() {
                    "none".to_owned()
                } else {
                    known.join(", ")
                }
            ));
        }
        // Every test in a file runs against one world. A screen an earlier
        // test opened would still be there, so `test_id = "sorter_sort"`
        // would match one node per screen and the second test would fail on
        // an ambiguous locator rather than on what it asserts.
        self.close_open_screens();
        let opened = self.open_screen(kind, fixture);
        let mut out = std::collections::BTreeMap::new();
        out.insert(
            "menu".to_owned(),
            Value::Int(i64::from(opened.menu.index().index())),
        );
        out.insert(
            "screen".to_owned(),
            Value::Int(i64::from(opened.screen.index().index())),
        );
        Ok(Value::Map(out))
    }

    /// Closes every screen currently open, so the next `open_screen` leaves
    /// exactly one. Menus are left to `close_menu`; a fresh `open_screen`
    /// installs its own.
    fn close_open_screens(&mut self) {
        let world = self.world_mut();
        let mut roots = world.query_filtered::<Entity, With<slotted_ui::ScreenRoot>>();
        let open: Vec<Entity> = roots.iter(world).collect();
        if open.is_empty() {
            return;
        }
        for root in open {
            world.trigger(slotted_ui::ScreenClosed { entity: root });
            world.entity_mut(root).despawn();
        }
        self.step(1);
    }

    /// `"empty"`, a registered alias, or a fixture table.
    fn resolve_fixture(&mut self, fixture: &Value) -> Result<ResolvedFixture, String> {
        match fixture {
            Value::Str(alias) if alias == "empty" => {
                let def = MenuDef::generic(0);
                Ok(ResolvedFixture::Built(BuiltFixture {
                    inventories: def
                        .inventory_sizes()
                        .into_iter()
                        .map(Inventory::new)
                        .collect(),
                    def: Arc::new(def),
                }))
            }
            Value::Str(alias) => {
                let found = self
                    .world()
                    .get_resource::<LuaFixtures>()
                    .and_then(|f| f.0.get(alias).cloned());
                found.map(ResolvedFixture::Alias).ok_or_else(|| {
                    format!("no fixture named {alias:?}: register one with register_fixture")
                })
            }
            table => {
                let parsed = LuaFixture::from_value(table)?;
                let registries = self
                    .world()
                    .get_resource::<slotted_ecs::Registries>()
                    .map(|r| r.0.clone())
                    .ok_or_else(|| {
                        "a fixture table names items, but the world has no Registries".to_owned()
                    })?;
                Ok(ResolvedFixture::Built(BuiltFixture {
                    def: parsed.def(),
                    inventories: parsed.inventories(&registries)?,
                }))
            }
        }
    }
}

/// `dir`'s `*.lua`, sorted.
fn test_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "lua"))
        .collect();
    files.sort();
    files
}

impl TestDriver for UiHarness {
    fn perform(&mut self, _world: &mut World, op: &TestOp) -> StepOutcome {
        StepOutcome::Done(self.perform_op(op))
    }
}
