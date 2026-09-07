//! The slider (menus M1 contract 3.4): a track, a fill, a thumb and a
//! readout, bound to a `Float`.
//!
//! `Left`/`Right` step (repeats come from `emit_ui_actions`), `PagePrev`/
//! `PageNext` jump a tenth of the range, a press on the track jumps to the
//! pointer and a drag scrubs. Every one of those is a `SetValue` with the
//! slider as `source`; the thumb moves only when the value comes back.

use bevy::picking::events::{Drag, DragEnd, Pointer, Press, Release};
use bevy::picking::hover::Hovered;
use bevy::picking::pointer::PointerButton;
use bevy::prelude::*;
use slotted_theme::{Themed, roles};

use crate::actions::UiAction;
use crate::def::{BindDef, LocKey};
use crate::nav::FocusedAction;
use crate::screen::SpawnCtx;
use crate::semantic::SemanticRole;
use crate::values::{BoundValue, Value, ValueBinding, ValueWriter};
use crate::widgets::controls::{self, ControlLook};
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

impl SliderState {
    /// Where `value` sits in the range, `0..=1`.
    pub fn fraction(&self) -> f64 {
        let span = self.max - self.min;
        if span.abs() <= f64::EPSILON {
            return 0.0;
        }
        ((self.value - self.min) / span).clamp(0.0, 1.0)
    }

    /// The value at `fraction` of the range, snapped to `step`.
    pub fn value_at(&self, fraction: f64) -> f64 {
        let raw = self.min + (self.max - self.min) * fraction.clamp(0.0, 1.0);
        self.snap(raw)
    }

    /// `raw` snapped to the step grid from `min` and clamped to the range.
    pub fn snap(&self, raw: f64) -> f64 {
        let snapped = if self.step > 0.0 {
            self.min + ((raw - self.min) / self.step).round() * self.step
        } else {
            raw
        };
        let (lo, hi) = if self.min <= self.max {
            (self.min, self.max)
        } else {
            (self.max, self.min)
        };
        snapped.clamp(lo, hi)
    }
}

/// The slider's painted children, on the row.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SliderParts {
    /// The track.
    pub track: Entity,
    /// The fill from the track's start to the thumb.
    pub fill: Entity,
    /// The thumb.
    pub thumb: Entity,
    /// The readout text.
    pub readout: Entity,
}

/// The readout's format string, on the row.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct SliderFormat(pub String);

/// Spawns a slider row.
#[allow(clippy::too_many_lines)]
pub fn spawn_slider(
    ctx: &mut SpawnCtx<'_>,
    label: Option<&LocKey>,
    def: SliderDef,
    bind: &BindDef,
) -> Entity {
    let tokens = ctx.tokens();
    let entity = controls::spawn_control_row(
        ctx,
        SemanticRole::Slider,
        kinds::slider(),
        label,
        bind.disabled,
    );
    let step = if def.step > 0.0 {
        def.step
    } else {
        (def.max - def.min).abs() * 0.01
    };
    let state = SliderState {
        value: def.min,
        min: def.min,
        max: def.max,
        step,
        dragging: false,
        disabled: bind.disabled,
    };
    let thumb_size = tokens.spacing.lg;
    let track_height = tokens.spacing.sm + 2.0;
    let track = ctx
        .world
        .spawn((
            Node {
                flex_grow: 1.0,
                height: Val::Px(track_height),
                min_width: Val::Px(thumb_size * 3.0),
                position_type: PositionType::Relative,
                // Room for the thumb to overhang both ends.
                margin: UiRect::horizontal(Val::Px(thumb_size * 0.5)),
                ..default()
            },
            Themed(roles::SLIDER),
            Pickable::default(),
            ChildOf(entity),
        ))
        .id();
    let fill = ctx
        .world
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::ZERO,
                top: Val::ZERO,
                bottom: Val::ZERO,
                width: Val::Percent(0.0),
                ..default()
            },
            Themed(roles::SLIDER_FILL),
            Pickable::IGNORE,
            ChildOf(track),
        ))
        .id();
    let thumb = ctx
        .world
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(0.0),
                top: Val::Percent(50.0),
                width: Val::Px(thumb_size),
                height: Val::Px(thumb_size),
                border: UiRect::all(Val::Px(crate::widgets::BORDER_WIDTH)),
                ..default()
            },
            UiTransform {
                translation: Val2::new(Val::Percent(-50.0), Val::Percent(-50.0)),
                ..UiTransform::IDENTITY
            },
            Themed(roles::SLIDER_THUMB),
            Pickable::default(),
            ChildOf(track),
        ))
        .id();
    let readout = ctx
        .world
        .spawn((
            Node {
                min_width: Val::Px(tokens.spacing.xl * 1.5),
                justify_content: JustifyContent::FlexEnd,
                flex_shrink: 0.0,
                ..default()
            },
            Text::new(format_readout(&def.format, &state)),
            TextLayout::justify(bevy::text::Justify::Right),
            Themed(roles::SLIDER_TEXT),
            Pickable::IGNORE,
            ChildOf(entity),
        ))
        .id();
    let mut row = ctx.world.entity_mut(entity);
    row.insert((
        state,
        SliderParts {
            track,
            fill,
            thumb,
            readout,
        },
        SliderFormat(def.format),
    ));
    if let Some(binding) = ValueBinding::from_def(bind, ctx.menu) {
        row.insert(binding);
    }
    row.observe(on_slider_action);
    row.observe(on_slider_press);
    row.observe(on_slider_drag);
    row.observe(on_slider_release);
    row.observe(on_slider_drag_end);
    entity
}

