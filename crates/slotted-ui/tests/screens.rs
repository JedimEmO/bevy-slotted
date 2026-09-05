//! Headless tests for screen spawning, the node-to-components mapping, input
//! translation, injection and theme application.
//!
//! The plugin list and the two workarounds (the camera's `target_info` and the
//! render-asset registrations) come from ADR 0002 and `spikes/headless-ui`.
//! This file deliberately does not depend on `slotted-test`: the harness is
//! being written against the same contract at the same time, and the point of
//! these tests is to check the contract from the other side.

use std::sync::Arc;
use std::time::Duration;

use bevy::app::{ScheduleRunnerPlugin, TaskPoolPlugin};
use bevy::asset::AssetPlugin;
use bevy::camera::{Camera, Camera2d, CameraPlugin, ComputedCameraValues, RenderTargetInfo};
use bevy::diagnostic::FrameCountPlugin;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::{ButtonState, InputPlugin};
use bevy::input_focus::directional_navigation::DirectionalNavigationPlugin;
use bevy::input_focus::tab_navigation::TabNavigationPlugin;
use bevy::input_focus::{InputDispatchPlugin, InputFocusPlugin};
use bevy::picking::pointer::{
    Location, PointerAction, PointerButton, PointerId, PointerInput, PointerLocation,
};
use bevy::picking::{InteractionPlugin, PickingPlugin};
use bevy::prelude::*;
use bevy::time::{TimePlugin, TimeUpdateStrategy};
use bevy::transform::TransformPlugin;
use bevy::ui::UiPlugin;
use bevy::ui::ui_transform::UiGlobalTransform;
use bevy::window::{PrimaryWindow, WindowPlugin, WindowResolution};

use slotted_ecs::{
    Inventory, MenuIdAllocator, Modifiers, SlotClicked, SlotRef, SlottedEcsPlugin, open_menu,
};
use slotted_model::{Actor, Button as ModelButton, MenuDef, SlotIx};
use slotted_theme::{ActiveTheme, Theme, Themed, roles};
use slotted_ui::def::{AnchorId, LocKey, ScreenDef, ScreenKind, Tags, TextRole, UiNodeDef};
use slotted_ui::{
    ExclusionZone, Injection, Injections, ItemView, ScreenLayout, ScreenRoot, ScreenSpawned,
    Screens, SemanticRole, SlottedUiPlugin, TestId, spawn_screen,
};

const GLASS: &str = include_str!("../../../assets/themes/glass.theme.ron");
const WIDTH: f32 = 1280.0;
const HEIGHT: f32 = 720.0;

/// Records every `SlotClicked` the app triggers, in order.
#[derive(Resource, Default, Debug)]
struct Clicks(Vec<SlotClicked>);

