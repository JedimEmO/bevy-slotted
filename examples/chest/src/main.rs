//! The moodboard chest, in a window, over a 3D scene.
//!
//! ```text
//! cargo run -p chest                          play with it
//! cargo run -p chest -- --shot shots/chest.png     capture and exit
//! cargo run -p chest -- --hover 0 --shot shots/chest-hover.png
//! cargo run -p chest -- --paint --shot shots/chest-paint.png   a right-drag mid-paint
//! cargo run -p chest -- --cheat                  browser Ctrl+click gives
//! cargo run -p chest -- --recipe minecraft:coal --shot shots/chest-recipe.png
//! cargo run -p chest -- --settings --shot shots/chest-settings.png
//! cargo run -p chest -- --record session.ron       record input, replay it in a test
//! cargo run -p chest -- --theme paper              the same screen in another theme
//! ```
//!
//! What to try once it is up: left-click a stack to pick it up and right-click
//! to place one; shift-click to send a stack across; hold right and drag over
//! empty slots to paint one item into each; press `1` to `9` over a slot to
//! swap it with that hotbar slot; hover for a tooltip and hold shift to expand
//! it; Tab and the arrow keys move a focus ring; the rail on the right sorts
//! and moves stacks in bulk; `Esc` closes the screen and `E` opens it again.
//! `F7` toggles the HUD position editor: drag a HUD layer to move it, `Esc`
//! puts the one you are dragging back, and where you leave them is saved to
//! `examples/chest/hud_layout.ron` (git-ignored). With `--record <path>` every
//! pointer, key and gamepad input is written there on exit, and
//! `UiHarness::replay` feeds it back frame by frame.
//! The browser panel docks beside the chest: type in its search field, press
//! `R` over a card for its recipes, `U` for its uses, `A` to bookmark it, and
//! `Backspace` to go back. The `+` button stays disabled because a chest has
//! no crafting grid to fill. `Tab` opens the settings screen over the chest,
//! every M1 control on three tabs bound to a value store; `E` and `Q` switch
//! its tabs, `Esc` closes it.
//!
//! The scene, the orbit and the screenshot plumbing are lifted from
//! `spikes/glass-ui`. Everything else comes out of `lib.rs`, which the headless
//! tests use unchanged.

use std::path::PathBuf;

use bevy::camera::NormalizedRenderTarget;
use bevy::picking::pointer::{Location, PointerAction, PointerId, PointerInput};
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::ui::ui_transform::UiGlobalTransform;
use bevy::window::{PrimaryWindow, WindowResolution};
use chest::{ChestBinding, ChestDemoPlugin, ChestSettingsPlugin};
use showcase::backdrop::{BackdropPlugin as SceneBackdropPlugin, MainCamera};
use slotted::prelude::*;
use slotted::theme::blur::{BackdropPlugin, BackdropSource};
use slotted_model::SlotIx;

const WIDTH: f32 = 1600.0;
const HEIGHT: f32 = 900.0;
/// How long the shot mode lets the scene settle before capturing.
const SHOT_AT: f32 = 2.0;

