# slotted-theme

Design tokens, themes and motion for
[slotted](https://github.com/mathiasmyrland/bevy-slotted).

A `Theme` is a RON asset mapping semantic `Role`s (`panel`, `slot`,
`slot.hover`, `tooltip.frame`) to `Material`s, plus a token table for spacing,
radii, elevation, durations, blur and colours. Widgets never name a colour. They
carry a `Themed` role and the apply system paints them whenever the theme loads,
changes on disk or is swapped.

Roles are dotted and fall back to their parent, so a theme that does not define
`slot.hover` gets `slot`.

## Main types

| Type | What it is |
|---|---|
| `Theme`, `ThemeLoader`, `ActiveTheme` | The asset, its `*.theme.ron` loader, and the handle currently painted. |
| `Role`, `roles::*` | The 36 well-known role names. Free-form strings, so a game can add its own. |
| `Material` | `Solid`, `Gradient`, `Sliced`, `Glass`, `Shader`, `Text`. |
| `Tokens`, `Spacing`, `Radii`, `Elevation`, `Durations`, `Blur` | The token table. |
| `Themed`, `Paint`, `apply_theme` | The component a widget carries, the pure material-to-Bevy translation, and the system. |
| `Motion`, `MotionPreset`, `Tween`, `ActiveMotions` | The duration scale, the reduced-motion switch, and the live-tween count `slotted-test` waits on. |

## Example

```rust
use bevy::prelude::*;
use slotted_theme::prelude::*;

fn setup(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(ActiveTheme(assets.load("themes/glass.theme.ron")));
}

fn a_themed_panel(mut commands: Commands) {
    commands.spawn((Node::default(), Themed::new("panel")));
}
```

Reduced motion is one resource:

```rust
use bevy::prelude::*;
use slotted_theme::Motion;
fn reduce(mut commands: Commands) {
    commands.insert_resource(Motion::REDUCED);
}
```

## Feature flags

| Feature | Default | Effect |
|---|---|---|
| `blur` | no | Backdrop blur for `Material::Glass`. A second camera renders the scene into a quarter-resolution image that a `UiMaterial` samples. Pulls in `bevy_render`; about one extra millisecond per frame while a glass screen is open (ADR 0003). Without it, `Glass` still deserialises and renders as a solid tint. |

## Licence

MIT OR Apache-2.0, at your option.