/// Records screen lifecycle events, in order.
#[derive(Resource, Default, Debug)]
struct Lifecycle(Vec<&'static str>);

struct Harness {
    app: App,
    window: Entity,
    pointer_pos: Vec2,
}

impl Harness {
    fn new() -> Self {
        let mut app = App::new();
        app.add_plugins((
            TaskPoolPlugin::default(),
            FrameCountPlugin,
            TimePlugin,
            ScheduleRunnerPlugin::run_once(),
            TransformPlugin,
            AssetPlugin::default(),
            WindowPlugin {
                primary_window: Some(Window {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    resolution: WindowResolution::new(WIDTH as u32, HEIGHT as u32)
                        .with_scale_factor_override(1.0),
                    ..default()
                }),
                exit_condition: bevy::window::ExitCondition::DontExit,
                close_when_requested: false,
                ..default()
            },
            InputPlugin,
            bevy::a11y::AccessibilityPlugin,
            CameraPlugin,
        ));
        app.add_plugins((
            bevy::text::TextPlugin,
            UiPlugin,
            PickingPlugin,
            InteractionPlugin,
            InputFocusPlugin,
            InputDispatchPlugin,
            TabNavigationPlugin,
            DirectionalNavigationPlugin,
            bevy::ui_widgets::UiWidgetsPlugins,
        ));
        // Workaround 2: no RenderPlugin, so nothing registers these assets.
        app.init_asset::<Image>();
        app.init_asset::<bevy::image::TextureAtlasLayout>();
        app.init_asset::<bevy::mesh::Mesh>();
        app.init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>();

        app.add_plugins((
            SlottedEcsPlugin,
            slotted_theme::SlottedThemePlugin,
            SlottedUiPlugin::default(),
        ));
        app.init_resource::<Clicks>()
            .init_resource::<Lifecycle>()
            .add_observer(|click: On<SlotClicked>, mut out: ResMut<Clicks>| {
                out.0.push(*click);
            })
            .add_observer(|_: On<ScreenSpawned>, mut out: ResMut<Lifecycle>| {
                out.0.push("spawned");
            })
            .add_observer(|_: On<ScreenLayout>, mut out: ResMut<Lifecycle>| {
                out.0.push("layout");
            });

        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_micros(
            16_667,
        )));

        // Workaround 1: fill in the camera's render target by hand.
        app.world_mut().spawn((
            Camera2d,
            Camera {
                computed: ComputedCameraValues {
                    target_info: Some(RenderTargetInfo {
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        physical_size: UVec2::new(WIDTH as u32, HEIGHT as u32),
                        scale_factor: 1.0,
                    }),
                    ..default()
                },
                ..default()
            },
            IsDefaultUiCamera,
        ));
        // Workaround 3: nothing spawns a mouse pointer without winit, and
        // `update_is_hovered` only ever looks at `PointerId::Mouse`.
        app.world_mut()
            .spawn((PointerId::Mouse, PointerLocation::default()));

        let window = app
            .world_mut()
            .query_filtered::<Entity, With<PrimaryWindow>>()
            .single(app.world())
            .expect("WindowPlugin spawned a primary window");

        let mut h = Self {
            app,
            window,
            pointer_pos: Vec2::new(-1.0, -1.0),
        };
        h.load_theme();
        h.step(1);
        h
    }

    /// Adds the glass theme as an asset directly, so the test does not depend
    /// on the asset directory's location.
    fn load_theme(&mut self) {
        let theme = Theme::from_ron(GLASS).expect("glass theme parses");
        let handle = self
            .app
            .world_mut()
            .resource_mut::<Assets<Theme>>()
            .add(theme);
        self.app.world_mut().insert_resource(ActiveTheme(handle));
    }

    fn step(&mut self, frames: usize) {
        for _ in 0..frames {
            self.app.update();
        }
    }

    fn world(&self) -> &World {
        self.app.world()
    }

    fn center_of(&self, entity: Entity) -> Vec2 {
        let node = self
            .world()
            .get::<ComputedNode>(entity)
            .expect("entity is laid out");
        let tf = self
            .world()
            .get::<UiGlobalTransform>(entity)
            .expect("entity has a transform");
        tf.translation * node.inverse_scale_factor()
    }

    fn location(&self, pos: Vec2) -> Location {
        Location {
            target: bevy::camera::NormalizedRenderTarget::Window(
                bevy::window::WindowRef::Primary
                    .normalize(Some(self.window))
                    .expect("primary window"),
            ),
            position: pos,
        }
    }

    fn pointer_move_to(&mut self, pos: Vec2) {
        let delta = pos - self.pointer_pos;
        self.pointer_pos = pos;
        let loc = self.location(pos);
        self.app.world_mut().write_message(PointerInput::new(
            PointerId::Mouse,
            loc,
            PointerAction::Move { delta },
        ));
        self.step(1);
    }

    fn pointer_button(&mut self, button: PointerButton, press: bool) {
        let loc = self.location(self.pointer_pos);
        let action = if press {
            PointerAction::Press(button)
        } else {
            PointerAction::Release(button)
        };
        self.app
            .world_mut()
            .write_message(PointerInput::new(PointerId::Mouse, loc, action));
        self.step(1);
    }

    fn click(&mut self, entity: Entity, button: PointerButton) {
        let pos = self.center_of(entity);
        self.pointer_move_to(pos);
        self.pointer_button(button, true);
        self.pointer_button(button, false);
    }

    fn hold_key(&mut self, key_code: KeyCode) {
        let window = self.window;
        self.app.world_mut().write_message(KeyboardInput {
            key_code,
            logical_key: Key::Shift,
            state: ButtonState::Pressed,
            text: None,
            repeat: false,
            window,
        });
        self.step(1);
    }

    /// Opens `MenuDef::chest(3)` over three fresh inventories and spawns the
    /// chest screen against it.
    fn open_chest(&mut self, screen: ScreenDef) -> (Entity, Entity) {
        let def = Arc::new(MenuDef::chest(3));
        let sizes = def.inventory_sizes();
        let kind = screen.kind.clone();
        let def_arc = self
            .app
            .world_mut()
            .resource_mut::<Screens>()
            .register(screen);
        assert_eq!(def_arc.kind, kind);

        let world = self.app.world_mut();
        let inventories: Vec<Entity> = sizes
            .iter()
            .map(|n| world.spawn(Inventory::new(*n)).id())
            .collect();
        let mut ids = world
            .remove_resource::<MenuIdAllocator>()
            .unwrap_or_default();
        let (menu, screen_root) = {
            let mut commands = world.commands();
            let menu = open_menu(&mut commands, &mut ids, def, inventories, Actor::SURVIVAL);
            let screen_root = spawn_screen(&mut commands, def_arc, Some(menu));
            (menu, screen_root)
        };
        world.flush();
        world.insert_resource(ids);
        self.step(2);
        (menu, screen_root)
    }

    fn slots(&mut self) -> Vec<(Entity, SlotRef)> {
        let mut q = self.app.world_mut().query::<(Entity, &SlotRef)>();
        let mut out: Vec<(Entity, SlotRef)> =
            q.iter(self.app.world()).map(|(e, r)| (e, *r)).collect();
        out.sort_by_key(|(_, r)| r.slot.0);
        out
    }
}

