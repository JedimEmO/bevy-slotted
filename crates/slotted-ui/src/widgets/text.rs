//! Text nodes (menus M1 contract 2.1 and 2.2): the plain `text` node with
//! wrapping, alignment, arguments and truncation, and the `rich_text`
//! paragraph built from the markup in [`crate::rich`].

use bevy::prelude::*;
use slotted_theme::Themed;

use crate::def::{LocKey, TextOpts, TextRole};
use crate::screen::SpawnCtx;
use crate::semantic::{LocText, SemanticLabel, SemanticRole, WidgetNode};
use crate::widgets::kinds;

/// On a text node: truncate with `…` past this many laid-out lines.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaxLines(pub u16);

/// Spawns a plain text node.
pub fn spawn_text(
    ctx: &mut SpawnCtx<'_>,
    key: &LocKey,
    style: TextRole,
    opts: &TextOpts,
) -> Entity {
    // M1-IMPL: A — `TextLayout` from `opts.align` and `opts.wrap`,
    // `MaxLines`, and `LocText.args` from `opts.args`.
    let _ = opts;
    ctx.spawn_node((
        Node::default(),
        Text::new(key.0.clone()),
        Themed(style.role()),
        SemanticRole::Text,
        SemanticLabel(key.0.clone()),
        WidgetNode(kinds::text()),
        LocText::new(key.clone()),
    ))
}

/// Spawns a rich text paragraph: one `Text` with a `TextSpan` child per run.
pub fn spawn_rich_text(
    ctx: &mut SpawnCtx<'_>,
    key: &LocKey,
    style: TextRole,
    opts: &TextOpts,
) -> Entity {
    // M1-IMPL: A
    let entity = spawn_text(ctx, key, style, opts);
    ctx.world
        .entity_mut(entity)
        .insert(WidgetNode(kinds::rich_text()));
    entity
}

/// `SlottedUiSet::Render`: enforces [`MaxLines`] by truncating the resolved
/// string until the laid-out line count fits.
pub fn enforce_max_lines(_texts: Query<(&MaxLines, &mut Text, &ComputedNode)>) {
    // M1-IMPL: A
}
