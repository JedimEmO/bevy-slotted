# Modern Inventory / Container UI Moodboard Research

Scope: Minecraft's interaction model (slot grid, hotbar, chests, crafting, machine GUIs, NEI-style item browser) with a modern visual skin, built as Bevy plugins. Research date: 2026-09-05.

---

## 1. Reference games

### Hytale (most relevant)
- **Idiom:** Stylized, flat-with-soft-depth panels in a muted dark palette with warm accent; item art is rendered 3D models on rounded slots rather than pixel sprites. Mojang-adjacent proportions (2-row hotbar area, big central inventory) but softer corners and calmer contrast.
- **Published design info:** The 2018 UI sneak peek states the goal of making it "as easy as possible for you to assess the strengths and weaknesses of your gear on the fly" via tooltips, and describes "incorporating crafting and processing directly into the inventory screen" (pocket crafting) so players can "quickly assemble the gear you need." https://hytale.com/news/2018/12/a-sneak-peek-at-hytale-s-user-interface
- The Spring 2025 update confirms "the UX team working on inventory and hotbar upgrades" and "a refreshed approach to inventory and hotbars." https://hytale.com/news/2025/3/2025-03-28-spring-2025-development-update
- Post-launch (EA Jan 13 2026): Update 5 (May 2026) added cursor-position highlighting on all item grids ("inventory, backpack, creative inventory, and workbench interfaces"); the July 2026 Chapter 1 preview shipped redesigned item tooltips with "better readability and styling." https://hytale.com/news/2026/5/update-5-patch-notes , https://hytale.com/news/2026/7/first-look-chapter-1-and-more
- **Juice:** hover highlight follows the cursor across grids, tooltips are a styled card (name, rarity line, stats, flavor) rather than a bare text box. Community mod HyUI demonstrates the GUI system is data-driven and skinnable. https://www.curseforge.com/hytale/mods/hyui , https://hytale-docs.pages.dev/gui/
- **Takeaway:** This is the proof that "Minecraft interaction + modern skin" is a coherent product. Crafting folded into the inventory screen is the single most copied idea.

### Minecraft Bedrock: Ore UI
- **Idiom:** React + TypeScript on Coherent Gameface, "rounded button corners, green accent colors (hero buttons), white secondary buttons, red destructive buttons," translucent dark panels with responsive scaling, "clearer layouts, updated controller hints." Screens migrated so far are menus, settings, death/sleep, social; **"container screens and inventory have not yet been migrated"** but "ddui" storage-block screens exist in previews. https://minecraft.wiki/w/Ore_UI
- Java resource-pack recreations ("Bedrock Ore-UI JE", "OreUI Recreation") show what a rounded, flat, translucent skin looks like on Java's container screens. https://modrinth.com/resourcepack/bedrock-ore-ui-je , https://www.planetminecraft.com/texture-pack/oreui-recreation/
- **Juice:** Mostly static; focus ring and button hover are the main motion. Reported console micro-freezes are a caution about web-tech UI cost.

### Minecraft Java modern-UI mods / clients
- **Modern UI (BloCamLimb):** "smooth font, animations, emoji, blur effect," Gaussian-blurred and tinted background behind GUIs, rounded anti-aliased tooltips, SDF-shader rounded rects/circles/rings. 8.5M+ downloads means players actively want this skin. https://github.com/BloCamLimb/ModernUI-MC , https://www.curseforge.com/minecraft/mc-mods/modern-ui
- Lunar/Feather/Badlion, Sodium/Iris settings, FancyMenu: dark translucent panels, thin 1px borders, flat toggles, subtle hover brightening. The "settings look" is dark glass + accent color + grotesque type.
- **Takeaway:** blur + rounded tooltip + smooth font is the minimum viable "modern" for the Minecraft audience.

### Vintage Story
- **Idiom:** Paper/parchment skeuomorphism, warm browns, hand-drawn frames. Tab key toggles the secondary panel (crafting grid / backpack / container). Handbook (H) is an in-game wiki with recipe pages. https://wiki.vintagestory.at/Crafting
- **Juice:** minimal; strength is information design (handbook), not motion.

