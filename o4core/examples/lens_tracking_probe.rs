//! Single-mechanism lens inversion experiment on otherwise unchanged production tracking.
#[path = "support/lens_tracker.rs"]
mod tracker;
use o4core::telemetry;
use serde_json::{json, Value};
use std::sync::atomic::AtomicBool;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 5 {
        return Err(
            "usage: lens_tracking_probe VIDEO CALIBRATION_JSON BASE_OBSERVATIONS OUTPUT_JSON"
                .into(),
        );
    }
    let cal: Value = serde_json::from_str(&std::fs::read_to_string(&a[2])?)?;
    let base: Value = serde_json::from_str(&std::fs::read_to_string(&a[3])?)?;
    let intervals: Vec<(f64, f64)> = serde_json::from_value(cal["intervals"].clone())?;
    let video = std::path::Path::new(&a[1]);
    let tel = telemetry::extract_quats(video)?;
    if tel.meta.camera_matrix.is_none() || tel.meta.distortion.is_none() {
        return Err("actual lens metadata required".into());
    }
    let candidate = tracker::video_rates(
        video,
        &intervals,
        &tel.meta,
        &AtomicBool::new(false),
        &|i, n| println!("{i}/{n}"),
        false,
        0,
        1,
    )?;
    let bt: Vec<f64> = serde_json::from_value(base["t"].clone())?;
    assert_eq!(candidate.t.len(), bt.len());
    assert!(candidate
        .t
        .iter()
        .zip(bt)
        .all(|(a, b)| (a - b).abs() < 1e-12));
    std::fs::write(
        &a[4],
        serde_json::to_vec(
            &json!({"intervals":intervals,"t":candidate.t,"baseline":base["omega"],"baseline_quality":base["quality"],"candidate":candidate.omega,"candidate_quality":candidate.quality,"metadata":format!("{:?}",tel.meta)}),
        )?,
    )?;
    Ok(())
}
