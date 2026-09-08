//! The hint bar (menus M2 contract 3.6): the `slotted:hint_bar` widget kind,
//! a row of glyph-and-label entries for what the focused node accepts.

use bevy::prelude::*;
use ron::Value;
use slotted_ui::{
    LocKey, ScreenRoot, SpawnCtx, UiAction, UiNodeDef, Widget, WidgetKind, WidgetRegistry,
};

/// The kind.
pub fn kind() -> WidgetKind {
    WidgetKind::new("slotted:hint_bar")
}

/// Marks a hint bar root.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HintBar;

/// One entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HintEntry {
    /// The action whose glyph is shown.
    pub action: UiAction,
    /// The verb.
    pub label: LocKey,
}

/// The entries a bar currently shows, for tests.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub struct HintEntries(pub Vec<HintEntry>);

/// `slotted:hint_bar` through the registry.
#[derive(Debug, Default, Clone, Copy)]
pub struct HintBarWidget;

impl Widget for HintBarWidget {
    fn spawn(&self, ctx: &mut SpawnCtx<'_>, params: &Value, _children: &[UiNodeDef]) -> Entity {
        // M2-IMPL: B
        let _ = params;
        ctx.spawn_node((
            Node {
                column_gap: Val::Px(ctx.tokens().spacing.md),
                align_items: AlignItems::Center,
                ..default()
            },
            slotted_theme::Themed(slotted_theme::roles::HINT_BAR),
            slotted_ui::SemanticRole::Custom("hint_bar".to_owned()),
            slotted_ui::WidgetNode(kind()),
            HintBar,
            HintEntries::default(),
            Pickable::IGNORE,
        ))
    }
}

/// `SlottedUiSet::Render`: rebuilds every bar's entries and children when
/// focus, the input mode, the glyph set, the bindings or the stack changed.
#[allow(clippy::too_many_arguments)]
pub fn update_hint_bars(
    _focus: Option<Res<bevy::input_focus::InputFocus>>,
    _mode: Res<slotted_ui::InputMode>,
    _glyphs: Res<slotted_ui::GlyphSet>,
    _bindings: Res<slotted_ui::UiBindings>,
    _stack: Res<slotted_ui::ScreenStack>,
    _bars: Query<(Entity, &mut HintEntries), With<HintBar>>,
    _roots: Query<&ScreenRoot>,
    _commands: Commands,
) {
    // M2-IMPL: B
}

/// Registers the kind and the system.
pub fn build(app: &mut App) {
    if let Some(mut registry) = app.world_mut().get_resource_mut::<WidgetRegistry>() {
        registry.register(kind(), HintBarWidget);
    }
    app.add_systems(
        Update,
        update_hint_bars.in_set(slotted_ui::SlottedUiSet::Render),
    );
}
