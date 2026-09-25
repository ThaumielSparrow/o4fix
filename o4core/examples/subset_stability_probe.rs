//! Read-only unchanged-tracker diagnostics with cached rate and quality parity.
#[allow(clippy::too_many_arguments)]
#[path = "support/subset_tracker.rs"]
mod tracker;
use o4core::telemetry;
use serde_json::{json, Value};
use std::{path::Path, sync::atomic::AtomicBool};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 4 {
        return Err(
            "usage: subset_stability_probe SOURCE CACHE NEW_OUTPUT (gap cache or spans cache)"
                .into(),
        );
    }
    if Path::new(&a[3]).exists() {
        return Err("output exists".into());
    }
    let cache: Value = serde_json::from_slice(&std::fs::read(&a[2])?)?;
    let mut intervals: Vec<(f64, f64)> = serde_json::from_value(cache["intervals"].clone())?;
    // Gap-observation caches can include severe targets; acquire only held-out clean intervals
    // when the caller supplies a dedicated subset cache. Existing full caches remain untouched.
    let tel = telemetry::extract_quats(Path::new(&a[1]))?;
    let variants: Vec<Value> = if let Some(v) = cache["variants"].as_array() {
        v.clone()
    } else {
        vec![
            json!({"gap":2,"t":cache["t"],"omega":cache["candidate"],"quality":cache["candidate_quality"]}),
        ]
    };
    if let Some(v) = cache.get("probe_intervals") {
        intervals = serde_json::from_value(v.clone())?;
    }
    let mut output = vec![];
    for v in variants {
        let gap = v["gap"].as_u64().ok_or("gap")? as usize;
        let r = tracker::video_rates(
            Path::new(&a[1]),
            &intervals,
            &tel.meta,
            &AtomicBool::new(false),
            &|d, n| println!("gap {gap}: {d}/{n}"),
            false,
            0,
            gap,
        )?;
        let ct: Vec<f64> = serde_json::from_value(v["t"].clone())?;
        let cr: Vec<[f64; 3]> = serde_json::from_value(v["omega"].clone())?;
        let cq: Vec<f64> = serde_json::from_value(v["quality"].clone())?;
        let mut max_rate: f64 = 0.;
        let mut max_quality: f64 = 0.;
        for i in 0..r.t.len() {
            let j = ct
                .iter()
                .position(|t| (*t - r.t[i]).abs() < 1e-9)
                .ok_or("new timestamp absent from cache")?;
            max_quality = max_quality.max((r.quality[i] - cq[j]).abs());
            for k in 0..3 {
                max_rate = max_rate.max((r.omega[i][k] - cr[j][k]).abs());
            }
        }
        if max_rate > 1e-12 || max_quality > 1e-12 {
            return Err(format!("parity failed: rates {max_rate}, quality {max_quality}").into());
        }
        output.push(json!({"gap":gap,"t":r.t,"omega":r.omega,"quality":r.quality,"diagnostics":r.diagnostics,"max_rate_parity_rad_s":max_rate,"max_quality_parity":max_quality}));
    }
    std::fs::write(
        &a[3],
        serde_json::to_vec(
            &json!({"intervals":intervals,"variants":output,"note":"Diagnostics only; unchanged half-resolution tracker, lens inversion, essential fit, seeds, pose branch, gap. Every acquired timestamp/rate/quality compared against saved cache."}),
        )?,
    )?;
    Ok(())
}
