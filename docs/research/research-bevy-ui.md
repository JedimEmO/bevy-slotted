# Bevy UI Ecosystem Report for a Minecraft-style UI / Inventory Plugin Set (as of 2026-09-05)

## 0. Version landscape

| Release | Date | UI headline |
|---|---|---|
| Bevy 0.17 | 2025-09-30 | Headless widgets (`bevy_ui_widgets`, experimental), Feathers (experimental), gradients, `ViewportNode`, `UiTransform`, event/observer rework (`On<T>`, `EntityEvent`), per-side border colors, `px()/percent()` helpers |
| Bevy 0.18 | 2026-01-13 | Auto directional navigation, `Popover` + `MenuPopup`, variable fonts / underline / strikethrough / OpenType features, pickable text sections, `IgnoreScroll`, `Val`/`Color` interpolation |
| Bevy 0.19 | 2026-06-19 (0.19.1 on 2026-08-13) | **Next Generation Scenes / BSN (`bsn!`)**, **`EditableText` first-party text input**, parley replaces cosmic-text, `FontSource`/`FontSize` enums, widgets + Feathers de-experimentalised, `bevy_scene` renamed `bevy_world_serialization` |

**Current release is 0.19.1.** Sibling projects on 0.17 are two releases behind; the 0.17 to 0.19 jump touches text (`TextFont.font: FontSource`, `font_size: FontSize`), widgets (`Core*` prefix dropped), and scenes (`DynamicScene` to `DynamicWorld`).

Sources:
- https://bevy.org/news/bevy-0-19/
- https://bevy.org/news/bevy-0-18/
- https://bevy.org/news/bevy-0-17/
- https://bevy.org/learn/migration-guides/0-18-to-0-19/

## 1. Built-in bevy_ui in 0.19

**Layout.** `Node` is the single style component (42 fields: `display` Flex/Grid/None, `position_type`, `overflow`, `overflow_clip_margin`, `scrollbar_width`, edges, sizing, alignment, `margin/padding/border: UiRect`, `border_radius`, full flexbox and CSS-grid fields, new `direction: InlineDirection`). Layout is taffy (0.14 upstream). `Node` requires `ComputedNode`, `ComputedStackIndex` (new in 0.19, was `ComputedNode::stack_index`), `ContentSize`, `ComputedUiTargetCamera`, `ComputedUiRenderTargetInfo`, `UiTransform`, `BackgroundColor`, `BorderColor` (per-side colors since 0.17), `FocusPolicy`, `ScrollPosition`, `Visibility`, `ZIndex`. Source: https://docs.rs/bevy_ui/latest/bevy_ui/struct.Node.html

**Images / 9-slice.** `bevy_ui::widget::ImageNode { color, image: Handle<Image>, texture_atlas: Option<TextureAtlas>, flip_x, flip_y, rect: Option<Rect>, image_mode: NodeImageMode, visual_box }`, constructors `new`, `from_atlas_image`, `solid_color`, `with_rect`, `with_mode`. `NodeImageMode` = `Auto | Stretch | Sliced(TextureSlicer) | Tiled { tile_x, tile_y, stretch_value }`. `TextureSlicer { border: BorderRect, center_scale_mode: SliceScaleMode, sides_scale_mode: SliceScaleMode, max_corner_scale }`, with `SliceScaleMode::Stretch | Tile { stretch_value }`. 9-slice + atlas + `rect` sub-region are all first-party. `TextureAtlasLayout` (`from_grid`) works for item icon sheets. Source: https://docs.rs/bevy_ui/latest/bevy_ui/widget/struct.ImageNode.html

**Decoration.** `BorderRadius`, `Outline`, `BoxShadow`, `BackgroundGradient` / `BorderGradient` (linear/conic/radial, Oklab default, 0.17), `UiMaterial` for custom shaders, `TextShadow`, `TextBackgroundColor`.

**Scroll.** `ScrollPosition` + `Overflow::scroll()`; `IgnoreScroll` (0.18) for sticky headers; `Scrollbar`/`ScrollArea` headless widgets; `Pointer<Scroll>` picking event.

