//! Driving the item browser from a test.
//!
//! `h.browser()` is the browser's half of the harness: it reads what the
//! panel is showing, types into the search field, opens recipe pages through
//! the same hotkeys a player presses, and clicks the transfer button through
//! real picking. Nothing here reaches into the browser's own systems; every
//! action goes through an entity the panel spawned or a message the panel
//! would have written.

use bevy::input_focus::FocusCause;
use bevy::prelude::*;
use bevy::text::{EditableText, TextEdit};
use slotted_browser::ui::dock::BrowserLayout;
use slotted_browser::ui::panel::BrowserPanel;
use slotted_browser::{BrowserRuntime, IndexState, Ingredient, RecipePage};
use slotted_ui::{Side, Tags};

use crate::harness::UiHarness;
use crate::locator::by;

impl UiHarness {
    /// The browser query object.
    pub fn browser(&mut self) -> Browser<'_> {
        Browser { harness: self }
    }
}

/// Everything a test does to the browser panel. Built by
/// [`UiHarness::browser`].
pub struct Browser<'a> {
    harness: &'a mut UiHarness,
}

impl Browser<'_> {
    /// The harness underneath, for anything this object does not wrap.
    pub fn harness(&mut self) -> &mut UiHarness {
        self.harness
    }

    /// Steps until the index build lands, capped by `max_settle_frames`.
    ///
    /// # Panics
    /// If the index is still building after the cap.
    #[track_caller]
    pub fn wait_for_index(&mut self) -> usize {
        for frame in 0..self.harness.max_settle_frames {
            if self
                .harness
                .world()
                .resource::<IndexState>()
                .ready()
                .is_some()
            {
                self.harness.step(1);
                return frame;
            }
            self.harness.step(1);
        }
        panic!(
            "the browser index was still building after {} frames",
            self.harness.max_settle_frames
        );
    }

    /// The ingredients the current query leaves visible, in display order.
    ///
    /// This is `BrowserRuntime::visible` resolved through the index, which is
    /// what the card grid binds from, so it is the whole result rather than
    /// the one page the grid happens to show.
    pub fn visible_entries(&self) -> Vec<Ingredient> {
        let world = self.harness.world();
        let Some(index) = world.resource::<IndexState>().ready() else {
            return Vec::new();
        };
        world
            .resource::<BrowserRuntime>()
            .visible
            .iter()
            .filter_map(|id| index.get(*id).map(|entry| entry.ingredient.clone()))
            .collect()
    }

    /// The ingredients the visible cards are bound to right now, in grid
    /// order. The page the player can actually see.
    pub fn visible_cards(&self) -> Vec<String> {
        self.harness
            .find_all(&by::role(slotted_ui::SemanticRole::Card))
            .into_iter()
            .filter(|e| self.harness.is_visible(*e))
            .filter_map(|e| {
                self.harness
                    .world()
                    .get::<Tags>(e)
                    .and_then(|tags| tags.get("entry").map(ToOwned::to_owned))
            })
            .collect()
    }

    /// Focuses the search field, replaces its contents with `text` one
    /// keystroke at a time, and settles.
    ///
    /// # Panics
    /// If no search field is on screen.
    #[track_caller]
    pub fn search(&mut self, text: &str) {
        let field = self.harness.find(&by::test_id("browser.search"));
        self.harness.set_focus(Some(field));
        if let Some(mut editable) = self.harness.world_mut().get_mut::<EditableText>(field) {
            editable.editor.set_text("");
            editable.queue_edit(TextEdit::TextEnd(false));
        }
        self.harness.step(1);
        self.harness.type_text(text);
        self.harness.settle();
    }

    /// Whether the search field holds the keyboard.
    pub fn search_has_focus(&self) -> bool {
        self.harness
            .world()
            .resource::<BrowserRuntime>()
            .has_keyboard_focus
    }

    /// Moves focus off the search field, so the hotkeys fire again.
    pub fn blur_search(&mut self) {
        if let Some(mut focus) = self
            .harness
            .world_mut()
            .get_resource_mut::<bevy::input_focus::InputFocus>()
        {
            focus.clear();
        }
        self.harness.step(1);
    }

    /// Focuses the search field the way Ctrl+F does.
    ///
    /// # Panics
    /// If no search field is on screen.
    #[track_caller]
    pub fn focus_search(&mut self) {
        let field = self.harness.find(&by::test_id("browser.search"));
        if let Some(mut focus) = self
            .harness
            .world_mut()
            .get_resource_mut::<bevy::input_focus::InputFocus>()
        {
            focus.set(field, FocusCause::Navigated);
        }
        self.harness.step(1);
    }

    /// Hovers `entity` and presses the recipes key, exactly as a player does.
    pub fn open_recipes(&mut self, entity: Entity) {
        let key = self.key(|k| k.recipes);
        self.harness.hover(entity);
        self.harness.key(key);
        self.harness.settle();
    }

    /// Hovers `entity` and presses the uses key.
    pub fn open_uses(&mut self, entity: Entity) {
        let key = self.key(|k| k.uses);
        self.harness.hover(entity);
        self.harness.key(key);
        self.harness.settle();
    }

    /// Hovers `entity` and presses the bookmark key.
    pub fn bookmark(&mut self, entity: Entity) {
        let key = self.key(|k| k.bookmark);
        self.harness.hover(entity);
        self.harness.key(key);
        self.harness.settle();
    }

    /// Presses the back key.
    pub fn back(&mut self) {
        let key = self.key(|k| k.back);
        self.harness.key(key);
        self.harness.settle();
    }

    /// Presses the close key.
    pub fn close(&mut self) {
        let key = self.key(|k| k.close);
        self.harness.key(key);
        self.harness.settle();
    }

    fn key(&self, pick: impl Fn(&slotted_browser::KeyMappings) -> KeyCode) -> KeyCode {
        pick(&self.harness.world().resource::<BrowserRuntime>().keys)
    }

    /// The open recipe page, if the recipe view is showing one.
    pub fn open_page(&self) -> Option<RecipePage> {
        self.harness
            .world()
            .resource::<BrowserRuntime>()
            .open
            .clone()
    }

    /// Clicks the `+` button through real picking, so a disabled button does
    /// nothing.
    ///
    /// # Panics
    /// If the recipe view is not showing.
    #[track_caller]
    pub fn transfer(&mut self) {
        let button = self.harness.find(&by::test_id("browser.transfer"));
        self.harness.click(button);
        self.harness.settle();
    }

    /// Whether the `+` button would accept a press.
    ///
    /// # Panics
    /// If the recipe view is not showing.
    #[track_caller]
    pub fn transfer_enabled(&self) -> bool {
        let button = self.harness.find(&by::test_id("browser.transfer"));
        self.harness
            .world()
            .get::<bevy::ui::InteractionDisabled>(button)
            .is_none()
    }

    /// Whether `screen` has a browser panel attached.
    pub fn is_attached(&self, screen: Entity) -> bool {
        self.panel_of(screen).is_some()
    }

    /// The placement the dock computed for `screen`'s panel.
    pub fn layout(&self, screen: Entity) -> Option<BrowserLayout> {
        self.panel_of(screen)
            .and_then(|panel| self.harness.world().get::<BrowserLayout>(panel).copied())
    }

    /// Which side the only panel on screen docked to; `None` when it did not
    /// fit and is hidden.
    pub fn dock_side(&self) -> Option<Side> {
        let world = self.harness.world();
        let mut query = world.try_query::<(Entity, &BrowserPanel)>()?;
        let panel = query.iter(world).next()?.0;
        world.get::<BrowserLayout>(panel).map(|layout| layout.side)
    }

    /// The `side=` tag on the only panel on screen, including `none`.
    pub fn dock_tag(&self) -> Option<String> {
        let world = self.harness.world();
        let mut query = world.try_query::<(Entity, &BrowserPanel)>()?;
        let panel = query.iter(world).next()?.0;
        world
            .get::<Tags>(panel)
            .and_then(|tags| tags.get("side").map(ToOwned::to_owned))
    }

    fn panel_of(&self, screen: Entity) -> Option<Entity> {
        let world = self.harness.world();
        let mut query = world.try_query::<(Entity, &BrowserPanel)>()?;
        query
            .iter(world)
            .find(|(_, panel)| panel.screen == screen)
            .map(|(entity, _)| entity)
    }
}
