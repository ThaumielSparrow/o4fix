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
    pub drift_rebase_above: f64,
    pub drift_decay_rate: f64,
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
            drift_rebase_above: c.drift_rebase_above,
            drift_decay_rate: c.drift_decay_rate,
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
            drift_rebase_above: self.drift_rebase_above,
            drift_decay_rate: self.drift_decay_rate,
        }
    }
}

/// Bumped whenever a stored settings.json needs rewriting on load. See
/// `GuiSettings::migrate`.
pub const CURRENT_SETTINGS_VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct GuiSettings {
    /// Field-level `default` (0), NOT the container default: a file written
    /// before versioning existed must read back as v0 so it gets migrated.
    #[serde(default)]
    pub settings_version: u32,
    pub profile: String,
    pub config: ConfigDto,
    pub output_dir: Option<String>,
    pub concurrent_files: usize,
}

impl Default for GuiSettings {
    fn default() -> Self {
        Self {
            settings_version: CURRENT_SETTINGS_VERSION,
            profile: "m2".into(),
            config: ConfigDto::default(),
            output_dir: None,
            concurrent_files: 1,
        }
    }
}

impl GuiSettings {
    /// Bring a stored settings.json up to `CURRENT_SETTINGS_VERSION`.
    /// Returns true if anything changed (caller persists).
    ///
    /// v0 -> v1 (0.1.2): drift rebase became default-on. A v0 file holding
    /// `drift_rebase_above: 0.0` recorded the *old default*, not a choice --
    /// the flag shipped in no release before 0.1.2 -- so adopt the new
    /// default rather than silently leaving those users on the old
    /// in-burst bridge. Runs once; a deliberate 0 set from 0.1.2 onwards is
    /// stored at v1 and never touched.
    fn migrate(&mut self) -> bool {
        if self.settings_version >= CURRENT_SETTINGS_VERSION {
            return false;
        }
        if self.config.drift_rebase_above == 0.0 {
            self.config.drift_rebase_above = Config::default().drift_rebase_above;
        }
        self.settings_version = CURRENT_SETTINGS_VERSION;
        true
    }
}

