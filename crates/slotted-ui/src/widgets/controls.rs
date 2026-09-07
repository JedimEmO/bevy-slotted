//! The `slotted:*` registry entries of every M1 control, so a `custom` node
//! or a mod's `register_widget` template can spawn them with the same
//! params as the typed variants.

use bevy::input_focus::tab_navigation::TabIndex;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use slotted_registry::Value;
use slotted_theme::{Role, Themed, roles};

use crate::def::{LocKey, UiNodeDef};
use crate::focus_ring::Focusable;
use crate::screen::{SpawnCtx, Widget, WidgetRegistry};
use crate::semantic::{LocText, SemanticLabel, SemanticRole, WidgetNode};
use crate::widgets::kinds;

// ---------------------------------------------------------------------------
// Shared control plumbing (menus M1 contract 0)
// ---------------------------------------------------------------------------

/// What a control looks like this frame, for [`state_role`]. Four flags
/// rather than an enum because they are four independent facts the paint
/// systems read off four different sources; the precedence lives in one
/// place, [`state_role_with`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct ControlLook {
    /// The pointer is over it or one of its children.
    pub hovered: bool,
    /// It holds `InputFocus`.
    pub focused: bool,
    /// Pressed, capturing, open: the control's own busy state.
    pub active: bool,
    /// Inert.
    pub disabled: bool,
}

/// `<base>.<state>` for the strongest state set, or `base` at rest.
/// Disabled wins, then active, focus, hover. A theme that lacks the dotted
/// role falls back to `base` through `Theme::material`.
pub fn state_role(base: &str, look: ControlLook) -> Role {
    state_role_with(base, look, "active")
}

/// [`state_role`] with the active suffix named (`button.pressed`,
/// `key_binding.capturing`).
pub fn state_role_with(base: &str, look: ControlLook, active: &str) -> Role {
    let suffix = if look.disabled {
        "disabled"
    } else if look.active {
        active
    } else if look.focused {
        "focus"
    } else if look.hovered {
        "hover"
    } else {
        return Role::new(base);
    };
    Role::new(format!("{base}.{suffix}"))
}

/// Spawns a control's label under `parent`: a `LocText` in the
/// `control.label` role, no semantic role of its own (the control's
/// `SemanticLabel` already names it, and a label child would double every
/// control in the tree snapshots).
pub fn spawn_control_label(world: &mut World, parent: Entity, key: &LocKey) -> Entity {
    world
        .spawn((
            Node {
                flex_shrink: 0.0,
                ..default()
            },
            Text::new(key.0.clone()),
            Themed(roles::CONTROL_LABEL),
            LocText::new(key.clone()),
            Pickable::IGNORE,
            ChildOf(parent),
        ))
        .id()
}

/// Spawns a control's row: `control_height` tall, a flex row with the
/// theme's gap, `Focusable`, `TabIndex(0)`, hoverable, and the semantic
/// bundle. The caller adds the state component and the children. The row
/// itself is not themed: the switch, the track, the pill carry the roles.
pub fn spawn_control_row(
    ctx: &mut SpawnCtx<'_>,
    semantic: SemanticRole,
    kind: crate::def::WidgetKind,
    label: Option<&LocKey>,
    disabled: bool,
) -> Entity {
    let tokens = ctx.tokens();
    let height = tokens.sizes.control_height;
    let entity = ctx.spawn_node((
        Node {
            height: Val::Px(height),
            min_height: Val::Px(height),
            padding: UiRect::axes(Val::Px(tokens.spacing.sm), Val::ZERO),
            align_items: AlignItems::Center,
            column_gap: Val::Px(tokens.spacing.md),
            flex_shrink: 0.0,
            ..default()
        },
        semantic,
        SemanticLabel(label.map(|k| k.0.clone()).unwrap_or_default()),
        WidgetNode(kind),
        Focusable,
        TabIndex(0),
        // In the directional graph, so a d-pad walks from row to row; a row
        // that consumes a direction claims it before nav sees it.
        bevy::ui::auto_directional_navigation::AutoDirectionalNavigation::default(),
        Hovered::default(),
        Pickable::default(),
    ));
    if disabled {
        ctx.world
            .entity_mut(entity)
            .insert(bevy::ui::InteractionDisabled);
    }
    if let Some(label) = label {
        spawn_control_label(ctx.world, entity, label);
    }
    entity
}

/// Spawns a `flex_grow: 1` gap that pushes what follows to the row's end.
pub fn spawn_control_spacer(world: &mut World, parent: Entity) -> Entity {
    world
        .spawn((
            Node {
                flex_grow: 1.0,
                ..default()
            },
            Pickable::IGNORE,
            ChildOf(parent),
        ))
        .id()
}

