//! Residual refinement (feedback-v1 wp1c), spec 2026-09-24.
pub mod geometry;
pub mod measure;
pub mod signal;

use crate::error::O4Error;
use crate::telemetry::Meta;
use rayon::prelude::*;
use std::path::Path;
use std::sync::atomic::AtomicBool;

#[derive(Clone, Debug, PartialEq)]
pub struct RefineConfig {
    /// Measurement window padding around each burst (s).
    pub margin_s: f64,
    /// Minimum measured context between a burst and its window edge (s).
    pub min_edge_margin_s: f64,
    /// Sensor rolling-shutter readout, top to bottom row (ms).
    pub readout_ms: f64,
    /// Render-crop filter on normalized coords: |x/z| < crop.0, |y/z| < crop.1.
    pub crop: (f64, f64),
    pub hp_hz: f64,
    pub gate_pad_s: f64,
    pub gate_fade_s: f64,
    /// Bursts whose correction exceeds this are left unchanged (deg).
    pub max_correction_deg: f64,
    /// Geometry check: non-burst high-passed residual RMS must stay below (deg/s).
    pub max_floor_rms_deg: f64,
    /// Geometry check: median |raw residual| / median telemetry rate on
    /// non-burst pairs with telemetry rate > 30 deg/s must stay below this.
    /// Measured 2026-09-24 (whole-clip, correct O4P mount): 0073 0.139,
    /// 0071 0.144, 0060 0.474 (0060's near-burst margins carry noisy
    /// telemetry; per-window 0.07-1.05). Wrong mount (identity, probe axes 0,
    /// 0073 late window 130-148 s): 1.864 (correct mount same window 0.116).
    /// Default = geometric midpoint of 0.474 and 1.864 = 0.94.
    pub max_motion_ratio: f64,
    pub max_features: i32,
    pub min_inliers: usize,
}

