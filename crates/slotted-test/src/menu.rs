//! Readers and drivers for `slotted-menu` (menus M2 contract 5): toasts, the
//! hint bar's entries, the confirm dialog's buttons and the `MenuChoice`s
//! the templates wrote. Behind the `menu` feature.

use bevy::prelude::*;
use slotted_menu::confirm::PendingConfirm;
use slotted_menu::hint_bar::HintEntries;
use slotted_menu::toast::{Toast, ToastColumn};
use slotted_menu::{HintEntry, MenuChoice};
use slotted_ui::ScreenStack;

use crate::harness::UiHarness;
use crate::locator::by;

/// Every `MenuChoice` since [`UiHarness::menu_choices`] last drained it.
#[derive(Resource, Debug, Default, Clone)]
pub struct MenuChoiceLog(pub Vec<MenuChoice>);

fn record_menu_choices(mut log: ResMut<MenuChoiceLog>, mut choices: MessageReader<MenuChoice>) {
    log.0.extend(choices.read().cloned());
}

/// Added by the builder: keeps the choices until a test reads them. The
/// message is registered here as well, so an app without `MenuPlugin` still
/// builds; `add_message` is idempotent.
#[derive(Debug, Default, Clone, Copy)]
pub struct MenuChoiceRecorder;

impl Plugin for MenuChoiceRecorder {
    fn build(&self, app: &mut App) {
        app.add_message::<MenuChoice>()
            .init_resource::<MenuChoiceLog>()
            .add_systems(Last, record_menu_choices);
    }
}

impl UiHarness {
    /// The toasts on screen, oldest first (the order they sit in the
    /// column, bottom to top). Empty when none is up.
    pub fn toasts(&mut self) -> Vec<Entity> {
        let world = self.app.world_mut();
        let mut columns = world.query_filtered::<&Children, With<ToastColumn>>();
        let Some(children) = columns.iter(world).next() else {
            return Vec::new();
        };
        let children: Vec<Entity> = children.iter().collect();
        children
            .into_iter()
            .filter(|e| world.get::<Toast>(*e).is_some())
            .collect()
    }

    /// The entries a hint bar shows. Panics when `entity` is not a
    /// `slotted:hint_bar` root.
    pub fn hint_entries(&self, entity: Entity) -> Vec<HintEntry> {
        self.world()
            .get::<HintEntries>(entity)
            .unwrap_or_else(|| panic!("{entity} is not a hint bar"))
            .0
            .clone()
    }

    /// Activates the `accept` button of the confirm dialog on top of the
    /// stack and runs a frame. Panics when the top is not a confirm dialog.
    pub fn confirm_accept(&mut self) {
        self.press_confirm_button("accept");
    }

    /// Activates the `cancel` button of the confirm dialog on top of the
    /// stack and runs a frame. Panics when the top is not a confirm dialog.
    pub fn confirm_cancel(&mut self) {
        self.press_confirm_button("cancel");
    }

    fn press_confirm_button(&mut self, id: &str) {
        let root = self
            .world()
            .get_resource::<ScreenStack>()
            .and_then(|s| s.top().map(|e| e.root))
            .unwrap_or_else(|| panic!("no screen is open, so nothing to {id}"));
        let pending = self
            .world()
            .get::<PendingConfirm>(root)
            .unwrap_or_else(|| panic!("the top screen is not a confirm dialog waiting for {id}"))
            .0
            .clone();
        // The primary accept and the danger accept share the `menu: accept`
        // tag; whichever survived the rewrite is the one to press.
        let button = self
            .try_find(&by::test_id(id).within(root))
            .or_else(|| {
                (id == "accept")
                    .then(|| self.try_find(&by::test_id("accept_danger").within(root)))?
            })
            .unwrap_or_else(|| panic!("the confirm dialog {pending:?} has no {id} button"));
        self.activate(button);
        self.step(1);
    }

    /// Every `MenuChoice` written since the last call (or since the app
    /// started), in order.
    pub fn menu_choices(&mut self) -> Vec<MenuChoice> {
        std::mem::take(&mut self.world_mut().get_resource_or_init::<MenuChoiceLog>().0)
    }
}
