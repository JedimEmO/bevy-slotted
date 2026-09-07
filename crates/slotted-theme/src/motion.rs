//! Motion budget and the minimal tween the ui crate animates with.

use std::time::Duration;

use bevy::prelude::*;
use bevy::ui::ui_transform::{UiTransform, Val2};
use serde::{Deserialize, Serialize};

use crate::tokens::{Durations, Tokens};

/// Global motion settings. Every animation multiplies its duration by `scale`
/// and skips to the end when `reduced` is set.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct Motion {
    /// 1.0 = theme durations as authored; 0.0 = instant.
    pub scale: f32,
    /// Reduced-motion switch: tweens complete on their first frame.
    pub reduced: bool,
}

impl Default for Motion {
    fn default() -> Self {
        Self {
            scale: 1.0,
            reduced: false,
        }
    }
}

impl Motion {
    /// Reduced motion, for tests and accessibility settings.
    pub const REDUCED: Self = Self {
        scale: 0.0,
        reduced: true,
    };

    /// Effective duration for `preset` from the three duration tiers. Zero
    /// when reduced. [`Motion::preset_duration`] also honours a theme's
    /// per-preset override.
    pub fn duration(&self, preset: MotionPreset, durations: &Durations) -> Duration {
        self.scaled(durations.tier_ms(preset))
    }

    /// Effective duration for `preset` under the whole token table: the
    /// theme's per-preset override when it has one, else the tier.
    pub fn preset_duration(&self, preset: MotionPreset, tokens: &Tokens) -> Duration {
        self.scaled(tokens.duration_ms(preset))
    }

    fn scaled(self, ms: u32) -> Duration {
        if self.reduced {
            return Duration::ZERO;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let micros = (f64::from(ms) * 1000.0 * f64::from(self.scale.max(0.0))).round() as u64;
        Duration::from_micros(micros)
    }

    /// A [`Tween`] on `target` lasting [`Motion::duration`] for `preset`,
    /// with the standard ease-out. Under reduced motion the duration is
    /// zero, so the tween lands on its end value the first time
    /// `advance_tweens` sees it.
    pub fn tween(&self, preset: MotionPreset, target: TweenTarget, durations: &Durations) -> Tween {
        Tween::new(target, self.duration(preset, durations))
    }

    /// A [`Tween`] shaped by the theme: per-preset duration and easing from
    /// `tokens`. This is what the ui crate uses, so a theme that says its
    /// drop is a stamp and its hover is a snap gets exactly that.
    pub fn preset_tween(
        &self,
        preset: MotionPreset,
        target: TweenTarget,
        tokens: &Tokens,
    ) -> Tween {
        Tween::new(target, self.preset_duration(preset, tokens)).with_easing(tokens.easing(preset))
    }
}

/// Named animations. The ui crate starts these; the theme decides how long
/// they take and how they ease. Serialises as the variant name, which is the
/// key of `tokens.motion.presets` in a theme file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MotionPreset {
    /// Slot or button hover in/out.
    Hover,
    /// Press down.
    Press,
    /// A stack landing in a slot.
    DropSquash,
    /// A stack travelling to its slot after quick-move.
    FlyToSlot,
    /// Children appearing one after another when a screen opens.
    Stagger,
    /// Tooltip and panel fade.
    Fade,
    /// A pushed screen sliding in from `spacing.xl` away (menus contract 3.2).
    Slide,
}

/// How a tween's progress is shaped. Every curve starts at 0 and ends at 1;
/// only [`Easing::Overshoot`] leaves that range on the way.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Easing {
    /// Ease-out cubic: fast start, gentle landing. The glass and paper
    /// default, and what every tween used before themes could choose.
    #[default]
    Standard,
    /// No easing.
    Linear,
    /// Ease-in-out cubic: slow both ends, for fades and page turns.
    EaseInOut,
    /// Ease-in cubic: gathers speed and stops dead. Paper's stamp-down drop:
    /// the squash runs from small to rest and lands hard.
    Stamp,
    /// Ease-out quintic: nearly all the travel in the first third. Neon's
    /// punchy hover and press.
    Snap,
    /// Back-out: overshoots the end value by about a tenth and settles.
    /// Neon's drop squash, the one curve that is allowed a spring.
    Overshoot,
}