/// The chest screen used by every test here: a title row with an anchor, a
/// 9x3 grid bound to inventory 0, a hotbar and an action rail.
fn chest_screen() -> ScreenDef {
    ScreenDef {
        kind: ScreenKind::new("demo:chest"),
        inherits: None,
        root: UiNodeDef::Panel {
            role: roles::PANEL,
            layout: slotted_ui::Layout {
                direction: slotted_ui::LayoutDirection::Column,
                gap: 1.0,
                padding: 2.0,
                width: None,
                height: None,
                center: false,
            },
            children: vec![
                UiNodeDef::Panel {
                    role: roles::PANEL,
                    layout: slotted_ui::Layout::default(),
                    children: vec![
                        UiNodeDef::Text {
                            key: LocKey("demo.chest.title".to_owned()),
                            style: TextRole::Title,
                            tags: Tags::new().with("test_id", "title"),
                        },
                        UiNodeDef::Anchor {
                            id: AnchorId::new("title_end"),
                        },
                    ],
                    tags: Tags::new(),
                },
                UiNodeDef::SlotGrid {
                    inventory: slotted_model::InventoryRef::new(0),
                    cols: 9,
                    rows: 3,
                    first: 0,
                    tags: Tags::new().with("region", "chest").with("test_id", "chest"),
                },
                UiNodeDef::Custom {
                    kind: slotted_ui::widgets::kinds::action_rail(),
                    params: slotted_registry::Value::Unit,
                    children: Vec::new(),
                    tags: Tags::new(),
                },
            ],
            tags: Tags::new().with("test_id", "root_panel"),
        },
        listring: vec![],
    }
}

