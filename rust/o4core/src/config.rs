/// Tuning parameters. Defaults are the tuned M2 profile (spec §8).
#[derive(Clone, Debug)]
pub struct Config {
    pub severe: f64,
    pub severe_pad: f64,
    pub severe_merge: f64,
    pub ramp: f64,
    pub light_cutoff: f64,
    pub strong_cutoff: f64,
    pub noise_low: f64,
    pub noise_high: f64,
    pub noise_band: (f64, f64),
    pub noise_window_ms: f64,
    pub hampel_window: usize,
    pub hampel_sigma: f64,
    pub optical_cutoff: f64,
    pub handback_cutoff: Option<f64>,
    pub fast_handback: (f64, f64),
    pub patch_pad: f64,
    pub patch_merge: f64,
    pub optical_noise: Option<(f64, f64)>,
    pub fast_wide_cutoff: f64,
    pub fast_wide_ramp: (f64, f64),
    pub fast_wide_accel: f64,
    pub anchor_mode: bool,
    pub anchor_cutoff: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            severe: 8.0,
            severe_pad: 0.2,
            severe_merge: 0.2,
            ramp: 0.3,
            light_cutoff: 25.0,
            strong_cutoff: 2.5,
            noise_low: 1.5,
            noise_high: 5.0,
            noise_band: (30.0, 180.0),
            noise_window_ms: 100.0,
            hampel_window: 7,
            hampel_sigma: 6.0,
            optical_cutoff: 8.0,
            handback_cutoff: None,
            fast_handback: (100.0, 250.0),
            patch_pad: 0.5,
            patch_merge: 1.0,
            optical_noise: None,
            fast_wide_cutoff: 0.0,
            fast_wide_ramp: (150.0, 300.0),
            fast_wide_accel: 1500.0,
            anchor_mode: false,
            anchor_cutoff: 1.5,
        }
    }
}

impl Config {
    /// M4 "sharp-turn" profile: wider fast-motion handback, accel gate on.
    pub fn m4() -> Self {
        Self {
            fast_wide_cutoff: 16.0,
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_match_spec() {
        let c = Config::default();
        assert_eq!(c.severe, 8.0);
        assert_eq!(c.noise_band, (30.0, 180.0));
        assert_eq!(c.fast_wide_cutoff, 0.0);
        assert_eq!(Config::m4().fast_wide_cutoff, 16.0);
        assert!(c.handback_cutoff.is_none() && c.optical_noise.is_none());
    }
}
