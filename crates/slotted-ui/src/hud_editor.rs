//! Dev-mode HUD position editor. Phase 6 contract section 2.2. Feature `dev`.

use std::path::PathBuf;
use std::sync::Arc;

use bevy::prelude::*;

use crate::hud::{HudAnchored, HudLayout};

/// While `true`, anchor wrappers are pickable and draggable.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HudEditMode(pub bool);

/// The key that toggles [`HudEditMode`].
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct HudEditKey(pub KeyCode);

impl Default for HudEditKey {
    fn default() -> Self {
        Self(KeyCode::F7)
    }
}

/// Drag snap in logical pixels.
pub const SNAP: f32 = 4.0;

/// Where a player's HUD layout is kept between sessions.
///
/// A port rather than a path, because the two places slotted runs disagree
/// about what "kept" means: a game writes a RON file beside its save, and a
/// browser tab has no filesystem at all and has to hand the bytes back to the
/// page for `localStorage`. Both are the same two calls over the same RON, so
/// the editor knows about neither.
///
/// Absent as a resource: the layout is not persisted, which is the default and
/// is right for a test.
pub trait HudLayoutStorage: Send + Sync + 'static {
    /// The stored layout, or `None` when nothing has been stored yet or the
    /// stored bytes were unreadable. An adapter reports the second on
    /// `tracing` and answers `None`: a layout that will not parse is a layer
    /// in the wrong place, never a reason to refuse to draw the HUD.
    fn load(&self) -> Option<HudLayout>;

    /// Persists `layout`. Called at most once a frame, and only after the
    /// layout actually changed.
    fn save(&self, layout: &HudLayout);
}

/// Where [`HudLayout`] is persisted. Absent: not persisted.
///
/// Insert one before adding the plugins: `HudLayoutStore::file(path)` for a
/// game with a disk, or `HudLayoutStore::new(..)` over your own
/// [`HudLayoutStorage`].
#[derive(Resource, Clone)]
pub struct HudLayoutStore(pub Arc<dyn HudLayoutStorage>);

impl HudLayoutStore {
    /// Wraps an adapter.
    pub fn new(storage: impl HudLayoutStorage) -> Self {
        Self(Arc::new(storage))
    }

    /// The RON file adapter, which is what a windowed game wants.
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self::new(FileHudLayout { path: path.into() })
    }
}

impl std::fmt::Debug for HudLayoutStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HudLayoutStore(..)")
    }
}

/// [`HudLayoutStorage`] over one RON file.
///
/// On `wasm32` both calls do nothing: there is no filesystem to write to, and
/// a page that wants the layout back asks for the RON through its own bridge
/// and keeps it in `localStorage`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHudLayout {
    /// File path.
    pub path: PathBuf,
}

