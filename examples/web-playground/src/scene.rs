//! The 3D backdrop, the theme, and the in-canvas console.
//!
//! What is left here after the showcase: the page furniture every scene shares.
//! The ring of spinning cubes under an orbiting camera is
//! [`showcase::backdrop`]'s, spawned once and never torn down, so the orbit
//! carries on across a scene switch and it is visible that the page did not
//! reload. The console overlay is this crate's own, because it belongs to the
//! page rather than to any scene.
//!
//! What used to be here -- the copper chest's menu, its contents table and the
//! `Startup` system that opened it -- moved to `showcase::mods` and
//! `scenes::mods`, which is where a scene's screen belongs now that there are
//! eight of them (docs/design/showcase-contract.md section 3.4).

use bevy::prelude::*;
use slotted::browser::BrowserPhase;
use slotted::prelude::*;
use slotted_packs::{LogEntry, ModFailed, ScriptLogs};
use slotted_script::LogLevel;

/// The screen kind `copper_chest/data.lua` registers.
pub const CHEST_SCREEN: &str = "copper_chest:chest";

/// Rows of nine in the container.
pub const ROWS: u16 = 3;

/// The scene, the screen and the console overlay.
#[derive(Debug, Default, Clone, Copy)]
pub struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ConsoleVisible>()
            .init_resource::<ConsoleErrors>()
            .insert_resource(showcase::backdrop::BackdropConfig::web())
            .add_plugins(showcase::backdrop::BackdropPlugin)
            .add_systems(
                Startup,
                (
                    setup_theme,
                    register_bundled_screens.before(BrowserPhase::ScreenHandlers),
                ),
            )
            .add_systems(
                Update,
                (collect_mod_errors, toggle_console, rebuild_console).chain(),
            );
    }
}

/// `Startup`, before the browser validates its handlers: the two screens this
/// crate compiles in.
///
/// They are registered for the app's life rather than by the scene that opens
/// them, because a `ScreenHandler` may not name a screen kind the registry has
/// never heard of -- `slotted-browser` validates that and, in a debug build,
/// panics on it. `ChestDemoPlugin` registers such a handler at `Startup`, long
/// before the Browser scene is entered, so the definitions have to be there
/// first. A scene still re-registers on `enter`, which is what picks up a mod
/// reload that replaced one.
fn register_bundled_screens(mut screens: ResMut<Screens>) {
    screens.register(showcase::chest::screen());
    screens.register(showcase::machine::screen());
}

/// The theme comes from the shared `assets/`, fetched over HTTP beside the
/// page rather than through `pack://`. The Themes scene replaces this
/// resource; `glass` is what the page opens on.
fn setup_theme(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(ActiveTheme(assets.load("themes/glass.theme.ron")));
}

// ---------------------------------------------------------------------------
// The in-canvas console
// ---------------------------------------------------------------------------

/// Whether the overlay shows. `F1` toggles it.
///
/// It starts visible natively and hidden in a browser, because the page has
/// its own console pane beside the canvas and a second copy of the same lines
/// over the game only covers the chest. See
/// [`canvas_console`](crate::canvas_console) for the two ways a page with no
/// pane of its own asks for it back.
#[derive(Resource, Debug, Clone, Copy)]
pub struct ConsoleVisible(pub bool);

impl Default for ConsoleVisible {
    fn default() -> Self {
        Self(crate::canvas_console::starts_visible())
    }
}

/// Marker on the console root.
#[derive(Component, Debug, Clone, Copy)]
pub struct ConsoleRoot;

/// Loader failures, kept because a `ModFailed` message lives two frames.
#[derive(Resource, Debug, Default, Clone)]
pub struct ConsoleErrors(pub Vec<String>);

/// Lines the overlay shows at most.
pub const CONSOLE_LINES: usize = 10;

fn collect_mod_errors(mut reader: MessageReader<ModFailed>, mut errors: ResMut<ConsoleErrors>) {
    for failure in reader.read() {
        let who = failure
            .mod_id
            .as_ref()
            .map_or_else(|| "loader".to_owned(), ToString::to_string);
        errors.0.push(format!("{who}: {}", failure.error));
    }
}

fn toggle_console(keys: Res<ButtonInput<KeyCode>>, mut visible: ResMut<ConsoleVisible>) {
    if keys.just_pressed(KeyCode::F1) {
        visible.0 = !visible.0;
    }
}

fn level_colour(level: LogLevel) -> Color {
    match level {
        LogLevel::Trace | LogLevel::Debug => Color::srgb(0.55, 0.58, 0.64),
        LogLevel::Info => Color::srgb(0.82, 0.86, 0.92),
        LogLevel::Warn => Color::srgb(0.98, 0.78, 0.35),
        LogLevel::Error => Color::srgb(0.98, 0.42, 0.42),
    }
}

fn console_lines(logs: &ScriptLogs, errors: &ConsoleErrors) -> Vec<(String, Color)> {
    let error_colour = level_colour(LogLevel::Error);
    let mut lines: Vec<(String, Color)> = errors
        .0
        .iter()
        .map(|message| (message.clone(), error_colour))
        .collect();
    lines.extend(logs.entries.iter().map(|entry: &LogEntry| {
        let who = entry
            .mod_id
            .as_ref()
            .map_or_else(|| "-".to_owned(), ToString::to_string);
        (
            format!("[{who}] {}", entry.message),
            level_colour(entry.level),
        )
    }));
    if lines.len() > CONSOLE_LINES {
        lines.drain(..lines.len() - CONSOLE_LINES);
    }
    lines
}

fn rebuild_console(
    logs: Res<ScriptLogs>,
    errors: Res<ConsoleErrors>,
    visible: Res<ConsoleVisible>,
    mut commands: Commands,
    roots: Query<Entity, With<ConsoleRoot>>,
) {
    if !(logs.is_changed() || errors.is_changed() || visible.is_changed()) {
        return;
    }
    for root in &roots {
        commands.entity(root).despawn();
    }
    if !visible.0 {
        return;
    }
    let lines = console_lines(&logs, &errors);
    let root = commands
        .spawn((
            ConsoleRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(12.0),
                bottom: Val::Px(12.0),
                width: Val::Px(560.0),
                max_height: Val::Px(180.0),
                padding: UiRect::all(Val::Px(8.0)),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexEnd,
                row_gap: Val::Px(2.0),
                overflow: Overflow::clip(),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.04, 0.05, 0.08, 0.82)),
            GlobalZIndex(zbands::DEV),
            Pickable::IGNORE,
        ))
        .id();
    for (text, colour) in lines {
        commands.spawn((
            Node::default(),
            Text::new(text),
            TextFont {
                font_size: bevy::text::FontSize::Px(12.0),
                ..default()
            },
            TextColor(colour),
            Pickable::IGNORE,
            ChildOf(root),
        ));
    }
}