/// A number for the readout: no decimals when whole, else up to two with
/// the trailing zeros dropped.
fn format_number(v: f64, precision: Option<usize>) -> String {
    if let Some(p) = precision {
        return format!("{v:.p$}");
    }
    if (v - v.round()).abs() < 1e-9 {
        return format!("{:.0}", v.round());
    }
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// `format` with `{value}`, `{min}`, `{max}` (each optionally `{name:.N}`)
/// substituted. Anything else is copied through.
pub fn format_readout(format: &str, state: &SliderState) -> String {
    let mut out = String::with_capacity(format.len());
    let mut rest = format;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            out.push_str(&rest[open..]);
            return out;
        };
        let spec = &after[..close];
        let (name, precision) = match spec.split_once(":.") {
            Some((name, p)) => (name, p.parse::<usize>().ok()),
            None => (spec, None),
        };
        let value = match name {
            "value" => Some(state.value),
            "min" => Some(state.min),
            "max" => Some(state.max),
            _ => None,
        };
        if let Some(v) = value {
            out.push_str(&format_number(v, precision));
        } else {
            out.push('{');
            out.push_str(spec);
            out.push('}');
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

/// Asks the binding for `value`, or takes it directly when unbound.
fn propose(
    entity: Entity,
    state: &mut SliderState,
    binding: Option<&ValueBinding>,
    value: f64,
    writer: &mut ValueWriter,
) {
    let value = state.snap(value);
    match binding {
        Some(binding) => writer.write(entity, binding, Value::Float(value)),
        None => state.value = value,
    }
}

/// Observer: `Left`/`Right` step, `PagePrev`/`PageNext` jump a tenth; all
/// four are claimed so focus stays put. Repeats count.
pub fn on_slider_action(
    action: On<FocusedAction>,
    mut rows: Query<(&mut SliderState, Option<&ValueBinding>)>,
    mut claims: ResMut<crate::actions::UiActionClaims>,
    mut writer: ValueWriter,
) {
    let Ok((mut state, binding)) = rows.get_mut(action.entity) else {
        return;
    };
    if state.disabled {
        return;
    }
    let span = state.max - state.min;
    let delta = match action.action {
        UiAction::Left => -state.step,
        UiAction::Right => state.step,
        UiAction::PagePrev => -span * 0.1,
        UiAction::PageNext => span * 0.1,
        _ => return,
    };
    claims.claim(action.action);
    let next = state.value + delta;
    propose(action.entity, &mut state, binding, next, &mut writer);
}

/// Where the pointer is along the track, when it pressed the track or one
/// of its children.
fn track_fraction(
    target: Entity,
    position: Vec2,
    parts: &SliderParts,
    parents: &Query<&ChildOf>,
    nodes: &Query<(&ComputedNode, &UiGlobalTransform)>,
) -> Option<f32> {
    let mut current = target;
    loop {
        if current == parts.track {
            break;
        }
        current = parents.get(current).ok()?.parent();
    }
    let (node, transform) = nodes.get(parts.track).ok()?;
    Some(controls::fraction_along(position, node, transform))
}

/// Observer: a primary press on the track jumps there and starts a scrub.
pub fn on_slider_press(
    press: On<Pointer<Press>>,
    mut rows: Query<(&mut SliderState, &SliderParts, Option<&ValueBinding>)>,
    parents: Query<&ChildOf>,
    nodes: Query<(&ComputedNode, &UiGlobalTransform)>,
    mut writer: ValueWriter,
) {
    if press.event().button != PointerButton::Primary {
        return;
    }
    let Ok((mut state, parts, binding)) = rows.get_mut(press.entity) else {
        return;
    };
    if state.disabled {
        return;
    }
    let Some(fraction) = track_fraction(
        press.original_event_target(),
        press.pointer_location.position,
        parts,
        &parents,
        &nodes,
    ) else {
        return;
    };
    state.dragging = true;
    let value = state.value_at(f64::from(fraction));
    propose(press.entity, &mut state, binding, value, &mut writer);
}

/// Observer: while scrubbing, every move is a write.
pub fn on_slider_drag(
    drag: On<Pointer<Drag>>,
    mut rows: Query<(&mut SliderState, &SliderParts, Option<&ValueBinding>)>,
    nodes: Query<(&ComputedNode, &UiGlobalTransform)>,
    mut writer: ValueWriter,
) {
    if drag.event().button != PointerButton::Primary {
        return;
    }
    let Ok((mut state, parts, binding)) = rows.get_mut(drag.entity) else {
        return;
    };
    if !state.dragging || state.disabled {
        return;
    }
    let Ok((node, transform)) = nodes.get(parts.track) else {
        return;
    };
    let fraction = controls::fraction_along(drag.pointer_location.position, node, transform);
    let value = state.value_at(f64::from(fraction));
    propose(drag.entity, &mut state, binding, value, &mut writer);
}

fn stop_drag(entity: Entity, rows: &mut Query<&mut SliderState>) {
    if let Ok(mut state) = rows.get_mut(entity)
        && state.dragging
    {
        state.dragging = false;
    }
}

/// Observer: the release ends the scrub.
pub fn on_slider_release(release: On<Pointer<Release>>, mut rows: Query<&mut SliderState>) {
    stop_drag(release.entity, &mut rows);
}

/// Observer: a drag that ends ends the scrub.
pub fn on_slider_drag_end(end: On<Pointer<DragEnd>>, mut rows: Query<&mut SliderState>) {
    stop_drag(end.entity, &mut rows);
}

/// Observer on [`BoundValue`]: the store's number becomes `value`.
pub fn on_slider_value(event: On<BoundValue>, mut rows: Query<&mut SliderState>) {
    let Ok(mut state) = rows.get_mut(event.entity) else {
        return;
    };
    let Some(value) = event.value.as_f64() else {
        return;
    };
    // Bit-identical is the right test here: the store is the truth, and a
    // value that came back unchanged must not mark the state changed.
    if state.value.to_bits() != value.to_bits() {
        state.value = value;
    }
}

/// `SlottedUiSet::Render`, after the value sync: the fill's width, the
/// thumb's position, the readout and the roles.
#[allow(clippy::type_complexity)]
pub fn paint_sliders(
    focus: Option<Res<bevy::input_focus::InputFocus>>,
    rows: Query<(
        Entity,
        Ref<SliderState>,
        &SliderParts,
        &SliderFormat,
        &Hovered,
    )>,
    mut themed: Query<&mut Themed>,
    mut nodes: Query<&mut Node>,
    mut texts: Query<&mut Text>,
) {
    let focused = focus.and_then(|f| f.get());
    for (entity, state, parts, format, hovered) in &rows {
        let role = controls::state_role(
            "slider",
            ControlLook {
                hovered: hovered.get(),
                focused: focused == Some(entity),
                active: state.dragging,
                disabled: state.disabled,
            },
        );
        if let Ok(mut t) = themed.get_mut(parts.track)
            && t.0 != role
        {
            t.0 = role;
        }
        if !state.is_changed() {
            continue;
        }
        #[allow(clippy::cast_possible_truncation)]
        let percent = (state.fraction() * 100.0) as f32;
        if let Ok(mut fill) = nodes.get_mut(parts.fill) {
            fill.width = Val::Percent(percent);
        }
        if let Ok(mut thumb) = nodes.get_mut(parts.thumb) {
            thumb.left = Val::Percent(percent);
        }
        if let Ok(mut text) = texts.get_mut(parts.readout) {
            let want = format_readout(&format.0, &state);
            if text.0 != want {
                text.0 = want;
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    fn state(value: f64) -> SliderState {
        SliderState {
            value,
            min: 0.0,
            max: 100.0,
            step: 5.0,
            dragging: false,
            disabled: false,
        }
    }

    #[test]
    fn the_readout_substitutes_value_min_max_and_precision() {
        let s = state(42.0);
        assert_eq!(format_readout("{value}%", &s), "42%");
        assert_eq!(format_readout("{value} of {max}", &s), "42 of 100");
        assert_eq!(format_readout("{value:.1}", &state(42.25)), "42.2");
        assert_eq!(format_readout("{value}", &state(42.25)), "42.25");
        assert_eq!(format_readout("{value}", &state(42.5)), "42.5");
        assert_eq!(format_readout("{min}..{max}", &s), "0..100");
        assert_eq!(format_readout("{other} {value", &s), "{other} {value");
    }

    #[test]
    fn snapping_walks_the_step_grid_from_min_and_clamps() {
        let s = state(0.0);
        assert_eq!(s.snap(42.0), 40.0);
        assert_eq!(s.snap(43.0), 45.0);
        assert_eq!(s.snap(-7.0), 0.0);
        assert_eq!(s.snap(140.0), 100.0);
        assert_eq!(s.value_at(0.5), 50.0);
        assert_eq!(state(25.0).fraction(), 0.25);
    }
}
