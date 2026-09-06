//! `*.screen.ron` as a Bevy asset.
//!
//! A screen is data, and until now the only ways in were Rust
//! ([`Screens::register`]) and the frozen registries. This adds the third: an
//! [`AssetLoader`] for `*.screen.ron` that parses through
//! [`ScreenDef::from_ron`], so a game writes
//! `screens.load(&asset_server, "screens/chest.screen.ron")` and gets asset
//! path resolution and, with `bevy/file_watcher`, hot reload for free.
//!
//! [`apply_screen_assets`] is the bridge: every `Added` or `Modified` event
//! registers the definition in [`Screens`] and re-opens any screen of that
//! kind that is on screen right now, through
//! [`crate::screen::respawn_open_screens`] -- the same
//! path `slotted-packs` takes after a mod reload.

use bevy::asset::io::Reader;
use bevy::asset::{AssetApp, AssetLoader, AssetPath, LoadContext};
use bevy::prelude::*;
use bevy::reflect::TypePath;

use crate::def::{ScreenDef, ScreenKind};
use crate::screen::{Screens, respawn_open_screens};

/// Why a `*.screen.ron` failed to load.
#[derive(Debug, thiserror::Error)]
pub enum ScreenAssetError {
    /// Read failure.
    #[error("could not read screen: {0}")]
    Io(#[from] std::io::Error),
    /// RON parse failure, or a tree that did not fit [`crate::UiNodeDef`].
    #[error("could not parse screen: {0}")]
    Ron(#[from] ron::error::SpannedError),
}

/// Loads `*.screen.ron` into a [`ScreenDef`].
#[derive(Default, TypePath)]
pub struct ScreenLoader;

impl AssetLoader for ScreenLoader {
    type Asset = ScreenDef;
    type Settings = ();
    type Error = ScreenAssetError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        _load_context: &mut LoadContext<'_>,
    ) -> Result<ScreenDef, ScreenAssetError> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let text = String::from_utf8_lossy(&bytes);
        Ok(ScreenDef::from_ron(&text)?)
    }

    fn extensions(&self) -> &[&str] {
        &["screen.ron"]
    }
}

/// The screen handles the game wants kept alive.
///
/// A `Handle<ScreenDef>` that nobody holds is dropped and its asset unloaded,
/// which would end hot reload the frame after the first load; parking the
/// handle here is what keeps the file watched. Registration into [`Screens`]
/// is [`apply_screen_assets`]'s job, not this resource's.
#[derive(Resource, Default, Debug, Clone)]
pub struct ScreenAssets(pub Vec<Handle<ScreenDef>>);

impl ScreenAssets {
    /// Starts loading `path` and keeps the handle.
    ///
    /// ```no_run
    /// # use bevy::prelude::*;
    /// # use slotted_ui::ScreenAssets;
    /// fn setup(assets: Res<AssetServer>, mut screens: ResMut<ScreenAssets>) {
    ///     screens.load(&assets, "screens/demo_chest.screen.ron");
    /// }
    /// ```
    pub fn load(
        &mut self,
        assets: &AssetServer,
        path: impl Into<AssetPath<'static>>,
    ) -> Handle<ScreenDef> {
        let handle = assets.load(path.into());
        self.0.push(handle.clone());
        handle
    }

    /// Keeps a handle loaded elsewhere.
    pub fn keep(&mut self, handle: Handle<ScreenDef>) {
        self.0.push(handle);
    }
}

/// `SlottedUiSet::Render`: a loaded or changed `*.screen.ron` becomes the
/// registered [`ScreenDef`], and any open screen of that kind respawns.
///
/// A file whose parse produced the definition already registered is ignored,
/// so touching a file without changing it does not blink the screen.
pub fn apply_screen_assets(
    mut events: MessageReader<AssetEvent<ScreenDef>>,
    defs: Res<Assets<ScreenDef>>,
    mut screens: ResMut<Screens>,
    mut commands: Commands,
) {
    let mut changed: Vec<ScreenKind> = Vec::new();
    for event in events.read() {
        let id = match event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } => *id,
            _ => continue,
        };
        let Some(def) = defs.get(id) else {
            continue;
        };
        if screens.get(&def.kind).is_some_and(|old| **old == *def) {
            continue;
        }
        screens.register(def.clone());
        changed.push(def.kind.clone());
    }
    if changed.is_empty() {
        return;
    }
    for kind in &changed {
        tracing::debug!(screen = %kind.0, "screen asset registered");
    }
    commands.queue(move |world: &mut World| respawn_open_screens(world, &changed));
}

/// Screen assets whose last load failed, so the failure is said once.
///
/// Keyed by asset id and cleared when the file loads again, so an author who
/// breaks a screen, reads the warning and fixes it is told again the next time
/// they break it.
#[derive(Resource, Default, Debug, Clone)]
pub struct FailedScreenAssets(std::collections::HashSet<AssetId<ScreenDef>>);

impl FailedScreenAssets {
    /// Whether `id`'s last load failed, so the screen being drawn for it is an
    /// older definition than the file on disk.
    ///
    /// A game with a developer overlay can read this and say so on screen,
    /// which is worth more than the log line: the author is looking at the
    /// game, not at a terminal.
    #[must_use]
    pub fn contains(&self, id: AssetId<ScreenDef>) -> bool {
        self.0.contains(&id)
    }

    /// Every screen asset whose last load failed.
    pub fn iter(&self) -> impl Iterator<Item = AssetId<ScreenDef>> + '_ {
        self.0.iter().copied()
    }

    /// How many are failing.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether every watched screen asset last loaded cleanly.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// `SlottedUiSet::Render`: says once, per screen file, that its last edit did
/// not load and that the definition already registered is what is still being
/// drawn.
///
/// Bevy's asset server logs a load failure of its own, but from an IO thread,
/// with nothing about screens in it. This is the line that tells the author
/// what it cost them: the screen on screen is the last good one, so an edit
/// that appears to do nothing has in fact not been read.
pub fn report_failed_screen_assets(
    assets: Res<ScreenAssets>,
    server: Res<AssetServer>,
    mut failed: ResMut<FailedScreenAssets>,
) {
    for handle in &assets.0 {
        let id = handle.id();
        match server.load_state(id) {
            bevy::asset::LoadState::Failed(error) => {
                if failed.0.insert(id) {
                    let path = server
                        .get_path(id)
                        .map_or_else(|| format!("{id:?}"), |path| path.to_string());
                    tracing::warn!(
                        screen = %path,
                        %error,
                        "a screen file did not load; the screen keeps the last \
                         definition that did, so this edit changed nothing"
                    );
                }
            }
            bevy::asset::LoadState::Loaded => {
                failed.0.remove(&id);
            }
            _ => {}
        }
    }
}

/// Registers the asset, the loader, [`ScreenAssets`] and
/// [`apply_screen_assets`].
///
/// A no-op when the app has no `AssetPlugin`: `init_asset` needs an
/// `AssetServer`, and the headless harnesses that skip Bevy's asset plumbing
/// still want the rest of `SlottedUiPlugin`.
pub(crate) fn register(app: &mut App) {
    if !app.world().contains_resource::<AssetServer>() {
        tracing::debug!("no AssetPlugin; `*.screen.ron` loading is off");
        return;
    }
    app.init_asset::<ScreenDef>()
        .register_asset_loader(ScreenLoader)
        .init_resource::<ScreenAssets>()
        .init_resource::<FailedScreenAssets>()
        .add_systems(
            Update,
            (apply_screen_assets, report_failed_screen_assets)
                .in_set(crate::plugin::SlottedUiSet::Render),
        );
}