#[test]
fn chest_screen_spawns_the_contracted_tree() {
    let mut h = Harness::new();
    let (menu, root) = h.open_chest(chest_screen());

    let screen = h
        .world()
        .get::<ScreenRoot>(root)
        .expect("root carries ScreenRoot");
    assert_eq!(screen.kind, ScreenKind::new("demo:chest"));
    assert_eq!(screen.menu, Some(menu));
    assert_eq!(
        h.world().get::<SemanticRole>(root),
        Some(&SemanticRole::Screen)
    );

    let slots = h.slots();
    assert_eq!(slots.len(), 27, "9x3 slots");
    for (i, (entity, slot_ref)) in slots.iter().enumerate() {
        assert_eq!(slot_ref.menu, menu);
        #[allow(clippy::cast_possible_truncation)]
        let expected = SlotIx(i as u16);
        assert_eq!(slot_ref.slot, expected, "slots are row-major from `first`");
        assert_eq!(
            h.world().get::<SemanticRole>(*entity),
            Some(&SemanticRole::Slot)
        );
        assert_eq!(
            h.world().get::<Themed>(*entity),
            Some(&Themed(roles::SLOT)),
            "an unhovered slot is themed `slot`"
        );
        assert!(h.world().get::<ItemView>(*entity).is_some());
        assert!(h.world().get::<bevy::ui_widgets::Button>(*entity).is_some());
        assert!(
            h.world()
                .get::<bevy::input_focus::tab_navigation::TabIndex>(*entity)
                .is_some()
        );
        assert_eq!(
            h.world()
                .get::<Tags>(*entity)
                .and_then(|t| t.get("region").map(ToOwned::to_owned)),
            Some("chest".to_owned()),
            "grid cells inherit the grid's tags"
        );
    }

    // The `test_id` tag becomes a `TestId` component.
    let mut q = h.app.world_mut().query::<(&TestId, &SemanticRole)>();
    let ids: Vec<String> = q.iter(h.app.world()).map(|(id, _)| id.0.clone()).collect();
    assert!(ids.contains(&"root_panel".to_owned()), "{ids:?}");
    assert!(ids.contains(&"title".to_owned()), "{ids:?}");

    // The action rail spawned one button per default action.
    let mut rails = h.app.world_mut().query::<&slotted_ui::RailAction>();
    assert_eq!(rails.iter(h.app.world()).count(), 4);
}

#[test]
fn screen_spawned_then_layout_fire_once_each() {
    let mut h = Harness::new();
    let _ = h.open_chest(chest_screen());
    h.step(3);
    let events = &h.world().resource::<Lifecycle>().0;
    assert_eq!(
        events.as_slice(),
        ["spawned", "layout"],
        "layout follows spawn, and each fires once"
    );
}

#[test]
fn a_pointer_click_on_a_slot_emits_slot_clicked() {
    let mut h = Harness::new();
    let (menu, _) = h.open_chest(chest_screen());
    let (slot, slot_ref) = h.slots()[4];
    assert_eq!(slot_ref.slot, SlotIx(4));

    h.click(slot, PointerButton::Primary);
    let clicks = &h.world().resource::<Clicks>().0;
    assert_eq!(clicks.len(), 1, "one click, one event: {clicks:?}");
    assert_eq!(clicks[0].entity, slot);
    assert_eq!(clicks[0].button, ModelButton::Left);
    assert_eq!(clicks[0].modifiers, Modifiers::NONE);
    assert_eq!(
        h.world().get::<SlotRef>(clicks[0].entity).copied(),
        Some(SlotRef {
            menu,
            slot: SlotIx(4)
        })
    );
}

#[test]
fn a_right_click_carries_the_button_and_shift_carries_the_modifier() {
    let mut h = Harness::new();
    let _ = h.open_chest(chest_screen());
    let (slot, _) = h.slots()[0];

    h.hold_key(KeyCode::ShiftLeft);
    h.click(slot, PointerButton::Secondary);

    let clicks = &h.world().resource::<Clicks>().0;
    assert_eq!(clicks.len(), 1, "{clicks:?}");
    assert_eq!(clicks[0].button, ModelButton::Right);
    assert_eq!(clicks[0].modifiers, Modifiers::SHIFT);
}

#[test]
fn hovering_a_slot_swaps_its_theme_role() {
    let mut h = Harness::new();
    let _ = h.open_chest(chest_screen());
    let (slot, _) = h.slots()[10];

    let pos = h.center_of(slot);
    h.pointer_move_to(pos);
    h.step(1);
    assert_eq!(
        h.world().get::<Themed>(slot),
        Some(&Themed(roles::SLOT_HOVER))
    );

    h.pointer_move_to(Vec2::new(2.0, 2.0));
    h.step(1);
    assert_eq!(h.world().get::<Themed>(slot), Some(&Themed(roles::SLOT)));
}

