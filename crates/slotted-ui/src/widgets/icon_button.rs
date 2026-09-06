//! `icon_button`: a button cycling through named states. Phase 6 contract
//! section 1.4. Not a `bevy_ui_widgets::Button`: the state must change before
//! `Activate` fires so `WidgetActivate.tags["state"]` is the new state.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use slotted_model::PropertyId;

use crate::def::{IconButtonState as StateDef, Tags};
use crate::screen::SpawnCtx;
use crate::semantic::{SemanticRole, WidgetNode};

/// Icon button state on the root. `Tags["state"]` mirrors
/// `states[current].id`.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct IconButtonState {
    /// The states, in cycle order.
    pub states: Vec<StateDef>,
    /// Index into `states`.
    pub current: usize,
    /// Property mirroring `current`, when bound.
    pub property: Option<PropertyId>,
    /// The menu the property lives on.
    pub menu: Option<Entity>,
}

impl IconButtonState {
    /// The current state's def.
    pub fn state(&self) -> &StateDef {
        &self.states[self.current.min(self.states.len().saturating_sub(1))]
    }
}

/// Cycle an icon button. Targets the button root. Pointer and keyboard
/// observers trigger it; the harness's `cycle` does too. The observer then
/// triggers `bevy_ui_widgets::Activate` itself.
#[derive(EntityEvent, Debug, Clone, Copy, PartialEq, Eq)]
pub struct IconButtonCycle {
    /// The button root.
    pub entity: Entity,
    /// `true` advances, `false` (shift) goes back.
    pub forward: bool,
}

/// Parameters of `slotted:icon_button`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct IconButtonParams {
    /// At least one state.
    #[serde(default)]
    pub states: Vec<StateDef>,
    /// Bound property.
    #[serde(default)]
    pub property: Option<PropertyId>,
}

/// Spawns an icon button. Contract 1.4.
pub fn spawn_icon_button(ctx: &mut SpawnCtx<'_>, params: &IconButtonParams, tags: &Tags) -> Entity {
    // PHASE6-IMPL: A. Node SLOT_SIZE square, Themed(ICON_BUTTON), Button role,
    // SemanticLabel(state label), Tags with state=<id>, TabIndex(0), Hovered,
    // Pickable, TooltipSource, ImageNode child; observers Pointer<Click>,
    // FocusedInput<KeyboardInput>, On<IconButtonCycle>. Warn and spawn an
    // inert button when `states` is empty.
    let mut tags = tags.clone();
    if let Some(first) = params.states.first() {
        tags.0.insert("state".to_owned(), first.id.clone());
    }
    let menu = ctx.menu;
    ctx.spawn_node((
        Node::default(),
        SemanticRole::Button,
        WidgetNode(crate::widgets::kinds::icon_button()),
        IconButtonState {
            states: params.states.clone(),
            current: 0,
            property: params.property,
            menu,
        },
        tags,
    ))
}

/// Observer: advances or rewinds `current`, rewrites the `state` tag, the
/// semantic label and the icon, triggers `Activate`, and `SetProperty` when
/// bound.
pub fn on_icon_button_cycle(
    event: On<IconButtonCycle>,
    mut buttons: Query<(
        &mut IconButtonState,
        &mut Tags,
        &mut crate::semantic::SemanticLabel,
    )>,
    mut commands: Commands,
) {
    // PHASE6-IMPL: A.
    let _ = (&event, &mut buttons, &mut commands);
}

/// Observer on `slotted_ecs::PropertyChanged`: sets `current` from the value
/// of a bound property without re-triggering `Activate`.
pub fn on_icon_button_property(
    event: On<slotted_ecs::PropertyChanged>,
    mut buttons: Query<(&mut IconButtonState, &mut Tags)>,
) {
    // PHASE6-IMPL: A.
    let _ = (&event, &mut buttons);
}