impl HudLayoutStorage for FileHudLayout {
    #[cfg(not(target_arch = "wasm32"))]
    fn load(&self) -> Option<HudLayout> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
            Err(e) => {
                tracing::warn!(path = %self.path.display(), %e, "reading the HUD layout");
                return None;
            }
        };
        match ron::from_str::<HudLayout>(&text) {
            Ok(loaded) => Some(loaded),
            Err(e) => {
                tracing::warn!(path = %self.path.display(), %e, "the HUD layout is not RON");
                None
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn load(&self) -> Option<HudLayout> {
        None
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn save(&self, layout: &HudLayout) {
        let text = match ron::ser::to_string_pretty(layout, ron::ser::PrettyConfig::default()) {
            Ok(text) => text,
            Err(e) => {
                tracing::warn!(%e, "serialising the HUD layout");
                return;
            }
        };
        if let Some(dir) = self.path.parent()
            && let Err(e) = std::fs::create_dir_all(dir)
        {
            tracing::warn!(path = %dir.display(), %e, "creating the HUD layout directory");
            return;
        }
        if let Err(e) = std::fs::write(&self.path, text) {
            tracing::warn!(path = %self.path.display(), %e, "writing the HUD layout");
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn save(&self, layout: &HudLayout) {
        let _ = layout;
    }
}

/// [`HudLayoutStorage`] over a value in memory.
///
/// The web playground's adapter: the world saves into it and the page reads the
/// RON back out through `hud_layout()`, which is the whole of "persist to
/// `localStorage`" as far as the library is concerned. Also the easy way for a
/// test to prove a layout survives a round trip without touching a disk.
#[derive(Debug, Default, Clone)]
pub struct MemoryHudLayout(Arc<std::sync::Mutex<Option<HudLayout>>>);

impl MemoryHudLayout {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// What is stored, as RON, or an empty string when nothing is.
    ///
    /// # Errors
    ///
    /// A layout that will not serialise, which cannot happen for the
    /// `BTreeMap` of plain numbers [`HudLayout`] is.
    pub fn to_ron(&self) -> Result<String, ron::Error> {
        match self.get() {
            Some(layout) => ron::ser::to_string(&layout),
            None => Ok(String::new()),
        }
    }

    /// Replaces what is stored with `text`.
    ///
    /// # Errors
    ///
    /// `text` is not a [`HudLayout`].
    pub fn from_ron(&self, text: &str) -> Result<(), ron::error::SpannedError> {
        let layout: HudLayout = ron::from_str(text)?;
        self.set(Some(layout));
        Ok(())
    }

    /// The stored layout.
    pub fn get(&self) -> Option<HudLayout> {
        self.lock().clone()
    }

    /// Replaces the stored layout.
    pub fn set(&self, layout: Option<HudLayout>) {
        *self.lock() = layout;
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<HudLayout>> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl HudLayoutStorage for MemoryHudLayout {
    fn load(&self) -> Option<HudLayout> {
        self.get()
    }

    fn save(&self, layout: &HudLayout) {
        self.set(Some(layout.clone()));
    }
}

/// Marks the outline child drawn around a wrapper in edit mode.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct HudEditFrame;

/// `SlottedUiSet::Input`: toggles edit mode on [`HudEditKey`].
pub fn toggle_hud_edit(
    keys: Res<ButtonInput<KeyCode>>,
    key: Res<HudEditKey>,
    mut mode: ResMut<HudEditMode>,
) {
    if keys.just_pressed(key.0) {
        mode.0 = !mode.0;
    }
}

/// Marks a wrapper whose drag observers are attached. The observers stay for
/// the app's life; leaving edit mode only takes the wrapper's `Pickable` and
/// its outline away, which is what stops the drags.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct HudEditable;

/// On a wrapper between `DragStart` and `DragEnd`: the offset the drag began
/// from, so the total drag distance places the layer and `Esc` can put it
/// back.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct HudDragging {
    /// `HudAnchor::offset` when the drag started.
    pub start: Vec2,
}

/// `SlottedUiSet::Render`: on `Changed<HudEditMode>` makes wrappers pickable
/// and adds or removes the [`HudEditFrame`] child and drag observers.
pub fn apply_hud_edit_mode(
    mode: Res<HudEditMode>,
    wrappers: Query<(Entity, &HudAnchored, Has<HudEditable>)>,
    frames: Query<(Entity, &ChildOf), With<HudEditFrame>>,
    mut commands: Commands,
) {
    for (entity, _, editable) in &wrappers {
        match (mode.0, editable) {
            (true, false) => {
                commands
                    .entity(entity)
                    .insert((Pickable::default(), HudEditable))
                    .observe(on_hud_drag_start)
                    .observe(on_hud_drag)
                    .observe(on_hud_drag_end);
                commands.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(0),
                        top: px(0),
                        width: percent(100),
                        height: percent(100),
                        border: UiRect::all(px(1)),
                        ..default()
                    },
                    slotted_theme::Themed(slotted_theme::roles::HUD_EDIT_FRAME),
                    HudEditFrame,
                    Pickable::IGNORE,
                    ChildOf(entity),
                ));
            }
            (false, true) => {
                commands
                    .entity(entity)
                    .insert(Pickable::IGNORE)
                    .remove::<HudEditable>()
                    .remove::<HudDragging>();
                for (frame, parent) in &frames {
                    if parent.parent() == entity {
                        commands.entity(frame).despawn();
                    }
                }
            }
            _ => {}
        }
    }
}

/// Observer on `Pointer<DragStart>`: remembers where the layer was.
pub fn on_hud_drag_start(
    drag: On<Pointer<DragStart>>,
    anchors: Query<&crate::hud::HudAnchor, With<HudAnchored>>,
    mut commands: Commands,
) {
    if let Ok(anchor) = anchors.get(drag.entity) {
        commands.entity(drag.entity).insert(HudDragging {
            start: anchor.offset,
        });
    }
}

/// Observer on `Pointer<DragEnd>`: the drag is over; the layout file is
/// written by [`save_hud_layout`] on the next `Last`.
pub fn on_hud_drag_end(drag: On<Pointer<DragEnd>>, mut commands: Commands) {
    commands.entity(drag.entity).try_remove::<HudDragging>();
}

/// Observer on `Pointer<Drag>` for a wrapper: moves `offset` by the drag
/// delta snapped to [`SNAP`], writing `HudAnchor` and `HudLayout`.
///
/// The offset is computed from the drag's total distance rather than
/// accumulated per event: snapping every event would quantise each delta on
/// its own, and a slow drag of one pixel per frame would never move at all.
/// `crate::hud::reanchor_hud_layers` turns the new anchor into a `Node`.
pub fn on_hud_drag(
    drag: On<Pointer<Drag>>,
    mut wrappers: Query<(
        &HudAnchored,
        &mut crate::hud::HudAnchor,
        Option<&HudDragging>,
    )>,
    mut layout: ResMut<HudLayout>,
) {
    let Ok((anchored, mut anchor, dragging)) = wrappers.get_mut(drag.entity) else {
        return;
    };
    let from = dragging.map_or(anchor.offset, |d| d.start);
    let moved = dragging.map_or(drag.delta, |_| drag.distance);
    let offset = snap(from + moved);
    if anchor.offset != offset {
        anchor.offset = offset;
    }
    layout.anchors.insert(anchored.id.clone(), *anchor);
}

/// `SlottedUiSet::Input`, after `UiActionEmit`: `Back` puts a layer being
/// dragged back where it was and is claimed, so the stack does not also pop
/// a screen on the same press (menus contract 2.5).
pub fn cancel_hud_drag(
    mut events: MessageReader<crate::actions::UiActionEvent>,
    mut claims: ResMut<crate::actions::UiActionClaims>,
    mut wrappers: Query<(
        Entity,
        &HudAnchored,
        &mut crate::hud::HudAnchor,
        &HudDragging,
    )>,
    mut layout: ResMut<HudLayout>,
    mut commands: Commands,
) {
    let back = events
        .read()
        .any(|e| e.action == crate::actions::UiAction::Back);
    if !back || wrappers.is_empty() {
        return;
    }
    claims.claim(crate::actions::UiAction::Back);
    for (entity, anchored, mut anchor, dragging) in &mut wrappers {
        anchor.offset = dragging.start;
        layout.anchors.insert(anchored.id.clone(), *anchor);
        commands.entity(entity).try_remove::<HudDragging>();
    }
}

/// `v` rounded to the nearest [`SNAP`] step on both axes.
fn snap(v: Vec2) -> Vec2 {
    (v / SNAP).round() * SNAP
}

/// `Startup`: reads [`HudLayoutStore`] into [`HudLayout`] when it holds one.
pub fn load_hud_layout(store: Option<Res<HudLayoutStore>>, mut layout: ResMut<HudLayout>) {
    let Some(store) = store else {
        return;
    };
    if let Some(loaded) = store.0.load() {
        *layout = loaded;
    }
}

/// On `DragEnd` and `AppExit`: writes [`HudLayout`] to [`HudLayoutStore`].
pub fn save_hud_layout(store: Option<Res<HudLayoutStore>>, layout: Res<HudLayout>) {
    let Some(store) = store else {
        return;
    };
    if !layout.is_changed() || layout.anchors.is_empty() {
        return;
    }
    store.0.save(&layout);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::{HudAnchor, HudLayerId};

    #[test]
    fn the_memory_adapter_round_trips_a_layout_through_ron() {
        let store = MemoryHudLayout::new();
        assert!(store.load().is_none());
        assert_eq!(store.to_ron().expect("an empty store serialises"), "");

        let mut layout = HudLayout::default();
        layout.anchors.insert(
            HudLayerId::new("slotted:hotbar"),
            HudAnchor {
                offset: Vec2::new(12.0, -40.0),
                ..HudAnchor::default()
            },
        );
        store.save(&layout);
        let text = store.to_ron().expect("a layout serialises");

        let second = MemoryHudLayout::new();
        second.from_ron(&text).expect("the RON parses back");
        assert_eq!(second.load(), Some(layout));
    }

    #[test]
    fn a_layout_that_is_not_ron_is_an_error_and_leaves_the_store_alone() {
        let store = MemoryHudLayout::new();
        assert!(store.from_ron("not ron at all").is_err());
        assert!(store.load().is_none());
    }
}
