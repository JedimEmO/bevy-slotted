//! The text field (menus M1 contract 4.1): a framed `EditableText` bound to
//! a `Text`, editing only after Accept.
//!
//! Three nodes. The row is the control: `Focusable`, `TabIndex(0)`,
//! [`TextFieldState`], the optional label to its left. The frame is the
//! themed box (`text_field`, `.focus`, `.disabled`) and holds the editable
//! text and the placeholder. Focus on the row shows the ring and edits
//! nothing; `Accept` (or a click) hands Bevy's `InputFocus` to the editable
//! child, which is what sets `TextEntryFocused` and stops keyboard actions.
//! Enter commits with a [`SetValue`] and returns focus to the row; `Back`
//! reverts to the last committed text, returns focus and claims, so the
//! screen does not pop. A gamepad `Accept` writes [`TextEntryRequested`] for
//! the game's on-screen keyboard and nothing else.

use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input_focus::tab_navigation::TabIndex;
use bevy::input_focus::{FocusCause, FocusedInput, InputFocus};
use bevy::picking::events::{Click, Pointer};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::text::{EditableText, EditableTextFilter, TextEdit};
use bevy::ui::auto_directional_navigation::AutoDirectionalNavigation;
use slotted_theme::{Themed, roles};

use crate::actions::{InputDevice, UiAction, UiActionClaims};
use crate::def::TextFilter;
use crate::def::{BindDef, LocKey};
use crate::focus_ring::Focusable;
use crate::nav::FocusedAction;
use crate::screen::SpawnCtx;
use crate::semantic::{LocText, SemanticLabel, SemanticRole, WidgetNode};
use crate::values::{BindingTarget, SetValue, Value, ValueBinding, ValueStore};
use crate::widgets::kinds;

/// The field's state, for tests.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct TextFieldState {
    /// Current text.
    pub text: String,
    /// Bevy's `InputFocus` is on the `EditableText`.
    pub editing: bool,
    /// What it accepts.
    pub filter: TextFilter,
    /// Inert.
    pub disabled: bool,
}

/// A gamepad `Accept` on a text field: the game's cue to show an on-screen
/// keyboard.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextEntryRequested {
    /// The field.
    pub entity: Entity,
}

/// On the row: its parts.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextFieldParts {
    /// The themed frame.
    pub frame: Entity,
    /// The `EditableText`.
    pub editable: Entity,
}

/// On the editable child: its row.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextFieldEditable {
    /// The row.
    pub field: Entity,
}

/// The text the field last committed (or was seeded with); what `Back`
/// reverts to.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub struct CommittedText(pub String);

/// On a placeholder node: hidden while the sibling editable has text.
/// The browser's search field uses it too.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextPlaceholder;

/// The per-character filter for a [`TextFilter`], if it filters anything.
/// A positional rule (one point, a leading minus) cannot be expressed per
/// character; the store's rule parses the committed text.
pub fn editable_filter(filter: TextFilter) -> Option<EditableTextFilter> {
    match filter {
        TextFilter::Any => None,
        TextFilter::Numeric => Some(EditableTextFilter::new(|c: char| {
            c.is_ascii_digit() || c == '.' || c == '-'
        })),
        TextFilter::Integer => Some(EditableTextFilter::new(|c: char| {
            c.is_ascii_digit() || c == '-'
        })),
    }
}

