//! Frozen subset average versus existing gap2 full fit, evaluated on identical support.
use o4core::quat;
use serde_json::{json, Value};
use std::path::Path;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 4 {
        return Err("usage: subset_ensemble_cache SUBSETS ORIGINAL_GAP_CACHE NEW_OUTPUT".into());
    }
    if Path::new(&a[3]).exists() {
        return Err("output exists".into());
    }
    let d: Value = serde_json::from_slice(&std::fs::read(&a[1])?)?;
    let original: Value = serde_json::from_slice(&std::fs::read(&a[2])?)?;
    let v = &d["variants"][0];
    if v["gap"].as_u64() != Some(2) {
        return Err("expected gap2 clean data".into());
    }
    let t: Vec<f64> = serde_json::from_value(v["t"].clone())?;
    let full: Vec<[f64; 3]> = serde_json::from_value(v["omega"].clone())?;
    let ct: Vec<f64> = serde_json::from_value(original["t"].clone())?;
    let cq1: Vec<f64> = serde_json::from_value(original["baseline_quality"].clone())?;
    let cq2: Vec<f64> = serde_json::from_value(original["candidate_quality"].clone())?;
    let mut quality = vec![];
    let mut mean = vec![];
    let mut fallback = 0;
    for (i, diag) in v["diagnostics"]
        .as_array()
        .ok_or("diagnostics")?
        .iter()
        .enumerate()
    {
        let j = ct
            .iter()
            .position(|x| (*x - t[i]).abs() < 1e-9)
            .ok_or("cache time")?;
        quality.push(cq1[j].min(cq2[j]));
        let h = &diag["subset"]["halves"];
        if h[0]["rotation_rad"].is_array() && h[1]["rotation_rad"].is_array() {
            let x: [f64; 3] = serde_json::from_value(h[0]["rotation_rad"].clone())?;
            let y: [f64; 3] = serde_json::from_value(h[1]["rotation_rad"].clone())?;
            mean.push(quat::qlog(quat::slerp(quat::qexp(x), quat::qexp(y), 0.5)).map(|v| v / 0.02));
        } else {
            mean.push(full[i]);
            fallback += 1;
        }
    }
    std::fs::write(
        &a[3],
        serde_json::to_vec(
            &json!({"intervals":d["intervals"],"t":t,"baseline":full,"candidate":mean,"baseline_quality":quality,"candidate_quality":quality,"fallback_pairs":fallback,
        "note":"No temporal smoothing: equal SO(3) midpoint of disjoint halves. Full-fit fallback when either subset lacks existing support. Same original gap-cache common confidence for both score streams."}),
        )?,
    )?;
    Ok(())
}
