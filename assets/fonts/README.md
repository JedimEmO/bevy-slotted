# Fonts

The three shipped themes name a type family each, and these are the files
behind those names. Every one is under the SIL Open Font License 1.1; each
family's directory carries the `OFL.txt` it was published with, which is what
the licence asks for when the files are redistributed.

| Family | File | Used by |
| --- | --- | --- |
| Manrope | `manrope/Manrope-Variable.ttf` | glass `body`, neon `body` |
| Barlow Condensed | `barlow-condensed/BarlowCondensed-SemiBold.ttf` | glass `display` |
| IBM Plex Sans | `ibm-plex-sans/IBMPlexSans-Variable.ttf` | paper `body` |
| IBM Plex Mono | `ibm-plex-mono/IBMPlexMono-Regular.ttf` | paper `mono` |
| Rajdhani | `rajdhani/Rajdhani-SemiBold.ttf` | neon `display` |

`BarlowCondensed-Regular.ttf` is here too, unreferenced, because a theme that
wants a lighter label face should not have to go and fetch one.

All six come from [google/fonts](https://github.com/google/fonts) `ofl/`. The
two variable files were renamed from their upstream `Family[axes].ttf` form so
no asset path contains brackets; nothing else about them was changed.

## Where they ship

In this repository, not in a published crate. No `slotted-*` crate carries an
`include` for assets: `assets/themes/*.theme.ron` is not shipped on crates.io
either, and a game copies the directory it wants into its own `assets/`. So
these files cost a consumer nothing until they ask for them, and the crate
sizes are unchanged. `cargo deny check licenses` reads crate manifests and
therefore never sees them; the licence obligation is met by the `OFL.txt`
beside each family.

## Doing without them

A theme's `fonts` block can drop `path:` and set `system: true` instead, which
sends the family name to the OS font list and needs Bevy's
`system_font_discovery` feature. With neither, Bevy's built-in face carries the
layout and the family name is documentation.
