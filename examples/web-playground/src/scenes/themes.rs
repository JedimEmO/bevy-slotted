//! Scene 5: the theme switcher, over the settings screen.
//!
//! `docs/design/showcase-contract.md` section 3.3, amended by
//! `docs/design/showcase-refresh-contract.md` section 4.4. The scene opens
//! `demo:settings`, the same screen the Menus scene reaches from the pause,
//! because it is the one screen in the showcase with tabs, sliders, selects,
//! toggles and rich text on it at once, and a theme is worth looking at on
//! all of those rather than on slot borders. The three theme buttons stay
//! in the page's control block, and [`apply`] is what they call.
//!
//! A swap repaints the open screen in place: the theme system resolves
//! tokens and materials per frame from whatever handle `ActiveTheme` holds,
//! nothing is respawned, and the focused row and every control's value are
//! exactly where they were.

use bevy::prelude::*;
use slotted::prelude::*;

use crate::scenes;
use crate::showcase::{ActiveScene, Scene, SceneHandler};

/// Scene 5.
pub struct ThemesScene;

impl SceneHandler for ThemesScene {
    fn enter(&self, world: &mut World) {
        {
            let mut commands = world.commands();
            showcase::settings::open_settings(&mut commands);
        }
        world.flush();
    }

    fn leave(&self, world: &mut World) {
        scenes::teardown(world);
    }
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

/// Loads `name` and replaces [`ActiveTheme`].
///
/// Repainting the open screen is the theme system's own job: the materials and
/// tokens are resolved per frame from whatever handle this resource holds, so
/// nothing is respawned and a screen keeps its cursor, its focus ring and its
/// drag.
///
/// In the Themes scene with nothing open (`Esc` pops the settings screen
/// like any other), the swap opens it again first: a theme button is a
/// request to see the theme, and an empty canvas shows none of it.
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
    let in_themes = world
        .get_resource::<ActiveScene>()
        .is_some_and(|scene| scene.0 == Scene::Themes);
    if in_themes && world.resource::<ScreenStack>().top().is_none() {
        ThemesScene.enter(world);
    }
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
