//! Stage two of a load: install what the freeze produced, with ownership.
//!
//! What this module guarantees:
//!
//! * every registry it writes to in `slotted-ui` is *reconciled*, not merely
//!   added to. Entries this pack set ships are installed under
//!   [`slotted_ui::Owner::Mod`]; entries the previous pack set shipped and
//!   this one does not are removed; entries the game registered in Rust
//!   ([`slotted_ui::Owner::Game`]) are never touched;
//! * a control script that fails to load leaves the mod running the script it
//!   already had, and the failure is reported rather than swallowed;
//! * nothing on screen is touched. Installing changes what a screen *would*
//!   spawn as; turning that into what is on screen is the invalidation
//!   stage's job, and the change set returned here is its input.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bevy::prelude::*;
#[cfg(feature = "ui")]
use slotted_model::Namespaced;
use slotted_registry::{AssetSource, FrozenRegistries};
use slotted_script::{API_VERSION, ModId, ScriptCommand, ScriptEvent, ScriptId, Stage};

#[cfg(feature = "ui")]
use slotted_ui::tooltip::TooltipParts;
#[cfg(feature = "ui")]
use slotted_ui::{ChangeSet, Injection, Injections, Owner, Screens, WidgetRegistry};

use crate::lifecycle::from_script_error;
use crate::lifecycle::{
    CollectedUi, ControlScript, ControlScripts, FrozenSnapshot, ModLoader, ScriptHost, StageChange,
    fail, keep_previous, log, script_mod_id,
};
#[cfg(feature = "ui")]
use crate::lifecycle::{PACKS_TOOLTIP_OWNER, PacksBakedIcons, send};
use crate::prepare::{read_script, remap_patch};
#[cfg(feature = "ui")]
use crate::tooltip::{ScriptTooltipPart, StaticPart, TemplateWidget};
#[cfg(feature = "ui")]
use crate::uidef::{AnchorId, ScreenKind, UiNodeDef, WidgetKind};
use crate::{ModEntry, ModError, ModWatch, PackLayout};

impl ModLoader {
    /// Replaces `slotted_ecs::Registries`, keeping the outgoing set so a
    /// reload can remap by name.
    pub(crate) fn install_registries(world: &mut World, frozen: Arc<FrozenRegistries>) {
        if let Some(previous) = world.get_resource::<slotted_ecs::Registries>() {
            let previous = previous.0.clone();
            world.insert_resource(FrozenSnapshot(previous));
        }
        world.insert_resource(slotted_ecs::Registries(frozen));
    }

    /// Contract 2.3 step 4: publish everything the frozen set feeds, then load
    /// the control scripts.
    #[cfg_attr(not(feature = "ui"), allow(clippy::unused_unit))]
    pub(crate) fn run_control_stage(
        world: &mut World,
        layout: &PackLayout,
        source: &dyn AssetSource,
        frozen: &Arc<FrozenRegistries>,
        #[cfg_attr(not(feature = "ui"), allow(unused_variables))] collected: &CollectedUi,
    ) -> StageChange {
        // Screens, widget templates, injections and tooltip parts, all of
        // which live in `slotted-ui`. A server build has none of them and
        // goes straight to the control scripts.
        #[cfg(feature = "ui")]
        let change = Self::publish_ui(world, frozen, collected);

        let dynamic = Self::load_control_scripts(world, layout, source, frozen);

        #[cfg(feature = "ui")]
        Self::publish_tooltip_part(world, collected, dynamic);
        #[cfg(not(feature = "ui"))]
        let _ = dynamic;

        crate::locale::load_locales(world, layout, source);
        populate_watch(world, layout, source);
        #[cfg(feature = "ui")]
        rebake_icons(world, frozen);

        // A recipe type a mod just registered has no category, and
        // `RecipeStore` silently drops every recipe of an unbound type. The
        // browser binds defaults in its own `Startup`, which is too early for a
        // harness that loads mods after `build` and never runs again for a
        // reload, so the freeze does it here too. `bind_defaults` skips types a
        // category already claims, so running it twice is free.
        #[cfg(feature = "ui")]
        {
            if let Some(mut categories) = world.get_resource_mut::<slotted_browser::Categories>() {
                categories.bind_defaults(frozen);
            }
            send(world, slotted_browser::RebuildBrowser);
        }

        #[cfg(feature = "ui")]
        return change;
    }

