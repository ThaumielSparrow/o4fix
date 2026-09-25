//! Residual refinement (feedback-v1 wp1c), spec 2026-09-24.
pub mod geometry;
pub mod measure;
pub mod signal;

use crate::error::O4Error;
use crate::telemetry::Meta;
use rayon::prelude::*;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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
    /// With fewer than 50 such pairs the check is not applicable and passes
    /// (logged by refine()).
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

/// Post-measurement decisions for one clip (pure; unit-tested on synthetic data).
#[derive(Debug)]
pub struct Plan {
    pub bursts: Vec<RefineBurst>,
    /// Per measured window: pair times and the window's own correction-angle track.
    pub per_window: Vec<(Vec<f64>, Vec<geometry::V3>)>,
    pub skipped_reason: Option<String>,
    pub stats: signal::GeometryStats,
    /// Informational log lines.
    pub notes: Vec<String>,
}

/// Minimum calm (non-burst) context needed to verify camera geometry: 1 s of pairs.
fn min_calm_pairs(fps: f64) -> usize {
    fps.ceil().max(1.0) as usize
}

/// Geometry check, burst eligibility (edge margins against the MEASURED
/// range of each window, so a truncated decode cannot refine a burst on
/// missing frames), per-burst cap, and one union-gated correction track per
/// window.
pub fn plan(
    conds: &[((f64, f64), signal::Conditioned)],
    intervals: &[(f64, f64)],
    cfg: &RefineConfig,
    fps: f64,
    duration: f64,
) -> Plan {
    let stats = signal::geometry_stats(
        &conds.iter().map(|x| &x.1).cloned().collect::<Vec<_>>(),
        intervals,
        cfg.gate_pad_s,
        cfg.gate_fade_s,
    );
    let mut notes = vec![];
    let skip = |reason: String, stats: signal::GeometryStats, notes: Vec<String>| Plan {
        bursts: vec![],
        per_window: vec![],
        skipped_reason: Some(reason),
        stats,
        notes,
    };
    let need = min_calm_pairs(fps);
    if stats.pairs < need {
        let reason = format!(
            "not enough calm context around bursts to verify geometry ({} calm pairs, need {need})",
            stats.pairs
        );
        return skip(reason, stats, notes);
    }
    if stats.motion_ratio.is_none() {
        notes.push("     refine: motion-ratio check not applicable (<50 fast calm pairs)".into());
    }
    if stats.hp_rms_deg.is_nan()
        || stats.hp_rms_deg >= cfg.max_floor_rms_deg
        || stats
            .motion_ratio
            .is_some_and(|r| r >= cfg.max_motion_ratio)
    {
        let reason = format!(
            "geometry check failed (floor {:.1} deg/s, motion ratio {:?}, {} pairs) - camera/lens conventions not recognised",
            stats.hp_rms_deg, stats.motion_ratio, stats.pairs
        );
        return skip(reason, stats, notes);
    }
    let dt = 1.0 / fps;
    let edge = cfg.min_edge_margin_s;
    let overlaps = |a: f64, b: f64, w: &(f64, f64)| a < w.1 && b > w.0;
    let skipped = |a: f64, b: f64, max_deg: f64, note: String| RefineBurst {
        start: a,
        end: b,
        max_deg,
        applied: false,
        note: Some(note),
    };
    let mut bursts = vec![];
    for &(a, b) in intervals {
        if !conds
            .iter()
            .any(|(w, c)| overlaps(a, b, w) && !c.t.is_empty())
        {
            let note = if b + edge > duration {
                "video ends before burst window"
            } else {
                "window not measurable"
            };
            bursts.push(skipped(a, b, 0.0, note.into()));
        }
    }
    let mut per_window = vec![];
    for (w, c) in conds {
        if c.t.is_empty() {
            continue;
        }
        let (t0, t1) = (c.t[0], c.t[c.t.len() - 1]);
        let mut accepted = vec![];
        for &(a, b) in intervals.iter().filter(|(a, b)| overlaps(*a, *b, w)) {
            if a - t0 < edge {
                bursts.push(skipped(a, b, 0.0, "too close to clip edge".into()));
                continue;
            }
            if t1 - b < edge {
                // the plan had room but the decode stopped short (FRAME_COUNT overestimate)
                let note = if w.1 - b >= edge {
                    "video ends before burst window"
                } else {
                    "too close to clip edge"
                };
                bursts.push(skipped(a, b, 0.0, note.into()));
                continue;
            }
            let m = signal::max_angle_deg(&signal::burst_correction(
                c,
                (a, b),
                cfg.gate_pad_s,
                cfg.gate_fade_s,
                dt,
            ));
            if m > cfg.max_correction_deg {
                bursts.push(skipped(a, b, m, format!("correction {m:.2} deg over cap")));
                continue;
            }
            accepted.push((a, b));
            bursts.push(RefineBurst {
                start: a,
                end: b,
                max_deg: m,
                applied: true,
                note: None,
            });
        }
        // one union gate (max over accepted bursts), never a sum of per-burst tracks
        let total = if accepted.is_empty() {
            vec![[0.0; 3]; c.t.len()]
        } else {
            signal::correction(c, &accepted, cfg.gate_pad_s, cfg.gate_fade_s, dt)
        };
        per_window.push((c.t.clone(), total));
    }
    bursts.sort_by(|x, y| x.start.total_cmp(&y.start));
    Plan {
        bursts,
        per_window,
        skipped_reason: None,
        stats,
        notes,
    }
}

