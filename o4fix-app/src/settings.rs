use o4core::config::Config;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// JSON mirror of o4core::config::Config. Field names follow the CLI flags
/// (o4fix-cli/src/args.rs), so `noise_window` here maps to
/// `Config.noise_window_ms`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ConfigDto {
    pub severe: f64,
    pub severe_pad: f64,
    pub severe_merge: f64,
    pub ramp: f64,
    pub light_cutoff: f64,
    pub strong_cutoff: f64,
    pub noise_low: f64,
    pub noise_high: f64,
    pub noise_band: [f64; 2],
    pub noise_window: f64,
    pub hampel_window: usize,
    pub hampel_sigma: f64,
    pub optical_cutoff: f64,
    pub handback_cutoff: Option<f64>,
    pub fast_handback: [f64; 2],
    pub gyro_trust_noise: [f64; 2],
    pub patch_pad: f64,
    pub patch_merge: f64,
    pub optical_noise: Option<[f64; 2]>,
    pub fast_wide_cutoff: f64,
    pub fast_wide_ramp: [f64; 2],
    pub fast_wide_accel: f64,
    pub anchor_mode: bool,
    pub anchor_cutoff: f64,
}

impl Default for ConfigDto {
    fn default() -> Self {
        Self::from_config(&Config::default())
    }
}

impl ConfigDto {
    pub fn from_config(c: &Config) -> Self {
        Self {
            severe: c.severe,
            severe_pad: c.severe_pad,
            severe_merge: c.severe_merge,
            ramp: c.ramp,
            light_cutoff: c.light_cutoff,
            strong_cutoff: c.strong_cutoff,
            noise_low: c.noise_low,
            noise_high: c.noise_high,
            noise_band: [c.noise_band.0, c.noise_band.1],
            noise_window: c.noise_window_ms,
            hampel_window: c.hampel_window,
            hampel_sigma: c.hampel_sigma,
            optical_cutoff: c.optical_cutoff,
            handback_cutoff: c.handback_cutoff,
            fast_handback: [c.fast_handback.0, c.fast_handback.1],
            gyro_trust_noise: [c.gyro_trust_noise.0, c.gyro_trust_noise.1],
            patch_pad: c.patch_pad,
            patch_merge: c.patch_merge,
            optical_noise: c.optical_noise.map(|(a, b)| [a, b]),
            fast_wide_cutoff: c.fast_wide_cutoff,
            fast_wide_ramp: [c.fast_wide_ramp.0, c.fast_wide_ramp.1],
            fast_wide_accel: c.fast_wide_accel,
            anchor_mode: c.anchor_mode,
            anchor_cutoff: c.anchor_cutoff,
        }
    }

    /// Exhaustive struct literal: adding a Config field breaks this at
    /// compile time (same guarantee as o4fix-cli's to_config).
    pub fn to_config(&self) -> Config {
        Config {
            severe: self.severe,
            severe_pad: self.severe_pad,
            severe_merge: self.severe_merge,
            ramp: self.ramp,
            light_cutoff: self.light_cutoff,
            strong_cutoff: self.strong_cutoff,
            noise_low: self.noise_low,
            noise_high: self.noise_high,
            noise_band: (self.noise_band[0], self.noise_band[1]),
            noise_window_ms: self.noise_window,
            hampel_window: self.hampel_window,
            hampel_sigma: self.hampel_sigma,
            optical_cutoff: self.optical_cutoff,
            handback_cutoff: self.handback_cutoff,
            fast_handback: (self.fast_handback[0], self.fast_handback[1]),
            gyro_trust_noise: (self.gyro_trust_noise[0], self.gyro_trust_noise[1]),
            patch_pad: self.patch_pad,
            patch_merge: self.patch_merge,
            optical_noise: self.optical_noise.map(|a| (a[0], a[1])),
            fast_wide_cutoff: self.fast_wide_cutoff,
            fast_wide_ramp: (self.fast_wide_ramp[0], self.fast_wide_ramp[1]),
            fast_wide_accel: self.fast_wide_accel,
            anchor_mode: self.anchor_mode,
            anchor_cutoff: self.anchor_cutoff,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct GuiSettings {
    pub profile: String,
    pub config: ConfigDto,
    pub output_dir: Option<String>,
    pub concurrent_files: usize,
}

impl Default for GuiSettings {
    fn default() -> Self {
        Self {
            profile: "m2".into(),
            config: ConfigDto::default(),
            output_dir: None,
            concurrent_files: 1,
        }
    }
}

// Pure file I/O (unit-testable without an AppHandle).
pub fn load_from(path: &Path) -> GuiSettings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}
pub fn save_to(path: &Path, s: &GuiSettings) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        path,
        serde_json::to_string_pretty(s).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

pub fn settings_path(app: &tauri::AppHandle) -> PathBuf {
    use tauri::Manager;
    app.path()
        .app_config_dir()
        .expect("app config dir")
        .join("settings.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dto_default_round_trips_to_config_default() {
        assert_eq!(ConfigDto::default().to_config(), Config::default());
        assert_eq!(
            ConfigDto::from_config(&Config::m4()).to_config(),
            Config::m4()
        );
    }

    #[test]
    fn settings_serde_round_trip() {
        let s = GuiSettings {
            profile: "m4".into(),
            config: ConfigDto::from_config(&Config::m4()),
            output_dir: Some("D:\\out".into()),
            concurrent_files: 3,
        };
        let j = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<GuiSettings>(&j).unwrap(), s);
    }

    #[test]
    fn load_missing_or_corrupt_falls_back_to_default() {
        let dir = std::env::temp_dir().join("o4fix_settings_test");
        let p = dir.join("settings.json");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(load_from(&p), GuiSettings::default()); // missing
        save_to(&p, &GuiSettings::default()).unwrap();
        assert_eq!(load_from(&p), GuiSettings::default()); // round trip
        std::fs::write(&p, "{not json").unwrap();
        assert_eq!(load_from(&p), GuiSettings::default()); // corrupt
        let _ = std::fs::remove_dir_all(&dir);
    }
}
