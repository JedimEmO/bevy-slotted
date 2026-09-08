//! Toasts (menus M2 contract 3.4): transient notices in their own z band,
//! outside the screen stack.
//!
//! A toast is spawned, never pushed: it shows over a page, a modal or
//! nothing, takes no focus and claims no action. The host is a full-window
//! `Pickable::IGNORE` node in `zbands::TOAST` holding a bottom-centre column
//! that stacks upward; each toast is the embedded `toast.node.ron` snippet
//! with its panel role swapped for the level's and its text rewritten. It
//! arrives with the theme's `Fade` preset, counts down on virtual time,
//! fades out and despawns. Past `Toasts::max_visible` the rest queue.

use std::collections::VecDeque;
use std::time::Duration;

use bevy::prelude::*;
use slotted_theme::{Motion, MotionPreset, Tween, TweenTarget};
use slotted_ui::{
    LocArgs, LocKey, ScreenKind, SemanticRole, SpawnCtx, UiNodeDef, WidgetKind, WidgetNode,
    active_tokens, zbands,
};

/// How loud a toast is; picks its role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ToastLevel {
    /// `toast.info`.
    #[default]
    Info,
    /// `toast.success`.
    Success,
    /// `toast.warning`.
    Warning,
    /// `toast.error`.
    Error,
}

impl ToastLevel {
    /// The theme role.
    pub fn role(self) -> slotted_theme::Role {
        use slotted_theme::roles;
        match self {
            Self::Info => roles::TOAST_INFO,
            Self::Success => roles::TOAST_SUCCESS,
            Self::Warning => roles::TOAST_WARNING,
            Self::Error => roles::TOAST_ERROR,
        }
    }
}

/// What a toast says.
#[derive(Debug, Clone, PartialEq)]
pub struct ToastSpec {
    /// The text, rich, with `args`.
    pub key: LocKey,
    /// Arguments.
    pub args: LocArgs,
    /// The level.
    pub level: ToastLevel,
    /// How long it stays; `None` = ten `durations.slow`.
    pub duration: Option<Duration>,
}

impl ToastSpec {
    /// An info toast.
    #[must_use]
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: LocKey(key.into()),
            args: LocArgs::new(),
            level: ToastLevel::Info,
            duration: None,
        }
    }

    /// The level.
    #[must_use]
    pub fn level(mut self, level: ToastLevel) -> Self {
        self.level = level;
        self
    }

    /// How long it stays.
    #[must_use]
    pub fn duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }

    /// One text argument.
    #[must_use]
    pub fn arg(mut self, name: impl Into<String>, value: impl Into<slotted_ui::Value>) -> Self {
        self.args.insert(name.into(), value.into());
        self
    }
}

/// Toast policy.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct Toasts {
    /// How many show at once; the rest queue.
    pub max_visible: usize,
}

impl Default for Toasts {
    fn default() -> Self {
        Self { max_visible: 3 }
    }
}

/// One toast on screen.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    /// Its level.
    pub level: ToastLevel,
    /// Virtual time left before it fades.
    pub remaining: Duration,
}

/// The full-window host the toasts stack in.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ToastHost;

/// The bottom-centre column under the host that the toasts are children of.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ToastColumn;

/// A toast spawned this frame, waiting for the theme to paint it so the
/// fade-in starts from the painted alpha. Removed by
/// [`start_toast_arrivals`].
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ToastArriving;

/// A toast whose time is up and whose fade-out is running; despawned when
/// its tween is gone.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ToastFading;

/// A toast waiting for room.
#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct ToastQueue(pub VecDeque<ToastSpec>);

/// The pseudo screen kind a toast spawns under, for the widgets that ask
/// their `SpawnCtx` which screen they belong to.
pub fn kind() -> ScreenKind {
    ScreenKind::new("slotted:toast")
}

/// The widget kind on a toast root.
pub fn widget_kind() -> WidgetKind {
    WidgetKind::new("slotted:toast")
}

/// Shows a toast, or queues it when [`Toasts::max_visible`] are already up.
pub fn toast(commands: &mut Commands, spec: ToastSpec) {
    commands.queue(move |world: &mut World| {
        let max = world.get_resource::<Toasts>().map_or(3, |t| t.max_visible);
        let showing = world.query::<&Toast>().iter(world).count();
        if showing < max {
            spawn_toast(world, spec);
        } else if let Some(mut queue) = world.get_resource_mut::<ToastQueue>() {
            queue.0.push_back(spec);
        }
    });
}

/// The column under the host, creating both when this is the first toast.
fn ensure_column(world: &mut World) -> Entity {
    if let Some(column) = world
        .query_filtered::<Entity, With<ToastColumn>>()
        .iter(world)
        .next()
    {
        return column;
    }
    let tokens = active_tokens(world);
    let host = world
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::FlexEnd,
                align_items: AlignItems::Center,
                padding: UiRect::bottom(Val::Px(tokens.spacing.lg)),
                ..default()
            },
            GlobalZIndex(zbands::TOAST),
            Pickable::IGNORE,
            SemanticRole::Custom("toast_host".to_owned()),
            ToastHost,
        ))
        .id();
    world
        .spawn((
            Node {
                // Reversed, so the first toast sits at the bottom edge and
                // every later one stacks above it.
                flex_direction: FlexDirection::ColumnReverse,
                align_items: AlignItems::Center,
                row_gap: Val::Px(tokens.spacing.sm),
                ..default()
            },
            Pickable::IGNORE,
            SemanticRole::Custom("toast_column".to_owned()),
            ToastColumn,
            ChildOf(host),
        ))
        .id()
}