### Valheim / Enshrouded / Palworld
- Valheim: flat, dark, low-chroma slots with a wood-and-parchment frame; the modding scene (QuickStackStore, InventorySlots) shows demand for quick-stack/sort/restock buttons and controller support that vanilla lacks. https://github.com/Goldenrevolver/QuickStackStore
- Enshrouded: glassy dark panels, but its sorter "just removes gaps but leaves items fragmented," a cautionary example that a sort button must group by category. https://steamcommunity.com/app/1203620/discussions/0/4630359023402723382/
- Palworld: bright flat cards with rounded corners, big type, strong rarity color bands.

### Terraria 1.4.x
- **Juice benchmark for quick-move:** 1.4.4's Quick Stack to Nearby Chests "now has a visual effect, showing the items which are being quick stacked moving towards the chests they go into," plus "Similar Stack" mode and favoriting via Alt-click (favorite border). https://terraria.wiki.gg/wiki/1.4.4
- **Idiom:** pixel, but the interaction set (quick stack, deposit all, loot all, sort, favorite lock) is the modern chest toolbar to copy.

### Core Keeper
- Compact dark slots, rounded, category tabs; quick-stack and sort buttons on chests; hover tooltip card with stats and rarity color header.

### Satisfactory
- **Idiom:** industrial flat orange-on-dark, sharp corners, thin dividers. "Relevant items" strip highlights items in your inventory usable in the machine you're looking at; player-toggleable large/small slot size. https://satisfactory.wiki.gg/wiki/Inventory
- **Juice:** machine GUIs animate progress bars, ctrl-click transfers all, hover tooltips have a deliberate delay (a common complaint, so keep delays short ~150ms).

### Deep Rock Galactic
- **Idiom:** chunky industrial, saturated orange/cyan, hard angles and diagonal cuts, very readable at TV distance. https://interfaceingame.com/games/deep-rock-galactic/
- Caution: a 2023 "bigger UI" pass got pushback for looking like "a console experience on PC monitors"; provide UI scale options.

### No Man's Sky
- **Idiom:** flat, high-chroma, rounded-rect slots with a soft inner glow; hold-to-confirm radial fills.
- **Juice:** icons pop when picked up (a bug made them scale 2x under non-uniform UI scale, illustrating that motion must run in UI-space units). https://steamcommunity.com/app/275850/discussions/0/1631916887506111229/

### Destiny 2
- **Idiom:** thin white hairlines on dark translucent panels, rarity as a solid colored tile behind a 3D-rendered icon, big hover detail card with perks.
- **Juice:** David Candland's GDC talk "Tenacious Design and The Interface of Destiny": free cursor on gamepad; "your cursor slows down slightly and the item/box bulges slightly, to impart a sense of stickiness." Also covers the icon creation pipeline. https://gdcvault.com/play/1023460/Tenacious-Design-and-The-Interface , http://www.cand.land/destiny/

### Diablo IV / Path of Exile 2
- Diablo IV Feb 2020 quarterly: painterly icons "didn't communicate clearly at small sizes," so they moved to icons "more directly based off the 3D models to give them natural texture and realism"; they "toned down the brightness and saturation of the icon backgrounds" and "added secondary visual cues for indicating rarity via the border decoration" for accessibility. Controller on PC uses one unified UI with "controller-friendly shortcuts or alternate flows." https://news.blizzard.com/en-us/article/23308274/diablo-iv-quarterly-updatefebruary-2020
- PoE2 redesigned the console menu as "a full-screen experience that provides a preview of your character," with fast-equip and fast-use shortcuts for controller. Multi-cell item sizes remain (1x1 to 2x4). https://www.mmogah.com/news/poe/path-of-exile-2-on-console-developer-diary

