# Minecraft-Style UI and Inventory Systems: Research Moodboard

## 1. Visual anatomy of the Java Edition GUI

**Coordinate system and scale.** All GUI layout is authored in "GUI pixels" on a virtual 320x240-minimum canvas; the window is divided by an integer GUI scale. Auto scale picks the largest integer N such that `N = max(1, min(floor(width/320), floor(height/240)))`, and the options menu offers Auto plus 1..N ([gui-scaler](https://github.com/zendiik/gui-scaler)). Everything downstream is integer-aligned pixel art; nothing is subpixel. Screens compute `leftPos = (width - imageWidth)/2`, `topPos = (height - imageHeight)/2` and draw everything relative to that origin ([NeoForge Screens](https://docs.neoforged.net/docs/1.21.5/gui/screens/)).

**Textures.** Pre-1.20.2, every GUI texture was a 256x256 PNG and `blit(x, y, u, v, w, h)` assumed 256x256 UV space. Container backgrounds are 176x166 (the player inventory, chest, furnace, crafting table etc. all share this footprint; the double chest is 176x222, 9x6 rows). Standard atlas coordinates: `widgets.png` 256x256 held the hotbar (182x22 at 0,0), the hotbar selection frame (24x23 at 0,22), and the three 200x20 button states stacked at v=46 (disabled), 66 (normal), 86 (hovered). In 1.20.2 (pack format 18) these sheets were split into individual sprites under `textures/gui/sprites/` and stitched at runtime into a `gui` atlas ([1.20.2 changelog](https://minecraft.wiki/w/Java_Edition_1.20.2)). Button states became `widget/button`, `widget/button_disabled`, `widget/button_highlighted`; text fields became `widget/text_field` / `widget/text_field_highlighted`; scrollbars `widget/scroller`; tabs `widget/tab`, `tab_selected`, `tab_highlighted`, `tab_selected_highlighted`.

**Sprite scaling metadata (the modern 9-slice model).** Each sprite may have a `.png.mcmeta` with a `gui.scaling` block. Types: `stretch` (default), `tile`, `nine_slice`. Nine-slice requires `width`, `height`, `border` (an int or `{left, top, right, bottom}` object) and optional `stretch_inner` (default false: edges and center tile rather than stretch) ([Resource pack wiki](https://minecraft.wiki/w/Resource_pack)). Actual vanilla example, `widget/slider.png.mcmeta` from 1.21.2 ([mcasset](https://mcasset.cloud/1.21.2-pre1/assets/minecraft/textures/gui/sprites/widget/slider.png.mcmeta)):

```json
{ "gui": { "scaling": { "type": "nine_slice", "width": 200, "height": 20, "border": 1 } } }
```

`GuiGraphics.blitSprite(sprite, x, y, w, h)` reads this metadata and handles the slicing, so a mod never hand-computes 9-slice UVs ([NeoForge Screens](https://docs.neoforged.net/docs/1.21.5/gui/screens/)). Mojang ships a tool, [slicer](https://github.com/Mojang/slicer/releases), to cut legacy 256x256 sheets into sprites for resource pack authors.

**Slots.** The slot cell pitch is 18px: a 16x16 item area with a 1px bevel border. The bevel is a dark top-left (approx #373737), light bottom-right (#FFFFFF) and mid (#8B8B8B) on the #C6C6C6 panel gray. Slot positions in vanilla menus are literal integers following the `8 + col*18` / `y0 + row*18` pattern: container rows start at y=18 (chest) or y=17 (Fabric's 3x3 example at x=62), player main inventory at y=84, hotbar at y=142 ([Fabric ScreenHandler tutorial](https://wiki.fabricmc.net/tutorial:screenhandler)). Hover highlight is a flat `fill(x, y, x+16, y+16, 0x80FFFFFF)`, i.e. white at 50 percent alpha, drawn over the item.

**Item decorations.** Count text is drawn only for count > 1, right-aligned at `x + 19 - 2 - textWidth, y + 6 + 3` with a drop shadow, in white. The durability bar is 13px wide by 2px tall at `(x+2, y+13)`: a black backing, then a colored bar whose width is `round(13 - damage*13/maxDamage)` and whose color is `hsv(max(0, 1 - damage/maxDamage) / 3, 1, 1)`, so hue sweeps green to red ([Durability wiki](https://minecraft.wiki/w/Durability)). Cooldown overlays draw a white translucent rectangle that shrinks upward.

**Tooltips.** Background `0xF0100010`, border gradient from `0x505000FF` (top) to `0x5028007F` (bottom); 1px padding rules, 12px per text line, rendered above everything else with a z-offset of 400 ([Coloured Tooltips config](https://github.com/Darkhax-Minecraft/Coloured-Tooltips), [Recoloured Tooltips](https://modrinth.com/mod/recoloured-tooltips)). Since 1.20.2 tooltips (and bundle tooltips) are also resource-pack-skinnable as 9-slice sprites (`tooltip/background`, `tooltip/frame`).

**Fonts.** The default font is an 8px-tall bitmap font from `font/ascii.png` (16x16 glyph grid of 8x8 cells, variable advance computed from the glyph's non-transparent columns), with `font/default.json` declaring providers in fallback order: `bitmap`, `unihex` (Unifont fallback for non-Latin), `ttf`, `space`, `reference`. "Force Unicode Font" swaps to Unifont for everything. Text shadow is the same string re-drawn at (+1, +1) with color multiplied by 0.25.

**Buttons.** 20px tall, 200px wide by default (`Button.DEFAULT_WIDTH = 150` in code, textures 200 wide), three states (disabled: gray text `0xA0A0A0`, normal, highlighted) plus focused outline for keyboard nav. Centered label with shadow.

## 2. Inventory interaction model

All from [Inventory wiki](https://minecraft.wiki/w/Inventory) and the protocol page:

| Input | Effect |
|---|---|
| Left click | Pick up whole stack / place whole stack / swap if incompatible / merge if same |
| Right click | Pick up half (ceil), or place a single item |
| Shift + click | Quick move to the "other" inventory (container <-> player, or hotbar <-> main) |
| Double click (holding items) | Collect all matching items into cursor up to max stack (PICKUP_ALL) |
| Left drag | Distribute cursor stack evenly across painted slots |
| Right drag | Place one item into each painted slot |
| Middle drag (creative) | Place a full stack into each painted slot |
| 1-9 | Swap hovered slot with hotbar slot N |
| F | Swap hovered slot with offhand (Java only) |
| Q / Ctrl+Q | Drop one / drop entire stack from hovered slot |
| Click outside window | Drop cursor stack (left: all, right: one); slot id -999 |
| Middle click (creative) | Clone full stack to cursor without removing |
| Shift + double click | Move all matching items between inventories |

**Stack sizes**: 64 default, 16 for snowballs/eggs/signs/ender pearls etc., 1 for tools/armor/anything with durability. Via the `max_stack_size` component any item can be forced to 1..99 (Bedrock up to 127) ([Data component format](https://minecraft.wiki/w/Data_component_format)).

**Merge rule**: two stacks are mergeable iff same item id and identical component patch (`ItemStack.isSameItemSameComponents`). Count caps at `min(item max stack, slot max stack)`; slots can lower the cap (e.g. beacon payment slot is 1, brewing bottle slots are 1). Result slots (crafting output, furnace output) refuse `mayPlace`, and taking from them fires `onTake` side effects (consume inputs, give XP).

## 3. Container architecture (Java Edition)

**Two-class split.** Every container GUI is a server-side `AbstractContainerMenu` (Yarn: `ScreenHandler`) plus a client-side `AbstractContainerScreen` (`HandledScreen`). The menu owns the list of `Slot` objects, the carried (cursor) stack, the rules for what may go where, `quickMoveStack` (shift-click), and `clicked(slotId, button, ClickType, player)`. The screen only draws and translates mouse input into menu clicks ([NeoForge Menus](https://docs.neoforged.net/docs/1.21.1/gui/menus/)).

**Slot object.** `Slot(container, containerIndex, x, y)` wraps one index into a backing `Container` and records its pixel position. Overridable: `mayPlace(stack)`, `mayPickup(player)`, `getMaxStackSize(stack)`, `onTake`, `isActive` (hide when a tab is not selected), `getNoItemIcon` (background sprite like the armor silhouettes). Menu slot ids are just the order of `addSlot` calls; convention is container slots first, then player main (27), then hotbar (9). So a chest menu is 0-26 chest, 27-53 main, 54-62 hotbar; a furnace is 0 input, 1 fuel, 2 output, 3-29, 30-38 ([Protocol/Inventory](https://minecraft.wiki/w/Java_Edition_protocol/Inventory)). The player's own inventory window is 0 craft output, 1-4 craft grid, 5-8 armor, 9-35 main, 36-44 hotbar, 45 offhand.

Per-window layouts (container slots first, then 27 main, then 9 hotbar): crafting table 0 output, 1-9 grid; brewing stand 0-2 bottles, 3 ingredient, 4 blaze powder; enchanting 0 item, 1 lapis; villager 0-1 inputs, 2 result; anvil/grindstone 0-1 inputs, 2 result; beacon 0 payment; hopper 0-4; loom 0 banner, 1 dye, 2 pattern, 3 result; smithing 0 template, 1 base, 2 addition, 3 result; cartography 0 map, 1 paper, 2 output; stonecutter 0 input, 1 result; horse saddle/armor then chest slots; lectern is the only window with no player inventory.

**ClickType enum**: `PICKUP, QUICK_MOVE, SWAP, CLONE, THROW, QUICK_CRAFT, PICKUP_ALL`, one-to-one with protocol "mode" 0-6. Drag (QUICK_CRAFT) is encoded in the button as `(stage << 2) | dragType`: start 0/4/8, add slot 1/5/9, end 2/6/10 for left/right/middle. SWAP buttons are 0-8 for hotbar and 40 for offhand; THROW is 0 for Q and 1 for Ctrl+Q; PICKUP_ALL is 0 forward and 1 reverse ([Click Container packet](https://minecraft.wiki/w/Java_Edition_protocol/Packets)).

**Sync model.** The server tracks `remoteSlots` and `remoteCarried`; every tick `broadcastChanges()` diffs and sends `Set Container Slot` packets, incrementing a `stateId`. The client applies clicks *predictively* (runs the same `clicked` logic locally) and sends `Click Container {windowId, stateId, slot, button, mode, changedSlots[], carriedItem}`. If the server's `stateId` differs from the client's, it replies with a full `Set Container Content` resync. Integer properties (furnace burn/progress, enchant seeds, anvil cost) go through `DataSlot`/`ContainerData` (Yarn `PropertyDelegate`) and are sent as `Set Container Property`; vanilla truncates these to 16 bits (NeoForge patches to full int) ([Forge Menus](https://docs.minecraftforge.net/en/latest/gui/menus/)). Well-known property ids: furnace 0 lit time, 1 lit duration, 2 cook progress, 3 cook total; enchanting 0-2 level cost, 3 seed, 4-6 enchant id, 7-9 level; anvil 0 repair cost; brewing 0 brew time, 1 fuel; stonecutter/loom 0 selected recipe; beacon 0 power, 1 primary, 2 secondary; lectern 0 page.

**Ghost items / desync.** Because client prediction plus hopper/tick-side mutations race, players see "ghost items" that vanish or reappear when clicked; the wiki has a whole page on it and Mojang's `handleContainerClick` contains a known hackfix ([Client-server desync](https://minecraft.wiki/w/Client-server_desync), [MC-171901](https://bugs.mojang.com/browse/MC-171901)).

**Modding layers.** Forge/NeoForge: register a `MenuType` (with `IMenuTypeExtension.create` for extra opening data via `RegistryFriendlyByteBuf`), open with `ServerPlayer.openMenu(MenuProvider)`, bind screen via `RegisterMenuScreensEvent`; a menu needs two constructors (server with real data, client with dummy `ContainerLevelAccess.NULL` and `SimpleContainerData`), and 100 open-menu ids cycle per player. Fabric: `ScreenHandlerType`/`ExtendedScreenHandlerType` (server writes opening data with `writeScreenOpeningData`), `HandledScreens.register` ([Fabric tutorial](https://wiki.fabricmc.net/tutorial:screenhandler)). Higher-level libs: [LibGui](https://github.com/cottonmc/libgui) (widget tree with `WGridPanel` snapping to the 18px slot grid, `WItemSlot`, themes via JSON), [owo-lib owo-ui](https://www.curseforge.com/minecraft/mc-mods/owo-lib) (declarative flow/grid layouts with `gap`, XML definitions hot-reloadable at runtime, surfaces for 9-slice panels), [Cloth Config](https://shedaniel.gitbook.io/cloth-config/) (stale, screen-only) and [YACL](https://docs.isxander.dev/yet-another-config-lib) (builder-based option screens styled like Sodium), MaLiLib (Masa's shared widget/config/hotkey lib).

**Modder pain points** recurring across forums: hardcoded pixel coordinates duplicated between menu (slot x/y) and screen (texture blit); slots "a few pixels off" after scale changes ([forum](https://www.minecraftforum.net/forums/mapping-and-modding-java-edition/minecraft-mods/modification-development/2579834-gui-slots-in-wrong-position-when-scaling)); `quickMoveStack` boilerplate re-implemented per menu with easy off-by-one index bugs; every menu needing two constructors (server and client) and manual `ContainerData` count checks; 16-bit property truncation; texture API churn (`blit` -> `blitSprite`, `GuiGraphics` rename, 1.20.2 sprite split, 1.21.2 `RenderType` parameter) breaking every GUI mod each release; no layout system in vanilla (only `LinearLayout`/`GridLayout`/`FrameLayout` helpers added in 1.19.4+); progress arrows drawn wrong because partial blits need both the UV sub-rect and the width computed from `progress * 24 / total`.

## 4. Screen catalogue and reusable widgets

| Screen | Distinctive widgets |
|---|---|
| Player inventory | 2x2 craft grid, armor slots with silhouette icons, offhand, 3D player preview that follows mouse, recipe book toggle |
| Crafting table | 3x3 grid, result slot, recipe book side panel (search box, craftable-only toggle, category tabs, paged grid) |
| Chest / double chest | pure 9x3 / 9x6 grid, background height 166 / 222 |
| Furnace / smoker / blast | flame (14x14 at 56,36, fills bottom-up from lit-time int) and arrow (24x17 at 79,34, fills left-to-right from progress int), recipe book |
| Brewing stand | vertical bubble animation and downward arrow driven by brew time; fuel bar |
| Anvil | `EditBox` rename field, "Enchantment cost" label turning red when too expensive, result slot |
| Enchanting | 3 hover buttons with SGA glyph text, lapis cost, book model, tooltips reveal one enchantment |
| Villager trading | scrollable list of trade buttons (7 visible, 6-px scrollbar) with price arrows, level progress bar, XP |
| Creative | 11 category tabs above/below (tab sprites, top vs bottom variants), search `EditBox`, 9x5 visible grid with 12x15 scroller, destroy-item slot, saved hotbar tab (C/X + number), survival tab ([Creative inventory](https://minecraft.wiki/w/Creative_inventory)) |
| Beacon | tiered effect selection buttons, confirm/cancel icon buttons, payment slot max 1 |
| Loom / stonecutter | scrollable icon grid of results (16-px cells, 4 columns), selected recipe highlighted, `Set Container Property` for selection |
| Hopper | 5 slots, background height 133 |
| Horse / llama | conditional slots (saddle/armor/chest) via `isActive`, mob preview |
| Shulker box | 9x3 with shulker-specific title, contents also shown in item tooltip |

Vanilla reusable widgets: `Button`, `ImageButton`, `CycleButton` (enum cycler), `Checkbox`, `AbstractSliderButton`, `EditBox`, `AbstractScrollWidget`/`AbstractSelectionList`/`ObjectSelectionList`, `Tooltip`, `TabNavigationBar`/`TabManager`, `PlainTextButton`, `StringWidget`, layouts (`LinearLayout`, `GridLayout`, `FrameLayout`, `HeaderAndFooterLayout`). Widgets implement `Renderable`, `GuiEventListener` and `NarratableEntry` (accessibility narration). Progress bars are always "int property + partial blit of a full sprite".

## 5. Bedrock JSON-UI and Ore UI

Bedrock's entire UI is data: `RP/ui/*.json` registered via `ui/_ui_defs.json` (`{"ui_defs": ["ui/my_screen.json"]}`), constants in `_global_variables.json`. Each file has a `namespace`; elements inherit via `"child_name@namespace.parent"` ([JSON UI intro](https://wiki.bedrock.dev/json-ui/json-ui-intro)). Control types: `panel`, `stack_panel`, `grid`, `collection_panel`, `scroll_view`, `label`, `image` (with `nineslice_size` int or `[x0,y0,x1,y1]`), `button` (`default_control`/`hover_control`/`pressed_control`/`locked_control`), `toggle` (`toggle_name` groups, `toggle_default_state`), `slider`, `dropdown`, `edit_box`, `input_panel`, `screen`, `factory`, `custom` (renderer: `inventory_item_renderer`, `hover_text_renderer`, `progress_bar_renderer`, `paper_doll_renderer`, `hotbar_renderer`, `gradient_renderer`, `heart_renderer`, `name_tag_renderer`, `3d_structure_renderer`). Layout via `size` (`"100%"`, `"100%c"` children-fit, `"100% - 4px"`), `offset`, `anchor_from`/`anchor_to` (9 anchors), `layer`, `alpha`, `clips_children`, `sound_name` on press ([JSON UI docs](https://wiki.bedrock.dev/json-ui/json-ui-documentation)).

Data flow is through **bindings**: `$variables` are compile-time template params; `#hardcoded_names` are engine-provided values via `"bindings": [{"binding_name": "#hud_title_text_string", "binding_type": "global"}]`, `binding_type: view` derives from expressions (`"source_property_name": "(#title = 'x')", "target_property_name": "#visible"`), `collection` binds a grid cell to a named engine collection (`inventory_items`, `hotbar_items`, `armor_items`, `furnace_ingredient_items`, `anvil_input_items`, `enchant_buttons`, `mob_effects_collection`, `boss_bars`...). Grids use `grid_dimensions [cols, rows]`, `grid_item_template`, `collection_name`, `grid_dimension_binding`, `maximum_grid_items`; cells read `#item_id_aux`. Overrides are non-destructive via a `modifications` array (`insert_front/back/after/before`, `remove`, `replace`, `move_front/back/after/before`, `swap`), so multiple packs compose ([Best practices](https://wiki.bedrock.dev/json-ui/best-practices.html)). Animations are declarative (`anim_type: size|offset|alpha|uv|flip_book|aseprite_flip_book|color|clip|wait` with `easing`, `from`, `to`, `duration`), and input via `button_mappings` from abstract ids like `button.menu_select` with `mapping_type: focused|pressed|global`. Vanilla source: [inventory_screen.json](https://github.com/ZtechNetwork/MCBVanillaResourcePack/blob/master/ui/inventory_screen.json).

**Ore UI** is Mojang's replacement: TypeScript + React on Coherent Gameface (formerly Hummingbird), with `@react-facet` observable state for per-frame updates without React re-render, MIT licensed ([Mojang/ore-ui](https://github.com/Mojang/ore-ui), [Ore UI wiki](https://minecraft.wiki/w/Ore_UI)). First shipped in beta 1.16.100.50 (July 2020, achievements screen), now covers sign-in, play, create/edit world, settings, death, profile, Realms and more; JSON UI is deprecated and Ore UI screens are *not* resource-pack moddable, a notable regression in openness. Community reverse-engineering docs: [core-ui-docs](https://github.com/Luminoso-256/core-ui-docs).

## 6. Data-driven content

Datapacks put recipes (`data/<ns>/recipe/*.json` with `type: crafting_shaped` `pattern`/`key`, `crafting_shapeless`, `smelting` with `cookingtime`/`experience`, `stonecutting`, `smithing_transform`), tags (`#minecraft:planks` used as ingredients), loot tables and advancements in JSON ([1.20.5](https://minecraft.wiki/w/Java_Edition_1.20.5)). Items since 1.20.5 are `{id, count, components}` where components is a typed map (`max_stack_size`, `max_damage`, `damage`, `unbreakable`, `custom_name`, `item_name`, `lore`, `rarity`, `enchantments`, `enchantment_glint_override`, `tooltip_display`, `container`, `bundle_contents`, `food`, `tool`, `equippable`, `use_cooldown`, `dyed_color`, `repair_cost`, `custom_model_data`, `custom_data`), with item ids implying a default component set and stacks storing only the patch; legacy NBT `tag` migrated into `custom_data` ([Data component format](https://minecraft.wiki/w/Data_component_format), [Fabric custom components](https://docs.fabricmc.net/develop/items/custom-data-components)). This is essentially an ECS-style typed component bag per item stack, which maps naturally onto Rust.

```json
{ "id": "minecraft:diamond_sword", "count": 1,
  "components": { "minecraft:damage": 100, "minecraft:enchantments": { "minecraft:sharpness": 3 } } }
```

## 7. Open-source references

- **[valence](https://github.com/valence-rs/valence)** (Rust, Bevy ECS server). `valence_inventory` is the best Rust model: `Inventory { title, kind: InventoryKind, slots: Box<[ItemStack]>, changed: u64 /*bitmask*/, readonly }`; `InventoryKind` enum of 25 window kinds with fixed slot counts (Generic9x1..9x6, Furnace, Beacon, Hopper, Player = 46); `OpenInventory { entity, client_changed }` component links a client to a container entity; `CursorItem`, `HeldItem`, `ClientInventoryState { window_id (mod 100), state_id, slots_changed, client_updated_cursor_item }`. `ClickSlotEvent { client, window_id, state_id, slot_id, button, mode: ClickMode, slot_changes, carried_item }` and `DropItemStackEvent { client, from_slot, stack }` are the public events. `InventoryWindow`/`InventoryWindowMut` present the combined view (container slots then player main) and translate ids via `convert_to_player_slot_id`. `validate_click_slot_packet` enforces per-`ClickMode` rules (Click, ShiftClick, Hotbar, CreativeMiddleClick, DropKey, Drag, DoubleClick) and a conservation-of-mass check (`calculate_net_item_delta`); on any mismatch it resyncs with a full `InventoryS2c`. Per-tick systems `update_player_inventories`/`update_open_inventories` send whole-inventory (`changed == u64::MAX`) or per-slot deltas from the bitmask ([lib.rs](https://raw.githubusercontent.com/valence-rs/valence/main/crates/valence_inventory/src/lib.rs), [validate.rs](https://raw.githubusercontent.com/valence-rs/valence/main/crates/valence_inventory/src/validate.rs), [docs.rs](https://docs.rs/valence_inventory)).
- **Luanti (Minetest)** formspecs: a string DSL (`list[current_player;main;0,3.6;8,1;]`, `listring[]` for shift-click routing between lists, `listcolors[]`, `background9[...;middle]` 9-slice, `formspec_version`) with server-authoritative inventory and `allow_metadata_inventory_put/take` callbacks ([Formspec API](https://api.luanti.org/formspec/), [Modding book](https://rubenwardy.com/minetest_modding_book/en/players/formspecs.html)); [unified_inventory](https://github.com/minetest-mods/unified_inventory) is the reference "creative inventory with tabs and search" built on it.
- Rust clients: [rustcraft](https://github.com/c2i-junia/rustcraft) (Bevy, client/server/shared split, working hotbar, ~200 stars), [voxelcraft](https://github.com/jtbirdsell/voxelcraft) (wgpu; hotbar with counts, durability bars, quick-move), [bevycraft](https://github.com/efimish/bevycraft), [voxel-world-rust](https://github.com/TriForMine/voxel-world-rust). azalea (Rust client library) reimplements the vanilla `Menu` slot logic and `ClickOperation` enum client-side and is worth reading for the pickup/split/swap state machine; wgpu-mc is renderer-only; feather is archived; Cuberite (C++) has a full server-side window/slot-area model (`cWindow`, `cSlotArea`) worth skimming for shift-click routing.

## Key takeaways for a Bevy plugin set

1. Separate a server/logic **Menu** (slots, carried stack, click rules, quick-move) from a client **Screen** (layout, rendering, input); make the Menu usable headless and testable without rendering.
2. Model clicks as a small enum mirroring `ClickType` x button, and implement all seven modes once, generically, over a slot list; individual menus only supply `may_place`, `may_pickup`, `max_stack`, `on_take`, and a quick-move routing table (like Luanti's `listring`).
3. Represent an inventory view as *container slots followed by player slots* with index translation, exactly as vanilla and valence do; keep a `changed` bitmask per inventory for cheap delta sync and dirty-redraw.
4. Item stacks as `{id, count, component patch}` with mergeability = same id and same patch; expose `max_stack_size` as a component so 64/16/1 are data, not code.
5. Use a virtual-pixel canvas with an integer GUI scale (Auto = fit 320x240) and snap everything to integers; provide the 18px slot grid as a first-class layout primitive.
6. Treat 9-slice as metadata attached to a sprite (`stretch | tile | nine_slice {border, stretch_inner}`) rather than as widget code, so skins can retarget panels, buttons, tooltips and scrollbars by swapping assets and a sidecar file.
7. Ship the vanilla-equivalent widget set: button (3 states + focus), toggle/cycle button, slider, text field, scrollable list, tab bar, progress bar (int-driven partial blit, both directions), tooltip with gradient frame.
8. Progress and other menu ints should flow through explicit sync'd properties (like `ContainerData`), not by re-reading world state from the UI; use full-width integers, not shorts.
9. Do client-side prediction for clicks with a state id and full-resync fallback, and make the validation a pure function with a conservation-of-items check so desync/ghost items are structurally impossible.
10. Adopt Bedrock JSON-UI ideas for data-driven screens: named collections bound to grids, template inheritance, `modifications` patch lists so multiple packs/mods compose, and abstract input actions rather than raw keys.
11. Keep slot layout in one place consumed by both logic and rendering to avoid the vanilla "coordinates duplicated in menu and screen" pain.
12. Make `is_active` / conditional slots and per-slot background icons part of the slot definition (horse, armor, creative tabs all need it).
13. Provide a creative-style paged/scrollable grid with search over a virtual item list, not real slots; it is a different widget from a container grid.
14. Item decorations (count text with shadow, 13x2 durability bar with HSV ramp, cooldown overlay) belong to an `ItemStack` renderer that any slot widget reuses.
15. Define recipes, tags and item defaults as JSON/RON assets loaded through Bevy's asset system, mirroring datapacks, so crafting-table and furnace menus are generic over recipe types.
