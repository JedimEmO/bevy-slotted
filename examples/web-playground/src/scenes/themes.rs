//! Scene 4: the theme switcher.
//!
//! `docs/design/showcase-contract.md` section 3.3. This is the one entry in
//! the rail that is not a scene on the canvas: selecting it keeps whatever is
//! already open and gives the page its three theme buttons, because "one
//! screen tree, three skins" is only worth looking at if the screen does not
//! change at the same moment the skin does.
//!
//! [`SceneHandler::overlays_current`] is how it says so, and
//! `crate::showcase::apply_scene_switch` is what honours it: the rail moves,
//! the canvas does not.

use bevy::prelude::*;

use crate::showcase::SceneHandler;

/// Scene 4.
pub struct ThemesScene;

impl SceneHandler for ThemesScene {
    fn overlays_current(&self) -> bool {
        true
    }

    fn enter(&self, _world: &mut World) {}

    fn leave(&self, _world: &mut World) {}
}

/// Themes the page may ask for, and the file each names under `assets/themes/`.
///
/// A fixed list rather than "whatever is in the directory": a browser tab
/// cannot list one, and an unknown name has to be a refusal the page can show
/// rather than an asset handle that never resolves.
pub const THEMES: [&str; 3] = ["glass", "paper", "neon"];

/// The asset path for a theme name, when it is one of [`THEMES`].
pub fn path_of(name: &str) -> Option<String> {
    THEMES
        .contains(&name)
        .then(|| format!("themes/{name}.theme.ron"))
}

/// Loads `name` and replaces [`ActiveTheme`](slotted::prelude::ActiveTheme).
///
/// Repainting the open screen is the theme system's own job: the materials and
/// tokens are resolved per frame from whatever handle this resource holds, so
/// nothing is respawned and a screen keeps its cursor, its focus ring and its
/// drag.
///
/// # Errors
///
/// A name that is not one of [`THEMES`].
pub fn apply(world: &mut World, name: &str) -> Result<(), String> {
    let path = path_of(name).ok_or_else(|| {
        format!(
            "`{name}` is not a bundled theme; the page has {}",
            THEMES.join(", ")
        )
    })?;
    let handle = world.resource::<AssetServer>().load(path);
    world.insert_resource(slotted::prelude::ActiveTheme(handle));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_bundled_themes_resolve_and_nothing_else_does() {
        assert_eq!(path_of("neon").as_deref(), Some("themes/neon.theme.ron"));
        assert_eq!(path_of("../../etc/passwd"), None);
        assert_eq!(path_of(""), None);
    }
}