#[test]
fn theme_application_paints_background_from_the_role() {
    let mut h = Harness::new();
    let _ = h.open_chest(chest_screen());
    let (slot, _) = h.slots()[0];
    h.step(1);

    let expected = Color::Srgba(bevy::color::Srgba::hex("141A24B8").expect("hex"));
    assert_eq!(
        h.world().get::<BackgroundColor>(slot).map(|b| b.0),
        Some(expected),
        "the `slot` role's fill"
    );
    let border = h
        .world()
        .get::<BorderColor>(slot)
        .expect("slot has a border colour");
    assert_eq!(
        border.top,
        Color::Srgba(bevy::color::Srgba::hex("2A34468C").expect("hex"))
    );
    assert_eq!(
        h.world()
            .get::<Node>(slot)
            .map(|n| n.border_radius.top_left),
        Some(Val::Px(8.0))
    );
}

#[test]
fn an_injection_at_an_anchor_adds_a_child_and_an_exclusion_zone() {
    let mut h = Harness::new();
    h.app
        .world_mut()
        .resource_mut::<Injections>()
        .0
        .push(Injection {
            target: ScreenKind::new("demo:chest"),
            anchor: AnchorId::new("title_end"),
            node: UiNodeDef::Text {
                key: LocKey("injected".to_owned()),
                style: TextRole::Muted,
                tags: Tags::new().with("test_id", "injected"),
            },
            exclusion: true,
        });
    let _ = h.open_chest(chest_screen());

    let mut q = h
        .app
        .world_mut()
        .query::<(Entity, &TestId, &ChildOf, &SemanticRole)>();
    let (entity, _, parent, role) = q
        .iter(h.app.world())
        .find(|(_, id, _, _)| id.0 == "injected")
        .expect("the injected node exists");
    assert_eq!(role, &SemanticRole::Text);
    assert!(
        h.world().get::<ExclusionZone>(entity).is_some(),
        "an injection asking for one is an exclusion zone"
    );
    assert_eq!(
        h.world()
            .get::<SemanticRole>(parent.parent())
            .expect("the parent is the anchor"),
        &SemanticRole::Anchor
    );

    let root = {
        let mut roots = h
            .app
            .world_mut()
            .query_filtered::<Entity, With<ScreenRoot>>();
        roots.iter(h.app.world()).next().expect("a screen root")
    };
    let zones: Vec<Rect> = {
        let mut state =
            bevy::ecs::system::SystemState::<slotted_ui::Exclusions>::new(h.app.world_mut());
        let exclusions = state
            .get(h.app.world())
            .expect("Exclusions is a valid param");
        exclusions.union(root)
    };
    assert_eq!(zones.len(), 1, "one exclusion zone under the screen");
}

#[test]
fn tweens_advance_with_the_virtual_clock_the_app_is_stepped_by() {
    use slotted_theme::{ActiveMotions, Tween, TweenTarget};

    let mut h = Harness::new();
    let entity = h
        .app
        .world_mut()
        .spawn((
            Node::default(),
            Tween::new(
                TweenTarget::Scale { from: 1.0, to: 2.0 },
                Duration::from_millis(100),
            ),
        ))
        .id();

    // Six frames of 16.667ms is 100ms, so the tween ends exactly then.
    h.step(5);
    assert_eq!(h.world().resource::<ActiveMotions>().0, 1);
    h.step(1);
    assert_eq!(h.world().resource::<ActiveMotions>().0, 0);
    assert!(h.world().get::<Tween>(entity).is_none());
    let scale = h
        .world()
        .get::<bevy::ui::ui_transform::UiTransform>(entity)
        .expect("the tween wrote a transform")
        .scale;
    assert!((scale.x - 2.0).abs() < 1e-5, "{scale:?}");
}

