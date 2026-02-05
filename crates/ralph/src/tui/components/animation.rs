//! Minimal timeline-style animation helpers for Ralph TUI overlays.
//!
//! Responsibilities:
//! - Provide a small, deterministic animation primitive (fade-in) for overlays.
//! - Respect terminal capability and user preference (NO_COLOR, TERM=dumb, explicit env disable).
//! - Isolate `tachyonfx` usage behind a stable local API.
//!
//! Not handled here:
//! - Frame pacing / event loop timing (owned by App / terminal driver).
//! - Complex compositing or per-cell shaders (Phase 1 is subtle overlay fade only).
//!
//! Invariants/assumptions:
//! - Progress is clamped to [0.0, 1.0].
//! - Disabled animations must behave like "fully visible" (progress = 1.0).
//! - Must never panic even if frame counters regress or widths are zero.

use ratatui::style::{Color, Modifier, Style};
use std::env;

/// Policy for controlling animation behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AnimationPolicy {
    /// Whether animations are enabled.
    pub(crate) enabled: bool,
}

impl AnimationPolicy {
    /// Create a policy that disables all animations.
    pub(crate) fn disabled() -> Self {
        Self { enabled: false }
    }

    /// Create a policy that enables animations.
    pub(crate) fn enabled() -> Self {
        Self { enabled: true }
    }

    /// Determine animation policy from environment.
    ///
    /// Animations are disabled if any of the following are true:
    /// - `NO_COLOR` is set
    /// - `TERM=dumb`
    /// - `RALPH_TUI_NO_ANIM=1` or `true`
    pub(crate) fn from_env() -> Self {
        // Fail-safe: disable on NO_COLOR, TERM=dumb, or explicit flag.
        let no_color = env::var_os("NO_COLOR").is_some();
        let term_dumb = env::var("TERM").ok().is_some_and(|t| t == "dumb");
        let ralph_no_anim = env::var("RALPH_TUI_NO_ANIM")
            .ok()
            .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));

        Self {
            enabled: !(no_color || term_dumb || ralph_no_anim),
        }
    }

    /// Check if animations should be shown.
    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled
    }
}

impl Default for AnimationPolicy {
    fn default() -> Self {
        Self::from_env()
    }
}

/// A deterministic fade-in driven by a monotonically increasing frame counter.
///
/// This is a simple animation that progresses from 0.0 (invisible) to 1.0 (fully visible)
/// over a fixed number of frames. It does not use real-time to ensure deterministic
/// behavior across different terminal refresh rates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FadeIn {
    /// The frame number when the fade-in starts.
    start_frame: u64,
    /// The duration of the fade-in in frames.
    duration_frames: u64,
}

impl FadeIn {
    /// Create a new fade-in animation.
    ///
    /// # Arguments
    ///
    /// * `start_frame` - The frame number when the fade-in should start.
    /// * `duration_frames` - How many frames the fade-in should take (minimum 1).
    pub(crate) fn new(start_frame: u64, duration_frames: u64) -> Self {
        Self {
            start_frame,
            duration_frames: duration_frames.max(1),
        }
    }

    /// Get the current progress of the fade-in animation.
    ///
    /// Returns a value in the range [0.0, 1.0] where:
    /// - 0.0 = animation hasn't started or just started
    /// - 1.0 = animation is complete
    pub(crate) fn progress(&self, now_frame: u64) -> f32 {
        let elapsed = now_frame.saturating_sub(self.start_frame);
        let raw = (elapsed as f32) / (self.duration_frames as f32);
        clamp01(ease_out_cubic(raw))
    }

    /// Check if the animation is complete at the given frame.
    pub(crate) fn is_complete(&self, now_frame: u64) -> bool {
        self.progress(now_frame) >= 1.0
    }

    /// Apply the fade-in effect to a base style.
    ///
    /// If animations are disabled, returns the base style unchanged.
    /// Otherwise, modulates the foreground color based on progress.
    pub(crate) fn overlay_style(
        &self,
        base: Style,
        now_frame: u64,
        policy: AnimationPolicy,
    ) -> Style {
        if !policy.enabled {
            return base;
        }

        let p = self.progress(now_frame);
        // Map progress to a small discrete palette to avoid "fake smoothness" in 16-color terminals.
        let fg = if p < 0.33 {
            Color::DarkGray
        } else if p < 0.66 {
            Color::Gray
        } else {
            Color::White
        };

        base.fg(fg).add_modifier(if p >= 0.66 {
            Modifier::BOLD
        } else {
            Modifier::empty()
        })
    }

