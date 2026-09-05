//! The 3D scene, the chest screen and the in-canvas console.
//!
//! The same demo `examples/modded` shows in a window: a ring of spinning
//! cubes under an orbiting camera, the copper chest screen the `copper_chest`
//! mod registered, the `Sort` button `sorter` injected into it, and the
//! tooltip line `appleskin_like` adds. Nothing here registers an item, a
//! recipe or a screen; all of it comes out of the bundled mods' Lua.

use std::sync::Arc;

use bevy::prelude::*;
use slotted::browser::{BrowserPhase, DefaultScreenHandler, ScreenHandlers};
use slotted::ecs::MenuIdAllocator;
use slotted::prelude::*;
use slotted_model::{Inventory, ItemStack, MenuDef, Namespaced};
use slotted_packs::{LogEntry, ModFailed, ScriptLogs};
use slotted_registry::FrozenRegistries;
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
            .add_systems(Startup, (setup_scene, setup_theme))
            .add_systems(
                Startup,
                register_browser_handler.in_set(BrowserPhase::ScreenHandlers),
            )
            .add_systems(Startup, open_chest.after(register_browser_handler))
            .add_systems(
                Update,
                (
                    orbit_camera,
                    spin_cubes,
                    (collect_mod_errors, toggle_console, rebuild_console).chain(),
                ),
            );
    }
}

/// Marks the camera the UI follows.
#[derive(Component)]
struct MainCamera;

/// Rotation speed of a demo cube.
#[derive(Component)]
struct Spin(f32);

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(60.0, 60.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.10, 0.12, 0.16),
            perceptual_roughness: 0.9,
            ..default()
        })),
    ));

    let cube = meshes.add(Cuboid::new(1.6, 1.6, 1.6));
    let palette = [
        Color::srgb(0.90, 0.55, 0.30),
        Color::srgb(0.32, 0.72, 0.95),
        Color::srgb(0.98, 0.75, 0.28),
        Color::srgb(0.45, 0.88, 0.55),
        Color::srgb(0.78, 0.45, 0.95),
        Color::srgb(0.98, 0.42, 0.36),
    ];
    #[allow(clippy::cast_precision_loss)]
    for (i, color) in palette.into_iter().enumerate() {
        let angle = i as f32 / palette.len() as f32 * std::f32::consts::TAU;
        commands.spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: color,
                perceptual_roughness: 0.35,
                metallic: 0.1,
                ..default()
            })),
            Transform::from_xyz(
                angle.cos() * 5.0,
                0.8 + (i as f32 * 0.35),
                angle.sin() * 5.0,
            )
            .with_rotation(Quat::from_rotation_y(angle)),
            Spin(0.4 + i as f32 * 0.1),
        ));
    }

    commands.spawn((
        DirectionalLight {
            illuminance: 12_000.0,
            // Shadow maps are the first thing to cost real time on WebGL2 and
            // the demo reads the same without them.
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(6.0, 12.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 6.0, 14.0).looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y),
        MainCamera,
        IsDefaultUiCamera,
    ));
}

/// The theme comes from the shared `assets/`, fetched over HTTP beside the
/// page rather than through `pack://`.
fn setup_theme(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(ActiveTheme(assets.load("themes/glass.theme.ron")));
}

fn orbit_camera(time: Res<Time>, mut cameras: Query<&mut Transform, With<MainCamera>>) {
    let t = time.elapsed_secs() * 0.18;
    for mut transform in &mut cameras {
        *transform = Transform::from_xyz(t.sin() * 14.0, 6.0, t.cos() * 14.0)
            .looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y);
    }
}

fn spin_cubes(time: Res<Time>, mut cubes: Query<(&mut Transform, &Spin)>) {
    for (mut transform, spin) in &mut cubes {
        transform.rotate_y(spin.0 * time.delta_secs());
        transform.rotate_x(spin.0 * 0.4 * time.delta_secs());
    }
}

// ---------------------------------------------------------------------------
// The menu
// ---------------------------------------------------------------------------

/// The menu behind the screen: container, player main, hotbar.
pub fn menu_def() -> Arc<MenuDef> {
    Arc::new(MenuDef::chest(ROWS))
}

struct Row {
    inventory: usize,
    slot: usize,
    item: &'static str,
    count: u32,
}

const fn row(inventory: usize, slot: usize, item: &'static str, count: u32) -> Row {
    Row {
        inventory,
        slot,
        item,
        count,
    }
}

const CONTAINER: usize = 0;
const MAIN: usize = 1;
const HOTBAR: usize = 2;

/// What the chest starts with. Every id was registered by a mod's `data.lua`.
const CONTENTS: &[Row] = &[
    row(CONTAINER, 0, "copper_chest:copper_chest", 4),
    row(CONTAINER, 1, "copper_chest:copper_ingot", 64),
    row(CONTAINER, 2, "copper_chest:copper_ingot", 31),
    row(CONTAINER, 5, "demo:apple", 12),
    row(CONTAINER, 11, "copper_chest:copper_chest", 1),
    row(MAIN, 0, "copper_chest:copper_ingot", 8),
    row(MAIN, 7, "demo:apple", 3),
    row(HOTBAR, 0, "copper_chest:copper_chest", 2),
    row(HOTBAR, 2, "demo:apple", 16),
];

/// The demo's inventories. Rows naming an unregistered id are skipped with a
/// warning rather than a panic: in a browser tab a panic is a blank canvas and
/// no way to find out why.
pub fn inventories(registries: &FrozenRegistries) -> Vec<Inventory> {
    let mut out: Vec<Inventory> = menu_def()
        .inventory_sizes()
        .into_iter()
        .map(Inventory::new)
        .collect();
    for row in CONTENTS {
        let Ok(name) = Namespaced::parse(row.item) else {
            warn!("bad id {:?} in the demo table", row.item);
            continue;
        };
        let Some(id) = registries.item_id(&name) else {
            warn!("{name} was not registered by any mod");
            continue;
        };
        out[row.inventory].set(row.slot, Some(ItemStack::new(id, row.count)));
    }
    out
}

fn register_browser_handler(mut handlers: ResMut<ScreenHandlers>) {
    handlers.register(
        ScreenKind::new(CHEST_SCREEN),
        Arc::new(DefaultScreenHandler),
    );
}

/// `Startup`: spawn the inventories, open the menu, spawn the mod's screen.
fn open_chest(
    mut commands: Commands,
    mut ids: ResMut<MenuIdAllocator>,
    registries: Option<Res<Registries>>,
    screens: Res<Screens>,
) {
    let Some(registries) = registries else { return };
    let kind = ScreenKind::new(CHEST_SCREEN);
    let Some(def) = screens.get(&kind).cloned() else {
        error!("{CHEST_SCREEN} is not registered: did the bundled data.lua run?");
        return;
    };
    let entities: Vec<Entity> = inventories(&registries)
        .into_iter()
        .map(|inventory| {
            commands
                .spawn(slotted::ecs::menu::Inventory(inventory))
                .id()
        })
        .collect();
    let menu = open_menu(
        &mut commands,
        &mut ids,
        menu_def(),
        entities,
        slotted_model::Actor::SURVIVAL,
    );
    spawn_screen(&mut commands, def, Some(menu));
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
