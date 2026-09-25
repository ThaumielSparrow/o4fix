/// Tuning parameters. Defaults are the tuned M2 profile (spec §8).
#[derive(Clone, Debug, PartialEq)]
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
    pub gyro_trust_noise: (f64, f64),
    pub patch_pad: f64,
    pub patch_merge: f64,
    pub optical_noise: Option<(f64, f64)>,
    pub fast_wide_cutoff: f64,
    pub fast_wide_ramp: (f64, f64),
    pub fast_wide_accel: f64,
    pub anchor_mode: bool,
    pub anchor_cutoff: f64,
    pub drift_rebase_above: f64,
    pub drift_decay_rate: f64,
    /// Source-frame residual refinement inside severe bursts (spec 2026-09-24).
    pub refine: bool,
    pub refine_cfg: crate::refine::RefineConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            severe: 8.0,
            severe_pad: 0.2,
            severe_merge: 0.2,
            ramp: 0.19,
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
            gyro_trust_noise: (200.0, 300.0),
            patch_pad: 0.5,
            patch_merge: 1.0,
            optical_noise: None,
            fast_wide_cutoff: 0.0,
            fast_wide_ramp: (150.0, 300.0),
            fast_wide_accel: 1500.0,
            anchor_mode: false,
            anchor_cutoff: 1.5,
            drift_rebase_above: 30.0,
            drift_decay_rate: 1.5,
            refine: true,
            refine_cfg: crate::refine::RefineConfig::default(),
        }
    }
}

impl Config {
    /// Shared by CLI and GUI; reject invalid arithmetic before decoding video.
    pub fn validate(&self) -> Result<(), crate::error::O4Error> {
        use crate::error::O4Error::InvalidConfig;
        for (name, value) in [
            ("severe", self.severe),
            ("severe_pad", self.severe_pad),
            ("severe_merge", self.severe_merge),
            ("patch_pad", self.patch_pad),
            ("patch_merge", self.patch_merge),
            ("fast_wide_cutoff", self.fast_wide_cutoff),
            ("fast_wide_accel", self.fast_wide_accel),
            ("hampel_sigma", self.hampel_sigma),
            ("drift_rebase_above", self.drift_rebase_above),
            ("drift_decay_rate", self.drift_decay_rate),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(InvalidConfig(format!(
                    "{name} must be finite and non-negative"
                )));
            }
        }
        for (name, value) in [
            ("ramp", self.ramp),
            ("light_cutoff", self.light_cutoff),
            ("strong_cutoff", self.strong_cutoff),
            ("optical_cutoff", self.optical_cutoff),
            ("anchor_cutoff", self.anchor_cutoff),
            ("noise_window", self.noise_window_ms),
            (
                "handback_cutoff",
                self.handback_cutoff.unwrap_or(self.optical_cutoff),
            ),
        ] {
            if !value.is_finite() || value <= 0.0 {
                return Err(InvalidConfig(format!("{name} must be finite and positive")));
            }
        }
        for (name, (lo, hi)) in [
            ("noise_low/high", (self.noise_low, self.noise_high)),
            ("noise_band", self.noise_band),
            ("fast_handback", self.fast_handback),
            ("gyro_trust_noise", self.gyro_trust_noise),
            ("fast_wide_ramp", self.fast_wide_ramp),
            (
                "optical_noise",
                self.optical_noise
                    .unwrap_or((self.noise_low, self.noise_high)),
            ),
        ] {
            if !lo.is_finite() || !hi.is_finite() || lo >= hi {
                return Err(InvalidConfig(format!("{name} must have finite LO < HI")));
            }
        }
        if self.noise_band.0 <= 0.0
            || self
                .hampel_window
                .checked_mul(2)
                .and_then(|n| n.checked_add(1))
                .is_none()
        {
            return Err(InvalidConfig(
                "noise_band must be positive and hampel_window must not overflow".into(),
            ));
        }
        self.refine_cfg.validate()?;
        Ok(())
    }

    pub fn validate_sample_rate(&self, fs: f64) -> Result<(), crate::error::O4Error> {
        use crate::error::O4Error::InvalidConfig;
        let nyquist = fs / 2.0;
        if !fs.is_finite()
            || fs <= 0.0
            || self.noise_band.0 >= self.noise_band.1.min(0.95 * nyquist)
        {
            return Err(InvalidConfig(
                "noise band is incompatible with telemetry sample rate".into(),
            ));
        }
        for cutoff in [
            self.light_cutoff,
            self.strong_cutoff,
            self.handback_cutoff.unwrap_or(self.optical_cutoff),
            self.fast_wide_cutoff,
            self.anchor_cutoff,
        ] {
            if cutoff >= nyquist {
                return Err(InvalidConfig(format!(
                    "filter cutoff {cutoff} must be below Nyquist ({nyquist} Hz)"
                )));
            }
        }
        Ok(())
    }

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
    fn invalid_settings_rejected_before_signal_processing() {
        for c in [
            Config {
                ramp: 0.0,
                ..Config::default()
            },
            Config {
                severe: f64::NAN,
                ..Config::default()
            },
            Config {
                noise_high: 1.5,
                ..Config::default()
            },
            Config {
                optical_noise: Some((5.0, 2.0)),
                ..Config::default()
            },
            Config {
                light_cutoff: f64::INFINITY,
                ..Config::default()
            },
            Config {
                drift_decay_rate: -1.0,
                ..Config::default()
            },
            Config {
                hampel_window: usize::MAX,
                ..Config::default()
            },
        ] {
            assert!(c.validate().is_err(), "accepted {c:?}");
        }
        assert!(Config::default().validate().is_ok());
        assert!(Config::m4().validate().is_ok());
        assert!(Config::default().validate_sample_rate(1000.0).is_ok());
        assert!(Config::default().validate_sample_rate(50.0).is_err());
        assert!(Config::default().validate_sample_rate(f64::NAN).is_err());
    }
    #[test]
    fn defaults_match_spec() {
        let c = Config::default();
        assert_eq!(c.severe, 8.0);
        assert_eq!(c.ramp, 0.19);
        assert_eq!(c.noise_band, (30.0, 180.0));
        assert_eq!(c.fast_wide_cutoff, 0.0);
        assert_eq!(Config::m4().fast_wide_cutoff, 16.0);
        assert!(c.handback_cutoff.is_none() && c.optical_noise.is_none());
        assert_eq!(c.drift_rebase_above, 30.0);
        assert_eq!(c.drift_decay_rate, 1.5);
        assert!(c.refine);
    }
}
