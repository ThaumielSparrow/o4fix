//! Orchestration: ports o4fix.process/process_mp4 (o4fix.py:586-662) with one
//! DELIBERATE REORDER (spec: pipeline deviation): severe intervals are
//! computed FIRST, so healthy clips return Outcome::Healthy before any
//! OpenCV work. For repairable clips the outputs are identical to Python;
//! only console-message order differs (burst count prints before optical
//! progress).
use crate::config::Config;
use crate::error::O4Error;
use crate::patch::BurstStat;
use crate::{detect, mp4, patch, quat, telemetry};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stage {
    Extract,
    Analyze,
    Optical,
    Splice,
    Write,
}

#[derive(Clone, Debug)]
pub struct Progress {
    pub stage: Stage,
    pub message: String,
    /// Overall job fraction [0,1]. Events with an empty `message` are
    /// pct-only ticks: CLI skips them, GUI drives the bar with them.
    pub pct: f64,
}

/// Optical-stage share of overall progress: calib intervals 0.10-0.30,
/// noisy intervals 0.30-0.85 (optical dominates wall time).
pub fn optical_pct(phase: crate::patch::OptPhase, done: usize, total: usize) -> f64 {
    let f = done as f64 / total.max(1) as f64;
    match phase {
        crate::patch::OptPhase::Calib => 0.10 + 0.20 * f,
        crate::patch::OptPhase::Noisy => 0.30 + 0.55 * f,
    }
}

#[derive(Debug)]
pub enum Outcome {
    Repaired {
        out: PathBuf,
        bursts: Vec<BurstStat>,
    },
    /// No severe bursts found — telemetry healthy, no output written.
    Healthy,
}

/// np.median: sort, average the two middle elements when n is even.
pub fn median(x: &[f64]) -> f64 {
    let mut v = x.to_vec();
    v.sort_by(f64::total_cmp);
    let m = v.len() / 2;
    if v.len() % 2 == 0 {
        (v[m - 1] + v[m]) / 2.0
    } else {
        v[m]
    }
}

/// Sample rate, identical to o4fix.py:590 `1/np.median(np.diff(t))`.
pub fn fs(t: &[f64]) -> f64 {
    1.0 / median(&t.windows(2).map(|w| w[1] - w[0]).collect::<Vec<_>>())
}

