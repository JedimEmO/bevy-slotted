//! Tabs (menus M1 contract 4.4): a tab bar over one page per tab.
//!
//! The root is a column: the bar, then the pages in tab order. Exactly one
//! page is visible; the others are `Visibility::Hidden`, `Display::None`
//! (so they take no room in the column either) and carry a
//! [`FocusMask`], which keeps their focusable descendants out of the focus
//! ring, the focused-action dispatch, and (through [`mask_focus`]) out of
//! `InputFocus` altogether. `TabPrev`/`TabNext` switch the tabs of the
//! screen the focus is on, from anywhere in it, and claim; `Accept` or a
//! click on a tab button switches too. `bind` carries the active tab's id.

use bevy::input_focus::tab_navigation::TabIndex;
use bevy::input_focus::{FocusCause, InputFocus};
use bevy::picking::events::{Click, Pointer};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::ui::auto_directional_navigation::AutoDirectionalNavigation;
use slotted_theme::{Themed, roles};

use crate::actions::{UiAction, UiActionClaims, UiActionEvent};
use crate::def::{BindDef, TabDef, Tags, UiNodeDef};
use crate::focus_ring::Focusable;
use crate::nav::{FocusMask, FocusedAction, first_focusable, screen_root_of};
use crate::screen::SpawnCtx;
use crate::semantic::{LocText, ScreenRoot, SemanticLabel, SemanticRole, WidgetNode};
use crate::values::{BindingTarget, SetValue, Value, ValueBinding, ValueStore};
use crate::widgets::kinds;

/// The tabs' state, for tests.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct TabsState {
    /// The active tab.
    pub active: usize,
    /// The tabs.
    pub tabs: Vec<TabDef>,
}

impl TabsState {
    /// The active tab's id.
    pub fn active_id(&self) -> Option<&str> {
        self.tabs.get(self.active).map(|t| t.id.as_str())
    }
}

/// On one button of the bar.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabButton {
    /// The tabs root.
    pub tabs: Entity,
    /// Which tab.
    pub index: usize,
}

/// On one page.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabPage {
    /// The tabs root.
    pub tabs: Entity,
    /// Which tab shows it.
    pub index: usize,
}

/// The tag a tab button carries with its id.
pub const TAB_TAG: &str = "tab";

/// Spawns a tab bar and its pages.
pub fn spawn_tabs(
    ctx: &mut SpawnCtx<'_>,
    tabs: &[TabDef],
    bind: &BindDef,
    children: &[UiNodeDef],
) -> Entity {
    let tokens = ctx.tokens();
    let entity = ctx.spawn_node((
        Node {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(tokens.spacing.sm),
            width: Val::Percent(100.0),
            flex_shrink: 0.0,
            min_height: Val::Px(0.0),
            ..default()
        },
        Themed(slotted_theme::Role::new_static("invisible")),
        SemanticRole::Tabs,
        SemanticLabel::default(),
        WidgetNode(kinds::tabs()),
        TabsState {
            active: 0,
            tabs: tabs.to_vec(),
        },
        crate::widgets::list::StoreSeen(0),
    ));
    if bind.disabled {
        ctx.world
            .entity_mut(entity)
            .insert(bevy::ui::InteractionDisabled);
    }
    if let Some(binding) = crate::widgets::list::binding_for(ctx, bind) {
        ctx.world.entity_mut(entity).insert(binding);
    }

    // The bar.
    let bar = ctx
        .world
        .spawn((
            Node {
                display: Display::Flex,
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(tokens.spacing.xs),
                padding: UiRect::all(Val::Px(tokens.spacing.xs)),
                flex_shrink: 0.0,
                align_items: AlignItems::Center,
                ..default()
            },
            Themed(roles::TABS_BAR),
            SemanticRole::Panel,
            Pickable::default(),
            ChildOf(entity),
        ))
        .id();
    for (index, tab) in tabs.iter().enumerate() {
        spawn_tab_button(ctx, bar, entity, index, tab, bind.disabled);
    }

    // The pages, one per tab in order; the first is shown.
    let parent = std::mem::replace(&mut ctx.parent, entity);
    for (index, child) in children.iter().enumerate() {
        let page = ctx.spawn_child(child);
        let mut e = ctx.world.entity_mut(page);
        e.insert(TabPage {
            tabs: entity,
            index,
        });
        if let Some(mut node) = e.get_mut::<Node>() {
            node.flex_shrink = 0.0;
        }
        if index != 0 {
            e.insert((Visibility::Hidden, FocusMask));
            if let Some(mut node) = e.get_mut::<Node>() {
                node.display = Display::None;
            }
        }
    }
    ctx.parent = parent;
    entity
}