/// The embedded snippet, rewritten for `spec`: the panel in the level's
/// role, the text showing `key` with `args`.
pub fn toast_def(spec: &ToastSpec) -> UiNodeDef {
    let mut def: UiNodeDef =
        ron::from_str(crate::templates::TOAST_NODE).expect("the embedded toast snippet parses");
    if let UiNodeDef::Panel { role, .. } = &mut def {
        *role = spec.level.role();
    }
    if let Some(UiNodeDef::RichText { key, opts, .. }) = def.find_mut("toast.text") {
        *key = spec.key.clone();
        opts.args = spec.args.clone();
    }
    def
}

/// Spawns one toast under the column, with [`Toast`] and [`ToastArriving`].
fn spawn_toast(world: &mut World, spec: ToastSpec) -> Entity {
    let column = ensure_column(world);
    let remaining = spec.duration.unwrap_or_else(|| {
        let slow = active_tokens(world).durations.slow;
        Duration::from_millis(u64::from(slow) * 10)
    });
    let def = toast_def(&spec);
    let mut ctx = SpawnCtx {
        world,
        screen: column,
        kind: kind(),
        menu: None,
        parent: column,
    };
    let entity = ctx.spawn_child(&def);
    world.entity_mut(entity).insert((
        Toast {
            level: spec.level,
            remaining,
        },
        ToastArriving,
        WidgetNode(widget_kind()),
        Pickable::IGNORE,
    ));
    entity
}

/// After `SlottedThemeSet::Apply`: a toast the theme has painted fades in
/// from zero to its painted alpha through the `Fade` preset. Under reduced
/// motion it simply appears.
pub fn start_toast_arrivals(
    motion: Res<Motion>,
    tokens: slotted_ui::tooltip::ThemeTokens,
    pending: Query<Entity, With<ToastArriving>>,
    mut backgrounds: Query<&mut BackgroundColor>,
    mut commands: Commands,
) {
    if pending.is_empty() {
        return;
    }
    let tokens = tokens.get();
    for entity in &pending {
        commands.entity(entity).remove::<ToastArriving>();
        if motion.reduced {
            continue;
        }
        if let Ok(mut background) = backgrounds.get_mut(entity) {
            let to = background.0.alpha();
            background.0.set_alpha(0.0);
            commands.entity(entity).insert(motion.preset_tween(
                MotionPreset::Fade,
                TweenTarget::Alpha { from: 0.0, to },
                &tokens,
            ));
        }
    }
}

/// `SlottedUiSet::Render`: counts every toast down on virtual time, starts
/// the fade-out of one whose time is up (or despawns it at once under
/// reduced motion), despawns a faded one, and dequeues into the room that
/// makes.
pub fn tick_toasts(
    time: Res<Time<Virtual>>,
    motion: Res<Motion>,
    policy: Res<Toasts>,
    tokens: slotted_ui::tooltip::ThemeTokens,
    mut queue: ResMut<ToastQueue>,
    mut toasts: Query<(
        Entity,
        &mut Toast,
        Has<ToastFading>,
        Has<Tween>,
        Option<&BackgroundColor>,
    )>,
    mut commands: Commands,
) {
    let delta = time.delta();
    let mut alive = 0usize;
    let mut tokens_cache: Option<slotted_theme::Tokens> = None;
    for (entity, mut toast, fading, tweening, background) in &mut toasts {
        if fading {
            if tweening {
                alive += 1;
            } else {
                commands.entity(entity).despawn();
            }
            continue;
        }
        alive += 1;
        toast.remaining = toast.remaining.saturating_sub(delta);
        if !toast.remaining.is_zero() {
            continue;
        }
        if motion.reduced {
            commands.entity(entity).despawn();
            alive -= 1;
            continue;
        }
        let from = background.map_or(1.0, |b| b.0.alpha());
        let tokens = tokens_cache.get_or_insert_with(|| tokens.get());
        commands.entity(entity).insert((
            ToastFading,
            motion.preset_tween(
                MotionPreset::Fade,
                TweenTarget::Alpha { from, to: 0.0 },
                tokens,
            ),
        ));
    }
    while alive < policy.max_visible {
        let Some(spec) = queue.0.pop_front() else {
            break;
        };
        alive += 1;
        commands.queue(move |world: &mut World| {
            spawn_toast(world, spec);
        });
    }
}

/// Registers the toast systems.
pub fn build(app: &mut App) {
    app.init_resource::<ToastQueue>().add_systems(
        Update,
        (
            tick_toasts.in_set(slotted_ui::SlottedUiSet::Render),
            start_toast_arrivals.after(slotted_theme::SlottedThemeSet::Apply),
        ),
    );
}
