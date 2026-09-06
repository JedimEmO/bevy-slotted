//! UI events to scripts and script commands back. Contract section 2.4.

use std::collections::{HashMap, HashSet, VecDeque};

use bevy::prelude::*;
use slotted_ecs::{MenuAction, OpenMenu, SlotClicked, SlotRef};
use slotted_model::{ClickAction, InventoryRef, ItemStack, MenuId, SlotIx, ToolbarAction};
use slotted_registry::FrozenRegistries;
use slotted_script::{Button, LogLevel, ModId, Modifiers, ScriptCommand, ScriptEvent, StackInfo};
use slotted_ui::def::ScreenKind;
use slotted_ui::{ScreenClosed, ScreenRoot, ScreenSpawned};

use crate::lifecycle::{fail, log};
use crate::{ControlScripts, ModError, ModSet, ScriptHost};

/// The event name a control script subscribes to for tooltip parts.
pub const TOOLTIP_BUILD: &str = "tooltip_build";

/// How many script calls one frame may make, across every mod. Interrupt
/// ticks bound a single call; this bounds the frame, so a storm of clicks
/// cannot stall the UI thread. No wall clock is involved.
pub const MAX_SCRIPT_CALLS_PER_FRAME: usize = 512;

/// Events collected this frame, delivered in `SlottedPacksSet::Dispatch`.
#[derive(Resource, Debug, Default)]
pub struct PendingScriptEvents(pub VecDeque<ScriptEvent>);

/// `(mod, call)` pairs already warned about, so a deprecation is reported
/// once and not once per frame.
#[derive(Resource, Debug, Default)]
pub struct WarnedDeprecations(pub HashSet<(ModId, String)>);

/// Which screen kind each open menu shows, so a `SlotClicked` can name its
/// screen without walking the hierarchy.
#[derive(Resource, Debug, Default)]
pub struct OpenScreens {
    /// Menu entity to its id and kind.
    pub by_menu: HashMap<Entity, (MenuId, ScreenKind)>,
    /// Screen root to menu entity.
    pub by_root: HashMap<Entity, Option<Entity>>,
}

impl OpenScreens {
    /// The kind shown for `menu`, if a screen is open on it.
    pub fn kind_of(&self, menu: Entity) -> Option<&ScreenKind> {
        self.by_menu.get(&menu).map(|(_, kind)| kind)
    }
}

/// A stack as a script sees it: names, not interned ids.
pub(crate) fn stack_info(frozen: &FrozenRegistries, stack: &ItemStack) -> StackInfo {
    let item = frozen.items.name_of(stack.id).map_or_else(
        || format!("slotted:unknown/{}", stack.id.0),
        ToString::to_string,
    );
    let mut components = std::collections::BTreeMap::new();
    for (id, value) in stack.patch.iter() {
        if let Some(name) = frozen.components.name(id) {
            components.insert(name.to_string(), value.clone());
        }
    }
    StackInfo {
        item,
        count: stack.count,
        components: slotted_model::Value::Map(components),
    }
}

/// Whether any control script listens for `event`; the collectors do no work
/// when nobody is subscribed.
fn anybody_wants(scripts: &ControlScripts, event: &str) -> bool {
    scripts.subscribed(event).next().is_some()
}

/// `SlotClicked` -> `ScriptEvent::SlotClick`. Only enqueues.
///
/// Naming the stack under the pointer needs the slot, its menu, that menu's
/// inventories and the frozen registries, which is what the parameter count
/// is; splitting it into a `SystemParam` would only move the same access.
#[allow(clippy::too_many_arguments)]
pub fn on_slot_clicked(
    click: On<SlotClicked>,
    scripts: Res<ControlScripts>,
    slots: Query<&SlotRef>,
    menus: Query<&OpenMenu>,
    inventories: Query<&slotted_ecs::Inventory>,
    open: Res<OpenScreens>,
    registries: Option<Res<slotted_ecs::Registries>>,
    mut pending: ResMut<PendingScriptEvents>,
) {
    if !anybody_wants(&scripts, "slot_click") {
        return;
    }
    let Ok(slot_ref) = slots.get(click.entity) else {
        return;
    };
    let Ok(menu) = menus.get(slot_ref.menu) else {
        return;
    };
    let Some(kind) = open.kind_of(slot_ref.menu) else {
        return;
    };
    let stack = registries.as_ref().and_then(|registries| {
        let def = menu.def.slots.get(slot_ref.slot.index())?;
        let inventory = menu.inventories.get(def.source.index())?;
        let stack = inventories
            .get(*inventory)
            .ok()?
            .0
            .get(def.index as usize)?;
        Some(stack_info(&registries.0, stack))
    });
    pending.0.push_back(ScriptEvent::SlotClick {
        menu: menu.id,
        screen: kind.0.to_string(),
        slot: slot_ref.slot.0,
        button: match click.button {
            slotted_model::Button::Left => Button::Left,
            slotted_model::Button::Right => Button::Right,
            slotted_model::Button::Middle => Button::Middle,
        },
        modifiers: Modifiers {
            shift: click.modifiers.shift,
            ctrl: click.modifiers.ctrl,
            alt: click.modifiers.alt,
        },
        stack,
    });
}

