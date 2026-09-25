//! Instrumented production optical_patch copy. Formula unchanged; records source weights.
use o4core::patch::{calibration_intervals, OptPhase, OpticalPatch};
use o4core::{
    config::Config,
    detect::{find_intervals, CleanDiag},
    dsp,
    error::O4Error,
    optical,
    telemetry::Meta,
};
use std::{path::Path, sync::atomic::AtomicBool};
const R2D: f64 = 180. / std::f64::consts::PI;
const D2R: f64 = std::f64::consts::PI / 180.;
// mirrors o4fix.py's optical_patch(video, tm, cleaned_rad, diag, fs, args, meta); log/on_interval/cancel are Rust-only additions (no print()/cancellation in Python)
#[allow(clippy::too_many_arguments)]
pub fn optical_patch(
    trace_path: &Path,
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
    let mut trace = Vec::new();
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
            trace.push(serde_json::json!({"t":tm[g],"index":g,"segment":[a,b],"segment_peak_noise":seg_noise,"trust":trust,"noise":diag.noise[g],"optical_weight":w,"handback":w_fast[i],"optical_deg_s":video_1k[i],"medium_deg_s":medium[g],"final_deg_s":out[g]}));
        }
    }
    std::fs::write(
        trace_path,
        serde_json::to_vec(
            &serde_json::json!({"rows":trace,"rates_deg_s":out,"supported":supported}),
        )
        .expect("serialize finite trace"),
    )
    .expect("write trace");
    Ok(OpticalPatch {
        rates: out
            .iter()
            .map(|r| core::array::from_fn(|k| r[k] * D2R))
            .collect(),
        supported,
    })
}