    /// Publishes everything the frozen set feeds into `slotted-ui`: screens,
    /// fluids, HUD layers, widget templates and injections.
    ///
    /// Every registry it writes to is shared with the game, so every write is
    /// a *reconcile* of exactly the mod-owned entries: what this load ships is
    /// installed, what the previous load shipped and this one does not is
    /// removed, and anything the game registered in Rust is left where it is.
    /// The returned [`ChangeSet`] is what those three diffs add up to, and
    /// [`invalidate_screens`] turns it into the screens that have to respawn.
    #[cfg(feature = "ui")]
    pub(crate) fn publish_ui(
        world: &mut World,
        frozen: &Arc<FrozenRegistries>,
        collected: &CollectedUi,
    ) -> ChangeSet {
        world.init_resource::<Screens>();
        world.init_resource::<Injections>();
        world.init_resource::<WidgetRegistry>();
        world.init_resource::<TooltipParts>();

        // Screens come straight from the frozen registry payloads. Phase 6
        // adds fluids and HUD layers the same way (contract 1.1, 2.1).
        let screens = world.resource_mut::<Screens>().load_from_registry(frozen);
        if let Some(mut fluids) = world.get_resource_mut::<slotted_ui::Fluids>() {
            fluids.load_from_registry(frozen);
        }
        if let Some(mut hud) = world.get_resource_mut::<slotted_ui::HudLayers>() {
            hud.load_from_registry(frozen);
        }

        // Widget templates: one `TemplateWidget` per registry entry, owned by
        // the mod whose namespace the kind is under.
        let mut widgets: Vec<(WidgetKind, Arc<dyn slotted_ui::Widget>, Owner)> = Vec::new();
        for (_, name, def) in frozen.widgets.iter() {
            match def.payload.clone().into_rust::<UiNodeDef>() {
                Ok(template) => widgets.push((
                    WidgetKind(name.clone()),
                    Arc::new(TemplateWidget {
                        kind: WidgetKind(name.clone()),
                        template,
                    }),
                    Owner::Mod(name.namespace().to_owned()),
                )),
                Err(error) => tracing::warn!(%name, %error, "widget payload is not a UiNodeDef"),
            }
        }
        let templates = world
            .resource_mut::<WidgetRegistry>()
            .reconcile_mods(widgets);

        let mut injections = Vec::new();
        for (mod_id, command) in &collected.injections {
            match to_injection(mod_id, command) {
                Ok(injection) => injections.push(injection),
                Err(error) => fail(world, Some(mod_id.clone()), error),
            }
        }
        let injections = slotted_ui::reconcile_mod_injections(
            &mut world.resource_mut::<Injections>(),
            injections,
        );

        ChangeSet {
            screens: screens.changed.into_iter().collect(),
            removed_screens: screens.removed.into_iter().collect(),
            templates: templates
                .changed
                .into_iter()
                .chain(templates.removed)
                .collect(),
            injections_added: injections.changed,
            injections_removed: injections.removed,
        }
    }

    /// Replaces packs' one `TooltipPart` with a fresh one over the statics and
    /// the control scripts that just subscribed to `tooltip_build`.
    #[cfg(feature = "ui")]
    pub(crate) fn publish_tooltip_part(
        world: &mut World,
        collected: &CollectedUi,
        dynamic: Vec<(ModId, ScriptId)>,
    ) {
        let mut statics = Vec::new();
        for (mod_id, command) in &collected.tooltip_parts {
            match to_static_part(mod_id, command) {
                Ok(part) => statics.push(part),
                Err(error) => fail(world, Some(mod_id.clone()), error),
            }
        }
        let host = world.get_resource::<ScriptHost>().cloned();
        let part = ScriptTooltipPart {
            host,
            statics,
            dynamic,
        };
        // Packs have exactly one part, and it keeps its position across a
        // reload: `TooltipParts` is ordered, and a part that moved to the end
        // on every reload would change what a tooltip reads.
        world
            .resource_mut::<TooltipParts>()
            .set_mod_part(Arc::new(part), Owner::Mod(PACKS_TOOLTIP_OWNER.to_owned()));
    }

