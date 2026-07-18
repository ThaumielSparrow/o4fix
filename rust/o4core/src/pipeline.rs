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
pub enum Stage { Extract, Analyze, Optical, Splice, Write }

#[derive(Clone, Debug)]
pub struct Progress {
    pub stage: Stage,
    pub message: String,
}

#[derive(Debug)]
pub enum Outcome {
    Repaired { out: PathBuf, bursts: Vec<BurstStat> },
    /// No severe bursts found — telemetry healthy, no output written.
    Healthy,
}

/// np.median: sort, average the two middle elements when n is even.
pub fn median(x: &[f64]) -> f64 {
    let mut v = x.to_vec();
    v.sort_by(f64::total_cmp);
    let m = v.len() / 2;
    if v.len() % 2 == 0 { (v[m - 1] + v[m]) / 2.0 } else { v[m] }
}

/// Sample rate, identical to o4fix.py:590 `1/np.median(np.diff(t))`.
pub fn fs(t: &[f64]) -> f64 {
    1.0 / median(&t.windows(2).map(|w| w[1] - w[0]).collect::<Vec<_>>())
}

pub fn process(video: &Path, out: Option<&Path>, cfg: &Config,
               on_progress: &(dyn Fn(Progress) + Sync), cancel: &AtomicBool)
    -> Result<Outcome, O4Error>
{
    let say = |stage: Stage, message: String|
        on_progress(Progress { stage, message });
    let check = || -> Result<(), O4Error> {
        if cancel.load(Ordering::Relaxed) { Err(O4Error::Cancelled) } else { Ok(()) }
    };

    check()?;
    let tel = telemetry::extract_quats(video)?;
    let fs = fs(&tel.t);
    say(Stage::Extract, format!(
        "   {} {}, {} quat samples @ {:.0} Hz, {:.1} s",
        tel.meta.camera, tel.meta.model, tel.q.len(), fs,
        tel.t[tel.t.len() - 1] - tel.t[0]));

    check()?;
    let (tm, omega) = quat::quats_to_rates(&tel.t, &tel.q);
    let (cleaned, diag) = detect::adaptive_clean(&omega, fs, cfg);
    let frac = diag.alpha.iter().filter(|&&a| a > 0.5).count() as f64
        / diag.alpha.len() as f64;
    say(Stage::Analyze, format!(
        "   spikes replaced: {:.1}% of samples, noise bursts cover: {:.1}% of flight",
        diag.spike_frac * 100.0, frac * 100.0));

    // ---- deliberate reorder: severe gate BEFORE optical ----
    let severe_mask: Vec<bool> = diag.noise.iter().map(|&n| n > cfg.severe).collect();
    let intervals = detect::find_intervals(&severe_mask, &tm,
                                           cfg.severe_pad, cfg.severe_merge, 0.2);
    if intervals.is_empty() {
        say(Stage::Analyze, format!(
            "   no severe bursts (> {:?} deg/s band-RMS) found - telemetry \
             looks healthy, nothing to repair", cfg.severe));
        return Ok(Outcome::Healthy);
    }
    let tot: f64 = intervals.iter().map(|(a, b)| b - a).sum();
    say(Stage::Analyze, format!(
        "   replacing orientation in {} severe bursts ({tot:.1} s)", intervals.len()));

    check()?;
    let log = |s: &str| say(Stage::Optical, s.to_string());
    let patched = patch::optical_patch(video, &tm, &cleaned, &diag, fs, cfg,
                                       &tel.meta, &log, cancel)?;

    check()?;
    let (q_out, bursts) = patch::splice_orientation(
        &tel.t, &tel.q, &patched, &intervals, cfg.ramp);
    for b in &bursts {
        say(Stage::Splice, format!(
            "     [{:7.2}, {:7.2}] optical drift over burst: {:5.2} deg",
            b.start, b.end, b.drift_deg));
    }

    check()?;
    let out_path: PathBuf = match out {
        Some(p) => p.to_path_buf(),
        None => {
            let stem = video.file_stem().unwrap_or_default().to_string_lossy();
            let ext = video.extension().map(|e| e.to_string_lossy()).unwrap_or_default();
            video.with_file_name(format!("{stem}_fixed.{ext}"))  // preserves .MP4 vs .mp4
        }
    };
    let wlog = |s: &str| say(Stage::Write, s.to_string());
    match mp4::inject_and_check(video, &out_path, &q_out, &wlog) {
        Ok(true) => {
            say(Stage::Write, format!(
                "   wrote {} - load it in Gyroflow like a stock recording",
                out_path.display()));
            Ok(Outcome::Repaired { out: out_path, bursts })
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
        assert_eq!(median(&[1.0, 2.0, 3.0, 4.0]), 2.5);   // even: average middle two
        assert_eq!(median(&[1.0, 3.0, 2.0]), 2.0);        // odd: middle after sort
        assert_eq!(median(&[5.0]), 5.0);
        assert!((fs(&[0.0, 0.001, 0.002, 0.0035]) - 1000.0).abs() < 1e-9); // median dt = 1 ms
    }
}
