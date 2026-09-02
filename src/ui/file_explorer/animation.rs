//! Spring integrator and easings for the file tree, mirroring the beui
//! motion spec (https://beui.dev/components/motion/file-tree).
//!
//! - row repositioning (expand/collapse/reorder): `layout="position"` spring
//! - hover pill sliding between rows: same spring + simple opacity fade
//! - row enter: opacity 0→1, y -6→0 over 0.22 s, ease-out cubic-bezier
//! - branch line: scaleY 0→1 (origin top) over 0.3 s, same ease-out
//! - chevron: rotate 0→90°, swap spring
//! - group icon swap: opacity/scale/rotate, swap spring
//!
//! Reduced motion: when the OS disables client-area animations
//! (`SPI_GETCLIENTAREAANIMATION` on Windows), every duration/spring is
//! collapsed to zero so states jump instantly.

/// Spring used for row repositioning and the sliding hover pill.
pub const SPRING_LAYOUT: Spring = Spring {
    stiffness: 360.0,
    damping: 32.0,
    mass: 0.6,
};

/// Spring used for chevron rotation and group icon swaps.
pub const SPRING_SWAP: Spring = Spring {
    stiffness: 460.0,
    damping: 30.0,
    mass: 0.55,
};

/// Duration of the row-enter and branch-line animations (seconds).
pub const ENTER_DURATION: f32 = 0.22;
pub const BRANCH_DURATION: f32 = 0.3;
/// Stagger per row position, capped (`min(position * 0.025, 0.1)`).
pub const ENTER_STAGGER_STEP: f32 = 0.025;
pub const ENTER_STAGGER_MAX: f32 = 0.1;

/// Cubic-bezier ease-out used by the beui spec: (0.16, 1, 0.3, 1).
pub fn ease_out_quint_expo(t: f32) -> f32 {
    // Closed-form of cubic-bezier(0.16, 1, 0.3, 1); accurate enough for UI.
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(4) * (1.0 - t * 0.3)
}

/// A critically-tunable spring. Integrated with semi-implicit Euler and a
/// clamped delta-time so a stall in the frame loop cannot explode it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    pub stiffness: f32,
    pub damping: f32,
    pub mass: f32,
}

impl Spring {
    /// Maximum `dt` fed to the integrator (seconds). Longer frame gaps are
    /// clamped, which keeps the motion stable at the cost of slowing down.
    const MAX_DT: f32 = 1.0 / 240.0;

    /// Advance the spring by `dt` seconds and return the new value/velocity.
    /// With reduced motion the target is reached instantly.
    pub fn step(&self, value: f32, velocity: f32, target: f32, dt: f32) -> (f32, f32) {
        if reduced_motion() || dt <= 0.0 {
            return (target, 0.0);
        }
        let mut value = value;
        let mut velocity = velocity;
        let mut remaining = dt;
        while remaining > 0.0 {
            let step = remaining.min(Self::MAX_DT);
            remaining -= step;
            let force = -self.stiffness * (value - target);
            let drag = -self.damping * velocity;
            let acceleration = (force + drag) / self.mass;
            velocity += acceleration * step;
            value += velocity * step;
        }
        (value, velocity)
    }

    /// Whether the spring has settled close enough to stop animating.
    pub fn settled(value: f32, velocity: f32, target: f32) -> bool {
        (value - target).abs() < 0.25 && velocity.abs() < 0.25
    }
}

/// Animated scalar driven by a spring, remembering its target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpringValue {
    pub value: f32,
    pub velocity: f32,
    pub target: f32,
}

impl SpringValue {
    pub fn at(target: f32) -> Self {
        Self {
            value: target,
            velocity: 0.0,
            target,
        }
    }

    pub fn retarget(&mut self, target: f32) {
        self.target = target;
    }

    pub fn step(&mut self, spring: Spring, dt: f32) {
        let (value, velocity) = spring.step(self.value, self.velocity, self.target, dt);
        self.value = value;
        self.velocity = velocity;
    }

    pub fn settled(&self) -> bool {
        Spring::settled(self.value, self.velocity, self.target)
    }
}

