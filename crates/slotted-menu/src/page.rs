//! Text pages (menus M2 contract 3.5).

use std::sync::Arc;

use bevy::prelude::*;
use slotted_ui::{LocArgs, LocKey, PushScreen, Screens};

/// What a page shows.
#[derive(Debug, Clone, PartialEq)]
pub struct PageSpec {
    /// The heading.
    pub title: LocKey,
    /// The body, rich text, with `args`.
    pub body: LocKey,
    /// Arguments.
    pub args: LocArgs,
}

impl PageSpec {
    /// A page.
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            title: LocKey(title.into()),
            body: LocKey(body.into()),
            args: LocArgs::new(),
        }
    }

    /// Adds an argument for the body.
    #[must_use]
    pub fn arg(mut self, name: impl Into<String>, value: impl Into<slotted_ui::Value>) -> Self {
        self.args.insert(name.into(), value.into());
        self
    }
}

/// Pushes a page: the registered `slotted:page` def with `title` and `body`
/// rewritten. Nothing happens, with a warning, when no page screen is
/// registered.
pub fn open_page(commands: &mut Commands, spec: PageSpec) {
    commands.queue(move |world: &mut World| {
        let kind = crate::kinds::page();
        let Some(mut def) = world
            .get_resource::<Screens>()
            .and_then(|screens| crate::templates::cloned(screens, &kind))
        else {
            tracing::warn!("open_page: no `slotted:page` screen is registered");
            return;
        };
        def.set_text("title", spec.title, LocArgs::new());
        def.set_text("body", spec.body, spec.args);
        let root = world.spawn_empty().id();
        PushScreen {
            root,
            def: Arc::new(def),
            menu: None,
        }
        .apply(world);
    });
}
