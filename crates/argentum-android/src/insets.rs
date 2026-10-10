pub const DEFAULT_DENSITY_DPI: u32 = 160;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AndroidWindowMetrics {
    pub window_width_px: i32,
    pub window_height_px: i32,
    pub content_top_px: i32,
    pub content_bottom_px: i32,
    pub system_bar_top_px: Option<i32>,
    pub system_bar_bottom_px: Option<i32>,
    pub ime_bottom_px: Option<i32>,
    pub density_dpi: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AndroidInsets {
    pub safe_top_dp: f32,
    pub safe_bottom_dp: f32,
    pub keyboard_dp: f32,
}

#[derive(Debug, Default)]
pub struct AndroidInsetsTracker {
    baseline: Option<AndroidWindowMetrics>,
}

impl AndroidInsetsTracker {
    pub fn update(&mut self, metrics: AndroidWindowMetrics) -> Option<AndroidInsets> {
        let normalized = normalize_metrics(metrics)?;
        let baseline = match self.baseline {
            Some(baseline)
                if baseline.window_width_px == normalized.window_width_px
                    && baseline.window_height_px == normalized.window_height_px
                    && baseline.density_dpi == normalized.density_dpi =>
            {
                baseline
            }
            _ => {
                self.baseline = Some(normalized);
                normalized
            }
        };

        let density = normalized.density_dpi.max(1) as f32;
        Some(AndroidInsets {
            safe_top_dp: pixels_to_dp(
                normalized
                    .system_bar_top_px
                    .unwrap_or(baseline.content_top_px),
                density,
            ),
            safe_bottom_dp: pixels_to_dp(
                normalized.system_bar_bottom_px.unwrap_or_else(|| {
                    baseline
                        .window_height_px
                        .saturating_sub(baseline.content_bottom_px)
                }),
                density,
            ),
            keyboard_dp: pixels_to_dp(
                normalized.ime_bottom_px.unwrap_or_else(|| {
                    baseline
                        .content_bottom_px
                        .saturating_sub(normalized.content_bottom_px)
                        .max(0)
                }),
                density,
            ),
        })
    }

    pub fn reset(&mut self) {
        self.baseline = None;
    }
}

fn normalize_metrics(metrics: AndroidWindowMetrics) -> Option<AndroidWindowMetrics> {
    if metrics.window_width_px <= 0
        || metrics.window_height_px <= 0
        || metrics.content_bottom_px <= 0
        || metrics.content_top_px < 0
    {
        return None;
    }

    let content_top_px = metrics.content_top_px.min(metrics.window_height_px);
    let content_bottom_px = metrics
        .content_bottom_px
        .min(metrics.window_height_px)
        .max(content_top_px);
    Some(AndroidWindowMetrics {
        content_top_px,
        content_bottom_px,
        system_bar_top_px: metrics
            .system_bar_top_px
            .filter(|inset| *inset >= 0)
            .map(|inset| inset.min(metrics.window_height_px)),
        system_bar_bottom_px: metrics
            .system_bar_bottom_px
            .filter(|inset| *inset >= 0)
            .map(|inset| inset.min(metrics.window_height_px)),
        ime_bottom_px: metrics
            .ime_bottom_px
            .filter(|inset| *inset >= 0)
            .map(|inset| inset.min(metrics.window_height_px)),
        density_dpi: if metrics.density_dpi == 0 {
            DEFAULT_DENSITY_DPI
        } else {
            metrics.density_dpi
        },
        ..metrics
    })
}

fn pixels_to_dp(pixels: i32, density_dpi: f32) -> f32 {
    pixels.max(0) as f32 * DEFAULT_DENSITY_DPI as f32 / density_dpi
}

#[cfg(test)]
mod tests {
    use super::{AndroidInsets, AndroidInsetsTracker, AndroidWindowMetrics};

    fn metrics(top: i32, bottom: i32) -> AndroidWindowMetrics {
        AndroidWindowMetrics {
            window_width_px: 1080,
            window_height_px: 2400,
            content_top_px: top,
            content_bottom_px: bottom,
            system_bar_top_px: None,
            system_bar_bottom_px: None,
            ime_bottom_px: None,
            density_dpi: 320,
        }
    }

    #[test]
    fn baseline_insets_are_converted_from_pixels_to_dp() {
        let mut tracker = AndroidInsetsTracker::default();
        assert_eq!(
            tracker.update(metrics(80, 2320)),
            Some(AndroidInsets {
                safe_top_dp: 40.0,
                safe_bottom_dp: 40.0,
                keyboard_dp: 0.0,
            })
        );
    }

    #[test]
    fn keyboard_reduction_is_measured_against_the_visible_baseline() {
        let mut tracker = AndroidInsetsTracker::default();
        tracker.update(metrics(80, 2320));
        assert_eq!(
            tracker.update(metrics(80, 1520)).unwrap().keyboard_dp,
            400.0
        );
        assert_eq!(tracker.update(metrics(80, 2320)).unwrap().keyboard_dp, 0.0);
    }

    #[test]
    fn explicit_system_bars_override_edge_to_edge_content_without_becoming_keyboard() {
        let mut tracker = AndroidInsetsTracker::default();
        let edge_to_edge = AndroidWindowMetrics {
            content_top_px: 0,
            content_bottom_px: 2400,
            system_bar_top_px: Some(80),
            system_bar_bottom_px: Some(120),
            ime_bottom_px: Some(0),
            ..metrics(0, 2400)
        };

        assert_eq!(
            tracker.update(edge_to_edge),
            Some(AndroidInsets {
                safe_top_dp: 40.0,
                safe_bottom_dp: 60.0,
                keyboard_dp: 0.0,
            })
        );

        assert_eq!(
            tracker
                .update(AndroidWindowMetrics {
                    ime_bottom_px: Some(800),
                    ..edge_to_edge
                })
                .unwrap(),
            AndroidInsets {
                safe_top_dp: 40.0,
                safe_bottom_dp: 60.0,
                keyboard_dp: 400.0,
            }
        );
    }

    #[test]
    fn orientation_or_density_changes_start_a_new_baseline() {
        let mut tracker = AndroidInsetsTracker::default();
        tracker.update(metrics(80, 2320));
        let rotated = AndroidWindowMetrics {
            window_width_px: 2400,
            window_height_px: 1080,
            content_top_px: 0,
            content_bottom_px: 1080,
            system_bar_top_px: None,
            system_bar_bottom_px: None,
            ime_bottom_px: None,
            density_dpi: 320,
        };
        assert_eq!(tracker.update(rotated).unwrap().keyboard_dp, 0.0);
        assert_eq!(
            tracker
                .update(AndroidWindowMetrics {
                    density_dpi: 480,
                    ..rotated
                })
                .unwrap()
                .keyboard_dp,
            0.0
        );
    }

    #[test]
    fn zero_density_uses_the_platform_default() {
        let mut tracker = AndroidInsetsTracker::default();
        let value = AndroidWindowMetrics {
            density_dpi: 0,
            ..metrics(40, 2360)
        };
        assert_eq!(tracker.update(value).unwrap().safe_top_dp, 40.0);
    }

    #[test]
    fn invalid_dimensions_do_not_replace_a_valid_baseline() {
        let mut tracker = AndroidInsetsTracker::default();
        tracker.update(metrics(80, 2320));
        assert_eq!(
            tracker.update(AndroidWindowMetrics {
                window_width_px: 0,
                ..metrics(0, 0)
            }),
            None
        );
        assert_eq!(
            tracker.update(metrics(80, 1520)).unwrap().keyboard_dp,
            400.0
        );
    }

    #[test]
    fn reset_discards_keyboard_state() {
        let mut tracker = AndroidInsetsTracker::default();
        tracker.update(metrics(80, 2320));
        tracker.update(metrics(80, 1520));
        tracker.reset();
        assert_eq!(tracker.update(metrics(80, 1520)).unwrap().keyboard_dp, 0.0);
    }
}