impl Default for RefineConfig {
    fn default() -> Self {
        Self {
            margin_s: 1.0,
            min_edge_margin_s: 0.5,
            readout_ms: 5.092569,
            crop: (1.30, 0.72),
            hp_hz: 1.0,
            gate_pad_s: 0.25,
            gate_fade_s: 0.15,
            max_correction_deg: 4.0,
            max_floor_rms_deg: 8.0,
            max_motion_ratio: 0.94,
            max_features: 1200,
            min_inliers: 30,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RefineBurst {
    pub start: f64,
    pub end: f64,
    pub max_deg: f64,
    pub applied: bool,
    pub note: Option<String>,
}

#[derive(Debug)]
pub struct RefineResult {
    pub q: Vec<[f64; 4]>,
    pub bursts: Vec<RefineBurst>,
    pub skipped_reason: Option<String>,
    pub stats: Option<signal::GeometryStats>,
    /// Applied correction-angle track (chain_windows output); empty when nothing applied.
    pub angle_t: Vec<f64>,
    pub angle: Vec<geometry::V3>,
}

impl RefineConfig {
    pub fn validate(&self) -> Result<(), O4Error> {
        let pos = [
            self.margin_s,
            self.readout_ms,
            self.crop.0,
            self.crop.1,
            self.hp_hz,
            self.gate_fade_s,
            self.max_correction_deg,
            self.max_floor_rms_deg,
            self.max_motion_ratio,
        ];
        let nonneg = [self.min_edge_margin_s, self.gate_pad_s];
        if pos.iter().any(|v| !v.is_finite() || *v <= 0.0)
            || nonneg.iter().any(|v| !v.is_finite() || *v < 0.0)
            || self.max_features < 40
            || self.min_inliers < 3
        {
            return Err(O4Error::InvalidConfig(
                "refine settings must be finite and positive".into(),
            ));
        }
        Ok(())
    }
}

fn plan_windows(intervals: &[(f64, f64)], margin: f64, duration: f64) -> Vec<(f64, f64)> {
    let mut w: Vec<(f64, f64)> = intervals
        .iter()
        .map(|&(a, b)| ((a - margin).max(0.0), (b + margin).min(duration)))
        .collect();
    w.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut out: Vec<(f64, f64)> = vec![];
    for (a, b) in w {
        match out.last_mut() {
            Some(l) if a <= l.1 => l.1 = l.1.max(b),
            _ => out.push((a, b)),
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
pub fn refine(
    video: &Path,
    t: &[f64],
    q: &[[f64; 4]],
    intervals: &[(f64, f64)],
    meta: &Meta,
    cfg: &RefineConfig,
    log: &(dyn Fn(&str) + Sync),
    cancel: &AtomicBool,
) -> Result<RefineResult, O4Error> {
    let unchanged = |reason: String| RefineResult {
        q: q.to_vec(),
        bursts: vec![],
        skipped_reason: Some(reason),
        stats: None,
        angle_t: vec![],
        angle: vec![],
    };
    if meta.camera_matrix.is_none() || meta.distortion.is_none() {
        return Ok(unchanged("no lens metadata in telemetry".into()));
    }
    let info = match measure::video_info(video) {
        Ok(i) => i,
        Err(e) => return Ok(unchanged(format!("cannot decode video ({e})"))),
    };
    let duration = info.frames as f64 / info.fps;
    let windows = plan_windows(intervals, cfg.margin_s, duration);
    let orient = geometry::Orientation { t, q };
    let measured: Vec<Result<Option<signal::Conditioned>, O4Error>> = windows
        .par_iter()
        .map(|&(a, b)| {
            match measure::measure_window(video, a, b, &info, meta, cfg, &orient, cancel) {
                Ok(s) => Ok(signal::condition(&s, info.fps, cfg.hp_hz)),
                Err(O4Error::Cancelled) => Err(O4Error::Cancelled),
                Err(e) => {
                    log(&format!(
                        "     refine: window {a:.2}-{b:.2}s unreadable ({e})"
                    ));
                    Ok(None)
                }
            }
        })
        .collect();
    let mut conds: Vec<((f64, f64), signal::Conditioned)> = vec![];
    for (w, r) in windows.iter().zip(measured) {
        if let Some(c) = r? {
            conds.push((*w, c));
        }
    }
    let stats = signal::geometry_stats(
        &conds.iter().map(|x| &x.1).cloned().collect::<Vec<_>>(),
        intervals,
        cfg.gate_pad_s,
        cfg.gate_fade_s,
    );
    if stats.pairs < 100
        || stats.hp_rms_deg.is_nan()
        || stats.hp_rms_deg >= cfg.max_floor_rms_deg
        || stats
            .motion_ratio
            .is_some_and(|r| r >= cfg.max_motion_ratio)
    {
        let mut r = unchanged(format!(
            "geometry check failed (floor {:.1} deg/s, motion ratio {:?}, {} pairs) - camera/lens conventions not recognised",
            stats.hp_rms_deg, stats.motion_ratio, stats.pairs));
        r.stats = Some(stats);
        return Ok(r);
    }
    let dt = 1.0 / info.fps;
    let mut bursts = vec![];
    let mut per_window: Vec<(Vec<f64>, Vec<geometry::V3>)> = vec![];
    for &(a, b) in intervals {
        if !conds.iter().any(|(w, _)| a >= w.0 && b <= w.1) {
            bursts.push(RefineBurst {
                start: a,
                end: b,
                max_deg: 0.0,
                applied: false,
                note: Some("window not measurable".into()),
            });
        }
    }
    for ((wa, wb), c) in &conds {
        let mut total = vec![[0.0; 3]; c.t.len()];
        for &(a, b) in intervals.iter().filter(|(a, b)| *a >= *wa && *b <= *wb) {
            if a - wa < cfg.min_edge_margin_s || wb - b < cfg.min_edge_margin_s {
                bursts.push(RefineBurst {
                    start: a,
                    end: b,
                    max_deg: 0.0,
                    applied: false,
                    note: Some("too close to clip edge".into()),
                });
                continue;
            }
            let ang = signal::burst_correction(c, (a, b), cfg.gate_pad_s, cfg.gate_fade_s, dt);
            let m = signal::max_angle_deg(&ang);
            if m > cfg.max_correction_deg {
                bursts.push(RefineBurst {
                    start: a,
                    end: b,
                    max_deg: m,
                    applied: false,
                    note: Some(format!("correction {m:.2} deg over cap")),
                });
                continue;
            }
            for (tot, v) in total.iter_mut().zip(&ang) {
                for k in 0..3 {
                    tot[k] += v[k];
                }
            }
            bursts.push(RefineBurst {
                start: a,
                end: b,
                max_deg: m,
                applied: true,
                note: None,
            });
        }
        per_window.push((c.t.clone(), total));
    }
    bursts.sort_by(|x, y| x.start.total_cmp(&y.start));
    if !bursts.iter().any(|b| b.applied) {
        return Ok(RefineResult {
            q: q.to_vec(),
            bursts,
            skipped_reason: None,
            stats: Some(stats),
            angle_t: vec![],
            angle: vec![],
        });
    }
    let (ta, aa) = signal::chain_windows(per_window);
    let q2 = signal::apply_increments(t, q, &ta, &aa);
    Ok(RefineResult {
        q: q2,
        bursts,
        skipped_reason: None,
        stats: Some(stats),
        angle_t: ta,
        angle: aa,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_clip_to_video_and_merge() {
        let w = plan_windows(&[(0.3, 0.8), (1.2, 1.5), (5.0, 6.0)], 1.0, 10.0);
        assert_eq!(w, vec![(0.0, 2.5), (4.0, 7.0)]);
    }
    #[test]
    fn default_refine_config_is_valid() {
        assert!(RefineConfig::default().validate().is_ok());
        assert!(RefineConfig {
            max_correction_deg: f64::NAN,
            ..Default::default()
        }
        .validate()
        .is_err());
    }
}