### Hades / Hades II
- **Idiom:** painted illustration, gold hairline frames, boon cards with god-colored gradients.
- **Juice:** cards slide in staggered, hover scales ~1.05 with a bright rim, selection has a satisfying chime; PS5 light bar matches boon color with haptics. The team is "heavily iterative," building motion in-engine rather than in docs. https://www.gamesradar.com/games/hades/the-game-is-the-design-document-hades-2-devs-dont-have-long-elaborate-plans-that-lay-out-the-future-of-the-things-were-making-because-supergiant-is-a-heavily-iterative-studio/

### Escape from Tarkov / Resident Evil 4 remake (tetris grids)
- Tarkov: "size and shape are dependent on equipment"; the same 2D grid mental model is reused across stash, traders, and raids. Drag ghost shows a green/red footprint. https://www.heiolenmarkus.com/blog/escape-from-tarkov-menu-ux-redesign
- RE4R attaché case: items "snap into place," rotate, and auto-arrange; case is a physical object with a satisfying open animation and per-item placement click. https://residentevil.fandom.com/wiki/Attache_Case
- Relevance: Minecraft slots are 1x1, but the ghost-footprint-with-validity-color pattern is worth borrowing for drag feedback.

### Stardew Valley
- Layered disclosure: "hovering over objects in menus like crafting displays a dialog box with more details." Known flaw: tooltips clip off-screen; clamp tooltips to the viewport. https://medium.com/swlh/deceptively-simple-design-cabde40af87f

### Subnautica / Astroneer
- Subnautica: PDA diegetic tablet, cyan-on-dark flat, hexagonal-ish icons. https://www.artstation.com/artwork/aYK35J
- Astroneer: fully diegetic holographic backpack, "the backpack spins off and zooms in," no HUD. https://www.gamedeveloper.com/business/an-analysis-of-astroneer-s-ui-system

### Factorio 2.0 Space Age / Dyson Sphere Program
- Factoriopedia (FFF-397): items, recipes, entities "merged into one entry"; "ALT + Left click on any in-game object" opens its page; back/forward history; shows "ingredients, usage information, required technologies." This is exactly a modern NEI. https://www.factorio.com/blog/post/fff-397
- FFF-426 split recipe info from output slots in the assembler GUI for clarity. https://factorio.com/blog/post/fff-426
- DSP: flat dark-blue panels, cyan accents, thin lines; community criticism is about too many separate windows, arguing for combined inventory+crafting. https://steamcommunity.com/app/1366540/discussions/0/4845401032850996413/

### Nightingale / Grounded
- Nightingale: "a more modern approach to the Victorian Era style," accessible and scalable, with illustrated schematic cards. Community found the first UI "crowded." https://tameta.ca/nightingale-ui-design-showcase
- Grounded: rounded, bright, soft-shadow cards; kid-friendly high legibility.

---

## 2. Design vocabulary

- **Glassmorphism over a 3D world:** blur alone fails on busy scenes; pair blur with a 10-30% tint overlay and one shared border token; text needs 4.5:1 against the tinted panel, not against the raw scene. Use glass for the outer panel only, keep slots opaque. https://ixdf.org/literature/topics/glassmorphism , https://figr.design/blog/glassmorphism-0e8b1
- **Depth:** soft shadows (2-3 elevation levels: panel, popover, drag ghost) replace Minecraft's bevels; a 1px inner top highlight keeps a hint of physicality.
- **Slots:** rounded 4-6px radius at 40px reads modern without looking like a phone app; square-with-2px reads industrial (Satisfactory).
- **Rarity color systems:** WoW grey/white/green/blue/purple/orange, Diablo similar with gold uniques, Destiny white/green/blue/purple/gold. Diablo's lesson: desaturate backgrounds, encode rarity in border decoration too. Never color alone (XAG). https://learn.microsoft.com/en-us/gaming/accessibility/xbox-accessibility-guidelines/102
- **Progressive disclosure:** compact tooltip (name, count, rarity) at 120-150ms hover; expanded card (stats, recipes, "used in") on Shift or dwell 600ms; Alt-click opens the encyclopedia page (Factorio).
- **Item cards:** the browser panel should render items as cards with 3D icon, name, mod badge, and rarity strip rather than bare 16px sprites.
- **Motion:** Material 3 tokens are a reasonable baseline: standard enter 200ms cubic-bezier(0.2,0,0,1), emphasized 280ms; short durations (50-200ms) for small components, longer for panels; M3 Expressive adds spring physics and shape morphing. https://m3.material.io/styles/motion/easing-and-duration/tokens-specs
  - hover: scale 1.04, 120ms; press: scale 0.96; drop: squash 1.1x/0.9y then spring back 180ms; fly-to-slot: 200-260ms with emphasized easing; staggered reveal 20-30ms per row; drag ghost at 85% opacity following cursor with 60ms lag spring.
