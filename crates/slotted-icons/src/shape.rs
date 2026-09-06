//! The shape vocabulary, shared by the two bakes.
//!
//! A [`ShapeKind`] is one silhouette drawn from one fixed viewpoint: three
//! quarters from above and to the left, exactly the angle the GPU rig puts
//! its meshes at. That is what lets the CPU bake and the GPU bake produce the
//! same reading of the same item, and it is why the CPU bake is a set of flat
//! polygons rather than a rasteriser: at 64 px the two are indistinguishable
//! once the rim light is on.
//!
//! Faces are listed back to front with a brightness each, in cell-normalised
//! coordinates (`0..1`, y down). [`silhouette`] is the union used for the rim
//! test.

use slotted_registry::icon::{IconColor, ShapeIcon, ShapeKind};

/// Which colour a face takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paint {
    /// The body colour.
    Body,
    /// The accent colour, falling back to the body colour.
    Accent,
    /// The accent colour, and the face is skipped entirely when the icon
    /// declares no accent. This is what an inlay band is: decoration that
    /// would be invisible in one colour.
    AccentOnly,
}

/// One flat face of a shape: a convex polygon and how much of the key light
/// reaches it.
pub struct Face {
    /// Corners, in cell-normalised coordinates.
    pub points: &'static [(f32, f32)],
    /// Multiplier on the body colour. 1.0 is fully lit.
    pub brightness: f32,
    /// Which colour it takes.
    pub paint: Paint,
}

const fn face(points: &'static [(f32, f32)], brightness: f32) -> Face {
    Face {
        points,
        brightness,
        paint: Paint::Body,
    }
}

const fn band(points: &'static [(f32, f32)], brightness: f32) -> Face {
    Face {
        points,
        brightness,
        paint: Paint::AccentOnly,
    }
}

const fn head(points: &'static [(f32, f32)], brightness: f32) -> Face {
    Face {
        points,
        brightness,
        paint: Paint::Accent,
    }
}

// A block seen corner-on: a rhombus lid over two side faces.
const CUBE_TOP: [(f32, f32); 4] = [(0.50, 0.14), (0.86, 0.33), (0.50, 0.52), (0.14, 0.33)];
const CUBE_LEFT: [(f32, f32); 4] = [(0.14, 0.33), (0.50, 0.52), (0.50, 0.88), (0.14, 0.69)];
const CUBE_RIGHT: [(f32, f32); 4] = [(0.86, 0.33), (0.86, 0.69), (0.50, 0.88), (0.50, 0.52)];
const CUBE_BAND: [(f32, f32); 4] = [(0.14, 0.47), (0.50, 0.66), (0.50, 0.76), (0.14, 0.57)];

// The same block at half height.
const SLAB_TOP: [(f32, f32); 4] = [(0.50, 0.30), (0.86, 0.49), (0.50, 0.68), (0.14, 0.49)];
const SLAB_LEFT: [(f32, f32); 4] = [(0.14, 0.49), (0.50, 0.68), (0.50, 0.86), (0.14, 0.67)];
const SLAB_RIGHT: [(f32, f32); 4] = [(0.86, 0.49), (0.86, 0.67), (0.50, 0.86), (0.50, 0.68)];
const SLAB_BAND: [(f32, f32); 4] = [(0.14, 0.56), (0.50, 0.75), (0.50, 0.81), (0.14, 0.62)];

// A tapered bar: a small lid, a tall left face, a short right one.
const INGOT_TOP: [(f32, f32); 4] = [(0.50, 0.32), (0.80, 0.45), (0.50, 0.58), (0.20, 0.45)];
const INGOT_LEFT: [(f32, f32); 4] = [(0.20, 0.45), (0.50, 0.58), (0.50, 0.74), (0.20, 0.61)];
const INGOT_RIGHT: [(f32, f32); 4] = [(0.80, 0.45), (0.80, 0.61), (0.50, 0.74), (0.50, 0.58)];
const INGOT_BAND: [(f32, f32); 4] = [(0.20, 0.50), (0.50, 0.63), (0.50, 0.69), (0.20, 0.56)];

// An octahedron seen corner-on: four facets meeting at the waist.
const GEM_UPPER_LEFT: [(f32, f32); 3] = [(0.50, 0.10), (0.50, 0.50), (0.17, 0.42)];
const GEM_UPPER_RIGHT: [(f32, f32); 3] = [(0.50, 0.10), (0.83, 0.42), (0.50, 0.50)];
const GEM_LOWER_LEFT: [(f32, f32); 3] = [(0.17, 0.42), (0.50, 0.50), (0.50, 0.90)];
const GEM_LOWER_RIGHT: [(f32, f32); 3] = [(0.50, 0.50), (0.83, 0.42), (0.50, 0.90)];
const GEM_BAND: [(f32, f32); 4] = [(0.17, 0.42), (0.50, 0.50), (0.83, 0.42), (0.50, 0.56)];