/// Time-based tween with the shared ease-out curve (row enter, branch line).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tween {
    pub elapsed: f32,
    pub duration: f32,
    pub delay: f32,
}

impl Tween {
    pub fn start(duration: f32) -> Self {
        Self::start_delayed(duration, 0.0)
    }

    /// Start after a delay while preserving a 0 progress value until then.
    pub fn start_delayed(duration: f32, delay: f32) -> Self {
        Self {
            elapsed: 0.0,
            duration: if reduced_motion() { 0.0 } else { duration },
            delay: if reduced_motion() {
                0.0
            } else {
                delay.max(0.0)
            },
        }
    }

    pub fn advance(&mut self, dt: f32) {
        self.elapsed = (self.elapsed + dt.max(0.0)).min(self.delay + self.duration);
    }

    /// Eased progress in 0..=1.
    pub fn progress(&self) -> f32 {
        if self.duration <= 0.0 {
            return 1.0;
        }
        if self.elapsed <= self.delay {
            return 0.0;
        }
        ease_out_quint_expo((self.elapsed - self.delay) / self.duration)
    }

    pub fn finished(&self) -> bool {
        self.elapsed >= self.delay + self.duration
    }
}

/// Staggered entry delay for row `position` (`min(position * 0.025, 0.1)`).
pub fn enter_delay(position: usize) -> f32 {
    if reduced_motion() {
        return 0.0;
    }
    (position as f32 * ENTER_STAGGER_STEP).min(ENTER_STAGGER_MAX)
}

/// Whether the OS asks for reduced motion. Read once, cached.
pub fn reduced_motion() -> bool {
    static REDUCED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *REDUCED.get_or_init(detect_reduced_motion)
}

fn detect_reduced_motion() -> bool {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_ACTION,
        };
        let mut enabled: windows_sys::core::BOOL = 1;
        // SAFETY: passing a valid pointer to a BOOL with the documented action.
        let result = unsafe {
            SystemParametersInfoW(
                SPI_GETCLIENTAREAANIMATION as SYSTEM_PARAMETERS_INFO_ACTION,
                0,
                &mut enabled as *mut _ as *mut core::ffi::c_void,
                0,
            )
        };
        result != 0 && enabled == 0
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spring_converges_to_target() {
        let spring = SPRING_LAYOUT;
        let mut value = 0.0;
        let mut velocity = 0.0;
        for _ in 0..240 {
            let (v, vel) = spring.step(value, velocity, 100.0, 1.0 / 60.0);
            value = v;
            velocity = vel;
        }
        assert!((value - 100.0).abs() < 0.5, "value={value}");
        assert!(velocity.abs() < 0.5);
    }

    #[test]
    fn spring_handles_large_dt_without_exploding() {
        let spring = SPRING_SWAP;
        let (value, velocity) = spring.step(0.0, 0.0, 90.0, 10.0);
        assert!(value.is_finite());
        assert!(velocity.is_finite());
        assert!((value - 90.0).abs() < 1.0);
    }

    #[test]
    fn tween_progress_is_monotonic_and_clamped() {
        let mut tween = Tween::start(ENTER_DURATION);
        let mut last = 0.0;
        for _ in 0..60 {
            tween.advance(1.0 / 60.0);
            let progress = tween.progress();
            assert!(progress >= last - 1e-6);
            assert!((0.0..=1.0).contains(&progress));
            last = progress;
        }
        assert!(tween.finished());
        assert!((tween.progress() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn delayed_tween_stays_hidden_until_its_delay_has_elapsed() {
        let mut tween = Tween::start_delayed(0.2, 0.1);
        tween.advance(0.09);
        assert_eq!(tween.progress(), 0.0);
        assert!(!tween.finished());

        tween.advance(0.02);
        assert!(tween.progress() > 0.0);
        tween.advance(1.0);
        assert!(tween.finished());
    }

    #[test]
    fn enter_stagger_is_capped() {
        assert_eq!(enter_delay(0), 0.0);
        assert!((enter_delay(4) - 0.1).abs() < 1e-6);
        assert_eq!(enter_delay(100), ENTER_STAGGER_MAX);
    }
}
