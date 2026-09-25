//! Control: identical tracks and estimator, alternate deterministic RANSAC seed.
#[path = "support/fb_tracker.rs"]
mod fb;
use serde_json::{json, Value};
use std::{path::Path, sync::atomic::AtomicBool};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 4 {
        return Err("usage: tracking_seed_probe VIDEO OBSERVATIONS_JSON OUTPUT_JSON".into());
    }
    let obs: Value = serde_json::from_str(&std::fs::read_to_string(&a[2])?)?;
    let intervals: Vec<(f64, f64)> = serde_json::from_value(obs["intervals"].clone())?;
    let tel = o4core::telemetry::extract_quats(Path::new(&a[1]))?;
    let cancel = AtomicBool::new(false);
    let parity = fb::video_rates(
        Path::new(&a[1]),
        &intervals,
        &tel.meta,
        &cancel,
        &|d, n| println!("parity {d}/{n}"),
        false,
        0,
        1,
    )?;
    let baseline: Vec<[f64; 3]> = serde_json::from_value(obs["baseline"].clone())?;
    let quality: Vec<f64> = serde_json::from_value(obs["baseline_quality"].clone())?;
    let times: Vec<f64> = serde_json::from_value(obs["t"].clone())?;
    assert_eq!(times, parity.t);
    assert_eq!(baseline.len(), parity.omega.len());
    assert_eq!(quality.len(), parity.quality.len());
    let max_rate_error = baseline
        .iter()
        .flatten()
        .zip(parity.omega.iter().flatten())
        .map(|(a, b)| (a - b).abs())
        .fold(0.0_f64, f64::max);
    let max_quality_error = quality
        .iter()
        .zip(&parity.quality)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0_f64, f64::max);
    // Cached JSON deserialization need not preserve the last binary digit.
    assert!(
        max_rate_error <= 1e-12,
        "tracker parity error {max_rate_error} rad/s"
    );
    assert!(
        max_quality_error <= 1e-12,
        "quality parity error {max_quality_error}"
    );
    println!("parity max rate error {max_rate_error}, quality {max_quality_error}");
    let other = fb::video_rates(
        Path::new(&a[1]),
        &intervals,
        &tel.meta,
        &cancel,
        &|d, n| println!("seed {d}/{n}"),
        false,
        1,
        1,
    )?;
    assert_eq!(times, other.t);
    std::fs::write(
        &a[3],
        serde_json::to_vec(
            &json!({"intervals":intervals,"t":times,"baseline":baseline,"baseline_quality":quality,"candidate":other.omega,"candidate_quality":other.quality,"parity_max_rate_error_rad_s":max_rate_error,"parity_max_quality_error":max_quality_error}),
        )?,
    )?;
    Ok(())
}