/// Command line. Absent flags mean "interactive, forever".
#[derive(Resource, Debug, Clone, Default)]
struct Cli {
    /// Capture one frame to this path and exit.
    shot: Option<PathBuf>,
    /// Park a synthetic mouse pointer over this menu slot.
    hover: Option<u16>,
    /// Pick a stack up and hold a right-drag over three empty slots, so a
    /// capture shows the phantom preview mid-paint.
    paint: bool,
    /// Open the chest as a player who may cheat, so the browser's Ctrl+click
    /// give is allowed.
    cheat: bool,
    /// Open the browser's recipe page for this item before capturing.
    recipe: Option<String>,
    /// Open the settings screen over the chest before capturing (menus M1).
    settings: bool,
    /// Record every input to this RON file, written on exit.
    record: Option<PathBuf>,
    /// Theme name: `glass` (default), `paper` or `neon`, loaded from
    /// `assets/themes/<name>.theme.ron`.
    theme: String,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let value = |flag: &str| args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone());
    let cli = Cli {
        shot: value("--shot").map(PathBuf::from),
        hover: value("--hover").and_then(|v| v.parse().ok()),
        paint: args.iter().any(|a| a == "--paint"),
        cheat: args.iter().any(|a| a == "--cheat"),
        recipe: value("--recipe"),
        settings: args.iter().any(|a| a == "--settings"),
        record: value("--record").map(PathBuf::from),
        theme: value("--theme").unwrap_or_else(|| "glass".to_owned()),
    };

    let registries = chest::load_registries();

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "slotted — chest".into(),
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    resolution: WindowResolution::new(WIDTH as u32, HEIGHT as u32)
                        .with_scale_factor_override(1.0),
                    ..default()
                }),
                ..default()
            })
            // Assets resolve against the workspace `assets/` directory, so the
            // binary runs from anywhere.
            .set(AssetPlugin {
                file_path: chest::assets_dir().to_string_lossy().into_owned(),
                ..default()
            }),
    )
    // `DefaultPlugins` carries `InputFocusPlugin`, `InputDispatchPlugin` and
    // the widget plugins, but not these two, and `slotted-ui`'s navigation
    // systems need their resources. The headless group adds them for you; a
    // windowed game has to say so itself (docs/FOLLOWUPS.md).
    .add_plugins((
        bevy::input_focus::tab_navigation::TabNavigationPlugin,
        bevy::input_focus::directional_navigation::DirectionalNavigationPlugin,
    ))
    // The ports go in before the plugin group, which is the documented way to
    // replace an adapter: here it is the frozen registries the data stage made.
    .insert_resource(Registries(registries))
    .add_plugins(SlottedPlugins::default())
    .add_plugins(BackdropPlugin {
        divisor: 4,
        clear_color: Color::srgb(0.043, 0.055, 0.078),
    })
    .add_plugins((ChestDemoPlugin, ChestSettingsPlugin))
    .insert_resource(ClearColor(Color::srgb(0.043, 0.055, 0.078)))
    .insert_resource(chest::CheatMode(cli.cheat))
    // The HUD position editor: `F7` toggles it, and where the player leaves
    // the layers is written beside the example rather than into `assets/`,
    // because it is this player's layout and not the game's data.
    .insert_resource(slotted::ui::hud_editor::HudEditKey(KeyCode::F7))
    .insert_resource(slotted::ui::hud_editor::HudLayoutStore::file(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("hud_layout.ron"),
    ))
    .add_plugins(SceneBackdropPlugin)
    .add_systems(Startup, (setup_screen, mark_backdrop_source))
    .add_systems(
        Update,
        (
            open_chest_when_loaded,
            park_pointer,
            park_paint,
            open_recipe_page,
            open_settings_for_shot,
            shot_and_exit,
        ),
    );

    if let Some(path) = cli.record.clone() {
        info!("recording input to {}", path.display());
        app.insert_resource(slotted::ui::recording::InputRecorder::new(
            path,
            Vec2::new(WIDTH, HEIGHT),
            1.0,
            std::time::Duration::from_secs_f64(1.0 / 60.0),
        ));
    }
    app.insert_resource(cli);

    app.run();
}

/// `Startup`, after [`showcase::backdrop::setup_backdrop`]: the blur pass
/// copies this camera's transform, so the image behind the panel lines up
/// with the world around it. The marker lives behind the facade's `blur`
/// feature, which the shared backdrop crate does not enable, so the example
/// that wants it says so itself.
fn mark_backdrop_source(mut commands: Commands, cameras: Query<Entity, With<MainCamera>>) {
    for camera in &cameras {
        commands.entity(camera).insert(BackdropSource);
    }
}

