//! The scroll panel (menus M1 contract 4.2): a panel that clips, scrolls on
//! the wheel, follows focus and pages on `PagePrev`/`PageNext`.

use bevy::prelude::*;

use crate::def::{Layout, UiNodeDef};
use crate::screen::SpawnCtx;

/// Marks a scroll panel.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScrollPanel;

/// Spawns a scroll panel and its children.
pub fn spawn_scroll(
    ctx: &mut SpawnCtx<'_>,
    layout: &Layout,
    scrollbar: bool,
    children: &[UiNodeDef],
) -> Entity {
    // M1-IMPL: C — `ScrollArea`, the scrollbar child, `flex_shrink: 0` rows.
    let _ = scrollbar;
    let mut layout = layout.clone();
    layout.overflow = crate::def::Overflow::Scroll;
    let entity = crate::widgets::spawn_panel(
        ctx,
        &slotted_theme::Role::new_static("invisible"),
        &layout,
        children,
    );
    ctx.world.entity_mut(entity).insert((
        ScrollPanel,
        crate::semantic::SemanticRole::ScrollView,
        crate::semantic::WidgetNode(crate::widgets::kinds::scroll()),
    ));
    entity
}

/// `SlottedUiSet::Render`: brings the focused node into view when it sits
/// under a scroll panel.
pub fn scroll_focus_into_view(
    _focus: Option<Res<bevy::input_focus::InputFocus>>,
    _parents: Query<&ChildOf>,
    _panels: Query<(), With<ScrollPanel>>,
    _commands: Commands,
) {
    // M1-IMPL: C
}