- **Design tokens:** 4/8 spacing grid, slot 40 or 48px, gap 4px, radii 4/8/12, elevation 0/1/2/3, one blur token.
- **Dark-first palettes:** panels at 8-14% luminance with 70-85% opacity; avoid pure black, tint toward the world's ambient hue.
- **Type:** UI grotesque (Inter, Manrope, Geist, IBM Plex Sans, Public Sans) for body; condensed display (Barlow Condensed, Oswald, Rajdhani) for headings and counts; variable fonts let stack counts tighten at small sizes. Tabular numerals for counts.
- **Icons:** 3D-rendered item icons with fixed 3-point rig and rim light (Destiny, Diablo IV, Hytale) beat flat sprites for a "modern" read; keep a flat monochrome icon set for UI chrome.
- **Accessibility:** colorblind-safe rarity with shape/border cues and user-configurable colors; text scale to 200%; visible gamepad focus ring (2px accent, 4px offset); reduced-motion toggle that shortens durations to 0-80ms and disables fly-to-slot.

---

## 3. Gamepad and cross-platform

- **Cursor-snapping grid focus** (most survival games): D-pad/stick moves focus one slot; hold to repeat; focus ring visible always. Requires per-slot focusable entities and a nav graph between panels (player inv, hotbar, container, crafting).
- **Free virtual cursor** (Destiny, Diablo PC-controller hybrid): stick moves a cursor with magnetism; the hovered slot slows the cursor and scales up. Better for mixed-density screens like an item browser, worse for pure grids.
- **Explicit action buttons in the UI:** Terraria, Core Keeper, Valheim mods expose Sort, Quick Stack, Deposit All, Loot All as visible buttons, so gamepad users get shift-click equivalents. Map to face buttons with on-screen glyph hints (Ore UI style).
- **Radial menus** for hotbar selection on stick (Hytale, Palworld, Grounded) reduce D-pad round trips.
- **Effect on slot design:** slots need three states beyond hover (focused, focused+held for split, selected-for-move); make the focus ring outside the slot so rarity borders stay visible; keep tap targets at 40px+ for Steam Deck touch.

---

## 4. Technical rendering notes

- **Backdrop blur:** do not blur per-node. Render the scene, downsample 1/4, run dual Kawase (2-3 down, 2-3 up passes, 4-5 taps each), then sample that texture in a UiMaterial for panels. Cost is a few hundred microseconds at 1080p; Gaussian scales O(k^2) or 2k separable, Kawase stays near constant with downsampling. https://blog.frost.kiwi/dual-kawase/ , https://github.com/itsRythem/ImGui-Blur
- **Rounded rects:** Bevy's `BorderRadius` and `BoxShadow` are SDF-based already, so no 9-slice atlases are needed; per-side `BorderColor` and `BackgroundGradient` (linear/conic/radial) landed in 0.17. Known edge artifact: background pixels bleeding at rounded corners when the parent has a different color (issue #17561). https://docs.rs/bevy/latest/bevy/ui/struct.BorderRadius.html , https://github.com/bevyengine/bevy/pull/18139 , https://github.com/bevyengine/bevy/issues/17561
- **Custom shaders:** `UiMaterial` lets a slot be a single quad with SDF border, glow, and animated legendary shimmer in one shader; pass rarity color and time as uniforms. https://bevy.org/examples/ui-user-interface/ui-material/
- **ViewportNode (0.17):** render a camera into a UI node; use for live-rotating 3D item previews in tooltips or the browser detail pane, not for every slot. https://bevy.org/news/bevy-0-17/
- **Icon baking:** a dedicated offscreen camera with a fixed 3-point rig (key, fill, cool rim from behind) renders each item/block to a texture atlas at 64-128px once, cached to disk; Minecraft-style items become extruded quads for the same treatment.
- **Glow:** additive layer quad behind the slot with the rarity color at 20-40% alpha, blurred SDF falloff in-shader; avoid full-screen bloom for UI.
- **Text:** Bevy uses cosmic-text with glyph atlases; crisp at UI scale but not MSDF. For huge headings consider pre-rendered MSDF via a UiMaterial or accept atlas re-rasterization. Text shadows exist for Text2d in 0.17; UI text shadow via `TextShadow`.
- **Widgets:** 0.17 headless widgets and Feathers provide Hovered/Pressed/Checked components and observer events usable for slot state without rolling custom interaction.

