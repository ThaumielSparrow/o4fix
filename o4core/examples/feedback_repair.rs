//! Research-only: apply a body-frame orientation adjustment to an existing repaired MP4.
//! ADJUST_JSON = {"t":[s...], "angle":[[x,y,z] rad...]} (sorted t). The angle is linearly
//! interpolated onto telemetry timestamps, zero before the first sample and held after
//! the last. Its increments are applied as extra body rotation per sample (see below), and
//! the result is written into SOURCE's layout at a NEW destination with inject/verify.
use o4core::{mp4, quat, telemetry};
use serde_json::{json, Value};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 5 {
        return Err("usage: feedback_repair SOURCE BASE_REPAIRED ADJUST_JSON DESTINATION".into());
    }
    let (source, out) = (Path::new(&a[1]), Path::new(&a[4]));
    if out.exists() {
        return Err("output exists".into());
    }
    mp4::validate_output(source, out)?;
    let src = telemetry::extract_quats(source)?;
    let base = telemetry::extract_quats(Path::new(&a[2]))?;
    if src.t != base.t {
        return Err("timestamp mismatch".into());
    }
    let adj: Value = serde_json::from_str(&std::fs::read_to_string(&a[3])?)?;
    let t: Vec<f64> = serde_json::from_value(adj["t"].clone())?;
    let ang: Vec<[f64; 3]> = serde_json::from_value(adj["angle"].clone())?;
    if t.len() != ang.len() || t.len() < 2 || t.windows(2).any(|w| w[1] <= w[0]) || ang.iter().flatten().any(|v| !v.is_finite()) {
        return Err("bad adjustment".into());
    }
    let mut j = 0usize;
    let mut max_deg: f64 = 0.;
    let angles: Vec<[f64; 3]> = base
        .t
        .iter()
        .map(|&ti| {
            let d = if ti < t[0] {
                [0.; 3]
            } else if ti >= t[t.len() - 1] {
                ang[ang.len() - 1]
            } else {
                while t[j + 1] <= ti {
                    j += 1;
                }
                let f = (ti - t[j]) / (t[j + 1] - t[j]);
                std::array::from_fn(|k| ang[j][k] + f * (ang[j + 1][k] - ang[j][k]))
            };
            max_deg = max_deg.max(d.iter().map(|v| v * v).sum::<f64>().sqrt().to_degrees());
            d
        })
        .collect();
    // Integrate as body-rate increments: q'_{i+1} = q'_i * (q_i^-1 q_{i+1}) * exp(dA_i).
    // Outside the adjustment the body rates are untouched, so a leftover offset is a
    // constant WORLD-frame rotation (invisible to Gyroflow with horizon lock off), not a
    // body-frame remount that would re-aim the virtual camera.
    let mut q = base.q.clone();
    for i in 0..q.len() - 1 {
        let inc: [f64; 3] = std::array::from_fn(|k| angles[i + 1][k] - angles[i][k]);
        if inc == [0.; 3] && q[i] == base.q[i] {
            continue;
        }
        let dq = quat::qmul(quat::qconj(base.q[i]), base.q[i + 1]);
        q[i + 1] = quat::qnorm(quat::qmul(quat::qmul(q[i], dq), quat::qexp(inc)));
    }
    let changed = q.iter().zip(&base.q).filter(|(x, y)| x != y).count();
    println!("{}", json!({"samples":q.len(),"changed":changed,"max_adjust_deg":max_deg}));
    if !mp4::inject_and_check(source, out, &q, &|s| println!("{s}"))? {
        return Err("verification failed".into());
    }
    Ok(())
}
