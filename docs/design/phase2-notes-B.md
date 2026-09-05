# Phase 2 notes, work package B (theme + ui)

Where the implementation had to depart from `docs/design/phase2-contract.md`, and
why. Nothing here changes another crate; every item is a local decision or a
request for a contract change in a later revision.

## Bevy 0.19 API facts the contract's table implies but does not spell out

1. **`BorderRadius` is a field of `Node`, not a component.** `apply_theme`
   writes `Node::border_radius` and nothing else on `Node`; that is the one
   `Node` field the theme owns. Layout stays the ui crate's.
2. **`TextFont::font_size` is a `FontSize` enum**, not an `f32`. The theme's
   `size` becomes `FontSize::Px(size)`.
3. **`TextureSlicer` and `BorderRect` live in `bevy_sprite`** and reach us
   through `bevy::ui::prelude`.
4. `apply_theme` takes `Option<Res<AssetServer>>` and, with `blur`,
   `Option<ResMut<Assets<GlassPanelMaterial>>>`. A headless app has neither, and
   a system that fails parameter validation panics the whole schedule.

## Deviations

5. **No `Option` in widget params.** Widget parameters travel through the
   registry as an untyped `ron::Value`, which forgets `Some` exactly as the
   registry's "Value-safe rule" says. `(inventory: 2)` therefore cannot
   deserialise into an `Option<InventoryRef>`. Every params field is a plain
   type with a `#[serde(default = "...")]`. Pinned by
   `widgets::param_tests::params_survive_the_untyped_value_round_trip`.
6. **Built-in variants do not go through `Widget::spawn`.** `Widget::spawn`
   only receives `params: &Value`, which cannot carry a typed `UiNodeDef`
   variant's fields without re-serialising them. `SpawnCtx::spawn_child`
   dispatches typed variants straight to `widgets::spawn_panel`, `spawn_slot`
   and friends; the `WidgetRegistry` still holds a `Widget` per built-in kind,
   and `Custom` nodes go through it. Both paths reach the same functions.
7. **`Pointer<Over>` starts a timer instead of requesting a tooltip.** The
   contract's mapping table has the `Over` observer trigger `TooltipRequest`
   directly, which would show a tooltip the instant the pointer crosses a slot.
   The observer inserts `HoverStart(Time<Virtual>::elapsed)` and `tooltip_delay`
   triggers the request once `durations.normal * 2` has passed, promoting to the
   expanded tier while shift is held. The harness's `request_tooltip` still
   triggers `TooltipRequest` directly and bypasses the delay.
8. **Drag painting triggers `MenuAction` directly.** `SlotClicked` carries a
   button and modifiers but no drag stage or kind, so the three-stage paint
   cannot round-trip through it. `on_slot_drag_start` / `drag_enter` /
   `drag_end` trigger `MenuAction(ClickAction::Drag { .. })`, which the contract
   allows ("toolbar buttons, number keys and the harness all trigger
   `MenuAction` directly"). A `DragPaint` resource suppresses the `Release` that
   picking sends after a drag, so a paint never also reads as a click.
9. **The rarity ring is a child, not the slot's role.** `Themed(slot.rarity.<r>)`
   sits on a `RarityRing` overlay child so it does not fight the slot's own
   hover / focus / carried state role. The child carries no `SemanticRole`, so
   the semantic tree is unchanged.
10. **Durability** is read from the `slotted:damage` and `slotted:max_damage`
    component keys of a stack's patch, interned through `Registries.components`.
    The fill's colour is interpolated green-to-red by the renderer and carries
    no role, because a role would be repainted by `apply_theme` a set later.
11. **Screen inheritance is not implemented.** `Screens::resolve` clones and
    warns, as the contract says Phase 2 screens do not inherit. What "the
    ancestor's tree with this screen's anchors kept" means for a child screen
    that also has a root needs a decision before Phase 3.
12. **`Favorite` is not yet a role swap.** The theme defines `slot.favorite`,
    but `slot_state_roles` implements only the four states the contract lists.

## Additions

13. `Tokens: Default` (the glass theme's numbers, empty colour maps) so a widget
    can lay itself out before any theme asset has loaded, and
    `Durations::hover_delay_ms`.
14. `slotted_theme::Paint`, the pure material-to-components mapping
    `apply_theme` is written in terms of. This is what the material tests
    assert on; it needs no `App`.
15. `roles::CARRIED`, and new roles in `assets/themes/glass.theme.ron`:
    `slot.favorite`, `slot.rarity.*`, `slot.durability`, `carried`,
    `tooltip.title`, `rail.button`, `invisible`. `roles::ALL` is unchanged, so
    `Theme::missing_roles` still checks exactly the well-known fifteen.
16. `SLOT_SIZE` is a 44px constant in `slotted-ui`, not a theme token. It
    should become one when a theme needs to change it.

## Fonts

17. Text uses Bevy's default font handle: no `font` is set on `TextFont`, so the
    embedded default is used and no Google font is fetched. "Condensed, tabular"
    stack counts need a font that ships with the crate; until then the count is
    the default face with a `TextShadow`.

## Blur

18. `BackdropPlugin` spawns a `Camera3d` at order `-1` rendering into a
    quarter-resolution image, following `spikes/glass-ui`. A game whose world is
    2D should not add it and should theme `panel` as `Solid`. The shader is
    `assets/shaders/glass_panel.wgsl`, copied from the spike unchanged.