/// Loads the theme and the screen, both through the `AssetServer`.
///
/// The screen goes through `ScreenLoader` rather than `std::fs` so the
/// running example hot-reloads: saving `assets/screens/demo_chest.screen.ron`
/// re-registers the definition and respawns the open chest on the same menu.
/// (`chest::demo_screen` still reads the same file with `std::fs`; the tests
/// use it to skip the asset server entirely.) The settings screen is loaded
/// the same way, over the compiled-in copy `SettingsDemoPlugin` registered.
fn setup_screen(
    mut commands: Commands,
    assets: Res<AssetServer>,
    cli: Res<Cli>,
    mut screen_assets: ResMut<slotted::ui::ScreenAssets>,
) {
    commands.insert_resource(ActiveTheme(
        assets.load(format!("themes/{}.theme.ron", cli.theme)),
    ));
    screen_assets.load(&assets, chest::SCREEN_PATH);
}

/// Opens the chest the first frame `demo:chest` is registered, which is the
/// frame `ScreenLoader` finished reading the file.
fn open_chest_when_loaded(
    mut commands: Commands,
    screens: Res<Screens>,
    registries: Res<Registries>,
    cheat: Res<chest::CheatMode>,
    mut opened: Local<bool>,
) {
    if *opened || screens.get(&ScreenKind::new(chest::CHEST)).is_none() {
        return;
    }
    *opened = true;
    chest::open_chest(&mut commands, &registries, cheat.0);
}

/// Drives the real picking backend from a synthetic pointer so `--hover` can
/// take a tooltip screenshot without a human at the mouse.
fn park_pointer(
    cli: Res<Cli>,
    windows: Query<Entity, With<PrimaryWindow>>,
    slots: Query<(&SlotRef, &UiGlobalTransform)>,
    mut out: MessageWriter<PointerInput>,
) {
    let Some(target) = cli.hover else { return };
    let Ok(window) = windows.single() else { return };
    let Some(normalized) = bevy::window::WindowRef::Primary.normalize(Some(window)) else {
        return;
    };
    for (slot, transform) in &slots {
        if slot.slot != SlotIx(target) {
            continue;
        }
        out.write(PointerInput::new(
            PointerId::Mouse,
            Location {
                target: NormalizedRenderTarget::Window(normalized),
                position: transform.translation,
            },
            PointerAction::Move { delta: Vec2::ZERO },
        ));
    }
}