/// `Activate` on a `WidgetNode` inside a screen -> `WidgetActivate`.
pub fn on_widget_activate(
    activate: On<bevy::ui_widgets::Activate>,
    scripts: Res<ControlScripts>,
    widgets: Query<(&slotted_ui::WidgetNode, Option<&slotted_ui::Tags>)>,
    parents: Query<&ChildOf>,
    roots: Query<&ScreenRoot>,
    menus: Query<&OpenMenu>,
    mut pending: ResMut<PendingScriptEvents>,
) {
    if !anybody_wants(&scripts, "widget_activate") {
        return;
    }
    let entity = activate.entity;
    let Ok((widget, tags)) = widgets.get(entity) else {
        return;
    };
    // Walk up to the screen root the widget belongs to; a widget outside one
    // has no screen to name and is not a script's business.
    let mut current = entity;
    let root = loop {
        if let Ok(root) = roots.get(current) {
            break Some(root);
        }
        match parents.get(current) {
            Ok(parent) => current = parent.parent(),
            Err(_) => break None,
        }
    };
    let Some(root) = root else {
        return;
    };
    pending.0.push_back(ScriptEvent::WidgetActivate {
        menu: root
            .menu
            .and_then(|menu| menus.get(menu).ok())
            .map(|m| m.id),
        screen: root.kind.0.to_string(),
        widget: widget.0.0.to_string(),
        tags: tags.map(|t| t.0.clone()).unwrap_or_default(),
    });
}

/// `ScreenSpawned` -> `ScreenOpened`, and records the screen.
pub fn on_screen_spawned(
    spawned: On<ScreenSpawned>,
    roots: Query<&ScreenRoot>,
    menus: Query<&OpenMenu>,
    mut open: ResMut<OpenScreens>,
    mut pending: ResMut<PendingScriptEvents>,
) {
    let Ok(root) = roots.get(spawned.entity) else {
        return;
    };
    open.by_root.insert(spawned.entity, root.menu);
    let id = root
        .menu
        .and_then(|menu| menus.get(menu).ok())
        .map(|m| m.id);
    if let (Some(menu), Some(id)) = (root.menu, id) {
        open.by_menu.insert(menu, (id, root.kind.clone()));
    }
    pending.0.push_back(ScriptEvent::ScreenOpened {
        menu: id,
        screen: root.kind.0.to_string(),
    });
}

/// `ScreenClosed` -> `ScreenClosed`, and forgets the screen.
pub fn on_screen_closed(
    closed: On<ScreenClosed>,
    roots: Query<&ScreenRoot>,
    menus: Query<&OpenMenu>,
    mut open: ResMut<OpenScreens>,
    mut pending: ResMut<PendingScriptEvents>,
) {
    let Ok(root) = roots.get(closed.entity) else {
        return;
    };
    open.by_root.remove(&closed.entity);
    if let Some(menu) = root.menu {
        open.by_menu.remove(&menu);
    }
    pending.0.push_back(ScriptEvent::ScreenClosed {
        menu: root
            .menu
            .and_then(|menu| menus.get(menu).ok())
            .map(|m| m.id),
        screen: root.kind.0.to_string(),
    });
}

/// Browser `OpenRecipes`, `OpenUses`, `SearchChanged` -> events.
pub fn collect_browser_events(
    registries: Option<Res<slotted_ecs::Registries>>,
    mut recipes: MessageReader<slotted_browser::OpenRecipes>,
    mut uses: MessageReader<slotted_browser::OpenUses>,
    mut search: MessageReader<slotted_browser::SearchChanged>,
    mut pending: ResMut<PendingScriptEvents>,
) {
    for message in recipes.read() {
        if let Some(item) = ingredient_name(registries.as_deref(), &message.0) {
            pending.0.push_back(ScriptEvent::RecipeLookup {
                item,
                mode: slotted_script::LookupMode::Recipes,
            });
        }
    }
    for message in uses.read() {
        if let Some(item) = ingredient_name(registries.as_deref(), &message.0) {
            pending.0.push_back(ScriptEvent::RecipeLookup {
                item,
                mode: slotted_script::LookupMode::Uses,
            });
        }
    }
    for message in search.read() {
        pending.0.push_back(ScriptEvent::SearchChanged {
            text: message.text.clone(),
        });
    }
}