    /// Loads every control script again, returning the `(mod, script)` pairs
    /// subscribed to `tooltip_build`.
    ///
    /// A mod whose new chunk will not compile, will not read or throws out of
    /// `control_start` **keeps the script it already had**, and the previous
    /// chunk is only unloaded once its replacement is running. The failure is
    /// still reported. The alternative, which this used to do, was to unload
    /// everything up front and leave that mod with no control script at all:
    /// in the web playground a single typo then stopped every click in the
    /// game until the page was reloaded, with nothing on screen to say why.
    pub(crate) fn load_control_scripts(
        world: &mut World,
        layout: &PackLayout,
        source: &dyn AssetSource,
        frozen: &Arc<FrozenRegistries>,
    ) -> Vec<(ModId, ScriptId)> {
        let _ = frozen;
        let host = world.get_resource::<ScriptHost>().cloned();
        let previous = world
            .get_resource::<ControlScripts>()
            .cloned()
            .unwrap_or_default();

        let mods: Vec<String> = layout
            .mods
            .load_order()
            .iter()
            .map(ToString::to_string)
            .collect();
        let mut scripts = ControlScripts::default();
        let mut dynamic = Vec::new();
        let entries: Vec<ModEntry> = layout.mods.iter().cloned().collect();
        for entry in &entries {
            let Some(relative) = entry.manifest.entry.control.clone() else {
                continue;
            };
            let mod_id = script_mod_id(entry.id());
            let Some(host) = host.as_ref() else {
                fail(world, Some(mod_id), ModError::NoRuntime);
                continue;
            };
            let text = match read_script(source, entry, &relative) {
                Ok(text) => text,
                Err(error) => {
                    fail(world, Some(mod_id.clone()), error);
                    keep_previous(&previous, &mut scripts, &mut dynamic, &mod_id);
                    continue;
                }
            };
            let mut runtime = host.lock();
            let script = match runtime.load(&mod_id, &relative, &text, Stage::Control) {
                Ok(script) => script,
                Err(source) => {
                    let error = from_script_error(&mod_id, source);
                    drop(runtime);
                    fail(world, Some(mod_id.clone()), error);
                    keep_previous(&previous, &mut scripts, &mut dynamic, &mod_id);
                    continue;
                }
            };
            let commands = runtime.call(
                script,
                &ScriptEvent::ControlStart {
                    api_version: API_VERSION,
                    mods: mods.clone(),
                },
            );
            drop(runtime);
            let commands = match commands {
                Ok(commands) => commands,
                Err(source) => {
                    let error = from_script_error(&mod_id, source);
                    fail(world, Some(mod_id.clone()), error);
                    host.lock().unload(script);
                    keep_previous(&previous, &mut scripts, &mut dynamic, &mod_id);
                    continue;
                }
            };
            let mut events = BTreeSet::new();
            for command in &commands {
                match command {
                    ScriptCommand::Subscribe { events: names } => {
                        events.extend(names.iter().cloned());
                    }
                    ScriptCommand::Log { level, message } => {
                        let (level, message) = (*level, message.clone());
                        log(world, Some(mod_id.clone()), level, message);
                    }
                    other => tracing::debug!(
                        mod_id = %mod_id,
                        command = other.name(),
                        "command at control_start is ignored"
                    ),
                }
            }
            if events.contains(crate::route::TOOLTIP_BUILD) {
                dynamic.push((mod_id.clone(), script));
            }
            scripts
                .by_mod
                .insert(mod_id, ControlScript { id: script, events });
        }

        // Only now, and only what was really replaced: a mod that kept its
        // previous chunk kept the handle with it, and unloading that handle
        // would leave the table pointing at a script the runtime has freed.
        if let Some(host) = host.as_ref() {
            let mut runtime = host.lock();
            for (mod_id, script) in &previous.by_mod {
                if scripts.by_mod.get(mod_id).map(|kept| kept.id) != Some(script.id) {
                    runtime.unload(script.id);
                }
            }
        }
        world.insert_resource(scripts);
        dynamic
    }
}