/// `log` and `progress` may be called from worker threads. `progress(f)`
/// reports the fraction of measurement windows finished (0 < f <= 1).
#[allow(clippy::too_many_arguments)]
pub fn refine(
    video: &Path,
    t: &[f64],
    q: &[[f64; 4]],
    intervals: &[(f64, f64)],
    meta: &Meta,
    cfg: &RefineConfig,
    log: &(dyn Fn(&str) + Sync),
    progress: &(dyn Fn(f64) + Sync),
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
    if t.len() < 2 || t.len() != q.len() {
        return Ok(unchanged("telemetry too short or inconsistent".into()));
    }
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
    let done = AtomicUsize::new(0);
    let measured: Vec<Result<Option<signal::Conditioned>, O4Error>> = windows
        .par_iter()
        .map(|&(a, b)| {
            let r = match measure::measure_window(video, a, b, &info, meta, cfg, &orient, cancel) {
                Ok(s) => Ok(signal::condition(&s, info.fps, cfg.hp_hz)),
                Err(O4Error::Cancelled) => Err(O4Error::Cancelled),
                Err(e) => {
                    log(&format!(
                        "     refine: window {a:.2}-{b:.2}s unreadable ({e})"
                    ));
                    Ok(None)
                }
            };
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            progress(d as f64 / windows.len() as f64);
            r
        })
        .collect();
    let mut conds: Vec<((f64, f64), signal::Conditioned)> = vec![];
    for (w, r) in windows.iter().zip(measured) {
        if let Some(c) = r? {
            conds.push((*w, c));
        }
    }
    let p = plan(&conds, intervals, cfg, info.fps, duration);
    for n in &p.notes {
        log(n);
    }
    if let Some(reason) = p.skipped_reason {
        let mut r = unchanged(reason);
        r.stats = Some(p.stats);
        return Ok(r);
    }
    if !p.bursts.iter().any(|b| b.applied) {
        return Ok(RefineResult {
            q: q.to_vec(),
            bursts: p.bursts,
            skipped_reason: None,
            stats: Some(p.stats),
            angle_t: vec![],
            angle: vec![],
        });
    }
    let (ta, aa) = signal::chain_windows(p.per_window);
    let q2 = signal::apply_increments(t, q, &ta, &aa);
    Ok(RefineResult {
        q: q2,
        bursts: p.bursts,
        skipped_reason: None,
        stats: Some(p.stats),
        angle_t: ta,
        angle: aa,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use signal::{condition, Conditioned, PairSeries};

    const FPS: f64 = 100.0;

    fn cond(t0: f64, t1: f64, resid: impl Fn(f64) -> geometry::V3, tel: f64) -> Conditioned {
        let n = ((t1 - t0) * FPS).round() as usize;
        let t: Vec<f64> = (0..n).map(|i| t0 + i as f64 / FPS).collect();
        let s = PairSeries {
            resid: t.iter().map(|&x| Some(resid(x))).collect(),
            inliers: vec![500; n],
            tel_rate: vec![tel; n],
            t,
        };
        condition(&s, FPS, 1.0).unwrap()
    }

    fn calm(x: f64) -> geometry::V3 {
        [
            0.02 * (2.0 * std::f64::consts::PI * 6.0 * x).sin(),
            0.01,
            0.0,
        ]
    }

    fn one(c: Conditioned, w: (f64, f64), iv: &[(f64, f64)], duration: f64) -> Plan {
        plan(&[(w, c)], iv, &RefineConfig::default(), FPS, duration)
    }

    #[test]
    fn overlapping_burst_gates_never_weigh_more_than_one() {
        let c = cond(0.0, 10.0, calm, 0.5);
        // 0.21 s apart: with 0.25 s pad the two gates overlap between the bursts
        let p = one(c.clone(), (0.0, 10.0), &[(4.0, 5.0), (5.21, 6.0)], 10.0);
        assert!(p.skipped_reason.is_none(), "{:?}", p.skipped_reason);
        assert!(p.bursts.iter().all(|b| b.applied));
        let ang = &p.per_window[0].1;
        let mut saw_full = false;
        for i in 0..c.t.len() - 1 {
            let h = c.hp[i + 1][0];
            if h.abs() < 1e-4 {
                continue;
            }
            // effective gate*conf weight: dA/dt = -w * hp
            let w = -(ang[i + 1][0] - ang[i][0]) * FPS / h;
            assert!(
                (-1e-6..=1.0 + 1e-6).contains(&w),
                "weight {w} at t={}",
                c.t[i + 1]
            );
            if (5.0..5.21).contains(&c.t[i + 1]) && (w - 1.0).abs() < 1e-6 {
                saw_full = true;
            }
        }
        assert!(saw_full, "weight 1 between the bursts");
    }

    #[test]
    fn burst_over_cap_is_skipped() {
        let big = |x: f64| {
            if (4.0..6.0).contains(&x) {
                [(2.0 * std::f64::consts::PI * 2.0 * x).sin(), 0.0, 0.0]
            } else {
                calm(x)
            }
        };
        let p = one(cond(0.0, 10.0, big, 0.5), (0.0, 10.0), &[(4.0, 6.0)], 10.0);
        assert!(p.skipped_reason.is_none(), "{:?}", p.skipped_reason);
        let b = &p.bursts[0];
        assert!(!b.applied && b.max_deg > 4.0, "{b:?}");
        assert!(b.note.as_deref().unwrap().contains("over cap"));
        assert!(p.per_window[0].1.iter().all(|v| *v == [0.0; 3]));
    }

    #[test]
    fn burst_too_close_to_clip_edge_is_skipped() {
        let iv = [(0.3, 1.5), (4.0, 5.0), (9.7, 9.9)];
        let p = one(cond(0.0, 10.0, calm, 0.5), (0.0, 10.0), &iv, 10.0);
        assert!(p.skipped_reason.is_none(), "{:?}", p.skipped_reason);
        let got: Vec<_> = p
            .bursts
            .iter()
            .map(|b| (b.applied, b.note.clone()))
            .collect();
        let edge = Some("too close to clip edge".to_string());
        assert_eq!(
            got,
            vec![(false, edge.clone()), (true, None), (false, edge)]
        );
    }

    #[test]
    fn truncated_decode_reports_video_end() {
        // planned window 3..9 but frames stop at 6.2 s (FRAME_COUNT overestimate)
        let iv = [(4.0, 5.0), (6.0, 8.0), (20.0, 21.0)];
        let p = one(cond(3.0, 6.2, calm, 0.5), (3.0, 9.0), &iv, 9.0);
        assert!(p.skipped_reason.is_none(), "{:?}", p.skipped_reason);
        let got: Vec<_> = p
            .bursts
            .iter()
            .map(|b| (b.applied, b.note.clone()))
            .collect();
        let end = Some("video ends before burst window".to_string());
        assert_eq!(got, vec![(true, None), (false, end.clone()), (false, end)]);
    }

    #[test]
    fn geometry_check_trips_on_floor_rms() {
        let noisy = |x: f64| [0.3 * (2.0 * std::f64::consts::PI * 6.0 * x).sin(), 0.0, 0.0];
        let p = one(
            cond(0.0, 10.0, noisy, 0.5),
            (0.0, 10.0),
            &[(4.0, 5.0)],
            10.0,
        );
        let r = p.skipped_reason.expect("must skip");
        assert!(r.contains("conventions not recognised"), "{r}");
        assert!(p.stats.hp_rms_deg > 8.0 && p.bursts.is_empty());
    }

    #[test]
    fn geometry_check_trips_on_motion_ratio() {
        // calm pairs moving at ~57 deg/s with a residual as large as the motion
        let p = one(
            cond(0.0, 10.0, |_| [1.0, 0.0, 0.0], 1.0),
            (0.0, 10.0),
            &[(4.0, 5.0)],
            10.0,
        );
        assert!(p.stats.hp_rms_deg < 8.0, "{:?}", p.stats);
        assert!(p.stats.motion_ratio.unwrap() > 0.94);
        assert!(p
            .skipped_reason
            .unwrap()
            .contains("conventions not recognised"));
    }

    #[test]
    fn motion_ratio_not_applicable_is_logged_and_passes() {
        let p = one(cond(0.0, 10.0, calm, 0.5), (0.0, 10.0), &[(4.0, 5.0)], 10.0);
        assert!(p.skipped_reason.is_none() && p.stats.motion_ratio.is_none());
        assert!(p
            .notes
            .iter()
            .any(|n| n.contains("motion-ratio check not applicable")));
    }

    #[test]
    fn too_little_calm_context_has_its_own_reason() {
        // window mostly covered by the burst gate: < 1 s of calm pairs
        let p = one(cond(3.5, 5.9, calm, 0.5), (3.5, 5.9), &[(4.0, 5.4)], 10.0);
        let r = p.skipped_reason.expect("must skip");
        assert!(r.starts_with("not enough calm context"), "{r}");
    }

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