#[test]
fn a_hotbar_is_a_nine_by_one_grid_with_hotbar_semantics() {
    let mut h = Harness::new();
    let mut screen = chest_screen();
    if let Some(children) = screen.root.children_mut() {
        children.push(UiNodeDef::Custom {
            kind: slotted_ui::widgets::kinds::hotbar(),
            params: ron::from_str("(first: 54, inventory: 2)").expect("params parse"),
            children: Vec::new(),
            tags: Tags::new().with("test_id", "hotbar"),
        });
    }
    let _ = h.open_chest(screen);

    let hotbar = {
        let mut q = h
            .app
            .world_mut()
            .query::<(Entity, &SemanticRole, &TestId)>();
        q.iter(h.app.world())
            .find(|(_, role, _)| **role == SemanticRole::Hotbar)
            .map(|(e, _, id)| (e, id.0.clone()))
            .expect("a hotbar node")
    };
    assert_eq!(hotbar.1, "hotbar");
    let children = h
        .world()
        .get::<Children>(hotbar.0)
        .expect("the hotbar has cells");
    assert_eq!(children.len(), 9);
    let first = h
        .world()
        .get::<SlotRef>(children[0])
        .expect("cells are slots");
    assert_eq!(first.slot, SlotIx(54));
    assert_eq!(
        h.world()
            .get::<Tags>(children[0])
            .and_then(|t| t.get("region").map(ToOwned::to_owned)),
        Some("hotbar".to_owned())
    );
    // 27 chest cells plus 9 hotbar cells.
    assert_eq!(h.slots().len(), 36);
}

#[test]
fn an_item_view_renders_a_count_and_a_semantic_label() {
    let mut h = Harness::new();
    let _ = h.open_chest(chest_screen());
    let (slot, _) = h.slots()[0];

    h.app
        .world_mut()
        .entity_mut(slot)
        .insert(ItemView::new(Some(slotted_model::ItemStack::new(
            slotted_model::ItemId(3),
            17,
        ))));
    h.step(2);

    let children = h.world().get::<Children>(slot).expect("slot children");
    let count = children
        .iter()
        .find(|c| h.world().get::<slotted_ui::ItemCount>(*c).is_some())
        .expect("a count child");
    assert_eq!(
        h.world().get::<Text>(count).map(|t| t.0.clone()),
        Some("17".to_owned())
    );
    assert_eq!(
        h.world().get::<Visibility>(count),
        Some(&Visibility::Inherited)
    );
    assert_eq!(
        h.world()
            .get::<slotted_ui::SemanticLabel>(slot)
            .map(|l| l.0.clone()),
        Some("item#3 x17".to_owned())
    );
}

#[test]
fn hovering_past_the_delay_composes_a_tooltip() {
    let mut h = Harness::new();
    let _ = h.open_chest(chest_screen());
    let (slot, _) = h.slots()[2];
    h.app
        .world_mut()
        .entity_mut(slot)
        .insert(ItemView::new(Some(slotted_model::ItemStack::new(
            slotted_model::ItemId(1),
            4,
        ))));

    let pos = h.center_of(slot);
    h.pointer_move_to(pos);
    h.step(1);
    assert!(
        h.world().get::<slotted_ui::TooltipContent>(slot).is_none(),
        "no tooltip before the delay"
    );

    // The glass theme's normal duration is 180ms, so the delay is 360ms:
    // 22 frames of 16.667ms.
    h.step(24);
    let content = h
        .world()
        .get::<slotted_ui::TooltipContent>(slot)
        .expect("a tooltip after the delay");
    assert_eq!(content.tier, slotted_ui::TooltipTier::Compact);
    assert_eq!(content.parts.len(), 2, "{:?}", content.parts);

    // Shift promotes it to the expanded tier.
    h.hold_key(KeyCode::ShiftLeft);
    h.step(2);
    let content = h
        .world()
        .get::<slotted_ui::TooltipContent>(slot)
        .expect("the tooltip is still up");
    assert_eq!(content.tier, slotted_ui::TooltipTier::Expanded);

    // A tooltip node was spawned under the tooltip layer, hosted by the slot.
    let mut hosts = h.app.world_mut().query::<&slotted_ui::TooltipHost>();
    assert!(hosts.iter(h.app.world()).any(|host| host.0 == slot));

    // Leaving the slot tears it down again.
    h.pointer_move_to(Vec2::new(2.0, 2.0));
    h.step(2);
    assert!(h.world().get::<slotted_ui::TooltipContent>(slot).is_none());
}