// A shaft from the lower left to the upper right, with a head on top.
const ROD_SHAFT: [(f32, f32); 4] = [(0.17, 0.79), (0.25, 0.86), (0.74, 0.33), (0.66, 0.26)];
const ROD_SHAFT_LIT: [(f32, f32); 4] = [(0.17, 0.79), (0.21, 0.825), (0.70, 0.295), (0.66, 0.26)];
const ROD_HEAD: [(f32, f32); 4] = [(0.55, 0.28), (0.72, 0.10), (0.90, 0.25), (0.73, 0.43)];
const ROD_HEAD_LIT: [(f32, f32); 3] = [(0.55, 0.28), (0.72, 0.10), (0.815, 0.185)];

/// The faces of a shape, back to front.
#[must_use]
pub fn faces(kind: ShapeKind) -> &'static [Face] {
    // Brightness comes from one rig: a key from the upper left, a dim fill
    // from the lower right. A lid catches all of the key, the left face most
    // of it, the right face only the fill.
    const CUBE: [Face; 4] = [
        face(&CUBE_TOP, 1.0),
        face(&CUBE_LEFT, 0.74),
        face(&CUBE_RIGHT, 0.50),
        band(&CUBE_BAND, 0.74),
    ];
    const SLAB: [Face; 4] = [
        face(&SLAB_TOP, 1.0),
        face(&SLAB_LEFT, 0.74),
        face(&SLAB_RIGHT, 0.50),
        band(&SLAB_BAND, 0.74),
    ];
    const INGOT: [Face; 4] = [
        face(&INGOT_TOP, 1.0),
        face(&INGOT_LEFT, 0.72),
        face(&INGOT_RIGHT, 0.48),
        band(&INGOT_BAND, 0.72),
    ];
    const GEM: [Face; 5] = [
        face(&GEM_UPPER_LEFT, 1.0),
        face(&GEM_UPPER_RIGHT, 0.78),
        face(&GEM_LOWER_LEFT, 0.58),
        face(&GEM_LOWER_RIGHT, 0.42),
        band(&GEM_BAND, 0.9),
    ];
    const ROD: [Face; 4] = [
        face(&ROD_SHAFT, 0.62),
        face(&ROD_SHAFT_LIT, 0.92),
        head(&ROD_HEAD, 0.7),
        head(&ROD_HEAD_LIT, 1.0),
    ];
    match kind {
        ShapeKind::Cube => &CUBE,
        ShapeKind::Slab => &SLAB,
        ShapeKind::Ingot => &INGOT,
        ShapeKind::Gem => &GEM,
        ShapeKind::Rod => &ROD,
        // A sphere has no flat faces; `sample` shades it analytically.
        ShapeKind::Sphere => &[],
    }
}

/// Centre and radius of the sphere silhouette, in cell-normalised units.
pub const SPHERE: (f32, f32, f32) = (0.5, 0.5, 0.35);

/// Whether `(u, v)` is inside the shape's outline at all.
#[must_use]
pub fn silhouette(kind: ShapeKind, u: f32, v: f32) -> bool {
    if kind == ShapeKind::Sphere {
        let (cx, cy, r) = SPHERE;
        return (u - cx).hypot(v - cy) <= r;
    }
    faces(kind)
        .iter()
        .any(|face| face.paint != Paint::AccentOnly && inside(face.points, u, v))
}

/// Even-odd point-in-polygon. The polygons here are convex, but the test does
/// not need them to be and this keeps the shape table free of constraints.
#[must_use]
pub fn inside(points: &[(f32, f32)], u: f32, v: f32) -> bool {
    let mut hit = false;
    let mut j = points.len() - 1;
    for i in 0..points.len() {
        let (xi, yi) = points[i];
        let (xj, yj) = points[j];
        if (yi > v) != (yj > v) && u < (xj - xi) * (v - yi) / (yj - yi) + xi {
            hit = !hit;
        }
        j = i;
    }
    hit
}

/// The cool rim light, from behind and to the lower right. Section 2 of
/// `docs/research/research-modern-ui.md`: it is the one cue that separates a
/// rendered icon from a flat sprite at 64 px.
pub const RIM_COLOR: [f32; 3] = [0.55, 0.74, 1.0];

/// How far into the shape the rim reaches, as a fraction of the cell.
const RIM_WIDTH: f32 = 0.055;

