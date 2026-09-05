//! `slotted:search_field`: the query text field. Package B.
//!
//! Wraps Bevy 0.19's `bevy_text::EditableText` (`EditableTextInputPlugin` is
//! in `UiWidgetsPlugins`; `TextPlugin` adds the clipboard) so it runs
//! headless. If the harness's `type_text` does not reach it, swap the inner
//! component for a minimal own field and keep this wrapper.

use bevy::prelude::*;
use slotted_registry::Value;
use slotted_theme::Themed;
use slotted_ui::{SemanticRole, SpawnCtx, UiNodeDef, Widget};

use super::roles;
use crate::events::SearchChanged;
use crate::runtime::BrowserRuntime;

/// Marker on the search field entity.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct SearchField;

/// The widget.
#[derive(Debug, Default, Clone, Copy)]
pub struct SearchFieldWidget;

impl Widget for SearchFieldWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, _params: &Value, _children: &[UiNodeDef]) -> Entity {
        // PHASE3-IMPL: B — `EditableText::new("")`, `TabIndex`, placeholder.
        ctx.spawn_node((
            Node {
                width: percent(100),
                min_height: px(28),
                ..default()
            },
            Themed(roles::SEARCH),
            SemanticRole::TextField,
            SearchField,
        ))
    }
}

/// `BrowserSet::Input`: write `SearchChanged` when the field's value differs
/// from `BrowserRuntime::filter_text`.
pub fn diff_search_field(
    _fields: Query<&SearchField>,
    _runtime: Res<BrowserRuntime>,
    _changed: MessageWriter<SearchChanged>,
) {
    // PHASE3-IMPL: B — read `EditableText::value()`.
}