// Pure file I/O (unit-testable without an AppHandle).
pub fn load_from(path: &Path) -> GuiSettings {
    let mut s: GuiSettings = std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    if s.migrate() {
        // Best-effort persist so the migration is one-time and visible in
        // the file; if the write fails it simply reruns on the next launch.
        let _ = save_to(path, &s);
    }
    s
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
            settings_version: CURRENT_SETTINGS_VERSION,
            profile: "m4".into(),
            config: ConfigDto::from_config(&Config::m4()),
            output_dir: Some("D:\\out".into()),
            concurrent_files: 3,
        };
        let j = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<GuiSettings>(&j).unwrap(), s);
    }

    /// 2026-08-13 incident: an old settings.json written before a Config
    /// field was added must still deserialize (missing keys fall back to
    /// Config::default() via the struct-level `#[serde(default)]`).
    #[test]
    fn config_dto_tolerates_settings_missing_drift_fields() {
        let old = serde_json::json!({
            "severe": 8.0, "severe_pad": 0.2, "severe_merge": 0.2, "ramp": 0.3,
            "light_cutoff": 25.0, "strong_cutoff": 2.5, "noise_low": 1.5,
            "noise_high": 5.0, "noise_band": [30.0, 180.0],
            "noise_window": 100.0, "hampel_window": 7, "hampel_sigma": 6.0,
            "optical_cutoff": 8.0, "handback_cutoff": null,
            "fast_handback": [100.0, 250.0],
            "gyro_trust_noise": [200.0, 300.0], "patch_pad": 0.5,
            "patch_merge": 1.0, "optical_noise": null,
            "fast_wide_cutoff": 0.0, "fast_wide_ramp": [150.0, 300.0],
            "fast_wide_accel": 1500.0, "anchor_mode": false,
            "anchor_cutoff": 1.5
            // no drift_rebase_above / drift_decay_rate keys
        });
        let dto: ConfigDto = serde_json::from_value(old).unwrap();
        // v0.1.2 flipped this default on; a pre-v0.1.1 settings.json (which
        // predates the key entirely) therefore adopts drift rebase.
        assert_eq!(dto.drift_rebase_above, 30.0);
        assert_eq!(dto.drift_decay_rate, 1.5);
        assert_eq!(dto.to_config(), Config::default());
    }

    /// A settings.json written by a pre-0.1.2 build that DID have the key
    /// (branch builds) stores an explicit 0.0 and no version stamp -- it is
    /// the old default, so v0 -> v1 adopts the new one.
    #[test]
    fn v0_settings_migrate_drift_rebase_on() {
        let mut s: GuiSettings = serde_json::from_value(serde_json::json!({
            "profile": "m2",
            "config": { "drift_rebase_above": 0.0 },
            "output_dir": null, "concurrent_files": 1
        }))
        .unwrap();
        assert_eq!(s.settings_version, 0, "unstamped file must read as v0");
        assert_eq!(s.config.drift_rebase_above, 0.0);
        assert!(s.migrate());
        assert_eq!(s.config.drift_rebase_above, 30.0);
        assert_eq!(s.settings_version, CURRENT_SETTINGS_VERSION);
        // idempotent: a second pass is a no-op
        assert!(!s.migrate());
        assert_eq!(s.config.drift_rebase_above, 30.0);
    }

    /// The horizon-lock case: 0 chosen deliberately under 0.1.2+ is stamped
    /// v1 and must survive untouched.
    #[test]
    fn v1_settings_keep_deliberate_zero() {
        let mut s = GuiSettings {
            settings_version: CURRENT_SETTINGS_VERSION,
            ..GuiSettings::default()
        };
        s.config.drift_rebase_above = 0.0;
        assert!(!s.migrate());
        assert_eq!(s.config.drift_rebase_above, 0.0);
    }

    /// Migration must not clobber other customized values.
    #[test]
    fn v0_migration_preserves_other_settings() {
        let mut s: GuiSettings = serde_json::from_value(serde_json::json!({
            "profile": "m4",
            "config": { "drift_rebase_above": 0.0, "fast_wide_cutoff": 16.0,
                        "severe": 6.5 },
            "output_dir": "D:\\out", "concurrent_files": 3
        }))
        .unwrap();
        assert!(s.migrate());
        assert_eq!(s.config.drift_rebase_above, 30.0);
        assert_eq!(s.config.fast_wide_cutoff, 16.0);
        assert_eq!(s.config.severe, 6.5);
        assert_eq!(s.profile, "m4");
        assert_eq!(s.output_dir.as_deref(), Some("D:\\out"));
        assert_eq!(s.concurrent_files, 3);
    }

    /// load_from persists the migration so it runs exactly once.
    #[test]
    fn load_from_persists_migration() {
        let dir = std::env::temp_dir().join("o4fix_migrate_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(
            &path,
            r#"{"profile":"m2","config":{"drift_rebase_above":0.0},
                "output_dir":null,"concurrent_files":1}"#,
        )
        .unwrap();

        let loaded = load_from(&path);
        assert_eq!(loaded.config.drift_rebase_above, 30.0);

        let on_disk: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(on_disk["settings_version"], CURRENT_SETTINGS_VERSION);
        assert_eq!(on_disk["config"]["drift_rebase_above"], 30.0);

        // second load is a plain read: a deliberate 0 now sticks
        std::fs::write(
            &path,
            serde_json::to_string(&GuiSettings {
                config: ConfigDto {
                    drift_rebase_above: 0.0,
                    ..ConfigDto::default()
                },
                ..GuiSettings::default()
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(load_from(&path).config.drift_rebase_above, 0.0);
        std::fs::remove_file(&path).ok();
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
