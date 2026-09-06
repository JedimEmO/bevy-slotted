# slotted-browser

The item and recipe browser for
[slotted](https://github.com/JedimEmO/bevy-slotted): a JEI-style overlay
that layers over any screen.

Everything is registered in ordered phases at `Startup`, indexed off the main
thread, searched with a prefix grammar, and shown in a panel docked beside the
open screen. Transfers and cheat-gives never touch inventories directly. They
trigger `slotted_ecs::MenuAction`s, so prediction and the authority see ordinary
clicks.

Which side the panel docks on, and how wide it is, comes from the exclusion
zones a screen and its injections publish, so a mod that adds a button to a
screen's header pushes the browser aside without either knowing about the other.

## Main types

| Type | What it is |
|---|---|
| `SlottedBrowserPlugin`, `BrowserConfig`, `BrowserPhase`, `BrowserSet` | The plugin, its config, and the registration and frame order. |
| `Ingredient`, `IngredientType` | What can appear in the browser. Items ship; a game adds fluids or energy. |
| `BrowserIndex`, `Entry`, `IndexState` | The search index, built off the main thread. |
| `search::*` | The prefix grammar: `@mod`, `#tag`, `$category`, plain text. |
| `Categories`, `RecipeStore`, `RecipePage` | Recipe categories and the recipe view. |
| `Bookmarks`, `Bookmark` | The bookmark strip. |
| `transfer::*` | Recipe transfer with a dry run that highlights what is missing. |
| `handlers::*` | The screen-handler registry: how the browser attaches to a screen it does not own. |
| `BrowserPanel`, `BrowserLayout`, `panel_def` | The panel's own screen tree. |

## Example

```rust
use bevy::prelude::*;
use slotted_browser::SlottedBrowserPlugin;

fn add(app: &mut App) {
    app.add_plugins(SlottedBrowserPlugin);
}
```

The facade turns this on with its `browser` feature, which is on by default.

## Feature flags

| Feature | Default | Effect |
|---|---|---|
| `dev` | no | Exclusion-zone highlighter, id tooltips, copy-recipe-id. No extra dependencies. |

## Licence

MIT OR Apache-2.0, at your option.
