//! `slotted:search_field`: the query text field. Package B.
//!
//! Wraps Bevy 0.19's `bevy_text::EditableText`. `EditableTextInputPlugin` is
//! in `UiWidgetsPlugins` and `TextPlugin` adds the clipboard and the
//! `apply_text_edits` system, so it edits headless: the harness's `type_text`
//! reaches it through `FocusedInput<KeyboardInput>` with no window at all.
//! There is therefore no `MinimalTextField` fallback.

use bevy::input_focus::tab_navigation::TabIndex;
use bevy::prelude::*;
use bevy::text::{EditableText, TextEdit};
use slotted_registry::Value;
use slotted_theme::Themed;
use slotted_ui::{SemanticRole, SpawnCtx, UiNodeDef, Widget};

use super::panel::{ChromeText, chrome_in, keys};
use super::roles;
use crate::events::SearchChanged;
use crate::runtime::BrowserRuntime;

/// Marker on the search field entity, holding the value last reconciled with
/// [`BrowserRuntime::filter_text`].
#[derive(Component, Debug, Default, Clone)]
pub struct SearchField {
    /// The last value this field and the runtime agreed on.
    pub last: String,
}

/// The widget.
#[derive(Debug, Default, Clone, Copy)]
pub struct SearchFieldWidget;

impl Widget for SearchFieldWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, _params: &Value, _children: &[UiNodeDef]) -> Entity {
        let label = chrome_in(ctx.world, keys::SEARCH_LABEL, "Search");
        let field = ctx.spawn_node((
            Node {
                width: percent(100),
                min_height: px(32),
                flex_shrink: 0.0,
                align_items: AlignItems::Center,
                padding: UiRect::axes(px(10), px(6)),
                ..default()
            },
            EditableText::new(""),
            TabIndex(0),
            slotted_ui::Focusable,
            Themed(roles::SEARCH),
            SemanticRole::TextField,
            slotted_ui::SemanticLabel(label),
            SearchField::default(),
        ));
        let placeholder = chrome_in(ctx.world, keys::SEARCH_PLACEHOLDER, PLACEHOLDER);
        ctx.world.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(10),
                ..default()
            },
            Text::new(placeholder),
            Themed(super::roles::HINT),
            ChromeText::new(keys::SEARCH_PLACEHOLDER, PLACEHOLDER),
            SearchPlaceholder,
            Pickable::IGNORE,
            ChildOf(field),
        ));
        field
    }
}

/// The English the placeholder falls back to.
const PLACEHOLDER: &str = "Search items";

/// The grey prompt shown while the field is empty.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct SearchPlaceholder;

/// `BrowserSet::Input`: reconcile the field and the runtime in both
/// directions. A typed character writes [`SearchChanged`]; a query set from
/// elsewhere (a category chip, a test) is written back into the field.
pub fn diff_search_field(
    mut fields: Query<(&mut SearchField, &mut EditableText)>,
    runtime: Res<BrowserRuntime>,
    mut changed: MessageWriter<SearchChanged>,
) {
    for (mut field, mut text) in &mut fields {
        if text.value() != field.last.as_str() {
            let value = text.value().to_string();
            field.last.clone_from(&value);
            changed.write(SearchChanged { text: value });
        } else if runtime.filter_text != field.last {
            field.last.clone_from(&runtime.filter_text);
            text.editor.set_text(&runtime.filter_text);
            text.queue_edit(TextEdit::TextEnd(false));
        }
    }
}

/// Replaces the query in every search field, as a category chip does.
pub fn set_query(fields: &mut Query<(&mut SearchField, &mut EditableText)>, query: &str) {
    for (mut field, mut text) in fields.iter_mut() {
        if text.value() == query {
            continue;
        }
        query.clone_into(&mut field.last);
        text.editor.set_text(query);
        text.queue_edit(TextEdit::TextEnd(false));
    }
}

/// `BrowserSet::Render`: the focused field paints itself with
/// `browser.search.focus`, and the placeholder steps aside as soon as there is
/// anything to read.
pub fn search_field_state_role(
    focus: Option<Res<bevy::input_focus::InputFocus>>,
    mut fields: Query<(Entity, &mut Themed, &EditableText, Option<&Children>), With<SearchField>>,
    mut placeholders: Query<&mut Visibility, With<SearchPlaceholder>>,
) {
    let focused = focus.and_then(|f| f.get());
    for (entity, mut themed, text, children) in &mut fields {
        let want = if text.value().to_string().is_empty() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        for child in children.into_iter().flatten() {
            if let Ok(mut visibility) = placeholders.get_mut(*child)
                && *visibility != want
            {
                *visibility = want;
            }
        }
        let want = if Some(entity) == focused {
            roles::SEARCH_FOCUS
        } else {
            roles::SEARCH
        };
        if themed.0 != want {
            themed.0 = want;
        }
    }
}