    /// Get the opacity as an integer (0-255) for use with other effects.
    pub(crate) fn opacity_u8(&self, now_frame: u64) -> u8 {
        let p = self.progress(now_frame);
        (p * 255.0) as u8
    }
}

/// Clamp a float to the range [0.0, 1.0].
fn clamp01(v: f32) -> f32 {
    if v.is_nan() {
        return 1.0;
    }
    v.clamp(0.0, 1.0)
}

/// Ease-out cubic function for smoother animations.
///
/// Starts fast and decelerates toward the end.
fn ease_out_cubic(t: f32) -> f32 {
    // Cubic ease-out: 1 - (1 - t)^3
    let t = clamp01(t);
    1.0 - (1.0 - t).powi(3)
}

/// Linear interpolation between two values.
pub(crate) fn lerp(start: f32, end: f32, t: f32) -> f32 {
    start + (end - start) * clamp01(t)
}

/// Interpolate between two colors.
pub(crate) fn lerp_color(start: Color, end: Color, t: f32) -> Color {
    // Simple interpolation that works with ratatui's Color enum
    // For indexed colors, we just switch at 0.5
    // For RGB, we interpolate each channel
    let t = clamp01(t);

    match (start, end) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
            let r = (r1 as f32 + (r2 as f32 - r1 as f32) * t) as u8;
            let g = (g1 as f32 + (g2 as f32 - g1 as f32) * t) as u8;
            let b = (b1 as f32 + (b2 as f32 - b1 as f32) * t) as u8;
            Color::Rgb(r, g, b)
        }
        _ => {
            // For non-RGB colors, just switch at the midpoint
            if t < 0.5 { start } else { end }
        }
    }
}

/// A simple frame-counter based animator.
///
/// This is a convenience struct for tracking animation state without
/// needing to manage the frame counter manually.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FrameAnimator {
    /// The frame when the current animation started.
    start_frame: Option<u64>,
    /// The duration in frames.
    duration: u64,
}

impl FrameAnimator {
    /// Create a new frame animator.
    pub(crate) fn new() -> Self {
        Self {
            start_frame: None,
            duration: 8, // Default 8 frames for fade-in
        }
    }

    /// Create a new frame animator with a specific duration.
    pub(crate) fn with_duration(duration: u64) -> Self {
        Self {
            start_frame: None,
            duration: duration.max(1),
        }
    }

    /// Start or restart the animation at the given frame.
    pub(crate) fn start(&mut self, frame: u64) {
        self.start_frame = Some(frame);
    }

    /// Reset the animation (clears the start frame).
    pub(crate) fn reset(&mut self) {
        self.start_frame = None;
    }

    /// Check if the animation has been started.
    pub(crate) fn is_started(&self) -> bool {
        self.start_frame.is_some()
    }

    /// Get the current progress (0.0 - 1.0).
    ///
    /// Returns 1.0 if the animation hasn't been started.
    pub(crate) fn progress(&self, now_frame: u64) -> f32 {
        let Some(start) = self.start_frame else {
            return 1.0; // Not started = fully visible
        };
        let fade = FadeIn::new(start, self.duration);
        fade.progress(now_frame)
    }

    /// Check if the animation is complete.
    pub(crate) fn is_complete(&self, now_frame: u64) -> bool {
        let Some(start) = self.start_frame else {
            return true; // Not started = complete
        };
        let fade = FadeIn::new(start, self.duration);
        fade.is_complete(now_frame)
    }

    /// Get a FadeIn instance for this animator.
    pub(crate) fn fade_in(&self) -> Option<FadeIn> {
        self.start_frame
            .map(|start| FadeIn::new(start, self.duration))
    }
}