/// The colour of `icon` at `(u, v)`, or `None` outside the silhouette.
///
/// Linear-ish sRGB components with alpha, ready to be written into an
/// `Rgba8UnormSrgb` image.
#[must_use]
#[allow(clippy::many_single_char_names)]
pub fn sample(icon: &ShapeIcon, u: f32, v: f32) -> Option<[f32; 4]> {
    let body = icon.color.0;
    let accent = icon.accent.unwrap_or(icon.color).0;
    // Data comes from a RON file or a Lua table, so neither is guaranteed to
    // be in range. Clamp once here, the way `gpu::material_of` clamps before
    // it fills a `StandardMaterial`, rather than letting a `metallic: 5.0`
    // brighten a face past white.
    let metallic = icon.metallic.clamp(0.0, 1.0);
    let roughness = icon.roughness.clamp(0.0, 1.0);
    let (mut rgb, alpha) = if icon.shape == ShapeKind::Sphere {
        let (cx, cy, r) = SPHERE;
        let (dx, dy) = ((u - cx) / r, (v - cy) / r);
        let d2 = dx * dx + dy * dy;
        if d2 > 1.0 {
            return None;
        }
        // A lambert term against a key from the upper left, plus the ambient
        // floor a fill light gives.
        let z = (1.0 - d2).sqrt();
        let n = [dx, dy, z];
        let key = (n[0] * -0.45 + n[1] * -0.6 + n[2] * 0.66).max(0.0);
        let shade = 0.34 + 0.78 * key;
        ([body[0] * shade, body[1] * shade, body[2] * shade], body[3])
    } else {
        let mut found: Option<(f32, [f32; 4])> = None;
        for face in faces(icon.shape) {
            if face.paint == Paint::AccentOnly && icon.accent.is_none() {
                continue;
            }
            if inside(face.points, u, v) {
                let color = if face.paint == Paint::Body {
                    body
                } else {
                    accent
                };
                found = Some((face.brightness, color));
            }
        }
        let (brightness, color) = found?;
        // Metal widens the gap between the lit and the shadowed face; a
        // dielectric keeps a flatter, chalkier read. Raising the face's
        // brightness to a power leaves the key face where it is and pulls
        // the shadowed ones down, which is the gap the direction asks for.
        let shade = brightness.powf(1.0 + metallic);
        let shade = shade.mul_add(1.0, -0.08 * roughness);
        (
            [color[0] * shade, color[1] * shade, color[2] * shade],
            color[3],
        )
    };

    // The rim: a pixel whose neighbour towards the lower right is outside the
    // shape sits on the edge the back light catches.
    let outside_behind = !silhouette(icon.shape, u + RIM_WIDTH, v + RIM_WIDTH * 0.55);
    if outside_behind {
        let strength = 0.55 + 0.3 * (1.0 - roughness);
        for i in 0..3 {
            rgb[i] = rgb[i].mul_add(1.0 - strength, RIM_COLOR[i] * strength);
        }
    } else if !silhouette(icon.shape, u - RIM_WIDTH * 0.7, v - RIM_WIDTH * 0.7) {
        // A soft specular on the key-facing edge, stronger on smooth metal.
        let strength = 0.30 * (1.0 - roughness) + 0.12 * metallic;
        for value in &mut rgb {
            *value = value.mul_add(1.0 - strength, strength);
        }
    }
    Some([rgb[0], rgb[1], rgb[2], alpha])
}

/// The shape a `None` icon falls back to: a cube in the item's hash colour,
/// so an item that declares nothing still reads as a lit 3D object rather
/// than as a missing texture.
#[must_use]
pub fn fallback_shape(color: IconColor) -> ShapeIcon {
    ShapeIcon {
        shape: ShapeKind::Cube,
        color,
        accent: None,
        metallic: 0.0,
        roughness: 0.6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_covers_its_centre() {
        for kind in ShapeKind::ALL {
            assert!(
                silhouette(kind, 0.5, 0.5),
                "{kind} leaves the centre of the cell empty"
            );
            assert!(!silhouette(kind, 0.02, 0.02), "{kind} reaches the corner");
        }
    }

    #[test]
    fn a_lid_is_brighter_than_the_shadowed_face() {
        let icon = ShapeIcon::new(ShapeKind::Cube, IconColor::rgb(128, 128, 128));
        let lid = sample(&icon, 0.5, 0.3).expect("on the lid");
        let right = sample(&icon, 0.7, 0.7).expect("on the right face");
        assert!(lid[0] > right[0], "{lid:?} vs {right:?}");
    }

    #[test]
    fn the_rim_is_cooler_than_the_body() {
        let icon = ShapeIcon::new(ShapeKind::Sphere, IconColor::rgb(200, 60, 60));
        let body = sample(&icon, 0.45, 0.45).expect("body");
        let rim = sample(&icon, 0.70, 0.72).expect("rim");
        assert!(
            rim[2] > body[2],
            "the rim should push blue up: {rim:?} vs {body:?}"
        );
    }

    #[test]
    fn an_accent_only_paints_when_one_was_declared() {
        let plain = ShapeIcon::new(ShapeKind::Rod, IconColor::rgb(110, 78, 52));
        let mut fancy = plain.clone();
        fancy.accent = Some(IconColor::rgb(220, 226, 232));
        // The head is always geometry; only its colour follows the accent.
        let at = (0.72, 0.22);
        let plain_head = sample(&plain, at.0, at.1).expect("a head");
        let fancy_head = sample(&fancy, at.0, at.1).expect("a head");
        assert!(fancy_head[0] > plain_head[0], "the accent is lighter here");
        // A band is decoration that only exists when there is a second colour.
        let cube = ShapeIcon::new(ShapeKind::Cube, IconColor::rgb(110, 110, 110));
        let mut inlaid = cube.clone();
        inlaid.accent = Some(IconColor::rgb(30, 200, 220));
        let band = (0.30, 0.60);
        assert!(
            sample(&inlaid, band.0, band.1).expect("band")[2]
                > sample(&cube, band.0, band.1).expect("face")[2]
        );
    }
}
