//! The slider (menus M1 contract 3.4): a track, a fill, a thumb and a
//! readout, bound to a `Float`.

use bevy::prelude::*;
use slotted_theme::{Themed, roles};

use crate::def::{BindDef, LocKey};
use crate::focus_ring::Focusable;
use crate::screen::SpawnCtx;
use crate::semantic::{SemanticLabel, SemanticRole, WidgetNode};
use crate::widgets::kinds;

/// The static part of a slider node.
#[derive(Debug, Clone, PartialEq)]
pub struct SliderDef {
    /// Range start.
    pub min: f64,
    /// Range end.
    pub max: f64,
    /// Step; `0` = one percent of the range.
    pub step: f64,
    /// Readout format.
    pub format: String,
}

/// The slider's state, for tests.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct SliderState {
    /// Current value.
    pub value: f64,
    /// Range start.
    pub min: f64,
    /// Range end.
    pub max: f64,
    /// Effective step.
    pub step: f64,
    /// A pointer drag is in progress.
    pub dragging: bool,
    /// Inert.
    pub disabled: bool,
}

/// Spawns a slider row.
pub fn spawn_slider(
    ctx: &mut SpawnCtx<'_>,
    label: Option<&LocKey>,
    def: SliderDef,
    bind: &BindDef,
) -> Entity {
    // M1-IMPL: B
    let entity = placeholder_row(
        ctx,
        roles::SLIDER,
        SemanticRole::Slider,
        kinds::slider(),
        label,
        false,
    );
    ctx.world.entity_mut(entity).insert(SliderState {
        value: def.min,
        min: def.min,
        max: def.max,
        step: def.step,
        dragging: false,
        disabled: bind.disabled,
    });
    entity
}

/// The skeleton's placeholder: a control-height row with the label, so the
/// tree lays out and locators find the node before the package fills it.
#[allow(dead_code)]
fn placeholder_row(
    ctx: &mut SpawnCtx<'_>,
    role: slotted_theme::Role,
    semantic: SemanticRole,
    kind: crate::def::WidgetKind,
    label: Option<&LocKey>,
    compact: bool,
) -> Entity {
    use bevy::input_focus::tab_navigation::TabIndex;
    let tokens = ctx.tokens();
    let height = if compact {
        tokens.sizes.control_height_compact
    } else {
        tokens.sizes.control_height
    };
    let entity = ctx.spawn_node((
        Node {
            height: Val::Px(height),
            min_height: Val::Px(height),
            padding: UiRect::axes(Val::Px(tokens.spacing.md), Val::Px(tokens.spacing.xs)),
            border: UiRect::all(Val::Px(crate::widgets::BORDER_WIDTH)),
            align_items: AlignItems::Center,
            column_gap: Val::Px(tokens.spacing.sm),
            flex_shrink: 0.0,
            ..default()
        },
        Themed(role),
        semantic,
        SemanticLabel(label.map(|k| k.0.clone()).unwrap_or_default()),
        WidgetNode(kind),
        Focusable,
        TabIndex(0),
        bevy::picking::hover::Hovered::default(),
        Pickable::default(),
    ));
    if let Some(label) = label {
        let parent = std::mem::replace(&mut ctx.parent, entity);
        crate::widgets::text::spawn_text(
            ctx,
            label,
            crate::def::TextRole::Label,
            &crate::def::TextOpts::default(),
        );
        ctx.parent = parent;
    }
    entity
}