/// The id a browser ingredient names, `#tag` included. Item ingredients carry
/// a dense id, so naming one needs the frozen registries.
fn ingredient_name(
    registries: Option<&slotted_ecs::Registries>,
    ingredient: &slotted_browser::Ingredient,
) -> Option<String> {
    use slotted_browser::IngredientValue;
    match &ingredient.value {
        IngredientValue::Item(id) => Some(registries?.items.name_of(*id)?.to_string()),
        IngredientValue::Tag(name) => Some(format!("#{name}")),
        IngredientValue::Fluid(name) | IngredientValue::Info(name) => Some(name.to_string()),
    }
}

/// Drains [`PendingScriptEvents`], calls every subscribed control script in
/// load order, and applies the commands. Exclusive so validation can see
/// `OpenMenu`s and trigger `MenuAction`s in one place.
pub fn dispatch_script_events(world: &mut World) {
    let events: Vec<ScriptEvent> = {
        let Some(mut pending) = world.get_resource_mut::<PendingScriptEvents>() else {
            return;
        };
        pending.0.drain(..).collect()
    };
    if events.is_empty() {
        return;
    }
    let Some(host) = world.get_resource::<ScriptHost>().cloned() else {
        return;
    };
    let order: Vec<ModId> = world
        .get_resource::<ModSet>()
        .map(|mods| {
            mods.load_order()
                .iter()
                .map(crate::lifecycle::script_mod_id)
                .collect()
        })
        .unwrap_or_default();

    let mut budget = MAX_SCRIPT_CALLS_PER_FRAME;
    for event in events {
        let name = event.name();
        let targets: Vec<(ModId, slotted_script::ScriptId)> = {
            let Some(scripts) = world.get_resource::<ControlScripts>() else {
                return;
            };
            order
                .iter()
                .filter_map(|mod_id| {
                    let script = scripts.by_mod.get(mod_id)?;
                    script
                        .events
                        .contains(name)
                        .then(|| (mod_id.clone(), script.id))
                })
                .collect()
        };
        for (mod_id, script) in targets {
            if budget == 0 {
                tracing::warn!(
                    event = name,
                    "per-frame script call budget exhausted; remaining handlers are skipped"
                );
                return;
            }
            budget -= 1;
            let result = host.lock().call(script, &event);
            let commands = match result {
                Ok(commands) => commands,
                Err(source) => {
                    // One mod's handler blowing up says nothing about the next
                    // mod's, so the loop keeps going.
                    let error = match source {
                        slotted_script::ScriptError::BudgetExceeded { .. }
                        | slotted_script::ScriptError::Memory { .. } => ModError::Budget {
                            mod_id: mod_id.clone(),
                        },
                        source => ModError::Script {
                            mod_id: mod_id.clone(),
                            source,
                        },
                    };
                    fail(world, Some(mod_id), error);
                    continue;
                }
            };
            for command in &commands {
                match apply_control_command(world, &mod_id, command) {
                    Ok(actions) => {
                        for action in actions {
                            world.trigger(action);
                        }
                    }
                    Err(error) => fail(world, Some(mod_id.clone()), error),
                }
            }
        }
    }
}

/// The menu entity whose `OpenMenu.id` is `id`.
fn menu_entity(
    world: &mut World,
    id: MenuId,
) -> Option<(Entity, std::sync::Arc<slotted_model::MenuDef>)> {
    world
        .query::<(Entity, &OpenMenu)>()
        .iter(world)
        .find(|(_, menu)| menu.id == id)
        .map(|(entity, menu)| (entity, menu.def.clone()))
}