impl Easing {
    /// Maps linear progress `t` in `0..=1` onto the curve.
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Standard => 1.0 - (1.0 - t).powi(3),
            Self::Linear => t,
            Self::EaseInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }
            Self::Stamp => t * t * t,
            Self::Snap => 1.0 - (1.0 - t).powi(5),
            Self::Overshoot => {
                const C1: f32 = 1.701_58;
                const C3: f32 = C1 + 1.0;
                1.0 + C3 * (t - 1.0).powi(3) + C1 * (t - 1.0).powi(2)
            }
        }
    }

    /// `true` for curves whose value leaves `0..=1`: a squash the paper
    /// theme's "no springs" rule forbids.
    pub const fn overshoots(self) -> bool {
        matches!(self, Self::Overshoot)
    }
}

/// What a [`Tween`] drives.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TweenTarget {
    /// `UiTransform` scale, uniform.
    Scale {
        /// Start.
        from: f32,
        /// End.
        to: f32,
    },
    /// `UiTransform` translation in px.
    Translate {
        /// Start.
        from: Vec2,
        /// End.
        to: Vec2,
    },
    /// `BackgroundColor` alpha.
    Alpha {
        /// Start.
        from: f32,
        /// End.
        to: f32,
    },
    /// `Node::width` and `Node::height` in logical px (Phase 6, side tabs).
    Size {
        /// Start.
        from: Vec2,
        /// End.
        to: Vec2,
    },
}

/// A running animation on a node. Removed when it completes.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct Tween {
    /// What is animated.
    pub target: TweenTarget,
    /// Total length; zero completes on the next frame.
    pub duration: Duration,
    /// Time elapsed so far, in `Time<Virtual>`.
    pub elapsed: Duration,
    /// The curve; [`Easing::Standard`] unless the theme says otherwise.
    pub easing: Easing,
}

/// The value a [`Tween`] holds at some point along its run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TweenValue {
    /// Uniform `UiTransform` scale.
    Scale(f32),
    /// `UiTransform` translation in logical px.
    Translate(Vec2),
    /// `BackgroundColor` alpha.
    Alpha(f32),
    /// `Node` width and height in logical px.
    Size(Vec2),
}

impl Tween {
    /// A tween that has not started, with the standard ease-out.
    pub fn new(target: TweenTarget, duration: Duration) -> Self {
        Self {
            target,
            duration,
            elapsed: Duration::ZERO,
            easing: Easing::Standard,
        }
    }

    /// The same tween on a different curve.
    #[must_use]
    pub fn with_easing(mut self, easing: Easing) -> Self {
        self.easing = easing;
        self
    }

    /// Eased progress. `1.0` once done; between `0..=1` except for
    /// [`Easing::Overshoot`], which passes the end value and comes back.
    pub fn progress(&self) -> f32 {
        if self.duration.is_zero() {
            return 1.0;
        }
        let t = (self.elapsed.as_secs_f32() / self.duration.as_secs_f32()).clamp(0.0, 1.0);
        self.easing.apply(t)
    }

    /// `true` once `elapsed >= duration`.
    pub fn is_done(&self) -> bool {
        self.elapsed >= self.duration
    }

    /// The interpolated value at the current progress.
    pub fn value(&self) -> TweenValue {
        let t = self.progress();
        match self.target {
            TweenTarget::Scale { from, to } => TweenValue::Scale(from.lerp(to, t)),
            TweenTarget::Translate { from, to } => TweenValue::Translate(from.lerp(to, t)),
            TweenTarget::Alpha { from, to } => TweenValue::Alpha(from.lerp(to, t)),
            TweenTarget::Size { from, to } => TweenValue::Size(from.lerp(to, t)),
        }
    }
}