/// Spawns a text field row.
#[allow(clippy::too_many_lines)]
pub fn spawn_text_field(
    ctx: &mut SpawnCtx<'_>,
    label: Option<&LocKey>,
    placeholder: Option<&LocKey>,
    filter: TextFilter,
    max_len: Option<u16>,
    bind: &BindDef,
) -> Entity {
    let tokens = ctx.tokens();
    let height = tokens.sizes.control_height;
    let entity = ctx.spawn_node((
        Node {
            height: Val::Px(height),
            min_height: Val::Px(height),
            width: Val::Percent(100.0),
            align_items: AlignItems::Center,
            column_gap: Val::Px(tokens.spacing.sm),
            flex_shrink: 0.0,
            ..default()
        },
        Themed(slotted_theme::Role::new_static("invisible")),
        SemanticRole::TextField,
        SemanticLabel(label.map(|k| k.0.clone()).unwrap_or_default()),
        WidgetNode(kinds::text_field()),
        TextFieldState {
            text: String::new(),
            editing: false,
            filter,
            disabled: bind.disabled,
        },
        CommittedText::default(),
        crate::widgets::list::StoreSeen(0),
        Focusable,
        TabIndex(0),
        AutoDirectionalNavigation::default(),
        Hovered::default(),
        Pickable::default(),
    ));
    if bind.disabled {
        ctx.world
            .entity_mut(entity)
            .insert(bevy::ui::InteractionDisabled);
    }
    if let Some(binding) = crate::widgets::list::binding_for(ctx, bind) {
        ctx.world.entity_mut(entity).insert(binding);
    }

    if let Some(label) = label {
        spawn_control_label(ctx, entity, label);
    }

    let frame = ctx
        .world
        .spawn((
            Node {
                flex_grow: 1.0,
                flex_shrink: 1.0,
                min_width: Val::Px(0.0),
                height: Val::Percent(100.0),
                padding: UiRect::axes(Val::Px(tokens.spacing.md), Val::Px(tokens.spacing.xs)),
                border: UiRect::all(Val::Px(crate::widgets::BORDER_WIDTH)),
                border_radius: BorderRadius::all(Val::Px(tokens.radii.sm)),
                align_items: AlignItems::Center,
                position_type: PositionType::Relative,
                overflow: Overflow::clip(),
                ..default()
            },
            Themed(if bind.disabled {
                roles::TEXT_FIELD_DISABLED
            } else {
                roles::TEXT_FIELD
            }),
            Pickable::default(),
            ChildOf(entity),
        ))
        .id();

    let mut editable = EditableText::new("");
    editable.max_characters = max_len.map(usize::from);
    let editable = ctx
        .world
        .spawn((
            Node {
                flex_grow: 1.0,
                min_width: Val::Px(0.0),
                ..default()
            },
            editable,
            Themed(roles::TEXT),
            TextFieldEditable { field: entity },
            Focusable,
            Pickable::IGNORE,
            ChildOf(frame),
        ))
        .id();
    if let Some(filter) = editable_filter(filter) {
        ctx.world.entity_mut(editable).insert(filter);
    }

    if let Some(placeholder) = placeholder {
        ctx.world.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(tokens.spacing.md),
                ..default()
            },
            Text::new(placeholder.0.clone()),
            Themed(roles::TEXT_FIELD_PLACEHOLDER),
            LocText::new(placeholder.clone()),
            TextPlaceholder,
            Pickable::IGNORE,
            ChildOf(frame),
        ));
    }

    ctx.world
        .entity_mut(entity)
        .insert(TextFieldParts { frame, editable });
    entity
}

/// A control's label: a `LocText` in the `control.label` role, as a child
/// of `row`.
pub fn spawn_control_label(ctx: &mut SpawnCtx<'_>, row: Entity, label: &LocKey) -> Entity {
    ctx.world
        .spawn((
            Node {
                flex_shrink: 0.0,
                ..default()
            },
            Text::new(label.0.clone()),
            Themed(roles::CONTROL_LABEL),
            SemanticRole::Text,
            SemanticLabel(label.0.clone()),
            LocText::new(label.clone()),
            Pickable::IGNORE,
            ChildOf(row),
        ))
        .id()
}

/// Puts `text` into the editor with the caret at the end.
fn set_editor_text(editable: &mut EditableText, text: &str) {
    editable.editor.set_text(text);
    editable.pending_edits.clear();
    editable.queue_edit(TextEdit::TextEnd(false));
}

/// Writes `text` to the row's binding.
fn write_binding(
    field: Entity,
    binding: Option<&ValueBinding>,
    text: &str,
    writes: &mut MessageWriter<SetValue>,
) {
    if let Some(BindingTarget::Store(key)) = binding.map(|b| &b.target) {
        writes.write(SetValue {
            key: key.clone(),
            value: Value::Text(text.to_owned()),
            source: Some(field),
        });
    }
}