/// Validates and applies one control-stage command.
///
/// # Errors
///
/// [`ModError::WrongStage`], [`ModError::UnknownMenu`], [`ModError::BadCommand`].
pub fn apply_control_command(
    world: &mut World,
    mod_id: &ModId,
    command: &ScriptCommand,
) -> Result<Vec<MenuAction>, ModError> {
    if command.allowed_stage() == Some(slotted_script::Stage::Data) {
        return Err(ModError::WrongStage {
            mod_id: mod_id.clone(),
            command: command.name().to_owned(),
        });
    }
    let bad = |message: String| ModError::BadCommand {
        mod_id: mod_id.clone(),
        command: command.name().to_owned(),
        message,
    };

    match command {
        ScriptCommand::Log { level, message } => {
            let (level, message) = (*level, message.clone());
            log(world, Some(mod_id.clone()), level, message);
            Ok(Vec::new())
        }
        ScriptCommand::Deprecated { call, since, hint } => {
            let key = (mod_id.clone(), call.clone());
            let fresh = world
                .get_resource_mut::<WarnedDeprecations>()
                .is_none_or(|mut warned| warned.0.insert(key));
            if fresh {
                let message = format!("`{call}` is deprecated since api {since}: {hint}");
                log(world, Some(mod_id.clone()), LogLevel::Warn, message);
            }
            Ok(Vec::new())
        }
        ScriptCommand::SetHud { layer, value } => {
            // PHASE6-IMPL: B. Contract 2.1: `value` is a map with optional
            // `tree`, `anchor`, `offset`, `scale`, `visible`; an unknown layer
            // is created on top of `slotted_ui::HudLayers`.
            let _ = value;
            tracing::debug!(mod_id = %mod_id, layer, "set_hud is not implemented yet");
            Ok(Vec::new())
        }
        ScriptCommand::HudUpdate { layer, path, value } => {
            // PHASE6-IMPL: B. Map `Str -> Text`, `Bool -> Visible`, number or
            // `{value, max}` -> `Fill`, then write a `slotted_ui::HudUpdate`.
            let _ = (path, value);
            tracing::debug!(mod_id = %mod_id, layer, "hud_update is not implemented yet");
            Ok(Vec::new())
        }
        // A control script's reply to `TooltipBuild` is read where the tooltip
        // is composed, not here.
        ScriptCommand::AddTooltipPart { .. } | ScriptCommand::Subscribe { .. } => Ok(Vec::new()),
        ScriptCommand::Sort { menu, inventory } => {
            let (entity, def) = resolve_menu(world, mod_id, *menu)?;
            check_inventory(&def, *inventory, &bad)?;
            Ok(vec![MenuAction {
                entity,
                action: ClickAction::Toolbar(ToolbarAction::Sort {
                    inventory: InventoryRef::new(*inventory),
                }),
            }])
        }
        ScriptCommand::QuickStack { menu, from, to } => {
            let (entity, def) = resolve_menu(world, mod_id, *menu)?;
            check_inventory(&def, *from, &bad)?;
            check_inventory(&def, *to, &bad)?;
            Ok(vec![MenuAction {
                entity,
                action: ClickAction::Toolbar(ToolbarAction::QuickStack {
                    from: InventoryRef::new(*from),
                    to: InventoryRef::new(*to),
                }),
            }])
        }
        ScriptCommand::ToggleFavorite { menu, slot } => {
            let (entity, def) = resolve_menu(world, mod_id, *menu)?;
            check_slot(&def, *slot, &bad)?;
            Ok(vec![MenuAction {
                entity,
                action: ClickAction::Toolbar(ToolbarAction::ToggleFavorite {
                    slot: SlotIx(*slot),
                }),
            }])
        }
        ScriptCommand::Move { menu, from, to } => {
            let (entity, def) = resolve_menu(world, mod_id, *menu)?;
            check_slot(&def, *from, &bad)?;
            check_slot(&def, *to, &bad)?;
            // The model has no move action, so this is the transfer plan a
            // player would perform by hand: take, place, put the rest back.
            Ok([*from, *to, *from]
                .into_iter()
                .map(|slot| MenuAction {
                    entity,
                    action: ClickAction::Pickup {
                        slot: SlotIx(slot),
                        button: slotted_model::Button::Left,
                    },
                })
                .collect())
        }
        ScriptCommand::Click { menu, action } => {
            let (entity, _) = resolve_menu(world, mod_id, *menu)?;
            Ok(vec![MenuAction {
                entity,
                action: *action,
            }])
        }
        other => Err(ModError::WrongStage {
            mod_id: mod_id.clone(),
            command: other.name().to_owned(),
        }),
    }
}

/// The open menu a command names, or [`ModError::UnknownMenu`].
fn resolve_menu(
    world: &mut World,
    mod_id: &ModId,
    menu: MenuId,
) -> Result<(Entity, std::sync::Arc<slotted_model::MenuDef>), ModError> {
    menu_entity(world, menu).ok_or(ModError::UnknownMenu {
        mod_id: mod_id.clone(),
        menu,
    })
}

/// An inventory index the menu definition actually addresses.
fn check_inventory(
    def: &slotted_model::MenuDef,
    inventory: u16,
    bad: &impl Fn(String) -> ModError,
) -> Result<(), ModError> {
    let count = def
        .slots
        .iter()
        .map(|slot| slot.source.index() + 1)
        .max()
        .unwrap_or(0);
    if usize::from(inventory) < count {
        Ok(())
    } else {
        Err(bad(format!(
            "inventory {inventory} is out of range; the menu has {count}"
        )))
    }
}

/// A slot index inside the menu.
fn check_slot(
    def: &slotted_model::MenuDef,
    slot: u16,
    bad: &impl Fn(String) -> ModError,
) -> Result<(), ModError> {
    if usize::from(slot) < def.slots.len() {
        Ok(())
    } else {
        Err(bad(format!(
            "slot {slot} is out of range; the menu has {}",
            def.slots.len()
        )))
    }
}