/// A hidden page is out of layout as well as out of sight: `Visibility::Hidden`
/// alone keeps a page's height in the column, so a tabs node with three
/// pages would be as tall as all three.
fn hide_page(visibility: &mut Visibility, node: &mut Node) {
    *visibility = Visibility::Hidden;
    node.display = Display::None;
}

fn show_page(visibility: &mut Visibility, node: &mut Node) {
    *visibility = Visibility::Inherited;
    node.display = Display::Flex;
}

fn spawn_tab_button(
    ctx: &mut SpawnCtx<'_>,
    bar: Entity,
    tabs: Entity,
    index: usize,
    tab: &TabDef,
    disabled: bool,
) -> Entity {
    let tokens = ctx.tokens();
    let height = tokens.sizes.control_height_compact;
    let button = ctx
        .world
        .spawn((
            Node {
                height: Val::Px(height),
                min_height: Val::Px(height),
                padding: UiRect::axes(Val::Px(tokens.spacing.md), Val::Px(0.0)),
                border: UiRect::all(Val::Px(crate::widgets::BORDER_WIDTH)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                column_gap: Val::Px(tokens.spacing.xs),
                flex_shrink: 0.0,
                ..default()
            },
            Themed(if index == 0 {
                roles::TAB_ACTIVE
            } else {
                roles::TAB
            }),
            SemanticRole::Tab,
            SemanticLabel(tab.label.0.clone()),
            TabButton { tabs, index },
            Focusable,
            TabIndex(0),
            AutoDirectionalNavigation::default(),
            Hovered::default(),
            Pickable::default(),
            Tags::new().with(TAB_TAG, &tab.id),
            ChildOf(bar),
        ))
        .id();
    if disabled {
        ctx.world
            .entity_mut(button)
            .insert(bevy::ui::InteractionDisabled);
    }
    if let Some(icon) = &tab.icon {
        let image = crate::widgets::icon_image(ctx.world, icon);
        let size = height - 2.0 * tokens.spacing.xs;
        ctx.world.spawn((
            Node {
                width: Val::Px(size),
                height: Val::Px(size),
                flex_shrink: 0.0,
                ..default()
            },
            image,
            Pickable::IGNORE,
            ChildOf(button),
        ));
    }
    ctx.world.spawn((
        Node::default(),
        Text::new(tab.label.0.clone()),
        Themed(roles::TEXT_LABEL),
        LocText::new(tab.label.clone()),
        Pickable::IGNORE,
        ChildOf(button),
    ));
    button
}

/// Makes tab `index` of `tabs` active: pages, masks, roles, the binding,
/// and focus when it was inside the tabs. Returns whether anything changed.
#[allow(clippy::too_many_arguments)]
fn activate(
    tabs: Entity,
    index: usize,
    states: &mut Query<(&mut TabsState, Option<&ValueBinding>)>,
    pages: &mut Query<(Entity, &TabPage, &mut Visibility, &mut Node)>,
    buttons: &Query<(Entity, &TabButton)>,
    parents: &Query<&ChildOf>,
    focusables: &Query<(), With<Focusable>>,
    children: &Query<&Children>,
    focus: Option<&mut InputFocus>,
    writes: &mut MessageWriter<SetValue>,
    commands: &mut Commands,
) -> bool {
    let Ok((mut state, binding)) = states.get_mut(tabs) else {
        return false;
    };
    if index >= state.tabs.len() || index == state.active {
        return false;
    }
    let previous = state.active;
    state.active = index;

    let mut new_page = None;
    for (page, tab_page, mut visibility, mut node) in pages.iter_mut() {
        if tab_page.tabs != tabs {
            continue;
        }
        if tab_page.index == index {
            new_page = Some(page);
            show_page(&mut visibility, &mut node);
            commands.entity(page).remove::<FocusMask>();
        } else {
            hide_page(&mut visibility, &mut node);
            commands.entity(page).insert(FocusMask);
        }
    }

    match binding.map(|b| &b.target) {
        Some(BindingTarget::Store(key)) => {
            writes.write(SetValue {
                key: key.clone(),
                value: Value::Text(state.tabs[index].id.clone()),
                source: Some(tabs),
            });
        }
        Some(BindingTarget::Property { menu, id }) => {
            commands.trigger(slotted_ecs::SetProperty {
                entity: *menu,
                id: *id,
                value: i32::try_from(index).unwrap_or(i32::MAX),
            });
        }
        None => {}
    }

    // Focus that sat on the old tab's button follows to the new one; focus
    // that sat in the old page goes to the new page's first focusable, or
    // to the button when the page has none.
    if let Some(focus) = focus
        && let Some(focused) = focus.get()
    {
        let on_button = buttons
            .get(focused)
            .is_ok_and(|(_, b)| b.tabs == tabs && b.index == previous);
        let in_old_page = pages
            .iter()
            .find(|(_, p, _, _)| p.tabs == tabs && p.index == previous)
            .is_some_and(|(page, _, _, _)| is_under(focused, page, parents));
        if on_button || in_old_page {
            let target = if in_old_page {
                new_page.and_then(|page| first_focusable(page, focusables, children))
            } else {
                None
            };
            let target = target.or_else(|| {
                buttons
                    .iter()
                    .find(|(_, b)| b.tabs == tabs && b.index == index)
                    .map(|(e, _)| e)
            });
            if let Some(target) = target {
                focus.set(target, FocusCause::Navigated);
            }
        }
    }
    true
}

/// Whether `entity` is `ancestor` or sits under it.
fn is_under(entity: Entity, ancestor: Entity, parents: &Query<&ChildOf>) -> bool {
    let mut current = entity;
    loop {
        if current == ancestor {
            return true;
        }
        match parents.get(current) {
            Ok(child_of) => current = child_of.parent(),
            Err(_) => return false,
        }
    }
}

/// Observer: `Accept` on a tab button activates its tab.
#[allow(clippy::too_many_arguments)]
pub fn on_tab_button_action(
    action: On<FocusedAction>,
    buttons: Query<(Entity, &TabButton)>,
    disabled: Query<Has<bevy::ui::InteractionDisabled>>,
    mut states: Query<(&mut TabsState, Option<&ValueBinding>)>,
    mut pages: Query<(Entity, &TabPage, &mut Visibility, &mut Node)>,
    parents: Query<&ChildOf>,
    focusables: Query<(), With<Focusable>>,
    children: Query<&Children>,
    mut focus: Option<ResMut<InputFocus>>,
    mut claims: ResMut<UiActionClaims>,
    mut writes: MessageWriter<SetValue>,
    mut commands: Commands,
) {
    if action.action != UiAction::Accept || action.repeat {
        return;
    }
    let Ok((_, button)) = buttons.get(action.entity) else {
        return;
    };
    if disabled.get(action.entity).unwrap_or(false) {
        return;
    }
    claims.claim(UiAction::Accept);
    activate(
        button.tabs,
        button.index,
        &mut states,
        &mut pages,
        &buttons,
        &parents,
        &focusables,
        &children,
        focus.as_deref_mut(),
        &mut writes,
        &mut commands,
    );
}

/// Observer: a click on a tab button activates its tab.
#[allow(clippy::too_many_arguments)]
pub fn on_tab_button_click(
    click: On<Pointer<Click>>,
    buttons: Query<(Entity, &TabButton)>,
    disabled: Query<Has<bevy::ui::InteractionDisabled>>,
    mut states: Query<(&mut TabsState, Option<&ValueBinding>)>,
    mut pages: Query<(Entity, &TabPage, &mut Visibility, &mut Node)>,
    parents: Query<&ChildOf>,
    focusables: Query<(), With<Focusable>>,
    children: Query<&Children>,
    mut focus: Option<ResMut<InputFocus>>,
    mut writes: MessageWriter<SetValue>,
    mut commands: Commands,
) {
    if click.button != bevy::picking::pointer::PointerButton::Primary {
        return;
    }
    let Ok((_, button)) = buttons.get(click.entity) else {
        return;
    };
    if disabled.get(click.entity).unwrap_or(false) {
        return;
    }
    if let Some(focus) = focus.as_deref_mut() {
        focus.set(click.entity, FocusCause::Pressed);
    }
    activate(
        button.tabs,
        button.index,
        &mut states,
        &mut pages,
        &buttons,
        &parents,
        &focusables,
        &children,
        focus.as_deref_mut(),
        &mut writes,
        &mut commands,
    );
}

/// `SlottedUiSet::Input`, after `UiActionEmit`: `TabPrev`/`TabNext` from
/// anywhere in the screen switch its tabs and claim.
///
/// The tabs are those of the screen root the focus sits under, the innermost
/// `tabs` node around the focus when there are several; with no focus, the
/// tabs of the top of the stack. Both actions wrap.
#[allow(clippy::too_many_arguments)]
pub fn tab_actions(
    mut events: MessageReader<UiActionEvent>,
    focus: Option<ResMut<InputFocus>>,
    stack: Option<Res<crate::stack::ScreenStack>>,
    roots: Query<(), With<ScreenRoot>>,
    parents: Query<&ChildOf>,
    tabs_nodes: Query<(Entity, Has<bevy::ui::InteractionDisabled>), With<TabsState>>,
    buttons: Query<(Entity, &TabButton)>,
    mut states: Query<(&mut TabsState, Option<&ValueBinding>)>,
    mut pages: Query<(Entity, &TabPage, &mut Visibility, &mut Node)>,
    focusables: Query<(), With<Focusable>>,
    children: Query<&Children>,
    mut claims: ResMut<UiActionClaims>,
    mut writes: MessageWriter<SetValue>,
    mut commands: Commands,
) {
    let mut focus = focus;
    for event in events.read() {
        let step: isize = match event.action {
            UiAction::TabPrev => -1,
            UiAction::TabNext => 1,
            _ => continue,
        };
        if claims.is_claimed(event.action) {
            continue;
        }
        let focused = focus.as_deref().and_then(InputFocus::get);
        // The innermost tabs around the focus, else the first tabs on the
        // focused screen (or the top of the stack).
        let mut target = focused.and_then(|f| {
            let mut current = f;
            loop {
                if tabs_nodes.contains(current) {
                    return Some(current);
                }
                current = parents.get(current).ok()?.parent();
            }
        });
        if target.is_none() {
            let root = focused
                .and_then(|f| screen_root_of(f, &parents, &roots))
                .or_else(|| stack.as_deref().and_then(|s| s.top().map(|e| e.root)));
            target = root.and_then(|root| {
                tabs_nodes
                    .iter()
                    .map(|(e, _)| e)
                    .find(|e| is_under(*e, root, &parents))
            });
        }
        let Some(tabs) = target else {
            continue;
        };
        if tabs_nodes.get(tabs).is_ok_and(|(_, disabled)| disabled) {
            continue;
        }
        let Ok((state, _)) = states.get(tabs) else {
            continue;
        };
        let len = state.tabs.len();
        if len == 0 {
            continue;
        }
        let next = (state.active.cast_signed() + step).rem_euclid(len.cast_signed());
        let next = next.cast_unsigned();
        claims.claim(event.action);
        activate(
            tabs,
            next,
            &mut states,
            &mut pages,
            &buttons,
            &parents,
            &focusables,
            &children,
            focus.as_deref_mut(),
            &mut writes,
            &mut commands,
        );
    }
}

/// `SlottedUiSet::Navigate`, after `enforce_focus_scope`: focus never rests
/// under a [`FocusMask`]. A focus that landed on a hidden page (Tab
/// navigation does not look at visibility) moves to the active page's first
/// focusable, else to the active tab's button.
#[allow(clippy::too_many_arguments)]
pub fn mask_focus(
    focus: Option<ResMut<InputFocus>>,
    masks: Query<(), With<FocusMask>>,
    parents: Query<&ChildOf>,
    pages: Query<(Entity, &TabPage)>,
    states: Query<&TabsState>,
    buttons: Query<(Entity, &TabButton)>,
    focusables: Query<(), With<Focusable>>,
    children: Query<&Children>,
) {
    let Some(mut focus) = focus else {
        return;
    };
    let Some(focused) = focus.get() else {
        return;
    };
    let Some(masked) = masked_ancestor(focused, &masks, &parents) else {
        return;
    };
    // The innermost masked page decides where focus goes.
    let target = pages.get(masked).ok().and_then(|(_, page)| {
        let state = states.get(page.tabs).ok()?;
        let active = pages
            .iter()
            .find(|(_, p)| p.tabs == page.tabs && p.index == state.active)
            .map(|(e, _)| e)
            .and_then(|page| first_focusable(page, &focusables, &children));
        active.or_else(|| {
            buttons
                .iter()
                .find(|(_, b)| b.tabs == page.tabs && b.index == state.active)
                .map(|(e, _)| e)
        })
    });
    match target {
        Some(target) => focus.set(target, FocusCause::Navigated),
        None => focus.clear(),
    }
}

/// The nearest [`FocusMask`] at or above `entity`.
pub fn masked_ancestor(
    entity: Entity,
    masks: &Query<(), With<FocusMask>>,
    parents: &Query<&ChildOf>,
) -> Option<Entity> {
    let mut current = entity;
    loop {
        if masks.contains(current) {
            return Some(current);
        }
        current = parents.get(current).ok()?.parent();
    }
}

/// `SlottedUiSet::Render`: paints the store's tab id into the state, and
/// every button from active, focus and hover.
pub fn sync_tabs(
    store: Res<ValueStore>,
    focus: Option<Res<InputFocus>>,
    mut states: Query<(
        Entity,
        &mut TabsState,
        &mut crate::widgets::list::StoreSeen,
        Option<&ValueBinding>,
    )>,
    mut pages: Query<(Entity, &TabPage, &mut Visibility, &mut Node)>,
    mut buttons: Query<(Entity, &TabButton, &Hovered, &mut Themed)>,
    mut commands: Commands,
) {
    let focused = focus.and_then(|f| f.get());
    for (entity, mut state, mut seen, binding) in &mut states {
        if let Some(BindingTarget::Store(key)) = binding.map(|b| &b.target)
            && seen.0 != store.version()
        {
            seen.0 = store.version();
            let want = store
                .get(key)
                .and_then(Value::as_str)
                .and_then(|id| state.tabs.iter().position(|t| t.id == id));
            if let Some(want) = want
                && want != state.active
            {
                state.active = want;
                for (page, tab_page, mut visibility, mut node) in pages.iter_mut() {
                    if tab_page.tabs != entity {
                        continue;
                    }
                    if tab_page.index == want {
                        show_page(&mut visibility, &mut node);
                        commands.entity(page).remove::<FocusMask>();
                    } else {
                        hide_page(&mut visibility, &mut node);
                        commands.entity(page).insert(FocusMask);
                    }
                }
            }
        }
        for (button, tab, hovered, mut themed) in &mut buttons {
            if tab.tabs != entity {
                continue;
            }
            let role = if tab.index == state.active {
                roles::TAB_ACTIVE
            } else if focused == Some(button) {
                roles::TAB_FOCUS
            } else if hovered.get() {
                roles::TAB_HOVER
            } else {
                roles::TAB
            };
            if themed.0 != role {
                themed.0 = role;
            }
        }
    }
}

/// Registers the tabs' observers and systems.
pub fn build(app: &mut App) {
    app.add_observer(on_tab_button_action)
        .add_observer(on_tab_button_click)
        .add_systems(
            Update,
            (
                tab_actions
                    .after(crate::actions::UiActionEmit)
                    .after(crate::nav::dispatch_focused_actions)
                    .in_set(crate::plugin::SlottedUiSet::Input),
                mask_focus
                    .after(crate::stack::enforce_focus_scope)
                    .before(crate::stack::record_stack_focus)
                    .in_set(crate::plugin::SlottedUiSet::Navigate),
                sync_tabs.in_set(crate::plugin::SlottedUiSet::Render),
            ),
        );
}