/// Observer: a [`FocusedAction`] on the row or on its editable child.
///
/// On the row: `Accept` from the keyboard enters editing; from a pad it
/// writes [`TextEntryRequested`]. On the editable child, while editing:
/// `Back` reverts and leaves, a pad `Accept` commits and leaves. Each is
/// claimed when acted on.
#[allow(clippy::too_many_arguments)]
pub fn on_text_field_action(
    action: On<FocusedAction>,
    rows: Query<(
        &TextFieldParts,
        &CommittedText,
        Option<&ValueBinding>,
        Has<bevy::ui::InteractionDisabled>,
    )>,
    editables: Query<&TextFieldEditable>,
    mut texts: Query<&mut EditableText>,
    mut focus: Option<ResMut<InputFocus>>,
    mut claims: ResMut<UiActionClaims>,
    mut requests: MessageWriter<TextEntryRequested>,
    mut writes: MessageWriter<SetValue>,
    mut commands: Commands,
) {
    if action.repeat {
        return;
    }
    // The row: enter editing, or ask for the on-screen keyboard.
    if let Ok((parts, _, _, disabled)) = rows.get(action.entity) {
        if disabled || action.action != UiAction::Accept {
            return;
        }
        claims.claim(UiAction::Accept);
        if action.device == InputDevice::Gamepad {
            requests.write(TextEntryRequested {
                entity: action.entity,
            });
            return;
        }
        if let Ok(mut text) = texts.get_mut(parts.editable) {
            text.queue_edit(TextEdit::TextEnd(false));
        }
        if let Some(focus) = focus.as_deref_mut() {
            focus.set(parts.editable, FocusCause::Navigated);
        }
        return;
    }
    // The editable child, while editing.
    let Ok(editable) = editables.get(action.entity) else {
        return;
    };
    let Ok((parts, committed, binding, _)) = rows.get(editable.field) else {
        return;
    };
    match action.action {
        UiAction::Back => {
            claims.claim(UiAction::Back);
            if let Ok(mut text) = texts.get_mut(parts.editable) {
                set_editor_text(&mut text, &committed.0);
            }
        }
        UiAction::Accept if action.device == InputDevice::Gamepad => {
            claims.claim(UiAction::Accept);
            let Ok(text) = texts.get(parts.editable) else {
                return;
            };
            let value = text.value().to_string();
            commands
                .entity(editable.field)
                .insert(CommittedText(value.clone()));
            write_binding(editable.field, binding, &value, &mut writes);
        }
        _ => return,
    }
    if let Some(focus) = focus.as_deref_mut() {
        focus.set(editable.field, FocusCause::Navigated);
    }
}

/// Observer: Enter on the editable child commits and leaves editing.
///
/// Keyboard actions are suppressed while a field has the keyboard, so this
/// is the one key the field reads itself; it is the logical `Enter`, which
/// Bevy's text input leaves unhandled for exactly this purpose.
pub fn on_text_field_enter(
    mut input: On<FocusedInput<KeyboardInput>>,
    editables: Query<&TextFieldEditable>,
    rows: Query<Option<&ValueBinding>, With<TextFieldParts>>,
    texts: Query<&EditableText>,
    mut focus: Option<ResMut<InputFocus>>,
    mut writes: MessageWriter<SetValue>,
    mut commands: Commands,
) {
    let Ok(editable) = editables.get(input.focused_entity) else {
        return;
    };
    let key = &input.input;
    if key.state != ButtonState::Pressed || key.repeat || key.logical_key != Key::Enter {
        return;
    }
    let Ok(binding) = rows.get(editable.field) else {
        return;
    };
    if texts
        .get(input.focused_entity)
        .is_ok_and(EditableText::is_composing)
    {
        return;
    }
    input.propagate(false);
    let value = texts
        .get(input.focused_entity)
        .map(|t| t.value().to_string())
        .unwrap_or_default();
    commands
        .entity(editable.field)
        .insert(CommittedText(value.clone()));
    write_binding(editable.field, binding, &value, &mut writes);
    if let Some(focus) = focus.as_deref_mut() {
        focus.set(editable.field, FocusCause::Navigated);
    }
}

/// Observer: a click anywhere on the row enters editing.
pub fn on_text_field_click(
    click: On<Pointer<Click>>,
    rows: Query<(&TextFieldParts, Has<bevy::ui::InteractionDisabled>)>,
    mut texts: Query<&mut EditableText>,
    mut focus: Option<ResMut<InputFocus>>,
) {
    if click.button != bevy::picking::pointer::PointerButton::Primary {
        return;
    }
    let Ok((parts, disabled)) = rows.get(click.entity) else {
        return;
    };
    if disabled {
        return;
    }
    if let Ok(mut text) = texts.get_mut(parts.editable) {
        text.queue_edit(TextEdit::TextEnd(false));
    }
    if let Some(focus) = focus.as_deref_mut() {
        focus.set(parts.editable, FocusCause::Pressed);
    }
}