---

## 5. Design directions

### A. "Obsidian Glass"
- Palette: base #0B0E14, panel #141A24 at 80% + blur, hairline #2A3446, accent #7FD1FF, warm accent #FFB454, text #E6EDF3.
- Type: Inter (body, tabular counts) + Barlow Condensed (headers, hotbar numbers).
- Materials: frosted dark glass outer panel, opaque matte slots with 6px radius, 1px inner top highlight, rarity as 2px outer ring plus corner notch glyph.
- Motion: springy (M3 emphasized), 1.04 hover scale, items fly to slot on shift-click with a soft trail, fly-out particles on quick stack (Terraria).
- Chest screen: one wide glass panel, chest grid top, player grid bottom, a vertical action rail on the right (Sort, Quick Stack, Deposit, Loot) with gamepad glyphs.
- Item browser: right-side glass column with search field, category chips, 3D icon cards in a 6-column grid, detail card slides in with recipes and "used in."

### B. "Field Manual" (paper/industrial, Factorio meets Nightingale)
- Palette: paper #EDE6D6, ink #1E1B18, rule #C9BFA8, accent orange #E0672B, teal #2B8C86, rarity as ink stamps.
- Type: IBM Plex Sans + IBM Plex Mono for counts; small caps section labels.
- Materials: opaque paper panels with soft 8px shadow, square slots with 2px radius and dotted grid, no blur, icons rendered with flat top-down rig for a technical-drawing feel.
- Motion: crisp and short (120-160ms standard easing), stamp-down on drop, page-turn on browser navigation, no springs.
- Chest screen: a two-column ledger, container left, player right, a header line with chest name and capacity bar.
- Item browser: an encyclopedia with tabs, Alt-click opens the entry, back/forward arrows, entries merge item/block/recipe like Factoriopedia.

### C. "Neon Workshop" (DRG and Palworld energy)
- Palette: charcoal #1A1C22, slate #262A33, lime #C7F464, magenta #FF5FA2, cyan #38E8FF, text #F5F7FA.
- Type: Manrope (body) + Rajdhani (display, counts).
- Materials: opaque panels with diagonal cut corners, slots with 8px radius and a colored bottom bar for rarity, item icons with hard rim light and drop shadow.
- Motion: punchy, 90-140ms, squash on drop, staggered grid reveal, rarity shimmer shader on legendary, audible clicks per slot.
- Chest screen: chest as a horizontal tray with a big title, quick-actions as chunky pills under it, player inventory docked bottom with the hotbar visually fused to it.
- Item browser: full-height panel with a big search bar, category icons in a left strip, card grid with hover pop and a magenta focus ring for gamepad.

---

## Key takeaways