/// An `Inject` command as a `slotted-ui` injection.
#[cfg(feature = "ui")]
pub(crate) fn to_injection(mod_id: &ModId, command: &ScriptCommand) -> Result<Injection, ModError> {
    let ScriptCommand::Inject {
        screen,
        anchor,
        node,
        exclusion,
    } = command
    else {
        unreachable!("only Inject commands are collected as injections");
    };
    let bad = |message: String| ModError::BadCommand {
        mod_id: ModId::new("packs").expect("a literal id"),
        command: "inject".to_owned(),
        message,
    };
    let target = ScreenKind(Namespaced::parse(screen).map_err(|e| bad(e.to_string()))?);
    let node: UiNodeDef =
        slotted_model::from_value(node.clone()).map_err(|e| bad(e.to_string()))?;
    Ok(Injection {
        target,
        anchor: AnchorId::new(anchor.clone()),
        node,
        exclusion: *exclusion,
        owner: Owner::Mod(mod_id.as_str().to_owned()),
    })
}

/// An `AddTooltipPart` command as a static part.
#[cfg(feature = "ui")]
pub(crate) fn to_static_part(
    mod_id: &ModId,
    command: &ScriptCommand,
) -> Result<StaticPart, ModError> {
    let ScriptCommand::AddTooltipPart {
        id,
        when,
        tier,
        nodes,
    } = command
    else {
        unreachable!("only AddTooltipPart commands are collected as tooltip parts");
    };
    let mut parsed = Vec::with_capacity(nodes.len());
    for node in nodes {
        parsed.push(
            slotted_model::from_value::<UiNodeDef>(node.0.clone()).map_err(|e| {
                ModError::BadCommand {
                    mod_id: mod_id.clone(),
                    command: "add_tooltip_part".to_owned(),
                    message: e.to_string(),
                }
            })?,
        );
    }
    Ok(StaticPart {
        mod_id: mod_id.clone(),
        id: id.clone(),
        when: when.clone(),
        tier: *tier,
        nodes: parsed,
    })
}

/// Loads every file a mod ships as a Bevy asset, purely so the file watcher
/// reports changes to it (contract 2.6).
///
/// A mod's scripts are watched when they live at `scripts/<mod id>/`, the one
/// layout where a logical `pack://` path names exactly one mod's file.
pub(crate) fn populate_watch(world: &mut World, layout: &PackLayout, source: &dyn AssetSource) {
    let Some(server) = world.get_resource::<AssetServer>().cloned() else {
        return;
    };
    let mut watch = ModWatch::default();
    for entry in layout.mods.iter() {
        let mod_id = script_mod_id(entry.id());
        let handles = watch.by_mod.entry(mod_id).or_default();
        for relative in entry
            .manifest
            .entry
            .data
            .iter()
            .chain(entry.manifest.entry.control.iter())
        {
            let logical = format!("scripts/{}/{relative}", entry.id());
            if source.read(&logical).is_ok() {
                handles.push(
                    server
                        .load::<crate::ScriptAsset>(format!("{}://{logical}", crate::PACK_SOURCE))
                        .untyped(),
                );
            }
        }
        for kind in slotted_registry::RegistryKind::ALL {
            let dir = format!("data/{}/{}", entry.id(), kind.dir());
            for path in source.list(&dir).unwrap_or_default() {
                handles.push(
                    server
                        .load::<crate::DataFile>(format!("{}://{path}", crate::PACK_SOURCE))
                        .untyped(),
                );
            }
        }
        let locale = format!(
            "locale/{}.ftl",
            world
                .get_resource::<crate::Locales>()
                .map_or_else(|| unic_langid::langid!("en-US"), |l| l.lang.clone())
        );
        if source.read(&locale).is_ok() {
            handles.push(
                server
                    .load::<crate::FtlAsset>(format!("{}://{locale}", crate::PACK_SOURCE))
                    .untyped(),
            );
        }
    }
    world.insert_resource(watch);
}

