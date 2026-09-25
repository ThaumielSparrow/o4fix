//! Compare baseline and a research-only forward/backward tracking gate.
#[path = "support/fb_tracker.rs"]
mod fb;
use o4core::{optical, telemetry};
use serde_json::json;
use std::{path::PathBuf, sync::atomic::AtomicBool};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("usage: tracking_probe VIDEO CALIBRATION_REPORT OUTPUT_DIR".into());
    }
    let video = PathBuf::from(&args[1]);
    let out = PathBuf::from(&args[3]);
    std::fs::create_dir_all(&out)?;
    let report: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&args[2])?)?;
    let mut intervals: Vec<(f64, f64)> = report["intervals"]
        .as_array()
        .ok_or("missing intervals")?
        .iter()
        .map(|r| (r[0].as_f64().unwrap(), r[1].as_f64().unwrap()))
        .collect();
    intervals.extend([(102., 110.), (221., 229.), (245., 253.), (304., 312.)]);
    let tel = telemetry::extract_quats(&video)?;
    let cancel = AtomicBool::new(false);
    let baseline = optical::video_rates(&video, &intervals, &tel.meta, &cancel, &|d, n| {
        println!("baseline {d}/{n}")
    })?;
    let candidate = fb::video_rates(
        &video,
        &intervals,
        &tel.meta,
        &cancel,
        &|d, n| println!("candidate {d}/{n}"),
        true,
        0,
        1,
    )?;
    assert_eq!(baseline.t, candidate.t);
    std::fs::write(
        out.join("observations.json"),
        serde_json::to_vec(
            &json!({"intervals":intervals,"t":baseline.t,"baseline":baseline.omega,"candidate":candidate.omega,"baseline_quality":baseline.quality,"candidate_quality":candidate.quality}),
        )?,
    )?;
    Ok(())
}
