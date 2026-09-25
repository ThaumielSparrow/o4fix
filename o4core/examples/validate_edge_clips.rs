//! Frozen validation of default M2 versus the visually accepted fixed-edge-offset candidate.
#[path = "support/edge_offset_patch.rs"]
mod edge;
use o4core::{config::Config, detect, mp4, patch, pipeline, quat, telemetry};
use serde_json::json;
use std::{path::Path, sync::atomic::AtomicBool};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 3 {
        return Err("usage: validate_edge_clips SOURCE NEW_OUTPUT_DIRECTORY".into());
    }
    let source = Path::new(&a[1]);
    let out = Path::new(&a[2]);
    std::fs::create_dir_all(out)?;
    let base_path = out.join("baseline.MP4");
    let edge_path = out.join("edgeoffset.MP4");
    for dest in [&base_path, &edge_path] {
        if dest.exists() {
            return Err("repair output already exists".into());
        }
        mp4::validate_output(source, dest)?;
    }
    let cfg = Config::default();
    cfg.validate()?;
    let tel = telemetry::extract_quats(source)?;
    if tel.t.len() < 100
        || tel.t.iter().any(|v| !v.is_finite())
        || tel.t.windows(2).any(|w| w[1] <= w[0])
        || tel.q.iter().flatten().any(|v| !v.is_finite())
    {
        return Err("invalid telemetry".into());
    }
    let fs = pipeline::fs(&tel.t);
    cfg.validate_sample_rate(fs)?;
    let (tm, raw) = quat::quats_to_rates(&tel.t, &tel.q);
    let (clean, diag) = detect::adaptive_clean(&raw, fs, &cfg);
    let intervals = detect::find_intervals(
        &diag
            .noise
            .iter()
            .map(|&v| v > cfg.severe)
            .collect::<Vec<_>>(),
        &tm,
        cfg.severe_pad,
        cfg.severe_merge,
        0.2,
    );
    let calib = patch::calibration_intervals(&tm, &clean, &diag);
    std::fs::write(
        out.join("detection.json"),
        serde_json::to_vec_pretty(
            &json!({"source":a[1],"fs":fs,"quaternion_count":tel.q.len(),"severe":intervals,"calibration":calib,"camera":tel.meta.camera,"model":tel.meta.model,"camera_matrix":tel.meta.camera_matrix,"distortion":tel.meta.distortion,"calib_w":tel.meta.calib_w,"calib_h":tel.meta.calib_h}),
        )?,
    )?;
    if intervals.is_empty() {
        return Err("no severe intervals".into());
    }
    let rates = patch::optical_patch(
        source,
        &tm,
        &clean,
        &diag,
        fs,
        &cfg,
        &tel.meta,
        &|s| println!("{s}"),
        &|p, d, n| println!("{p:?} {d}/{n}"),
        &AtomicBool::new(false),
    )?;
    rates.require_coverage(&tel.t, &intervals)?;
    let (base, bs) = patch::splice_orientation(
        &tel.t,
        &tel.q,
        &rates.rates,
        &intervals,
        0.3,
        cfg.drift_rebase_above,
        cfg.drift_decay_rate,
    );
    let (candidate, cs) = edge::splice_orientation(
        &tel.t,
        &tel.q,
        &rates.rates,
        &intervals,
        0.19,
        cfg.drift_rebase_above,
        cfg.drift_decay_rate,
    );
    assert_eq!(bs.len(), cs.len());
    assert!(bs
        .iter()
        .zip(&cs)
        .all(|(a, b)| a.drift_deg == b.drift_deg && a.rebased == b.rebased));
    let stats: Vec<_> = cs
        .iter()
        .map(|s| json!({"interval":[s.start,s.end],"drift_deg":s.drift_deg,"rebased":s.rebased}))
        .collect();
    std::fs::write(
        out.join("repair-metrics.json"),
        serde_json::to_vec_pretty(
            &json!({"source":a[1],"candidate":"fixed edge offsets, ramp .19, padding .2; unchanged M2 optics/calibration/trust/decay","same_drift_and_rebase_decisions":true,"bursts":stats,"coverage_passed":true}),
        )?,
    )?;
    for (dest, q) in [(&base_path, &base), (&edge_path, &candidate)] {
        if !mp4::inject_and_check(source, dest, q, &|s| println!("{s}"))? {
            return Err("writeback verification failed".into());
        }
    }
    std::fs::write(
        out.join("verified.json"),
        serde_json::to_vec_pretty(
            &json!({"baseline":base_path,"candidate":edge_path,"timestamp_and_quaternion_writeback_verified":true}),
        )?,
    )?;
    Ok(())
}
