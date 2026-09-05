# Phase 3, package A: implementation notes

Logic package for `slotted-browser` (ingredients, categories, recipes, index,
search, bookmarks, transfer, handlers, runtime, validation). Everything the
contract (`docs/design/phase3-contract.md`) specifies is implemented; this file
records only the places where the implementation had to decide something the
contract left open, or where it departs from the letter of the contract.

## Shared files touched

`lib.rs`, `plugin.rs`, `events.rs` and `Cargo.toml` were **not** changed. No
additive change to them turned out to be necessary.

## Additive changes inside package A's own files

- `BrowserIndex` gained `type_order: Vec<IngredientTypeId>`, the ingredient
  types in registration order. `SortStage::IngredientType` needs a rank and
  `sort` only sees the index, so the index carries it. `BrowserIndex::type_rank`
  reads it. `Entry` is unchanged.
- `StringBitsetMap` gained a second `BTreeMap` filed by the part after the first
  `:`. That is what makes `@demo` match `slotted:demo` and `#tools` match
  `c:tools`, in the same `O(log d + matches)` range scan.
- `RecipeStore` gained a shared `RwLock<HashMap<(RecipeRef, Option<Ingredient>),
  Arc<RecipeLayout>>>` for the layout memo, plus `layout_shared` (no final
  clone) and `clear_layout_cache`.
- `Subtypes::remove` and `ScreenHandlers::remove`, so the phase validators can
  drop a dangling registration the way `TransferHandlers::remove` already did.
- `InfoType` now holds its pages (`InfoType::new`). See the deviation below.

## Deviations and open decisions

1. **Info pages reach `InfoType` through the type, not `IngredientCtx`.** The
   contract puts info entries in an `InfoPages` resource but gives
   `IngredientType` methods only an `IngredientCtx { registries, subtypes }`.
   Rather than widen that signature (which needs a contract amendment),
   `InfoType` carries an `Arc<Vec<InfoPage>>` and `index::build::start_index_build`
   re-registers it from `InfoPages` before every build. A page registered in any
   phase is therefore indexed, including after `RebuildBrowser`.

2. **Layouts memoise on the whole focus, not on `focus.is_some()`.** The
   contract says "memoised per (recipe, focus is Some)". Keying on the boolean
   would serve one focus's alternative ordering to a different focus, so the key
   is `(RecipeRef, Option<Ingredient>)`.

3. **`SimpleTransfer` returns the remainder only when there is one.** The
   contract's "or `Pickup{first empty source, Left}` when `src` was emptied"
   describes a case that cannot arise: the plan picks the whole source stack up,
   so `src` is always empty when the remainder goes back, and when the source
   was fully consumed the cursor is empty and no return click is needed. Emitting
   one anyway would be an action `apply_click` refuses. `max` repeats whole sets,
   grouped by source within each set, capped by the smallest input cell's stack
   cap.

4. **`GridNotEmpty` covers an occupied cell the recipe does not use.** A grid
   slot holding a stack that is not among the alternatives of the input mapped to
   that cell refuses the transfer, and a cell the recipe does not fill has no
   alternatives, so anything in it refuses too. A cell that already holds a
   matching stack is accepted and still receives its item.

5. **`TagType::icon` stays `IconRef::Missing`.** `IngredientType::icon` sees only
   an `IconSource`, not the registries, so a tag cannot resolve a member to draw.
   The panel cycles member icons instead; the contract already gives it that job.

6. **`ItemType::tooltip_text` is name, rarity and `slotted:*` component keys**, as
   the contract says, plus the subtype key when the entry has one, so a subtype
   variant is reachable by `$`.

7. **`validate_screen_handlers` skips an empty `Screens`.** An app with no screens
   at all (a headless test, a dedicated server) is not a misregistration, so the
   check only runs once at least one screen kind exists. Under
   `BrowserConfig::strict` a real dangling handler still panics.

8. **`execute_transfers` and `execute_gives` find the menu by screen root.** They
   take the first `ScreenRoot` that drives a menu and is not the browser's own
   panel kind (`slotted:browser`). The contract gives package A no other handle
   on which screen the panel is docked to.

9. **`apply_navigation` opens on the first category that has recipes.** A focus
   nothing produces (or consumes) leaves the recipe view as it was rather than
   opening an empty page. `RecipeNav::Page` wraps at both ends of the open
   category's recipe list; `RecipeNav::Category` is ignored for a category with
   no recipes for the current focus.

## Tests

`tests/common/mod.rs` duplicates the eleven-item fixture and the three demo
recipes that `slotted_test::TestRegistries::basic()` holds. The dependency runs
the other way (`slotted-test` depends on `slotted-browser`), so the browser's own
tests cannot import it. Keep the two tables in step.

## Left for a later phase

`plugin::rebuild_on_request` still carries its `PHASE3-IMPL: A` marker. On
`RebuildBrowser` it rebuilds `RecipeStore` and restarts the index build, which is
everything Phase 3 needs. Re-running the `Startup` phase validators is hot
reload, which the contract puts in Phase 4, and it needs the schedule to be
re-runnable rather than a change in this file. `plugin.rs` is shared, so it was
left untouched.

## Measured

`benches/index.rs`, release, 50 000 synthetic items across ten namespaces and
ten tags:

| Benchmark | Time |
|---|---|
| `index_build_50k` | 476 ms |
| `substring_index_build_50k` | 103 ms |
| `index_query_50k` (three terms, one OR group, one negation) | 2.7 ms |

The first cut measured 927 ms for the build. `SubstringIndex` now precomputes
each position's text end (`ends`), so a suffix is a slice instead of a scan to
the next boundary on every comparison in the sort; that halved the build.

## Fixed by the integration review

- **`a | b` was an AND.** `search::scan`'s `end_term` cleared the pending "a `|` came before this
  term" flag whenever it was called, including for the whitespace that follows a `|`. Spaces
  around the operator are ordinary in a typed query, so the flag now survives a call that pushes
  no term.
- **The edit-mode blacklist could not hide anything.** `HiddenEntries` derives `Default`, whose
  `Bitset` addresses zero bits, and `Bitset::insert` ignores an out-of-range index, so
  `set_hidden` was a silent no-op in a running app. `Bitset::grow` was added and `set_hidden`
  grows the set to fit the id, which works whether the blacklist is edited before or after the
  index lands.