#[test]
fn the_carried_layer_mirrors_the_menu_and_follows_the_pointer() {
    let mut h = Harness::new();
    let (menu, _) = h.open_chest(chest_screen());
    let carried_node = {
        let mut q = h
            .app
            .world_mut()
            .query_filtered::<Entity, With<slotted_ui::CarriedItem>>();
        q.iter(h.app.world()).next().expect("a carried node")
    };
    assert_eq!(
        h.world().get::<Visibility>(carried_node),
        Some(&Visibility::Hidden),
        "nothing is carried yet"
    );

    h.app
        .world_mut()
        .entity_mut(menu)
        .insert(slotted_ecs::Carried(Some(slotted_model::ItemStack::new(
            slotted_model::ItemId(2),
            1,
        ))));
    h.pointer_move_to(Vec2::new(300.0, 200.0));
    h.step(1);

    assert_eq!(
        h.world().get::<Visibility>(carried_node),
        Some(&Visibility::Inherited)
    );
    let node = h.world().get::<Node>(carried_node).expect("a node");
    let half = slotted_ui::SLOT_SIZE * 0.5;
    assert_eq!(node.left, Val::Px(300.0 - half));
    assert_eq!(node.top, Val::Px(200.0 - half));
    assert_eq!(
        h.world()
            .get::<ItemView>(carried_node)
            .and_then(|v| v.stack.as_ref().map(|s| s.id)),
        Some(slotted_model::ItemId(2))
    );
}

#[test]
fn an_action_rail_button_triggers_a_toolbar_menu_action() {
    #[derive(Resource, Default)]
    struct Actions(Vec<slotted_model::ClickAction>);

    let mut h = Harness::new();
    h.app.init_resource::<Actions>().add_observer(
        |a: On<slotted_ecs::MenuAction>, mut out: ResMut<Actions>| {
            out.0.push(a.action);
        },
    );
    let _ = h.open_chest(chest_screen());

    let sort = {
        let mut q = h
            .app
            .world_mut()
            .query::<(Entity, &slotted_ui::RailAction)>();
        q.iter(h.app.world())
            .find(|(_, a)| matches!(a.0, slotted_model::ToolbarAction::Sort { .. }))
            .map(|(e, _)| e)
            .expect("a sort button")
    };
    h.click(sort, PointerButton::Primary);
    h.step(1);

    let actions = &h.world().resource::<Actions>().0;
    assert_eq!(
        actions.as_slice(),
        [slotted_model::ClickAction::Toolbar(
            slotted_model::ToolbarAction::Sort {
                inventory: slotted_model::InventoryRef::new(0)
            }
        )],
        "the rail button fired exactly one toolbar action"
    );
}

#[test]
fn arrow_keys_move_focus_between_slots() {
    let mut h = Harness::new();
    let _ = h.open_chest(chest_screen());
    let slots = h.slots();
    let first = slots[0].0;
    h.app
        .world_mut()
        .resource_mut::<bevy::input_focus::InputFocus>()
        .set(first, bevy::input_focus::FocusCause::Navigated);
    h.step(1);

    let window = h.window;
    h.app.world_mut().write_message(KeyboardInput {
        key_code: KeyCode::ArrowRight,
        logical_key: Key::ArrowRight,
        state: ButtonState::Pressed,
        text: None,
        repeat: false,
        window,
    });
    h.step(1);

    let focused = h
        .world()
        .resource::<bevy::input_focus::InputFocus>()
        .get()
        .expect("focus moved somewhere");
    assert_ne!(focused, first);
    assert_eq!(
        h.world().get::<SlotRef>(focused).map(|r| r.slot),
        Some(SlotIx(1)),
        "the slot to the right of slot 0"
    );
}