**Z-order / cameras.** `ZIndex` (local), `GlobalZIndex` (cross-tree), `ComputedStackIndex`; `UiTargetCamera` per root, `IsDefaultUiCamera`. UI nodes ignore `RenderLayers` (issue https://github.com/bevyengine/bevy/issues/17400), so overlay layering is done with `GlobalZIndex` or multiple UI cameras, not layers.

**Transforms.** Since 0.17 UI uses `UiTransform` / `UiGlobalTransform` (2D, translation in `Val`, rotation, scale) instead of `Transform`. Useful for a wobble on the dragged item, not for arbitrary 3D.

**Viewport in UI.** `ViewportNode::new(camera)` (0.17) renders a camera with `RenderTarget::Image` into a node; picking through it works with the `bevy_ui_picking_backend` feature. Example: https://bevy.org/examples/ui-user-interface/viewport-node/ . This is the sanctioned way to put a spinning 3D block in a slot; cost is one render-to-texture camera per live viewport, so for hundreds of slots pre-render icons to an atlas and reserve `ViewportNode` for the hovered/held item.

**Picking.** `bevy_picking` is first-party (bevy_mod_picking merged in 0.15). Events are `Pointer<E>` `EntityEvent`s that bubble up `ChildOf`: `Over, Out, Enter, Leave, Move, Press, Release, Click, DragStart, Drag, DragEnd, DragEnter, DragOver, DragLeave, DragDrop, Scroll, Cancel`. Naming is Press/Release (not Down/Up). Attach with `.observe(|ev: On<Pointer<Click>>| ..)` or in BSN `on(|press: On<Pointer<Press>>| ..)`. `Interaction` (None/Hovered/Pressed) and `RelativeCursorPosition` still exist as polling alternatives; `Hovered`/`Pressed` from `bevy_ui_widgets` are the observer-era equivalents. Since 0.19, `bevy_picking` no longer pulls `bevy_input_focus`; focus-on-click is under the `ui_picking` feature. Source: https://docs.rs/bevy_picking/latest/bevy_picking/events/index.html

**Events rework (0.17).** `Event` (observers) vs `Message` (buffered `MessageWriter/Reader`); `Trigger<T>` became `On<T>`; `#[derive(EntityEvent)]` with `#[entity_event(propagate)]`; lifecycle `OnAdd` became `Add`. An inventory plugin should expose its own `EntityEvent`s (e.g. `SlotClicked { entity, button, modifiers }`) that follow this shape.

**Text.** 0.19 moved from cosmic-text to **parley**. `TextFont { font: FontSource, font_size: FontSize, weight: FontWeight, style, width, font_smoothing, .. }`. `FontSource::Handle(..) | Family("..") (needs system_font_discovery) | Monospace | SansSerif | ...`; `FontSize::Px | Rem | Vw | Vh | VMin | VMax` (`RemSize` resource). `LetterSpacing`, `Underline`/`Strikethrough` (+colors), `FontFeatures`. Fonts are still `Handle<Font>` assets loaded from TTF/OTF; **no first-party bitmap/BMFont loader**. Text sections are individually pickable since 0.18 (hyperlinks, per-word tooltips).

**Text input.** 0.19 ships **`EditableText`** (`bevy_ui_widgets::EditableTextInputPlugin`): cursor/selection, word nav, clipboard (`system_clipboard` feature), IME, multiline + soft wrap, `EditableTextFilter`, `SelectAllOnFocus`, `max_characters`. Missing: placeholder, undo/redo, password masking. Example: https://bevy.org/examples/ui-user-interface/text-input/ . Good enough for a search box in a recipe/creative tab.

**Headless widgets (`bevy_ui_widgets`, 0.19.1).** No longer behind `experimental_*`; included in the `ui` feature. Components: `Button, Checkbox, RadioButton, RadioGroup, Slider(+SliderValue/Range/Step/Precision/Thumb), Scrollbar(+ScrollbarThumb), ScrollArea, ListBox/ListItem, MenuButton/MenuItem/MenuPopup, Popover (0.18)`. State: `Hovered, Pressed, Checked, InteractionDisabled, ActiveDescendant`. Events: `Activate, ValueChange<T>, SetSliderValue, SetChecked, ToggleChecked, MenuEvent, ScrollIntoView`. Docs still say "experimental and under active development. The API is likely to change substantially." Source: https://docs.rs/bevy_ui_widgets/latest/

**Feathers (`bevy_feathers` 0.19.1).** Editor-styled widget set on top of widgets, ported to BSN in 0.19 (old fns renamed `button_bundle`, etc.). Modules: `containers, controls, cursor, dark_theme, display, focus, font_styles, palette, rounded_corners, theme, tokens`. Adds text/number input, dropdown, disclosure, pane/subpane/group, list view, scrollbar, ColorPlane. README: "still experimental and unfinished... prioritized consistency over customization", intended for tooling, and suggests copying the code. **Not a fit for a Minecraft look, but its token-based theme (`ThemeBackgroundColor`, `tokens`) and cursor-icon module are worth copying as patterns.** Source: https://docs.rs/bevy_feathers/latest/

**Focus & navigation.** `bevy_input_focus` (`InputFocus` resource, `.get()/.set()/.clear()` since 0.19; `InputDispatchPlugin` is in `DefaultPlugins`). `TabGroup { order }` + `TabIndex(i32)` with `TabNavigationPlugin`; `DirectionalNavigationMap` (manual graph) and 0.18's `AutoDirectionalNavigation` component + `AutoDirectionalNavigator` system param (`AutoNavigationConfig`; manual edges override auto). Keyboard/gamepad slot navigation in a grid is therefore mostly free. Examples: https://bevy.org/examples/ui-user-interface/auto-directional-navigation/ , https://bevy.org/examples/ui-user-interface/tab-navigation/

**BSN (Bevy Scene Notation).** Landed in 0.19 (PR https://github.com/bevyengine/bevy/pull/23413): `bsn!` macro, `Template`/`FromTemplate` derive (anything `Default + Clone` works), patches/layering, `Children [...]`, scene functions returning `impl Scene`, `bsn_list!`, `#EntityName` references, inline `on(observer)`, `asset_value()`, `@Player {..}` via `#[derive(SceneComponent)]` with props, `.spawn()` to make a scene fn a system. **Caveat: no `.bsn` file asset loader yet** (code-only; asset-driven workflow and glTF integration "planned for a future release"). The old RON `.scn.ron` path survives as `bevy_world_serialization` (`WorldAsset`, `DynamicWorld`, `DynamicWorldBuilder` now takes `&TypeRegistry`), retained for glTF and round-trip serialization. Hot-reloading UI today means either hot-reloading your own asset format (COB-style) or waiting for `.bsn` files. Bevy's editor roadmap says `.ron` is enough for MVP and `.bsn` is the target (https://bevyengine.github.io/bevy_editor_prototypes/roadmap.html). Source: https://bevy.org/news/bevy-0-19/

**Also relevant.** `SettingsPlugin` (0.19) for persisted app settings via `#[derive(SettingsGroup)]`; `#[derive(Reflect)]` auto-registration since 0.17 (via `inventory` crate), so reflected item/slot types need no manual `register_type` on desktop/web (generic types still do); `ButtonInput<Key>` for layout-aware keys (0.17). Open design issue "Unified Bevy User Interface" (https://github.com/bevyengine/bevy/issues/22345, viridia, Jan 2026) plans to merge bevy_ui and diegetic 2D/3D UI; only rectangular clipping exists today; no timeline.

## 2. Pixel-art fitness

- **Images**: `ImagePlugin::default_nearest()` sets nearest sampling for all textures; per-image `ImageSampler::nearest()` also works. 9-slice with `SliceScaleMode::Tile` keeps pixel edges crisp; `max_corner_scale` prevents corners blowing up.
- **Integer scaling**: `UiScale(f32)` resource multiplies all `Val::Px`; font size = size x window scale x UiScale, rounded to whole pixels. Pick `UiScale` = floor(window_h / 240) (Minecraft "GUI scale") and everything stays integer-aligned; also honor `Window.scale_factor` (HiDPI) by dividing it out. Example: https://bevy.org/examples/ui-user-interface/ui-scaling/
- **Text**: `TextFont.font_smoothing = FontSmoothing::None` uses nearest sampling on the glyph atlas. A `FontSmoothing` bug (setting ignored, always antialiased) was fixed in PR https://github.com/bevyengine/bevy/pull/22455 (2026), so verify on 0.19.1. With a TTF pixel font (Monocraft, Minecraftia style) at its native px size x integer UiScale, results are crisp. Historical blurry-text issues: https://github.com/bevyengine/bevy/issues/10720 , https://github.com/bevyengine/bevy/issues/12064 , https://github.com/bevyengine/bevy/discussions/11443
- **Bitmap fonts**: none first-party (parley/swash only rasterise vector fonts; no BMFont/.fnt or MSDF support). Third-party: **bevy_image_font** 0.11 (Bevy 0.18, 2026-02-20; PNG + RON layout; renders via `Sprite`, via `ImageNode` in UI (`ImageFontPreRenderedUiText`), or atlas sprites; no newlines/wrapping; https://github.com/ilyvion/bevy_image_font ), bevy_pxtxt 0.2 (2024, dead), extol_image_font / extol_pixel_font (2024, predecessors). No 0.19 bitmap font crate today. Minecraft's own font is a PNG glyph sheet, so a plugin set would likely ship its own tiny image-font renderer (pre-render to an `Image` or spawn glyph `ImageNode`s) rather than depend on these.
- **Pixel cameras** (world, not UI): bevy_pixel_camera 0.13 (2024, dead), bevy_pixcam 0.19 (2026-06-22, maintained fork), bevy_smooth_pixel_camera 0.4.1 (2026-04-19), bevy_modern_pixel_camera 0.5.1 (2026-01-22). Not needed for UI; `UiScale` covers it.
- **3D item in a slot**: `ViewportNode` + camera with `RenderTarget::Image` + `RenderLayers` to isolate the item mesh; or bake 16x16/32x32 icons at startup with one camera into an atlas and use `ImageNode::from_atlas_image`. Item counts / durability bars are just child `Text`/`Node`s.

## 3. Third-party UI crates (latest version, date, Bevy target; crates.io as of 2026-09-05)

| Crate | Latest | Bevy | Status / fit |
|---|---|---|---|
| bevy_egui | 0.42.0, 2026-08-16 | 0.19 | Alive, huge (2.5M dl). Immediate-mode; great for debug/inspector overlays, wrong look for a game inventory. |
| bevy_lunex | 0.7.0, 2026-08-31 | 0.19 | Alive (957 stars). Retained, clay-inspired layout on plain ECS with worldspace/3D UI, supports `SpriteImageMode::Sliced`. Alternative to bevy_ui; lock-in risk. https://github.com/bytestring-net/bevy_lunex |
| bevy_cobweb_ui | 0.22.2, 2026-01-14 | 0.17 | **Archived 2026-01-14 ("No longer maintained as of 01/13/2026")**. COB format, hot reload, themes, localization, widgets. Dead, but COB is the best reference for a hot-reloadable UI asset format. https://github.com/UkoeHB/bevy_cobweb_ui |
| sickle_ui | 0.4.0, 2024-10-03 | 0.14 | Dead. |
| bevy-ui-navigation | 0.33.1, 2023-11-15 | 0.12 | Dead; replaced by first-party `bevy_input_focus`. |
| haalka | 0.7.1, 2026-02-11 | 0.18 | Alive-ish (FRP signals, MoonZoon/Dominator style); no 0.19 in compat table. Fit only if you want signals. https://github.com/databasedav/haalka |
| bevy_hui | 0.7.0, 2026-06-20 | 0.19 | Alive. HTML-like templates loaded as assets with hot reload; small (7.5k dl). Plausible authoring layer but immature. |
| bevy_flair | 0.8.1, 2026-08-21 | 0.19 | Alive (154 stars). CSS for bevy_ui: selectors, `:hover/:active`, `var()`, transitions, keyframes, hot reload. Strong candidate for theming/hover states. https://github.com/eckz/bevy_flair |
| bevy_dioxus | 0.1.1, 2022-07 | old | Dead. |
| bevy-ui-dsl | 0.9.0, 2024-07 | 0.14 | Dead; BSN supersedes. |
| bevy_simple_text_input | 0.15.0, 2026-06-19 | 0.19 | Alive but obsolete now that `EditableText` exists. |
| bevy_text_edit | 0.9.0, 2026-06-22 | 0.19 | Alive; same story. |
| bevy_ui_text_input | 0.7.0, 2026-01-29 | 0.18 | Lagging. |
| bevy_cosmic_edit | 0.26.0, 2024-12 | 0.15 | Dead (cosmic-text gone from Bevy). |
| bevy_mod_picking | 0.20.1, 2024-07 | 0.14 | Merged upstream as `bevy_picking`. |
| bevy_tweening | 0.16.0, 2026-06-28 | 0.19 | Alive; tweens `Node`/`UiTransform`/`BackgroundColor` via lenses. Good for slot-hover pulses and item fly-to animations. |
| bevy_easings | 0.19.0, 2026-06-24 | 0.19 | Alive; simpler alternative. 0.18 added `TryStableInterpolate` for `Val`/`Color` in core. |
| bevy_asset_loader | 0.27.0, 2026-06-21 | 0.19 | Alive; loading states, `#[derive(AssetCollection)]`, dynamic assets from RON (incl. texture atlas layouts). |
| bevy_common_assets | 0.17.0, 2026-06-21 | 0.19 | Alive; RON/JSON/TOML/YAML/MsgPack `Asset` loaders in one line. Ideal for item/recipe/theme definitions. |
| leafwing-input-manager | 0.21.0, 2026-06-22 | 0.19 | Alive; action maps. |
| bevy_enhanced_input | 0.26.0, 2026-06-19 | 0.19 | Alive; Unreal-style contexts, observer-based. Either works for open-inventory / hotbar 1-9 / drop. |
| bevy-inspector-egui | 0.37.0, 2026-06-20 | 0.19 | Alive; debug only. |
| kayak_ui | 0.5.0, 2024-02 | 0.12 | Dead. |
| belly | not on crates.io | | Dead (GitHub only). |
| bevy_ninepatch | 0.10.0, 2023-03 | 0.9 | Dead; superseded by `NodeImageMode::Sliced`. |
| bevy_nine_slice_ui | 0.7.0, 2024-07 | 0.14 | Dead; same. |
| bevy_mod_ui_texture_atlas_image | 0.4.1, 2023-04 | 0.10 | Dead; `ImageNode.texture_atlas` covers it. |
| bevy_ui_anchor | 0.12.0, 2026-07-08 | 0.19 | Alive; anchors UI nodes to world entities (nameplates). |
| bevy_defer | 0.18.0, 2026-06-21 | 0.19 | Alive; async coroutines, handy for scripted UI sequences. |
| i-cant-believe-its-not-bsn | 0.3.0, 2024-12 | 0.15 | Dead; BSN shipped. |
| bevy_quill / bevy_reactor (viridia) | 0.1.7, 2024-08 / not on crates.io | 0.14 | Dead; viridia's work became `bevy_ui_widgets`/Feathers. |
| bevy_ui_bits | not on crates.io | | Not found. |
| pyri_tooltip | 0.7.0, 2026-08-23 | 0.19 | Alive (benfrankel, 53k dl). `Tooltip::cursor(..)`, `TooltipPlacement`, `TooltipActivation`/`TooltipDismissal` delays, rich text, primary-tooltip entity. Good drop-in or reference. |
| bevy_nested_tooltips | 0.8.0, 2026-09-05 | 0.19 | Alive, tiny. |
| bevy_modal | 0.4.1, 2026-08-13 | 0.19 | Overlay stack with blocking scrim and deterministic layering; tiny but exactly the overlay-layering problem. |
| bevy_ui_actions | 0.2.6, 2026-07-08 | 0.16 | Has `Draggable`/`DropTarget`/`DragGhost`, `OnDragStart/OnDrop/OnDragCancel`, tooltips, tabs, modals, 3D previews. One-person, 182 downloads, 0.16 only. Reference only. |
| bevy_ecss / tomt_bevycss | 0.7.0 2024-02 / 0.7.1 2025-12 | <=0.15 | Dead-ish; bevy_flair is the living CSS option. |
| bevy_immediate | 0.8.0, 2026-06-20 | 0.19 | Immediate-mode UI over bevy_ui nodes; niche. |
| bevy_material_ui | 0.2.7, 2026-01-25 | 0.17/0.18 | Material Design 3 widgets; wrong aesthetic. |
| bevy_dragndrop | 0.2.0, 2023-11 | 0.12 | Dead. |
| bevy_ui_extras | 0.20.0, 2025-04 | 0.15/0.16 | Lagging utility grab-bag. |
| bevy_rich_text3d | 0.7.0, 2026-07-15 | 0.19 | Mesh-based 3D rich text; not UI. |
| bevy_mod_scripting | 0.21.0, 2026-07-30 | 0.19 | Lua/Rhai scripting; possible mod-hook layer later. |

## 4. Existing inventory / item crates and reference games

- **bevy_inventory 0.1.0** (2026-05-02, Bevy 0.18, 103 downloads, KBVE monorepo https://github.com/KBVE/kbve/tree/main/packages/rust/bevy/bevy_inventory ): `Inventory`, `ItemStack`, `ItemKind` trait, auto-stacking, `swap_slots/remove_at_slot/has_room_for`, serde; optional `bevy` feature adds `InventoryPlugin`, `LootEvent`, `InventoryFullEvent`, `SplitStackAction/MergeStackAction/MoveSlotAction`. No UI. Small but the name is taken.
- **bevy_items 0.1.1** (2026-08-17, 44 dl): protobuf-driven item DB codegen. Tiny.
- **bevy_mod_static_inventory 0.1.0** (2023-09): dead. **bevy_escape_core 0.0.1** (2026-08, 18 dl): capacity-bounded inventory, tiny. **bevy_pins 0.1.1** (2025-10): Hollow Knight charm system, tiny.
- Names `bevy_item`, `bevy_slot`, `bevy_hotbar`, `bevy_crafting`, `bevy_stack`, `bevy_inventory_ui` are **free** on crates.io.
- **valence_inventory** (Valence Minecraft server in Rust, 0.2.0-alpha 2023): server-side Minecraft inventory/slot semantics (click modes, window types) written in Rust. Best Rust reference for exact vanilla click rules even though it is not Bevy UI.
- Bevy voxel games (vx_bevy updated 2026-04, voxel-world-rust, logic_voxels with renet, projekto, bevy_voxel_world 0.17 on Bevy 0.19) ship at most a hotbar; none has a reusable inventory UI. `voxel-framework` 0.4.0 (2026-09-04, 9 dl) mentions a hotbar.
- **Veloren** (Rust, not Bevy): main menu/character select in iced (`IcedUi`); the in-game HUD (bag with 4 bag slots, crafting, trade window with price tooltips and drag-to-offer) is still conrod-based custom UI. Useful for UX reference, not code reuse. https://gitlab.com/veloren/veloren , https://docs.veloren.net/veloren_common/trade/index.html
- **Luanti (Minetest)**: `formspec` strings are the canonical data-driven inventory UI DSL (`list[...]`, `listring[]`, `image_button`, 9-slice `background9[]`, `style[]`); a text format that mods emit at runtime, server-driven, client renders. Good model for a serializable UI description a server can ship.

## 5. Data-driven definition options

- **bevy_reflect**: `#[derive(Reflect)]` with auto-registration (0.17+); `TypeRegistry`, `DynamicStruct`, `ReflectComponent` (0.19: `ReflectResource` is now a ZST marker; use `ReflectComponent`); 0.19 reorganised modules (`structs`, `enums`, `list`, `map`, ...), `FieldIter` yields `(name, value)`, `DynamicStruct::index_of` became `Struct::index_of_name`. Fine for reflect-driven theme patching and inspector tooling.
- **Scenes**: `bsn!` for code-defined UI templates (composable, patchable, observers inline); `.bsn` files not yet loadable; RON `WorldAsset`/`DynamicWorld` remain for serialized worlds. For a hot-reloadable *theme* file today, define your own `Asset` (RON via bevy_common_assets) and re-apply it to tagged nodes on `AssetEvent::Modified`.
- **Hot reload**: `file_watcher` cargo feature + `AssetPlugin { watch_for_changes_override: Some(true), .. }`; observe `AssetEvent<T>`. Scene hot reload was fixed in PR https://github.com/bevyengine/bevy/pull/18358. Example: https://docs.rs/crate/bevy/latest/source/examples/asset/hot_asset_reloading.rs
- **Asset processing**: `asset_processor` feature, `AssetMode::Processed`, `.meta` files and `Process` impls; can pre-bake icon atlases or pack 9-slice sheets. Underused in the ecosystem but stable since 0.12.
- **Atlases**: `TextureAtlasLayout::from_grid(UVec2, cols, rows, padding, offset)` + `TextureAtlas { layout, index }` on `ImageNode`; `TextureAtlasBuilder` for runtime packing; bevy_asset_loader dynamic assets can declare atlases in RON.
- **bevy_asset_loader 0.27**: `AssetCollection` derive, `#[asset(texture_atlas_layout(...))]`, `standard_dynamic_assets` RON, loading states with progress; pairs with `bevy_common_assets 0.17` for item/recipe/theme RON.
- **SettingsPlugin** (0.19) for GUI-scale/user prefs persistence (`SettingsPlugin::new("com.example.mygame")`, `SaveSettingsDeferred`).

## 6. Networking / authority

All three big crates are on Bevy 0.19: **bevy_replicon 0.44** (2026-09-01; server-authoritative component replication, client events/triggers, `bevy_replicon_snap` for prediction, `bevy_timewarp` for rollback; https://github.com/simgine/bevy_replicon ), **lightyear 0.29** (2026-08-10; now built on replicon, adds prediction/interpolation/authority/visibility/pre-spawn; https://github.com/cBournhonesque/lightyear ), **aeronet 0.21** (2026-06-24; transport layer only). For an inventory the sane model is Minecraft's: the server owns `Inventory` components and replicates slot contents; the client sends `SlotAction { window_id, slot, button, mode, state_id }` as a client event/trigger and *optimistically* applies it to a local shadow inventory so the cursor stack updates instantly; the server's replicated state (with a monotonically increasing state id) overwrites the shadow on arrival, and a mismatch triggers a full window resync (exactly vanilla's `ClickContainer` + `stateId` scheme). Full rollback (timewarp) is overkill for slot clicks; a per-window sequence number is enough. Keep the UI crate networking-agnostic: expose an `InventoryModel` trait / event pair so both a local and a replicon-backed backend fit.

## Key takeaways for a Bevy plugin set

1. **Build on built-in bevy_ui (0.19), not a third-party framework.** Everything the ecosystem used to add (9-slice, atlases, picking, drag events, focus, tab/directional nav, text input, headless widgets, gradients, viewport-in-node) is now first-party and maintained; cobweb_ui is archived, sickle/kayak/belly/quill are dead, lunex is the only live alternative and it is a lock-in.
2. **Target 0.19 now, structure for 0.20.** 0.17 to 0.19 is a real migration (parley text, `FontSource`/`FontSize`, `Core*` prefix removal, `bevy_scene` rename, `ComputedStackIndex`). Do not ship against 0.17.
3. **Use `bsn!` for widget templates but do not depend on `.bsn` files yet.** Hot-reloadable *data* (themes, item defs, recipes, layout tables) should be your own RON assets via bevy_common_assets; hot-reloadable *structure* waits for the `.bsn` loader.
4. **Build on `bevy_ui_widgets` state components (`Hovered`, `Pressed`, `InteractionDisabled`, `Checked`) and `Activate`/`ValueChange` events** rather than `Interaction`, so slots interoperate with focus/nav and Feathers-style theming. Expect API churn; wrap them behind your own components.
5. **Skip Feathers as a dependency; copy its patterns** (design tokens, `ThemeBackgroundColor`-style token components, cursor module).
6. **You must ship a bitmap/pixel font path yourself.** No first-party or 0.19 third-party bitmap fonts. Two viable routes: TTF pixel font + `FontSmoothing::None` + integer `UiScale` (cheap, uses parley), or an own PNG glyph-sheet renderer (Minecraft `ascii.png` style) built on `ImageNode` atlas indices or a pre-rendered `Image`.
7. **9-slice theming is first-party** (`NodeImageMode::Sliced`, `Tiled`) but a *theme layer* mapping semantic roles (panel, slot, slot-hover, button-pressed, tooltip-frame) to slicer+image+colour is yours to build; bevy_flair (CSS) is the one living option if you want stylesheet selectors instead.
8. **Drag & drop**: use `Pointer<DragStart/Drag/DragEnd/DragDrop/DragEnter/DragLeave>` observers, but implement Minecraft semantics (cursor-held stack, click-to-pick/click-to-place, shift-click, drag-paint distribution, number-key swaps, double-click collect) yourself; no crate does this.
9. **Dragged item / cursor stack z-order**: spawn it under a dedicated UI root with high `GlobalZIndex` (or a second UI camera with `UiTargetCamera`), `PositionType::Absolute`, `Pickable::IGNORE`; UI ignores `RenderLayers`.
10. **Overlay layering** (HUD < inventory window < tooltip < cursor stack < modal scrim): `GlobalZIndex` bands plus a small overlay stack (bevy_modal 0.4 is a tiny reference).
11. **Tooltips**: pyri_tooltip 0.7 (0.19) is a solid drop-in or template; `Popover` from bevy_ui_widgets handles edge-aware placement for dropdowns/context menus.
12. **Keyboard/gamepad nav is nearly free**: `TabGroup`/`TabIndex` + `AutoDirectionalNavigation` on slot nodes; add manual `DirectionalNavigationMap` edges for hotbar-to-grid wrap.
13. **3D block icons**: pre-render to an atlas with one hidden camera at startup; use `ViewportNode` only for live previews (hovered/held item, armor stand).
14. **Icons & item data**: `TextureAtlasLayout` + `ImageNode::from_atlas_image`, item/recipe RON via bevy_common_assets, atlases and loading via bevy_asset_loader; enable `file_watcher` in dev.
15. **Keep the model crate UI-free and network-agnostic** (slot/stack/transfer rules, vanilla click modes as in valence_inventory), with an event boundary so a replicon/lightyear backend can be authoritative and the client applies optimistic slot actions with state-id resync.
16. **Name check**: `bevy_inventory` is taken (tiny, 0.18); `bevy_item`, `bevy_slot`, `bevy_hotbar`, `bevy_crafting`, `bevy_stack`, `bevy_inventory_ui` are free.

## Sources

- Bevy release notes: https://bevy.org/news/bevy-0-19/ , https://bevy.org/news/bevy-0-18/ , https://bevy.org/news/bevy-0-17/
- Migration guide: https://bevy.org/learn/migration-guides/0-18-to-0-19/
- docs.rs (0.19.1): https://docs.rs/bevy_ui/latest/ , https://docs.rs/bevy_ui_widgets/latest/ , https://docs.rs/bevy_feathers/latest/ , https://docs.rs/bevy_picking/latest/bevy_picking/events/index.html
- Bevy examples: viewport-node, text-input, tab-navigation, auto-directional-navigation, ui-scaling under https://bevy.org/examples/ui-user-interface/
- Bevy issues/PRs: #22345 (Unified UI), #17400 (UI ignores RenderLayers), #10720 / #12064 / discussion #11443 (blurry text), PR #22455 (FontSmoothing fix), PR #23413 (BSN), PR #18358 (scene hot reload), PR #16795 (tab navigation), PR #21668 (auto directional nav)
- crates.io API for all version/date/dependency data (queried 2026-09-05)
- GitHub READMEs: bevy_cobweb_ui, bevy_lunex, bevy_flair, haalka, bevy_image_font, bevy_replicon, lightyear
- Veloren: https://gitlab.com/veloren/veloren ; Editor roadmap: https://bevyengine.github.io/bevy_editor_prototypes/roadmap.html
