use crate::config::Config;
use crate::detect::{find_intervals, CleanDiag};
use crate::dsp;
use crate::error::O4Error;
use crate::optical;
use crate::telemetry::Meta;
use std::path::Path;
use std::sync::atomic::AtomicBool;

const R2D: f64 = 180.0 / std::f64::consts::PI;
const D2R: f64 = std::f64::consts::PI / 180.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptPhase {
    Calib,
    Noisy,
}

pub struct OpticalPatch {
    pub rates: Vec<[f64; 3]>,
    /// Rate samples actually supported by an accepted optical segment.
    pub supported: Vec<bool>,
}

impl OpticalPatch {
    /// Every integrated rate in a severe burst must have optical support.
    /// Refuse the clip rather than silently rebase a filtered-gyro fallback.
    pub fn require_coverage(&self, t: &[f64], intervals: &[(f64, f64)]) -> Result<(), O4Error> {
        for &(a, b) in intervals {
            let i0 = dsp::searchsorted_left(t, a);
            let i1 = dsp::searchsorted_right(t, b)
                .saturating_sub(1)
                .min(t.len() - 1);
            if i1 >= i0 + 8 && (i0..i1).any(|i| !self.supported.get(i).copied().unwrap_or(false)) {
                return Err(O4Error::OpticalCoverage { start: a, end: b });
            }
        }
        Ok(())
    }
}