/// Re-bakes the placeholder atlas, but only over an `Icons` packs itself put
/// there: a game that supplied its own icon source keeps it.
#[cfg(feature = "ui")]
pub(crate) fn rebake_icons(world: &mut World, frozen: &FrozenRegistries) {
    let ours = world.contains_resource::<PacksBakedIcons>();
    if world.contains_resource::<slotted_icons::Icons>() && !ours {
        return;
    }
    if !world.contains_resource::<Assets<Image>>()
        || !world.contains_resource::<Assets<bevy::image::TextureAtlasLayout>>()
    {
        return;
    }
    // Shape-aware: a mod that declared `icon = { shape = "cube", .. }` gets
    // the same lit primitive the base game's items get.
    let items: Vec<slotted_icons::IconItem<'_>> = frozen
        .items
        .iter()
        .map(|(id, name, def)| slotted_icons::IconItem {
            id,
            name,
            icon: def.icon.as_ref(),
        })
        .collect();
    let baked = slotted_icons::bake_icon_atlas(&items, slotted_icons::CELL);
    let images = baked
        .images
        .iter()
        .filter_map(|(id, path)| {
            world
                .get_resource::<bevy::asset::AssetServer>()
                .map(|server| (*id, server.load::<Image>(path.clone())))
        })
        .collect();
    let image = world.resource_mut::<Assets<Image>>().add(baked.image);
    let layout = world
        .resource_mut::<Assets<bevy::image::TextureAtlasLayout>>()
        .add(baked.layout);
    world.insert_resource(slotted_icons::Icons::new(slotted_icons::AtlasIcons {
        image,
        layout,
        index: baked.index,
        images,
        missing: baked.missing,
    }));
    world.insert_resource(PacksBakedIcons);
    // Tell `slotted-icons` this atlas is a baked one, not a game's own
    // source, so a later change to `Registries` rebakes over it. Without
    // this, a game that installs its own registries after the pack load
    // keeps an atlas indexed by somebody else's item ids.
    world.insert_resource(slotted_icons::BakedByPlugin);
}

pub(crate) fn remap_inventories(
    world: &mut World,
    old: &FrozenRegistries,
    new: &FrozenRegistries,
) -> Vec<ModError> {
    let mut vanished: BTreeMap<String, u32> = BTreeMap::new();
    let mut dropped: BTreeSet<(String, String)> = BTreeSet::new();
    let mut remap = |stack: &mut Option<slotted_model::ItemStack>| {
        let Some(current) = stack.as_ref() else {
            return false;
        };
        let Some(name) = old.items.name_of(current.id) else {
            let entry = vanished.entry(format!("{:?}", current.id)).or_default();
            *entry += current.count;
            *stack = None;
            return true;
        };
        let mut changed = match new.items.id_of(name) {
            Some(id) if id == current.id => false,
            Some(id) => {
                if let Some(stack) = stack.as_mut() {
                    stack.id = id;
                }
                true
            }
            None => {
                *vanished.entry(name.to_string()).or_default() += current.count;
                *stack = None;
                return true;
            }
        };
        if let Some(stack) = stack.as_mut() {
            changed |= remap_patch(old, new, name, &mut stack.patch, &mut dropped);
        }
        changed
    };

    let entities: Vec<Entity> = world
        .query_filtered::<Entity, With<slotted_ecs::Inventory>>()
        .iter(world)
        .collect();
    for entity in entities {
        let Some(mut inventory) = world.get_mut::<slotted_ecs::Inventory>(entity) else {
            continue;
        };
        for index in 0..inventory.0.len() {
            let mut slot = inventory.0.get(index).cloned();
            if remap(&mut slot) {
                inventory.0.set(index, slot);
            }
        }
    }

    let carried: Vec<Entity> = world
        .query_filtered::<Entity, With<slotted_ecs::Carried>>()
        .iter(world)
        .collect();
    for entity in carried {
        let Some(mut held) = world.get_mut::<slotted_ecs::Carried>(entity) else {
            continue;
        };
        let mut slot = held.0.clone();
        if remap(&mut slot) {
            held.0 = slot;
        }
    }

    // Ghost and filter hints live on the menu state, not in an inventory, and
    // they are stacks like any other: a hint that still points at the old
    // dense id would draw the wrong item after a reload.
    let open: Vec<Entity> = world
        .query_filtered::<Entity, With<slotted_ecs::OpenMenu>>()
        .iter(world)
        .collect();
    for entity in open {
        let Some(mut menu) = world.get_mut::<slotted_ecs::OpenMenu>(entity) else {
            continue;
        };
        let slots: Vec<slotted_model::SlotIx> = menu.state.hints.keys().copied().collect();
        for slot in slots {
            let mut hint = menu.state.hint(slot).cloned();
            if remap(&mut hint) {
                menu.state.set_hint(slot, hint);
            }
        }
        let mut carried = menu.state.carried.clone();
        if remap(&mut carried) {
            menu.state.carried = carried;
        }
    }

    vanished
        .into_iter()
        .map(|(item, count)| ModError::ItemVanished { item, count })
        .chain(
            dropped
                .into_iter()
                .map(|(component, item)| ModError::ComponentVanished { component, item }),
        )
        .collect()
}