/// `--paint`: pick the fullest chest stack up and hold a right-drag across
/// three empty slots, one synthetic pointer event per frame, leaving the
/// button down. What a capture then shows is a paint in progress: three
/// phantoms with `+1` and a cursor that has already counted itself down.
#[allow(clippy::too_many_arguments, clippy::similar_names)]
fn park_paint(
    cli: Res<Cli>,
    time: Res<Time>,
    windows: Query<Entity, With<PrimaryWindow>>,
    slots: Query<(&SlotRef, &UiGlobalTransform, &slotted::ui::ItemView)>,
    mut out: MessageWriter<PointerInput>,
    mut plan: Local<Vec<PointerAction>>,
    mut spots: Local<Vec<Vec2>>,
    mut step: Local<usize>,
) {
    if !cli.paint || time.elapsed_secs() < SHOT_AT * 0.4 {
        return;
    }
    let Ok(window) = windows.single() else { return };
    let Some(normalized) = bevy::window::WindowRef::Primary.normalize(Some(window)) else {
        return;
    };
    if plan.is_empty() {
        // The fullest container slot is what we pick up; the first three
        // empty ones are what we paint into.
        let mut filled: Vec<(u32, Vec2)> = Vec::new();
        let mut empty: Vec<(u16, Vec2)> = Vec::new();
        for (slot, transform, view) in &slots {
            if slot.slot.0 >= 27 {
                continue;
            }
            match view.stack.as_ref() {
                Some(stack) => filled.push((stack.count, transform.translation)),
                None => empty.push((slot.slot.0, transform.translation)),
            }
        }
        filled.sort_by_key(|(count, _)| std::cmp::Reverse(*count));
        empty.sort_by_key(|(ix, _)| *ix);
        let (Some((_, source)), true) = (filled.first(), empty.len() >= 3) else {
            warn!("--paint needs a full slot and three empty ones");
            return;
        };
        *spots = vec![*source, empty[0].1, empty[1].1, empty[2].1];
        // The deltas are real: `bevy_picking` reads a zero-delta move as the
        // pointer standing still and never calls it a drag.
        let step = |from: usize, to: usize| PointerAction::Move {
            delta: spots[to] - spots[from],
        };
        *plan = vec![
            PointerAction::Move { delta: Vec2::ZERO },
            PointerAction::Press(bevy::picking::pointer::PointerButton::Primary),
            PointerAction::Release(bevy::picking::pointer::PointerButton::Primary),
            step(0, 1),
            PointerAction::Press(bevy::picking::pointer::PointerButton::Secondary),
            step(1, 2),
            step(2, 3),
        ];
    }
    let Some(action) = plan.get(*step).copied() else {
        return;
    };
    // Which spot the pointer is at for each step of the plan above.
    let position = spots[match *step {
        0..=2 => 0,
        3 | 4 => 1,
        5 => 2,
        _ => 3,
    }];
    out.write(PointerInput::new(
        PointerId::Mouse,
        Location {
            target: NormalizedRenderTarget::Window(normalized),
            position,
        },
        action,
    ));
    *step += 1;
}

/// `--recipe <ns:item>`: open the browser's recipe page once, before the shot.
fn open_recipe_page(
    cli: Res<Cli>,
    time: Res<Time>,
    registries: Res<Registries>,
    mut out: MessageWriter<slotted::browser::OpenRecipes>,
    mut done: Local<bool>,
) {
    let Some(name) = cli.recipe.clone() else {
        return;
    };
    if *done || time.elapsed_secs() < SHOT_AT * 0.5 {
        return;
    }
    *done = true;
    let Ok(id) = slotted_model::Namespaced::parse(&name) else {
        warn!("--recipe {name} is not a namespaced id");
        return;
    };
    let Some(item) = registries.item_id(&id) else {
        warn!("--recipe {name} is not a registered item");
        return;
    };
    out.write(slotted::browser::OpenRecipes(
        slotted::browser::Ingredient::item(item),
    ));
}

/// `--settings`: push the settings screen once, before the shot, and put the
/// player on the keyboard so the focus ring shows.
fn open_settings_for_shot(
    cli: Res<Cli>,
    time: Res<Time>,
    mut mode: ResMut<slotted::ui::InputMode>,
    mut commands: Commands,
    mut done: Local<bool>,
) {
    if !cli.settings || *done || time.elapsed_secs() < SHOT_AT * 0.5 {
        return;
    }
    *done = true;
    *mode = slotted::ui::InputMode::Keyboard;
    showcase::settings::open_settings(&mut commands);
}

/// `--shot`: capture at [`SHOT_AT`] seconds, then leave.
fn shot_and_exit(
    mut commands: Commands,
    time: Res<Time>,
    cli: Res<Cli>,
    binding: Res<ChestBinding>,
    mut exit: MessageWriter<AppExit>,
    mut taken: Local<bool>,
) {
    let Some(path) = cli.shot.clone() else { return };
    if time.elapsed_secs() < SHOT_AT {
        return;
    }
    if !*taken {
        *taken = true;
        if binding.open.is_none() {
            warn!("capturing with no chest screen open");
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        info!("capturing {}", path.display());
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
        return;
    }
    // One extra half-second so the screenshot reaches the disk.
    if time.elapsed_secs() > SHOT_AT + 0.5 {
        exit.write(AppExit::Success);
    }
}