/// Where `position` (logical window px) falls along `node`'s width, in
/// `0..=1`. The node's rect comes from the last layout pass.
pub fn fraction_along(position: Vec2, node: &ComputedNode, transform: &UiGlobalTransform) -> f32 {
    let size = node.size() * node.inverse_scale_factor();
    let center = transform.translation * node.inverse_scale_factor();
    if size.x <= f32::EPSILON {
        return 0.0;
    }
    ((position.x - (center.x - size.x * 0.5)) / size.x).clamp(0.0, 1.0)
}

/// Every M1 control through the registry: the params are the typed variant's
/// fields, re-read as that variant with `type` filled in.
#[derive(Debug, Clone, Copy)]
struct VariantWidget(&'static str);

impl Widget for VariantWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, params: &Value, children: &[UiNodeDef]) -> Entity {
        // Re-tag the params map as the typed variant and spawn that. The
        // children go through RON text, which every `UiNodeDef` round-trips.
        // A params payload that is not a map, or does not type, logs and
        // spawns an invisible panel, like every other malformed params.
        let mut map = match params {
            Value::Map(m) => m.clone(),
            _ => ron::Map::new(),
        };
        map.insert(
            Value::String("type".to_owned()),
            Value::String(self.0.to_owned()),
        );
        if !children.is_empty() {
            let kids: Vec<Value> = children
                .iter()
                .filter_map(|c| ron::to_string(c).ok())
                .filter_map(|text| ron::from_str::<Value>(&text).ok())
                .collect();
            map.insert(Value::String("children".to_owned()), Value::Seq(kids));
        }
        let typed = slotted_registry::to_model(&Value::Map(map))
            .map_err(|e| e.to_string())
            .and_then(|v| slotted_model::from_value::<UiNodeDef>(v).map_err(|e| e.to_string()));
        match typed {
            Ok(def) => ctx.spawn_child(&def),
            Err(e) => {
                tracing::warn!(kind = self.0, error = %e, "malformed control params");
                crate::widgets::spawn_panel(
                    ctx,
                    &slotted_theme::Role::new_static("invisible"),
                    &crate::def::Layout::default(),
                    &[],
                )
            }
        }
    }
}

/// Registers every M1 control kind.
pub fn register(registry: &mut WidgetRegistry) {
    registry.register(kinds::rich_text(), VariantWidget("rich_text"));
    registry.register(kinds::toggle(), VariantWidget("toggle"));
    registry.register(kinds::slider(), VariantWidget("slider"));
    registry.register(kinds::select(), VariantWidget("select"));
    registry.register(kinds::radio_group(), VariantWidget("radio_group"));
    registry.register(kinds::key_binding(), VariantWidget("key_binding"));
    registry.register(kinds::text_field(), VariantWidget("text_field"));
    registry.register(kinds::list(), VariantWidget("list"));
    registry.register(kinds::scroll(), VariantWidget("scroll"));
    registry.register(kinds::tabs(), VariantWidget("tabs"));
    registry.register(kinds::separator(), VariantWidget("separator"));
    registry.register(kinds::spacer(), VariantWidget("spacer"));
    registry.register(kinds::image(), VariantWidget("image"));
    registry.register(kinds::close(), CloseWidget);
}

/// `slotted:close`: a button whose `Activate` pops the screen stack (menus
/// M1 contract 3.2).
#[derive(Debug, Default, Clone, Copy)]
pub struct CloseWidget;

impl Widget for CloseWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, params: &Value, _children: &[UiNodeDef]) -> Entity {
        let opts: crate::def::ButtonOpts = slotted_registry::to_model(params)
            .ok()
            .and_then(|v| slotted_model::from_value(v).ok())
            .unwrap_or_default();
        // `spawn_button` attaches the pop observer for this kind itself.
        crate::widgets::spawn_button(ctx, Some(&kinds::close()), &opts)
    }
}

/// Observer on a `slotted:close` button: `Activate` pops the screen stack.
pub fn on_close_activate(_activate: On<bevy::ui_widgets::Activate>, mut commands: Commands) {
    crate::stack::pop_screen(&mut commands);
}

/// Registers the value controls' observers and paint systems (package B).
/// The paint systems run in `Render` after [`crate::values::ValueSync`], so
/// a value that arrived this frame is painted this frame.
pub fn build(app: &mut App) {
    use crate::widgets::{key_binding, radio_group, select, slider, toggle};
    app.add_observer(crate::widgets::on_button_accept)
        .add_observer(toggle::on_toggle_value)
        .add_observer(slider::on_slider_value)
        .add_observer(select::on_select_value)
        .add_observer(select::on_open_select_popup)
        .add_observer(select::on_close_select_popup)
        .add_observer(radio_group::on_radio_value)
        .add_systems(
            Update,
            (
                crate::widgets::button_roles,
                toggle::paint_toggles,
                slider::paint_sliders,
                select::paint_selects,
                radio_group::paint_radio_groups,
                key_binding::paint_key_bindings,
            )
                .after(crate::values::ValueSync)
                .in_set(crate::plugin::SlottedUiSet::Render),
        );
}