/// `SlottedUiSet::Render`: keeps [`TextFieldState`] current, seeds the
/// editor from the store, writes live while a filtered field is edited,
/// commits when editing ends without Enter or `Back`, and paints the frame.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn sync_text_fields(
    store: Res<ValueStore>,
    focus: Option<Res<InputFocus>>,
    mut rows: Query<(
        Entity,
        &TextFieldParts,
        &mut TextFieldState,
        &mut CommittedText,
        &mut crate::widgets::list::StoreSeen,
        Option<&ValueBinding>,
        Has<bevy::ui::InteractionDisabled>,
    )>,
    mut texts: Query<&mut EditableText>,
    mut frames: Query<&mut Themed, Without<EditableText>>,
    mut writes: MessageWriter<SetValue>,
) {
    let focused = focus.and_then(|f| f.get());
    for (entity, parts, mut state, mut committed, mut seen, binding, disabled) in &mut rows {
        let Ok(mut text) = texts.get_mut(parts.editable) else {
            continue;
        };
        let editing = focused == Some(parts.editable);

        // Seed from the store while not editing.
        if let Some(BindingTarget::Store(key)) = binding.map(|b| &b.target)
            && seen.0 != store.version()
            && !editing
        {
            seen.0 = store.version();
            let want = match store.get(key) {
                Some(Value::Text(s)) => s.clone(),
                Some(Value::Int(i)) => i.to_string(),
                Some(Value::Float(f)) => f.to_string(),
                Some(Value::Bool(b)) => b.to_string(),
                None => String::new(),
            };
            if committed.0 != want {
                committed.0.clone_from(&want);
            }
            if text.value().to_string() != want {
                set_editor_text(&mut text, &want);
            }
        }

        let current = text.value().to_string();
        if state.editing && !editing && current != committed.0 {
            // Focus left without Enter or Back (a click elsewhere): commit.
            committed.0.clone_from(&current);
            write_binding(entity, binding, &current, &mut writes);
        }
        if editing && state.filter != TextFilter::Any && state.text != current {
            write_binding(entity, binding, &current, &mut writes);
        }
        if state.text != current {
            state.text = current;
        }
        if state.editing != editing {
            state.editing = editing;
        }
        if state.disabled != disabled {
            state.disabled = disabled;
        }

        let role = if disabled {
            roles::TEXT_FIELD_DISABLED
        } else if editing || focused == Some(entity) {
            roles::TEXT_FIELD_FOCUS
        } else {
            roles::TEXT_FIELD
        };
        if let Ok(mut themed) = frames.get_mut(parts.frame)
            && themed.0 != role
        {
            themed.0 = role;
        }
    }
}

/// `SlottedUiSet::Render`: a [`TextPlaceholder`] shows only while the
/// `EditableText` beside it is empty.
pub fn sync_placeholders(
    texts: Query<(&EditableText, &ChildOf)>,
    mut placeholders: Query<(&ChildOf, &mut Visibility), With<TextPlaceholder>>,
) {
    for (parent, mut visibility) in &mut placeholders {
        // Beside the editable in a frame (this control), or under the
        // editable itself (the browser's search field).
        let owner = texts
            .get(parent.parent())
            .map(|(text, _)| text)
            .ok()
            .or_else(|| {
                texts
                    .iter()
                    .find(|(_, child_of)| child_of.parent() == parent.parent())
                    .map(|(text, _)| text)
            });
        let sibling_empty = owner.is_none_or(|text| text.value().to_string().is_empty());
        let want = if sibling_empty {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != want {
            *visibility = want;
        }
    }
}

/// Registers the text field's observers and systems.
pub fn build(app: &mut App) {
    app.add_observer(on_text_field_action)
        .add_observer(on_text_field_enter)
        .add_observer(on_text_field_click)
        .add_systems(
            Update,
            (sync_text_fields, sync_placeholders)
                .chain()
                .in_set(crate::plugin::SlottedUiSet::Render),
        );
}