/// Original calibration-window selection, shared with experiment tooling.
pub fn calibration_intervals(
    tm: &[f64],
    cleaned: &[[f64; 3]],
    diag: &CleanDiag,
) -> Vec<(f64, f64)> {
    // calibration sections (o4fix.py:420-431)
    let calib_all = find_intervals(
        &diag.alpha.iter().map(|&a| a < 0.02).collect::<Vec<_>>(),
        tm,
        -0.2,
        0.0,
        3.0,
    );
    let motion: Vec<f64> = cleaned
        .iter()
        .map(|r| (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt() * R2D)
        .collect();
    let mut scored: Vec<(f64, f64, f64)> = calib_all
        .iter()
        .map(|&(a, b)| {
            let vals: Vec<f64> = tm
                .iter()
                .zip(&motion)
                .filter(|(t, _)| **t >= a && **t <= b)
                .map(|(_, m)| *m)
                .collect();
            let mean = vals.iter().sum::<f64>() / vals.len() as f64;
            let std =
                (vals.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / vals.len() as f64).sqrt(); // np.std: population, ddof=0
            (std, a, b.min(a + 4.0))
        })
        .collect();
    scored.sort_by(|x, y| y.partial_cmp(x).unwrap()); // sort(reverse=True), tuple order
    scored.iter().take(6).map(|&(_, a, b)| (a, b)).collect()
}

// mirrors o4fix.py's optical_patch(video, tm, cleaned_rad, diag, fs, args, meta); log/on_interval/cancel are Rust-only additions (no print()/cancellation in Python)
#[allow(clippy::too_many_arguments)]
pub fn optical_patch(
    video: &Path,
    tm: &[f64],
    cleaned: &[[f64; 3]],
    diag: &CleanDiag,
    fs: f64,
    cfg: &Config,
    meta: &Meta,
    log: &(dyn Fn(&str) + Sync),
    on_interval: &(dyn Fn(OptPhase, usize, usize) + Sync),
    cancel: &AtomicBool,
) -> Result<OpticalPatch, O4Error> {
    // alpha_opt: separate optical trigger if configured (o4fix.py:405-412)
    let alpha_opt: Vec<f64> = match cfg.optical_noise {
        Some((lo, hi)) => {
            let a: Vec<f64> = diag
                .noise
                .iter()
                .map(|&n| ((n - lo) / (hi - lo)).clamp(0.0, 1.0))
                .collect();
            dsp::uniform_filter1d(&a, ((0.2 * fs) as usize).max(3))
        }
        None => diag.alpha.clone(),
    };
    let noisy = find_intervals(
        &alpha_opt.iter().map(|&a| a > 0.15).collect::<Vec<_>>(),
        tm,
        cfg.patch_pad,
        cfg.patch_merge,
        0.2,
    );
    if noisy.is_empty() {
        log("   optical patch: no noisy sections detected, skipping");
        return Ok(OpticalPatch {
            rates: cleaned.to_vec(),
            supported: vec![false; cleaned.len()],
        });
    }

    let calib = calibration_intervals(tm, cleaned, diag);
    if calib.is_empty() {
        log("   optical patch: no clean calibration sections");
        return Err(O4Error::CalibrationFailed { r2: None });
    }

    let total: f64 = noisy.iter().chain(&calib).map(|(a, b)| b - a).sum();
    log(&format!(
        "   optical patch: analyzing {} noisy + {} calibration sections ({:.0} s of video)...",
        noisy.len(),
        calib.len(),
        total
    ));
    let opt_c = optical::video_rates(video, &calib, meta, cancel, &|d, n| {
        on_interval(OptPhase::Calib, d, n)
    })?;
    let gyro_deg: Vec<[f64; 3]> = cleaned
        .iter()
        .map(|r| core::array::from_fn(|k| r[k] * R2D))
        .collect();
    let Some(al) = optical::fit_video_alignment(&opt_c, tm, &gyro_deg, fs) else {
        log("   optical patch: calibration failed");
        return Err(O4Error::CalibrationFailed { r2: None });
    };
    log(&format!(
        "   optical patch: video/gyro alignment R2={:.3}, time offset {:.0} ms",
        al.r2,
        al.shift * 1000.0
    ));
    if !al.r2.is_finite() || al.r2 < 0.8 {
        return Err(O4Error::CalibrationFailed { r2: Some(al.r2) });
    }

    let opt_n = optical::video_rates(video, &noisy, meta, cancel, &|d, n| {
        on_interval(OptPhase::Noisy, d, n)
    })?;
    if opt_n.t.is_empty() {
        // DEVIATION from o4fix.py:456 (`return clean`): python's caller detects
        // that via `patched is clean` and refuses to write output; Rust makes
        // the refusal explicit so pipeline::process can't silently splice
        // unrepaired rates. Same user-visible outcome: no output, clear message.
        log("   optical patch: no optical samples in noisy sections");
        return Err(O4Error::CalibrationFailed { r2: None });
    }
    // patch_deg = degrees(ov) @ N ; tv += shift (o4fix.py:451-452)
    let patch_deg: Vec<[f64; 3]> = opt_n
        .omega
        .iter()
        .map(|o| core::array::from_fn(|c| (0..3).map(|r| o[r] * R2D * al.n[r][c]).sum()))
        .collect();
    let tv: Vec<f64> = opt_n.t.iter().map(|t| t + al.shift).collect();

    let mut out: Vec<[f64; 3]> = cleaned
        .iter()
        .map(|r| core::array::from_fn(|k| r[k] * R2D))
        .collect();
    let light = &diag.light;

    // rate-aware handback (o4fix.py:458-468)
    let hb_cut = cfg.handback_cutoff.unwrap_or(cfg.optical_cutoff);
    let mut medium = dsp::filtfilt3(&dsp::butter_low(2, hb_cut / (fs / 2.0)), light);
    let mag: Vec<f64> = medium
        .iter()
        .map(|r| (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt())
        .collect();
    let rate_mag = dsp::uniform_filter1d(&mag, ((0.1 * fs) as usize).max(3));
    let (lo_r, hi_r) = cfg.fast_handback;

    // fast-wide branch (M4; o4fix.py:474-487)
    if cfg.fast_wide_cutoff != 0.0 {
        let wide = dsp::filtfilt3(
            &dsp::butter_low(2, cfg.fast_wide_cutoff / (fs / 2.0)),
            light,
        );
        let (lo_w, hi_w) = cfg.fast_wide_ramp;
        let mut w_wide: Vec<f64> = rate_mag
            .iter()
            .map(|&m| ((m - lo_w) / (hi_w - lo_w).max(1e-6)).clamp(0.0, 1.0))
            .collect();
        if cfg.fast_wide_accel != 0.0 {
            let grad: Vec<f64> = dsp::gradient(&rate_mag)
                .iter()
                .map(|g| (g * fs).abs())
                .collect();
            let acc = dsp::uniform_filter1d(&grad, ((0.1 * fs) as usize).max(3));
            for i in 0..w_wide.len() {
                w_wide[i] *= (1.0 - acc[i] / cfg.fast_wide_accel).clamp(0.0, 1.0);
            }
        }
        let w_wide = dsp::uniform_filter1d(&w_wide, ((0.15 * fs) as usize).max(3));
        for i in 0..medium.len() {
            for k in 0..3 {
                medium[i][k] = (1.0 - w_wide[i]) * medium[i][k] + w_wide[i] * wide[i][k];
            }
        }
    }

    // per-burst optical splice-in (o4fix.py:489-534)
    let vfps = if tv.len() > 1 {
        let m = tv.len().min(100);
        let mut d: Vec<f64> = tv[..m].windows(2).map(|w| w[1] - w[0]).collect();
        d.sort_by(f64::total_cmp);
        let mid = d.len() / 2;
        let med = if d.len() % 2 == 0 {
            (d[mid - 1] + d[mid]) / 2.0
        } else {
            d[mid]
        };
        1.0 / med
    } else {
        100.0
    };
    let bq = dsp::butter_low(2, cfg.optical_cutoff.min(0.45 * vfps) / (vfps / 2.0));
    let strong = &diag.strong;
    let mut supported = vec![false; tm.len()];
    for &(a, b) in &noisy {
        let midx: Vec<usize> = (0..tv.len())
            .filter(|&i| tv[i] >= a - 0.3 && tv[i] <= b + 0.3)
            .collect();
        if midx.len() < 30 {
            continue;
        }
        let seg_t: Vec<f64> = midx.iter().map(|&i| tv[i]).collect();
        let mut seg_o: Vec<[f64; 3]> = midx.iter().map(|&i| patch_deg[i]).collect();
        let seg_q: Vec<f64> = midx.iter().map(|&i| opt_n.quality[i]).collect();
        let frac_bad = seg_q.iter().filter(|&&q| q < 0.3).count() as f64 / seg_q.len() as f64;
        if frac_bad > 0.3 {
            log(&format!(
                "   optical patch: {a:.1}-{b:.1}s rejected ({:.0}% low-quality flow)",
                frac_bad * 100.0
            ));
            continue;
        }
        let bad: Vec<bool> = seg_q.iter().map(|&q| q < 0.3).collect();
        if bad.iter().any(|&x| x) && !bad.iter().all(|&x| x) {
            let gt: Vec<f64> = seg_t
                .iter()
                .zip(&bad)
                .filter(|(_, &b)| !b)
                .map(|(t, _)| *t)
                .collect();
            for k in 0..3 {
                let gv: Vec<f64> = seg_o
                    .iter()
                    .zip(&bad)
                    .filter(|(_, &b)| !b)
                    .map(|(o, _)| o[k])
                    .collect();
                let bt: Vec<f64> = seg_t
                    .iter()
                    .zip(&bad)
                    .filter(|(_, &b)| b)
                    .map(|(t, _)| *t)
                    .collect();
                let fill = dsp::interp(&bt, &gt, &gv);
                let mut fi = 0;
                for (i, &isbad) in bad.iter().enumerate() {
                    if isbad {
                        seg_o[i][k] = fill[fi];
                        fi += 1;
                    }
                }
            }
        }
        seg_o = dsp::filtfilt3(&bq, &seg_o);
        let gm: Vec<usize> = (0..tm.len())
            .filter(|&i| tm[i] >= a && tm[i] <= b)
            .collect();
        let tq: Vec<f64> = gm.iter().map(|&i| tm[i]).collect();
        let video_1k: Vec<[f64; 3]> = {
            let per: Vec<Vec<f64>> = (0..3)
                .map(|k| dsp::interp(&tq, &seg_t, &seg_o.iter().map(|r| r[k]).collect::<Vec<_>>()))
                .collect();
            (0..tq.len())
                .map(|i| [per[0][i], per[1][i], per[2][i]])
                .collect()
        };
        // handback rate estimate (o4fix.py:509-532): in monster bursts
        // (30-180 Hz band-RMS of 300+ deg/s) the phantom leaks below 8 Hz
        // and inflates the gyro LP magnitude past the handback ramp, so a
        // gyro-only estimate hands orientation back to the very gyro being
        // repaired. Optical never invents motion, so above the
        // gyro_trust_noise ramp - judged on the segment's PEAK noise, since
        // the LF leak persists where the instantaneous RMS momentarily dips -
        // the estimate switches to min(gyro, optical): handback then engages
        // only when both sources agree the motion is genuinely fast.
        let vmag: Vec<f64> = video_1k
            .iter()
            .map(|r| (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt())
            .collect();
        let rate_opt = dsp::uniform_filter1d(&vmag, ((0.1 * fs) as usize).max(3));
        let (lo_n, hi_n) = cfg.gyro_trust_noise;
        let seg_noise = gm.iter().map(|&i| diag.noise[i]).fold(f64::MIN, f64::max);
        let trust = (1.0 - (seg_noise - lo_n) / (hi_n - lo_n).max(1e-6)).clamp(0.0, 1.0);
        let wf0: Vec<f64> = (0..gm.len())
            .map(|i| {
                let wf_gyro = ((rate_mag[gm[i]] - lo_r) / (hi_r - lo_r).max(1e-6)).clamp(0.0, 1.0);
                let r_min = rate_mag[gm[i]].min(rate_opt[i]);
                let wf_min = ((r_min - lo_r) / (hi_r - lo_r).max(1e-6)).clamp(0.0, 1.0);
                trust * wf_gyro + (1.0 - trust) * wf_min
            })
            .collect();
        let w_fast = dsp::uniform_filter1d(&wf0, ((0.15 * fs) as usize).max(3));
        let burst: Vec<[f64; 3]> = if cfg.anchor_mode {
            // optical = LF drift anchor on band-limited gyro (o4fix.py:513-525)
            let g: Vec<[f64; 3]> = gm.iter().map(|&i| strong[i]).collect();
            let mut corr: Vec<[f64; 3]> = (0..g.len())
                .map(|i| core::array::from_fn(|k| video_1k[i][k] - g[i][k]))
                .collect();
            let ba = dsp::butter_low(2, cfg.anchor_cutoff / (fs / 2.0));
            let nseg = corr.len();
            let taps = ba.b.len().max(ba.a.len());
            if nseg > 3 * taps * 10 {
                let padlen = (nseg - 1).min((2.0 * fs) as usize);
                let cols: Vec<Vec<f64>> = (0..3)
                    .map(|k| {
                        dsp::filtfilt_padlen(
                            &ba,
                            &corr.iter().map(|r| r[k]).collect::<Vec<_>>(),
                            padlen,
                        )
                    })
                    .collect();
                corr = (0..nseg)
                    .map(|i| [cols[0][i], cols[1][i], cols[2][i]])
                    .collect();
            }
            (0..g.len())
                .map(|i| core::array::from_fn(|k| g[i][k] + (1.0 - w_fast[i]) * corr[i][k]))
                .collect()
        } else {
            (0..gm.len())
                .map(|i| {
                    core::array::from_fn(|k| {
                        let wf = w_fast[i];
                        (1.0 - wf) * video_1k[i][k] + wf * medium[gm[i]][k]
                    })
                })
                .collect()
        };
        // steep ramp + partner blend (o4fix.py:528-534)
        for (i, &g) in gm.iter().enumerate() {
            // Rate timestamps sit between frames. O4 telemetry can extend
            // one final frame plus half a frame beyond the last frame pair;
            // also account for the fitted clock shift. Longer decoder gaps
            // must not be extrapolated across the rest of a severe burst.
            let edge_slack = 1.5 / vfps + al.shift.abs();
            supported[g] =
                tm[g] >= seg_t[0] - edge_slack && tm[g] <= seg_t[seg_t.len() - 1] + edge_slack;
            let w = (alpha_opt[g] / 0.35).clamp(0.0, 1.0);
            for k in 0..3 {
                let partner = if cfg.optical_noise.is_some() {
                    out[g][k]
                } else {
                    light[g][k]
                };
                out[g][k] = (1.0 - w) * partner + w * burst[i][k];
            }
        }
    }
    Ok(OpticalPatch {
        rates: out
            .iter()
            .map(|r| core::array::from_fn(|k| r[k] * D2R))
            .collect(),
        supported,
    })
}

#[derive(Clone, Copy, Debug)]
pub struct BurstStat {
    pub start: f64,
    pub end: f64,
    pub drift_deg: f64,
    pub rebased: bool,
}

/// Write q_out[k0..k1] = O(t) (x) q_raw with the offset O decaying toward
/// identity at decay_rate deg/s (0 = carry forever). Samples reached after
/// the decay completes are left untouched (bit-identical raw). Returns the
/// offset remaining at index k1. Ports o4fix.py's `_apply_offset_span`.
fn apply_offset_span(
    q_out: &mut [[f64; 4]],
    q_raw: &[[f64; 4]],
    t: &[f64],
    off: [f64; 4],
    k0: usize,
    k1: usize,
    decay_rate: f64,
) -> [f64; 4] {
    use crate::quat::{qexp, qlog, qmul, qnorm};
    let v = qlog(off);
    let ang = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if ang < 1e-12 || k1 <= k0 {
        return if ang < 1e-12 {
            [1.0, 0.0, 0.0, 0.0]
        } else {
            off
        };
    }
    let axis = [v[0] / ang, v[1] / ang, v[2] / ang];
    let rate = decay_rate.to_radians();
    let mut rem = ang;
    for k in k0..k1 {
        let a = if decay_rate > 0.0 {
            (ang - rate * (t[k] - t[k0])).max(0.0)
        } else {
            ang
        };
        if a > 0.0 {
            let o = qexp([a * axis[0], a * axis[1], a * axis[2]]);
            q_out[k] = qmul(o, q_raw[k]);
        }
    }
    if decay_rate > 0.0 {
        rem = (ang - rate * (t[k1.min(t.len() - 1)] - t[k0])).max(0.0);
    }
    qnorm(qexp([rem * axis[0], rem * axis[1], rem * axis[2]]))
}

/// Replace q_raw inside each interval with integrated omega_patch, pinned to
/// raw at both edges (o4fix.py:137-178, `_apply_offset_span` +
/// `splice_orientation`). Accumulated drift is spread across the interval as
/// a smoothstep-over-time rotation-vector correction; ramp_s slerp
/// cross-fades at the edges. Samples outside intervals are returned
/// bit-identical (clean-zone guarantee — feeds mp4::patch_video's
/// unchanged-row original-bytes path). (Measured 2026-07-21: rate-weighted
/// spreading of the correction is WORSE on both test clips; keep uniform.)
///
/// Drift rebase (spec 2026-08-13): bursts whose implied bridge rate
/// 1.5*drift/duration exceeds rebase_above (deg/s; 0 disables) skip the
/// smoothstep endpoint correction entirely — the path lands on the optical
/// endpoint and the drift is carried forward as a constant orientation
/// offset on the following samples (invisible to stabilization, which only
/// sees rate of orientation error). The offset decays toward identity at
/// decay_rate deg/s (0 = carry forever) and composes across bursts. Samples
/// under an identity offset keep their original bit patterns.
pub fn splice_orientation(
    t: &[f64],
    q_raw: &[[f64; 4]],
    omega_patch: &[[f64; 3]],
    intervals: &[(f64, f64)],
    ramp_s: f64,
    rebase_above: f64,
    decay_rate: f64,
) -> (Vec<[f64; 4]>, Vec<BurstStat>) {
    use crate::quat::{qconj, qexp, qlog, qmul, qnorm, slerp, smoothstep};
    let mut q_out = q_raw.to_vec();
    let mut stats = Vec::new();
    let mut off = [1.0, 0.0, 0.0, 0.0];
    let mut prev_end = 0usize;
    for &(a, b) in intervals {
        let i0 = crate::dsp::searchsorted_left(t, a);
        let i1 = crate::dsp::searchsorted_right(t, b)
            .saturating_sub(1)
            .min(t.len() - 1);
        if i1 < i0 + 8 {
            continue;
        } // python: if i1 - i0 < 8

        off = apply_offset_span(&mut q_out, q_raw, t, off, prev_end, i0, decay_rate);
        let off_pre = off;
        let n = i1 - i0;

        // sequential integration: qs[k+1] = qs[k] * qexp(omega*dt), seeded
        // from the pre-burst offset frame
        let mut qs: Vec<[f64; 4]> = Vec::with_capacity(n + 1);
        qs.push(qmul(off_pre, q_raw[i0]));
        for k in 0..n {
            let dt = t[i0 + k + 1] - t[i0 + k];
            let o = omega_patch[i0 + k];
            qs.push(qmul(qs[k], qexp([o[0] * dt, o[1] * dt, o[2] * dt])));
        }
        for q in qs.iter_mut() {
            *q = qnorm(*q);
        } // python normalizes ONCE, after the loop

        // endpoint drift vs the raw endpoint (in the same offset frame)
        let e = qlog(qmul(qconj(qs[n]), qmul(off_pre, q_raw[i1])));
        let drift_deg = (e[0] * e[0] + e[1] * e[1] + e[2] * e[2])
            .sqrt()
            .to_degrees();
        let dur = (t[i1] - t[i0]).max(1e-9);
        let rebased = rebase_above > 0.0 && 1.5 * drift_deg / dur > rebase_above;
        if rebased {
            // land on the optical endpoint; carry the drift forward as a
            // constant offset instead of bridging it inside the burst
            off = qnorm(qmul(qs[n], qconj(q_raw[i1])));
        } else {
            for k in 0..=n {
                let s = smoothstep((t[i0 + k] - t[i0]) / dur);
                qs[k] = qmul(qs[k], qexp([s * e[0], s * e[1], s * e[2]]));
                // NOTE: python does NOT renormalize after this multiply — neither do we
            }
        }

        // base path the edge ramps blend toward: pre-offset frame at entry,
        // post-offset frame at exit (identical unless rebased); smoothstep
        // interpolation keeps it continuous and flat at both edges
        let d_off = qlog(qmul(off, qconj(off_pre)));
        let off_pre_ang = {
            let v = qlog(off_pre);
            (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
        };
        for k in 0..=n {
            let tt = t[i0 + k];
            // Fixed edge offsets (gyro-trace-v1): in a rebased burst the carried
            // offset changes only in the fully replaced interior, so the raw
            // path blended in at the entry/exit ramps does not move. Overlapping
            // ramps fall back to the whole-burst interpolation.
            let s = if rebased && dur > 2.0 * ramp_s {
                smoothstep((tt - t[i0] - ramp_s) / (dur - 2.0 * ramp_s))
            } else {
                smoothstep((tt - t[i0]) / dur)
            };
            let base = if !rebased && off_pre_ang < 1e-12 {
                q_raw[i0 + k] // exact original path (bit-identical branch)
            } else {
                let o = qmul(qexp([s * d_off[0], s * d_off[1], s * d_off[2]]), off_pre);
                qmul(o, q_raw[i0 + k])
            };
            let r = smoothstep((tt - t[i0]) / ramp_s).min(smoothstep((t[i1] - tt) / ramp_s));
            q_out[i0 + k] = slerp(base, qs[k], r);
        }
        stats.push(BurstStat {
            start: a,
            end: b,
            drift_deg,
            rebased,
        });
        prev_end = i1 + 1;
    }
    apply_offset_span(&mut q_out, q_raw, t, off, prev_end, t.len(), decay_rate);
    (q_out, stats)
}

#[cfg(test)]
mod splice_rebase_tests {
    use super::*;
    use crate::quat::{qexp, quats_to_rates};
    #[test]
    fn rejected_optical_segment_cannot_reach_rebase() {
        let (t, _, rates) = make_case(60.0, [0.0, 0.0, 1.0]);
        let mut patch = OpticalPatch {
            supported: vec![true; rates.len()],
            rates,
        };
        assert!(patch.require_coverage(&t, &[(2.0, 3.0)]).is_ok());
        // Even one unmeasured integrated rate makes the burst unsupported.
        patch.supported[2500] = false;
        assert!(matches!(
            patch.require_coverage(&t, &[(2.0, 3.0)]),
            Err(O4Error::OpticalCoverage { .. })
        ));
        // Rejected flow elsewhere does not prevent a supported repair.
        assert!(patch.require_coverage(&t, &[(4.0, 5.0)]).is_ok());
    }

    /// Mirrors python/tests/test_splice_rebase.py::make_case: 1 kHz
    /// still-camera clip, one 1 s burst [2, 3] whose raw endpoint is
    /// drift_deg away from where zero patch rates integrate to.
    fn make_case(drift_deg: f64, axis: [f64; 3]) -> (Vec<f64>, Vec<[f64; 4]>, Vec<[f64; 3]>) {
        let fs = 1000.0;
        let n = (10.0 * fs) as usize;
        let t: Vec<f64> = (0..n).map(|i| i as f64 / fs).collect();
        let norm = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
        let ax = [axis[0] / norm, axis[1] / norm, axis[2] / norm];
        let q_raw: Vec<[f64; 4]> = t
            .iter()
            .map(|&tt| {
                let frac = ((tt - 2.0) / 1.0).clamp(0.0, 1.0);
                let rad = drift_deg.to_radians() * frac;
                qexp([rad * ax[0], rad * ax[1], rad * ax[2]])
            })
            .collect();
        let omega = vec![[0.0, 0.0, 0.0]; n - 1]; // optical says: camera did not move
        (t, q_raw, omega)
    }

    fn rates_deg(t: &[f64], q: &[[f64; 4]]) -> (Vec<f64>, Vec<[f64; 3]>) {
        let (tm, om) = quats_to_rates(t, q);
        (
            tm,
            om.iter()
                .map(|r| core::array::from_fn(|k| r[k].to_degrees()))
                .collect(),
        )
    }

    fn max_norm_in_window(tm: &[f64], om: &[[f64; 3]], lo: f64, hi: f64) -> f64 {
        tm.iter()
            .zip(om)
            .filter(|(&t, _)| t > lo && t < hi)
            .map(|(_, r)| (r[0] * r[0] + r[1] * r[1] + r[2] * r[2]).sqrt())
            .fold(f64::MIN, f64::max)
    }

    #[test]
    fn rebase_off_is_previous_behavior() {
        let (t, q_raw, omega) = make_case(60.0, [0.0, 0.0, 1.0]);
        let (q_out, stats) = splice_orientation(&t, &q_raw, &omega, &[(2.0, 3.0)], 0.3, 0.0, 1.5);
        assert!(!stats[0].rebased);
        let i1 = crate::dsp::searchsorted_right(&t, 3.0).saturating_sub(1);
        let dot: f64 = (0..4)
            .map(|k| q_out[i1][k] * q_raw[i1][k])
            .sum::<f64>()
            .abs();
        assert!((2.0 * dot.min(1.0).acos()).to_degrees() < 1e-7);
        for i in 0..t.len() {
            if t[i] < 2.0 || t[i] > 3.0 {
                assert_eq!(
                    q_out[i], q_raw[i],
                    "outside interval must be bit-identical at i={i}"
                );
            }
        }
        let (tm, om) = rates_deg(&t, &q_out);
        assert!(max_norm_in_window(&tm, &om, 2.2, 2.8) > 30.0);
    }

    #[test]
    fn rebase_kills_in_burst_fake_rate() {
        // implied 1.5*60/1 = 90 deg/s
        let (t, q_raw, omega) = make_case(60.0, [0.0, 0.0, 1.0]);
        let (q_out, stats) = splice_orientation(&t, &q_raw, &omega, &[(2.0, 3.0)], 0.3, 30.0, 1.5);
        assert!(stats[0].rebased);
        assert!((stats[0].drift_deg - 60.0).abs() < 1.0);
        let (tm, om) = rates_deg(&t, &q_out);
        assert!(max_norm_in_window(&tm, &om, 2.35, 2.65) < 2.0);
        assert!(max_norm_in_window(&tm, &om, 3.4, 40.0) < 1.5 + 0.1);
    }

    #[test]
    fn offset_decays_to_bit_identical_raw() {
        // implied 1.5*6/1 = 9 deg/s
        let (t, q_raw, omega) = make_case(6.0, [0.0, 0.0, 1.0]);
        let (q_out, stats) = splice_orientation(&t, &q_raw, &omega, &[(2.0, 3.0)], 0.3, 5.0, 1.5);
        assert!(stats[0].rebased);
        // 6 deg at 1.5 deg/s -> gone 4 s after the burst; far samples bit-exact
        let far = 3.0 + 6.0 / 1.5 + 0.5;
        for i in 0..t.len() {
            if t[i] > far {
                assert_eq!(
                    q_out[i], q_raw[i],
                    "far sample must be bit-identical at i={i}"
                );
            }
        }
    }

    #[test]
    fn offsets_compose_across_bursts() {
        let fs = 1000.0;
        let n = (20.0 * fs) as usize;
        let t: Vec<f64> = (0..n).map(|i| i as f64 / fs).collect();
        // two bursts, each drifting raw 40 deg further about z
        let q_raw: Vec<[f64; 4]> = t
            .iter()
            .map(|&tt| {
                let frac = (tt - 2.0).clamp(0.0, 1.0) + (tt - 8.0).clamp(0.0, 1.0);
                let rad = 40.0f64.to_radians() * frac;
                qexp([0.0, 0.0, rad])
            })
            .collect();
        let omega = vec![[0.0, 0.0, 0.0]; n - 1];
        let (q_out, stats) = splice_orientation(
            &t,
            &q_raw,
            &omega,
            &[(2.0, 3.0), (8.0, 9.0)],
            0.3,
            30.0,
            0.0, // carry forever
        );
        assert_eq!(
            stats.iter().map(|s| s.rebased).collect::<Vec<_>>(),
            vec![true, true]
        );
        // with zero decay the offset after burst 2 is the composed 80 deg:
        // q_out stays at identity attitude (optical said "no motion") forever
        let (tm, om) = rates_deg(&t, &q_out);
        assert!(max_norm_in_window(&tm, &om, 9.4, f64::MAX) < 0.5);
        let end_err = crate::quat::qlog(*q_out.last().unwrap());
        let end_err_deg =
            (end_err[0] * end_err[0] + end_err[1] * end_err[1] + end_err[2] * end_err[2])
                .sqrt()
                .to_degrees();
        assert!(end_err_deg < 1.0);
    }

    #[test]
    fn identity_offset_spans_bit_identical() {
        let (t, q_raw, omega) = make_case(60.0, [0.0, 0.0, 1.0]);
        let (q_out, _) = splice_orientation(&t, &q_raw, &omega, &[(2.0, 3.0)], 0.3, 1000.0, 1.5); // gate never trips
        for i in 0..t.len() {
            if t[i] < 2.0 || t[i] > 3.0 {
                assert_eq!(
                    q_out[i], q_raw[i],
                    "outside interval must be bit-identical at i={i}"
                );
            }
        }
    }

    #[test]
    fn stationary_camera_rebased_burst_has_no_edge_motion() {
        // camera still; raw drift of 60 deg confined to the burst middle
        let t: Vec<f64> = (0..5001).map(|i| i as f64 / 1000.).collect();
        let q: Vec<[f64; 4]> = t
            .iter()
            .map(|&s| {
                qexp([
                    0.,
                    0.,
                    60_f64.to_radians() * ((s - 2.3) / 0.4).clamp(0., 1.),
                ])
            })
            .collect();
        let rates = vec![[0.; 3]; t.len() - 1];
        let (out, st) = splice_orientation(&t, &q, &rates, &[(2., 3.)], 0.19, 30., 0.);
        assert!(st[0].rebased);
        let (_, r) = quats_to_rates(&t, &out);
        let peak = r.iter().map(|v| v[2].abs().to_degrees()).fold(0., f64::max);
        assert!(peak < 1e-8, "false rate {peak} deg/s");
    }

    #[test]
    fn non_rebased_burst_path_unchanged_by_edge_fix() {
        let t: Vec<f64> = (0..5001).map(|i| i as f64 / 1000.).collect();
        let q: Vec<[f64; 4]> = t
            .iter()
            .map(|&s| qexp([3_f64.to_radians() * ((s - 2.3) / 0.4).clamp(0., 1.), 0., 0.]))
            .collect();
        let rates = vec![[0.; 3]; t.len() - 1];
        let (out, st) = splice_orientation(&t, &q, &rates, &[(2., 3.)], 0.19, 30., 0.);
        assert!(!st[0].rebased);
        // pre-burst samples keep original bits
        assert!(out[..1900].iter().zip(&q[..1900]).all(|(a, b)| a == b));
    }

    #[test]
    fn short_rebased_burst_falls_back_to_whole_burst_interpolation() {
        // burst (2.0, 2.3) is 0.3 s; ramp 0.19 s => dur (0.3) <= 2*ramp
        // (0.38), so the edge-offset fix's `dur > 2.0 * ramp_s` guard must
        // be false and splice_orientation falls back to the pre-fix
        // `smoothstep((tt - t[i0]) / dur)` whole-burst interpolation
        // (same formula used for non-rebased bursts). That fallback branch
        // is deliberately NOT fixed by this change (short/overlapping
        // ramps), so this test only checks it stays numerically sane
        // rather than asserting near-zero edge rate.
        let t: Vec<f64> = (0..5001).map(|i| i as f64 / 1000.).collect();
        let q: Vec<[f64; 4]> = t
            .iter()
            .map(|&s| {
                qexp([
                    0.,
                    0.,
                    60_f64.to_radians() * ((s - 2.0) / 0.1).clamp(0., 1.),
                ])
            })
            .collect();
        let rates = vec![[0.; 3]; t.len() - 1];
        let (out, st) = splice_orientation(&t, &q, &rates, &[(2.0, 2.3)], 0.19, 30., 0.);
        assert!(
            st[0].rebased,
            "burst must rebase to exercise the fallback branch"
        );
        for qq in &out {
            assert!(qq.iter().all(|v| v.is_finite()), "non-finite output quat");
            let norm = (qq[0] * qq[0] + qq[1] * qq[1] + qq[2] * qq[2] + qq[3] * qq[3]).sqrt();
            assert!(
                (norm - 1.0).abs() < 1e-6,
                "output quat not unit-norm: {norm}"
            );
        }
        let (_, r) = quats_to_rates(&t, &out);
        let peak = r.iter().map(|v| v[2].abs().to_degrees()).fold(0., f64::max);
        // Bound justification: smoothstep's max slope is 1.5, so the
        // offset-blend contribution to the rate is bounded on the order of
        // (1.5 / dur) * drift_deg = (1.5 / 0.3) * 60 = 300 deg/s; 5000 deg/s
        // leaves an order of magnitude of headroom for quaternion
        // cross-terms without masking an actual blow-up (NaN/inf from a
        // near-zero `dur`, which is guarded upstream but not by this
        // formula itself).
        assert!(peak < 5000.0, "fallback branch rate blew up: {peak} deg/s");
    }
}