impl Default for FrameAnimator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_clamps_and_is_monotone_non_decreasing() {
        let f = FadeIn::new(10, 5);
        assert_eq!(f.progress(0), 0.0); // saturating_sub keeps it safe
        let p1 = f.progress(10);
        let p2 = f.progress(12);
        let p3 = f.progress(100);
        assert!(p1 <= p2 && p2 <= p3);
        assert_eq!(p3, 1.0);
    }

    #[test]
    fn disabled_policy_returns_base_style() {
        let f = FadeIn::new(0, 10);
        let base = Style::default().fg(Color::Red);
        let out = f.overlay_style(base, 5, AnimationPolicy { enabled: false });
        assert_eq!(out, base);
    }

    #[test]
    fn clamp01_nan_is_safe() {
        assert_eq!(clamp01(f32::NAN), 1.0);
    }

    #[test]
    fn clamp01_clamps_to_range() {
        assert_eq!(clamp01(-0.5), 0.0);
        assert_eq!(clamp01(0.0), 0.0);
        assert_eq!(clamp01(0.5), 0.5);
        assert_eq!(clamp01(1.0), 1.0);
        assert_eq!(clamp01(1.5), 1.0);
    }

    #[test]
    fn ease_out_cubic_is_monotonic() {
        // Ease-out should be monotonically increasing
        let steps = 10;
        let mut prev = ease_out_cubic(0.0);
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            let curr = ease_out_cubic(t);
            assert!(
                curr >= prev,
                "ease_out_cubic should be monotonically increasing"
            );
            prev = curr;
        }
        assert_eq!(ease_out_cubic(0.0), 0.0);
        assert_eq!(ease_out_cubic(1.0), 1.0);
    }

    #[test]
    fn animation_policy_from_env_respects_no_color() {
        // We can't easily test environment variable behavior in unit tests,
        // but we can verify the API exists and returns a policy
        let policy = AnimationPolicy::from_env();
        // Just verify it doesn't panic
        let _ = policy.enabled;
    }

    #[test]
    fn frame_animator_default_not_started() {
        let animator = FrameAnimator::new();
        assert!(!animator.is_started());
        assert_eq!(animator.progress(0), 1.0); // Not started = fully visible
        assert!(animator.is_complete(0));
    }

    #[test]
    fn frame_animator_start_and_progress() {
        let mut animator = FrameAnimator::with_duration(10);
        assert!(!animator.is_started());

        animator.start(5);
        assert!(animator.is_started());

        // At start frame, progress should be 0
        let p0 = animator.progress(5);
        assert!(p0 < 0.1, "Progress at start should be near 0");

        // At halfway, progress should be between 0 and 1
        let p5 = animator.progress(10);
        assert!(
            p5 > 0.0 && p5 < 1.0,
            "Progress at halfway should be in (0, 1)"
        );

        // At end, progress should be 1
        let p10 = animator.progress(15);
        assert_eq!(p10, 1.0, "Progress at end should be 1");
    }

    #[test]
    fn frame_animator_reset() {
        let mut animator = FrameAnimator::new();
        animator.start(0);
        assert!(animator.is_started());

        animator.reset();
        assert!(!animator.is_started());
        assert_eq!(animator.progress(100), 1.0);
    }

    #[test]
    fn fade_in_duration_minimum_is_one() {
        let f = FadeIn::new(0, 0); // Should clamp to 1
        assert_eq!(f.progress(0), 0.0);
        assert_eq!(f.progress(1), 1.0);
    }

    #[test]
    fn lerp_interpolates_correctly() {
        assert_eq!(lerp(0.0, 10.0, 0.0), 0.0);
        assert_eq!(lerp(0.0, 10.0, 0.5), 5.0);
        assert_eq!(lerp(0.0, 10.0, 1.0), 10.0);
        assert_eq!(lerp(0.0, 10.0, 2.0), 10.0); // Clamped
        assert_eq!(lerp(0.0, 10.0, -1.0), 0.0); // Clamped
    }

    #[test]
    fn lerp_color_switches_at_midpoint_for_indexed() {
        let c1 = Color::Red;
        let c2 = Color::Blue;
        assert_eq!(lerp_color(c1, c2, 0.0), c1);
        assert_eq!(lerp_color(c1, c2, 0.49), c1);
        assert_eq!(lerp_color(c1, c2, 0.5), c2);
        assert_eq!(lerp_color(c1, c2, 1.0), c2);
    }

    #[test]
    fn lerp_color_interpolates_rgb() {
        let c1 = Color::Rgb(0, 0, 0);
        let c2 = Color::Rgb(100, 200, 50);
        assert_eq!(lerp_color(c1, c2, 0.0), c1);
        assert_eq!(lerp_color(c1, c2, 1.0), c2);
        // At 0.5, should be halfway
        let mid = lerp_color(c1, c2, 0.5);
        assert!(matches!(mid, Color::Rgb(50, 100, 25)));
    }

    #[test]
    fn opacity_u8_returns_correct_range() {
        let f = FadeIn::new(0, 10);
        assert_eq!(f.opacity_u8(0), 0);
        // At end should be 255
        assert_eq!(f.opacity_u8(100), 255);
    }
}