/// Number of [`Tween`] components alive after the last `SlottedThemeSet::Motion`
/// run. `slotted-test`'s `settle()` waits for zero.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ActiveMotions(pub u32);

/// Advances tweens by `Time<Virtual>`, writes the interpolated value onto
/// `UiTransform` or `BackgroundColor`, removes finished ones, and updates
/// [`ActiveMotions`].
///
/// Never reads the wall clock. With [`Motion::reduced`] every tween lands on
/// its final value the first time this runs, which is what makes a
/// reduced-motion harness settle in one frame.
pub fn advance_tweens(
    time: Res<Time<Virtual>>,
    motion: Res<Motion>,
    mut commands: Commands,
    mut tweens: Query<(
        Entity,
        &mut Tween,
        Option<&mut UiTransform>,
        Option<&mut BackgroundColor>,
        Option<&mut Node>,
    )>,
    mut active: ResMut<ActiveMotions>,
) {
    let delta = time.delta();
    let mut alive = 0u32;
    for (entity, mut tween, transform, background, node) in &mut tweens {
        if motion.reduced {
            tween.elapsed = tween.duration;
        } else {
            tween.elapsed = tween.elapsed.saturating_add(delta);
        }
        match tween.value() {
            TweenValue::Scale(s) => match transform {
                Some(mut tf) => tf.scale = Vec2::splat(s),
                None => {
                    commands.entity(entity).insert(UiTransform {
                        scale: Vec2::splat(s),
                        ..UiTransform::IDENTITY
                    });
                }
            },
            TweenValue::Translate(v) => {
                let translation = Val2::px(v.x, v.y);
                match transform {
                    Some(mut tf) => tf.translation = translation,
                    None => {
                        commands.entity(entity).insert(UiTransform {
                            translation,
                            ..UiTransform::IDENTITY
                        });
                    }
                }
            }
            TweenValue::Alpha(a) => match background {
                Some(mut bg) => bg.0 = bg.0.with_alpha(a),
                None => {
                    tracing::trace!(?entity, "alpha tween on a node with no BackgroundColor");
                }
            },
            // Writes the size onto the node; the side tab
            // relies on this to grow and shrink.
            TweenValue::Size(size) => match node {
                Some(mut node) => {
                    node.width = Val::Px(size.x);
                    node.height = Val::Px(size.y);
                }
                None => {
                    tracing::trace!(?entity, "size tween on an entity with no Node");
                }
            },
        }
        if tween.is_done() {
            commands.entity(entity).remove::<Tween>();
        } else {
            alive += 1;
        }
    }
    if active.0 != alive {
        active.0 = alive;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: Durations = Durations {
        fast: 100,
        normal: 200,
        slow: 400,
        hover_delay: 120,
    };

    fn app() -> App {
        let mut app = App::new();
        app.init_resource::<Time<Virtual>>()
            .init_resource::<Motion>()
            .init_resource::<ActiveMotions>()
            .add_systems(Update, advance_tweens);
        app
    }

    /// Advance the app by exactly `delta`, the way the harness does.
    fn step(app: &mut App, delta: Duration) {
        app.world_mut()
            .resource_mut::<Time<Virtual>>()
            .advance_by(delta);
        app.update();
    }

    #[test]
    fn reduced_motion_is_instant() {
        assert_eq!(
            Motion::REDUCED.duration(MotionPreset::FlyToSlot, &D),
            Duration::ZERO
        );
        assert_eq!(
            Motion::default().duration(MotionPreset::Hover, &D),
            Duration::from_millis(100)
        );
        let half = Motion {
            scale: 0.5,
            reduced: false,
        };
        assert_eq!(
            half.duration(MotionPreset::Stagger, &D),
            Duration::from_millis(200)
        );
    }

    #[test]
    fn every_preset_maps_to_a_duration_token() {
        let m = Motion::default();
        assert_eq!(
            m.duration(MotionPreset::Hover, &D),
            Duration::from_millis(100)
        );
        assert_eq!(
            m.duration(MotionPreset::Press, &D),
            Duration::from_millis(100)
        );
        assert_eq!(
            m.duration(MotionPreset::DropSquash, &D),
            Duration::from_millis(200)
        );
        assert_eq!(
            m.duration(MotionPreset::Fade, &D),
            Duration::from_millis(200)
        );
        // The flight is a normal-tier motion, not a slow one: it decorates a
        // model change that has already landed.
        assert_eq!(
            m.duration(MotionPreset::FlyToSlot, &D),
            Duration::from_millis(200)
        );
        assert_eq!(
            m.duration(MotionPreset::Stagger, &D),
            Duration::from_millis(400)
        );
    }

    #[test]
    fn every_easing_starts_at_zero_and_ends_at_one() {
        for easing in [
            Easing::Standard,
            Easing::Linear,
            Easing::EaseInOut,
            Easing::Stamp,
            Easing::Snap,
            Easing::Overshoot,
        ] {
            assert!(easing.apply(0.0).abs() < 1e-6, "{easing:?} at 0");
            assert!((easing.apply(1.0) - 1.0).abs() < 1e-5, "{easing:?} at 1");
        }
        // Only the overshoot leaves the range; that is what "no springs"
        // checks against.
        assert!(Easing::Overshoot.apply(0.8) > 1.0);
        assert!(Easing::Overshoot.overshoots());
        for easing in [
            Easing::Standard,
            Easing::Stamp,
            Easing::Snap,
            Easing::EaseInOut,
        ] {
            assert!(!easing.overshoots());
            for i in 0..=20_u8 {
                let v = easing.apply(f32::from(i) / 20.0);
                assert!((0.0..=1.0).contains(&v), "{easing:?} left the range: {v}");
            }
        }
        // The stamp lands late and hard; the snap is nearly there early.
        assert!(Easing::Stamp.apply(0.5) < 0.2);
        assert!(Easing::Snap.apply(0.33) > 0.85);
    }

    #[test]
    fn preset_tokens_override_the_tier_and_choose_the_easing() {
        let mut tokens = Tokens {
            durations: D,
            ..Tokens::default()
        };
        tokens.motion.easing = Easing::Snap;
        tokens.motion.presets.insert(
            MotionPreset::DropSquash,
            crate::tokens::MotionSpec {
                duration: 140,
                easing: Some(Easing::Overshoot),
            },
        );
        let m = Motion::default();
        // Hover has no override: the fast tier and the theme's easing.
        assert_eq!(
            m.preset_duration(MotionPreset::Hover, &tokens),
            Duration::from_millis(100)
        );
        assert_eq!(tokens.easing(MotionPreset::Hover), Easing::Snap);
        // The squash has both.
        assert_eq!(
            m.preset_duration(MotionPreset::DropSquash, &tokens),
            Duration::from_millis(140)
        );
        let tween = m.preset_tween(
            MotionPreset::DropSquash,
            TweenTarget::Scale {
                from: 0.88,
                to: 1.0,
            },
            &tokens,
        );
        assert_eq!(tween.easing, Easing::Overshoot);
        assert_eq!(tween.duration, Duration::from_millis(140));
        // Reduced motion wins over any token.
        assert_eq!(
            Motion::REDUCED.preset_duration(MotionPreset::DropSquash, &tokens),
            Duration::ZERO
        );
    }

    #[test]
    fn tween_progress_eases_out() {
        let mut t = Tween::new(
            TweenTarget::Alpha { from: 0.0, to: 1.0 },
            Duration::from_millis(100),
        );
        assert!(t.progress().abs() < f32::EPSILON);
        t.elapsed = Duration::from_millis(50);
        // Ease-out cubic at t=0.5 is 1 - 0.5^3 = 0.875.
        assert!((t.progress() - 0.875).abs() < 1e-6, "{}", t.progress());
        assert_eq!(t.value(), TweenValue::Alpha(0.875));
        t.elapsed = Duration::from_millis(100);
        assert!(t.is_done());
        assert!((t.progress() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_zero_duration_tween_is_complete_immediately() {
        let t = Tween::new(TweenTarget::Scale { from: 1.0, to: 1.2 }, Duration::ZERO);
        assert!(t.is_done());
        assert_eq!(t.value(), TweenValue::Scale(1.2));
    }

    #[test]
    fn scale_tween_advances_deterministically_over_fixed_frames() {
        let mut app = app();
        let e = app
            .world_mut()
            .spawn(Tween::new(
                TweenTarget::Scale { from: 1.0, to: 2.0 },
                Duration::from_millis(100),
            ))
            .id();
        // Frame 1: 25ms elapsed, ease-out cubic gives 1 - 0.75^3 = 0.578125.
        step(&mut app, Duration::from_millis(25));
        let scale = app.world().get::<UiTransform>(e).expect("transform").scale;
        assert!((scale.x - 1.578_125).abs() < 1e-5, "{scale:?}");
        assert_eq!(app.world().resource::<ActiveMotions>().0, 1);

        // Three more frames land exactly on the end.
        for _ in 0..3 {
            step(&mut app, Duration::from_millis(25));
        }
        let scale = app.world().get::<UiTransform>(e).expect("transform").scale;
        assert!((scale.x - 2.0).abs() < 1e-6, "{scale:?}");
        assert!(app.world().get::<Tween>(e).is_none(), "tween was removed");
        assert_eq!(app.world().resource::<ActiveMotions>().0, 0);
    }

    #[test]
    fn alpha_tween_writes_background_colour() {
        let mut app = app();
        let e = app
            .world_mut()
            .spawn((
                BackgroundColor(Color::srgba(1.0, 0.0, 0.0, 0.0)),
                Tween::new(
                    TweenTarget::Alpha { from: 0.0, to: 1.0 },
                    Duration::from_millis(50),
                ),
            ))
            .id();
        step(&mut app, Duration::from_millis(50));
        let alpha = app
            .world()
            .get::<BackgroundColor>(e)
            .expect("colour")
            .0
            .alpha();
        assert!((alpha - 1.0).abs() < 1e-6, "{alpha}");
        assert!(app.world().get::<Tween>(e).is_none());
    }

    #[test]
    fn reduced_motion_finishes_a_tween_on_its_first_frame() {
        let mut app = app();
        app.insert_resource(Motion::REDUCED);
        let e = app
            .world_mut()
            .spawn(Tween::new(
                TweenTarget::Translate {
                    from: Vec2::ZERO,
                    to: Vec2::new(10.0, -4.0),
                },
                Duration::from_millis(400),
            ))
            .id();
        step(&mut app, Duration::from_millis(1));
        let tf = app.world().get::<UiTransform>(e).expect("transform");
        assert_eq!(tf.translation, Val2::px(10.0, -4.0));
        assert!(app.world().get::<Tween>(e).is_none());
        assert_eq!(app.world().resource::<ActiveMotions>().0, 0);
    }

    #[test]
    fn active_motions_counts_running_tweens() {
        let mut app = app();
        for _ in 0..3 {
            app.world_mut().spawn(Tween::new(
                TweenTarget::Scale { from: 0.0, to: 1.0 },
                Duration::from_millis(100),
            ));
        }
        step(&mut app, Duration::from_millis(16));
        assert_eq!(app.world().resource::<ActiveMotions>().0, 3);
        step(&mut app, Duration::from_millis(200));
        assert_eq!(app.world().resource::<ActiveMotions>().0, 0);
    }
}