1. Hytale is the existence proof: Minecraft interaction, modern skin, crafting folded into the inventory, cursor-highlighted grids, styled tooltips.
2. Ore UI's visual language (rounded, flat, translucent dark, green hero buttons, controller hints) is what Mojang's own audience now expects.
3. Blur is a panel-level effect, not a slot-level one; downsample and use dual Kawase into a UiMaterial.
4. Keep slots opaque and tint glass 10-30% so text passes 4.5:1 regardless of the world behind it.
5. 3D-rendered icons with a fixed rim-lit rig are the largest single visual upgrade; bake once to an atlas.
6. Encode rarity in color plus border decoration or a glyph, with user-configurable colors (Diablo IV, Xbox guideline 102).
7. Motion budget: 120-200ms for slots, 200-280ms for panels, spring on drop, fly-to-slot on quick-move, and a reduced-motion switch.
8. Expose shift-click behaviors as visible buttons (Sort, Quick Stack, Deposit, Loot) so gamepad and touch users have parity; sort must group by category.
9. Gamepad needs a distinct focus ring outside the slot, a nav graph between panels, and optionally Destiny-style magnetic cursor for the browser.
10. The item browser should be Factoriopedia-shaped: merged entries, Alt-click deep link, history, "used in" lists.
11. Tooltips are two-tier cards: compact on hover, expanded on dwell or modifier; clamp to viewport.
12. Use Bevy 0.17 primitives directly: BorderRadius, BoxShadow, BackgroundGradient, per-side BorderColor, UiMaterial for glow/shimmer, ViewportNode for live previews, headless widget states for interaction.
13. Design tokens up front: 4/8 spacing, 40/48px slots, radii 4/8/12, three elevation levels, one blur token, one duration scale.
14. Offer UI scale and slot size options; DRG and NMS show scaling missteps get noticed fast.
15. Pick one direction and commit; glass, paper, and neon each demand different icon lighting and motion, mixing them reads as unfinished.

---

## Sources

- https://hytale.com/news/2018/12/a-sneak-peek-at-hytale-s-user-interface
- https://hytale.com/news/2025/3/2025-03-28-spring-2025-development-update
- https://hytale.com/news/2026/5/update-5-patch-notes
- https://hytale.com/news/2026/7/first-look-chapter-1-and-more
- https://hytale-docs.pages.dev/gui/
- https://minecraft.wiki/w/Ore_UI
- https://modrinth.com/resourcepack/bedrock-ore-ui-je
- https://github.com/BloCamLimb/ModernUI-MC
- https://wiki.vintagestory.at/Crafting
- https://github.com/Goldenrevolver/QuickStackStore
- https://terraria.wiki.gg/wiki/1.4.4
- https://satisfactory.wiki.gg/wiki/Inventory
- https://interfaceingame.com/games/deep-rock-galactic/
- https://gdcvault.com/play/1023460/Tenacious-Design-and-The-Interface
- http://www.cand.land/destiny/
- https://news.blizzard.com/en-us/article/23308274/diablo-iv-quarterly-updatefebruary-2020
- https://www.mmogah.com/news/poe/path-of-exile-2-on-console-developer-diary
- https://www.heiolenmarkus.com/blog/escape-from-tarkov-menu-ux-redesign
- https://residentevil.fandom.com/wiki/Attache_Case
- https://medium.com/swlh/deceptively-simple-design-cabde40af87f
- https://www.gamedeveloper.com/business/an-analysis-of-astroneer-s-ui-system
- https://www.factorio.com/blog/post/fff-397
- https://factorio.com/blog/post/fff-426
- https://tameta.ca/nightingale-ui-design-showcase
- https://ixdf.org/literature/topics/glassmorphism
- https://m3.material.io/styles/motion/easing-and-duration/tokens-specs
- https://learn.microsoft.com/en-us/gaming/accessibility/xbox-accessibility-guidelines/102
- https://blog.frost.kiwi/dual-kawase/
- https://github.com/itsRythem/ImGui-Blur
- https://bevy.org/news/bevy-0-17/
- https://docs.rs/bevy/latest/bevy/ui/struct.BorderRadius.html
- https://github.com/bevyengine/bevy/pull/18139
- https://github.com/bevyengine/bevy/issues/17561
- https://bevy.org/examples/ui-user-interface/ui-material/