/// Run the full O4P repair pipeline on `video`.
///
/// - `video`: input MP4 with embedded O4P telemetry.
/// - `out`: output path; `None` writes `VIDEO_fixed.MP4` next to the input
///   (extension case preserved; see the default-naming branch below).
/// - `cfg`: tuning parameters (detection thresholds, optical/patch knobs).
/// - `on_progress`: called with a `Progress` message as each stage runs.
/// - `cancel`: checked between stages and inside the optical-flow work
///   (`optical::video_rates` polls it per interval and per frame); set it
///   to abort early.
///
/// Returns `Outcome::Healthy` when no severe bursts are found — telemetry
/// looks clean and NO output file is written. Otherwise returns
/// `Outcome::Repaired` with the output path and per-burst optical-drift
/// stats.
pub fn process(
    video: &Path,
    out: Option<&Path>,
    cfg: &Config,
    on_progress: &(dyn Fn(Progress) + Sync),
    cancel: &AtomicBool,
) -> Result<Outcome, O4Error> {
    let say = |stage: Stage, pct: f64, message: String| {
        on_progress(Progress {
            stage,
            message,
            pct,
        })
    };
    let check = || -> Result<(), O4Error> {
        if cancel.load(Ordering::Relaxed) {
            Err(O4Error::Cancelled)
        } else {
            Ok(())
        }
    };

    check()?;
    let tel = telemetry::extract_quats(video)?;
    // rate-domain filtering needs > filtfilt padlen (15) samples; 100 = 0.1 s
    // at 1 kHz, far below any real clip - turns degenerate-input panics
    // (median/fs, filtfilt, find_intervals) into a clean error
    if tel.t.len() < 100 {
        return Err(O4Error::NoTelemetry(format!(
            "only {} telemetry samples - clip too short to analyze",
            tel.t.len()
        )));
    }
    let fs = fs(&tel.t);
    say(
        Stage::Extract,
        0.05,
        format!(
            "   {} {}, {} quat samples @ {:.0} Hz, {:.1} s",
            tel.meta.camera,
            tel.meta.model,
            tel.q.len(),
            fs,
            tel.t[tel.t.len() - 1] - tel.t[0]
        ),
    );

    check()?;
    let (tm, omega) = quat::quats_to_rates(&tel.t, &tel.q);
    let (cleaned, diag) = detect::adaptive_clean(&omega, fs, cfg);
    let frac = diag.alpha.iter().filter(|&&a| a > 0.5).count() as f64 / diag.alpha.len() as f64;
    say(
        Stage::Analyze,
        0.08,
        format!(
            "   spikes replaced: {:.1}% of samples, noise bursts cover: {:.1}% of flight",
            diag.spike_frac * 100.0,
            frac * 100.0
        ),
    );

    // ---- deliberate reorder: severe gate BEFORE optical ----
    let severe_mask: Vec<bool> = diag.noise.iter().map(|&n| n > cfg.severe).collect();
    let intervals =
        detect::find_intervals(&severe_mask, &tm, cfg.severe_pad, cfg.severe_merge, 0.2);
    if intervals.is_empty() {
        say(
            Stage::Analyze,
            1.0,
            format!(
                "   no severe bursts (> {:?} deg/s band-RMS) found - telemetry \
             looks healthy, nothing to repair",
                cfg.severe
            ),
        );
        return Ok(Outcome::Healthy);
    }
    let tot: f64 = intervals.iter().map(|(a, b)| b - a).sum();
    say(
        Stage::Analyze,
        0.10,
        format!(
            "   replacing orientation in {} severe bursts ({tot:.1} s)",
            intervals.len()
        ),
    );

    check()?;
    let opt_pct = std::sync::atomic::AtomicU64::new(0.10f64.to_bits());
    let log = |s: &str| {
        say(
            Stage::Optical,
            f64::from_bits(opt_pct.load(Ordering::Relaxed)),
            s.to_string(),
        )
    };
    let on_interval = |ph, d, n| {
        let p = optical_pct(ph, d, n);
        opt_pct.store(p.to_bits(), Ordering::Relaxed);
        say(Stage::Optical, p, String::new()); // pct-only tick
    };
    let patched = patch::optical_patch(
        video,
        &tm,
        &cleaned,
        &diag,
        fs,
        cfg,
        &tel.meta,
        &log,
        &on_interval,
        cancel,
    )?;

    check()?;
    let (q_out, bursts) = patch::splice_orientation(&tel.t, &tel.q, &patched, &intervals, cfg.ramp);
    for b in &bursts {
        say(
            Stage::Splice,
            0.87,
            format!(
                "     [{:7.2}, {:7.2}] optical drift over burst: {:5.2} deg",
                b.start, b.end, b.drift_deg
            ),
        );
    }

    check()?;
    let out_path: PathBuf = match out {
        Some(p) => p.to_path_buf(),
        None => {
            let stem = video.file_stem().unwrap_or_default().to_string_lossy();
            let ext = video
                .extension()
                .map(|e| e.to_string_lossy())
                .unwrap_or_default();
            let name = if ext.is_empty() {
                format!("{stem}_fixed")
            } else {
                format!("{stem}_fixed.{ext}")
            }; // preserves .MP4 vs .mp4
            video.with_file_name(name)
        }
    };
    let wlog = |s: &str| say(Stage::Write, 0.92, s.to_string());
    match mp4::inject_and_check(video, &out_path, &q_out, &wlog) {
        Ok(true) => {
            say(
                Stage::Write,
                1.0,
                format!(
                    "   wrote {} - load it in Gyroflow like a stock recording",
                    out_path.display()
                ),
            );
            Ok(Outcome::Repaired {
                out: out_path,
                bursts,
            })
        }
        Ok(false) => {
            let _ = std::fs::remove_file(&out_path);
            Err(O4Error::VerifyFailed)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&out_path); // partial copy possible
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_matches_numpy() {
        assert_eq!(median(&[1.0, 2.0, 3.0, 4.0]), 2.5); // even: average middle two
        assert_eq!(median(&[1.0, 3.0, 2.0]), 2.0); // odd: middle after sort
        assert_eq!(median(&[5.0]), 5.0);
        assert!((fs(&[0.0, 0.001, 0.002, 0.0035]) - 1000.0).abs() < 1e-9); // median dt = 1 ms
    }

    #[test]
    fn optical_pct_anchors() {
        use crate::patch::OptPhase::*;
        assert!((optical_pct(Calib, 0, 4) - 0.10).abs() < 1e-12);
        assert!((optical_pct(Calib, 4, 4) - 0.30).abs() < 1e-12);
        assert!((optical_pct(Noisy, 0, 23) - 0.30).abs() < 1e-12);
        assert!((optical_pct(Noisy, 23, 23) - 0.85).abs() < 1e-12);
        assert!(optical_pct(Noisy, 0, 0).is_finite()); // total=0 guarded by max(1)
        assert!(optical_pct(Calib, 1, 4) < optical_pct(Calib, 2, 4)); // monotone
    }
}
