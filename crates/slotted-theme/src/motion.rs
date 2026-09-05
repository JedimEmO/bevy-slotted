//! Motion budget and the minimal tween the ui crate animates with.

use std::time::Duration;

use bevy::prelude::*;

use crate::tokens::Durations;

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

    /// Effective duration for `preset`. Zero when reduced.
    pub fn duration(&self, preset: MotionPreset, durations: &Durations) -> Duration {
        if self.reduced {
            return Duration::ZERO;
        }
        let ms = match preset {
            MotionPreset::Hover | MotionPreset::Press => durations.fast,
            MotionPreset::DropSquash | MotionPreset::Fade => durations.normal,
            MotionPreset::FlyToSlot | MotionPreset::Stagger => durations.slow,
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let micros = (f64::from(ms) * 1000.0 * f64::from(self.scale.max(0.0))).round() as u64;
        Duration::from_micros(micros)
    }
}

/// Named animations. The ui crate starts these; the theme decides how long
/// they take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
}

/// A running animation on a node. Removed when it completes. Ease-out cubic.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct Tween {
    /// What is animated.
    pub target: TweenTarget,
    /// Total length; zero completes on the next frame.
    pub duration: Duration,
    /// Time elapsed so far, in `Time<Virtual>`.
    pub elapsed: Duration,
}

impl Tween {
    /// Progress in 0..=1 after easing.
    pub fn progress(&self) -> f32 {
        if self.duration.is_zero() {
            return 1.0;
        }
        let t = (self.elapsed.as_secs_f32() / self.duration.as_secs_f32()).clamp(0.0, 1.0);
        1.0 - (1.0 - t).powi(3)
    }

    /// `true` once `elapsed >= duration`.
    pub fn is_done(&self) -> bool {
        self.elapsed >= self.duration
    }
}

/// Number of [`Tween`] components alive after the last `SlottedThemeSet::Motion`
/// run. `slotted-test`'s `settle()` waits for zero.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ActiveMotions(pub u32);

/// Advances tweens by `Time<Virtual>`, writes the interpolated value, removes
/// finished ones, and updates [`ActiveMotions`].
// PHASE2-IMPL: agent B. Apply `TweenTarget` to `UiTransform` / `BackgroundColor`.
pub fn advance_tweens(
    time: Res<Time<Virtual>>,
    mut commands: Commands,
    mut tweens: Query<(Entity, &mut Tween)>,
    mut active: ResMut<ActiveMotions>,
) {
    let mut alive = 0u32;
    for (entity, mut tween) in &mut tweens {
        tween.elapsed = tween.elapsed.saturating_add(time.delta());
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

    #[test]
    fn reduced_motion_is_instant() {
        let d = Durations {
            fast: 100,
            normal: 200,
            slow: 400,
        };
        assert_eq!(
            Motion::REDUCED.duration(MotionPreset::FlyToSlot, &d),
            Duration::ZERO
        );
        assert_eq!(
            Motion::default().duration(MotionPreset::Hover, &d),
            Duration::from_millis(100)
        );
        let half = Motion {
            scale: 0.5,
            reduced: false,
        };
        assert_eq!(
            half.duration(MotionPreset::FlyToSlot, &d),
            Duration::from_millis(200)
        );
    }

    #[test]
    fn tween_progress_eases_out() {
        let mut t = Tween {
            target: TweenTarget::Alpha { from: 0.0, to: 1.0 },
            duration: Duration::from_millis(100),
            elapsed: Duration::ZERO,
        };
        assert!(t.progress().abs() < f32::EPSILON);
        t.elapsed = Duration::from_millis(50);
        assert!(t.progress() > 0.5);
        t.elapsed = Duration::from_millis(100);
        assert!(t.is_done());
        assert!((t.progress() - 1.0).abs() < f32::EPSILON);
    }
}
