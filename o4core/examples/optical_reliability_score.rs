//! Cached held-out clean-gyro reliability diagnostics. No calibration refit or repair.
#[allow(dead_code)]
#[path = "support/reliability_alignment.rs"]
mod audit;
use o4core::{
    alignment::CalibrationData,
    config::Config,
    detect,
    optical::{Alignment, OpticalRates},
    pipeline, quat, telemetry,
};
use serde_json::{json, Value};
use std::path::Path;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 5 {
        return Err(
            "usage: optical_reliability_score SOURCE CALIBRATION GAP_OBSERVATIONS NEW_OUTPUT"
                .into(),
        );
    }
    if Path::new(&args[4]).exists() {
        return Err("output exists".into());
    }
    let cal: Value = serde_json::from_slice(&std::fs::read(&args[2])?)?;
    let obs: Value = serde_json::from_slice(&std::fs::read(&args[3])?)?;
    let tel = telemetry::extract_quats(Path::new(&args[1]))?;
    let fs = pipeline::fs(&tel.t);
    let (tm, om) = quat::quats_to_rates(&tel.t, &tel.q);
    let (clean, _) = detect::adaptive_clean(&om, fs, &Config::default());
    let gyro: Vec<_> = clean.iter().map(|r| r.map(f64::to_degrees)).collect();
    let t: Vec<f64> = serde_json::from_value(obs["t"].clone())?;
    let base: Vec<[f64; 3]> = serde_json::from_value(obs["baseline"].clone())?;
    let gap: Vec<[f64; 3]> = serde_json::from_value(obs["candidate"].clone())?;
    let bq: Vec<f64> = serde_json::from_value(obs["baseline_quality"].clone())?;
    let cq: Vec<f64> = serde_json::from_value(obs["candidate_quality"].clone())?;
    if [base.len(), gap.len(), bq.len(), cq.len()]
        .iter()
        .any(|&n| n != t.len())
    {
        return Err("cache shape".into());
    }
    let mut rows = vec![];
    let mut folds = vec![];
    for (fold, f) in cal["held_out"]
        .as_array()
        .ok_or("folds")?
        .iter()
        .enumerate()
    {
        let lo = f["interval"][0].as_f64().ok_or("lo")?;
        let hi = f["interval"][1].as_f64().ok_or("hi")?;
        let fit = &f["baseline"];
        let al = Alignment {
            shift: fit["shift_ms"].as_f64().ok_or("shift")? / 1000.,
            n: serde_json::from_value(fit["matrix"].clone())?,
            r2: 0.,
        };
        let ii: Vec<_> = (0..t.len())
            .filter(|&i| t[i] >= lo - 0.02 && t[i] <= hi + 0.02)
            .collect();
        let mut streams = vec![];
        let mut scores = vec![];
        for rates in [&base, &gap] {
            let data = OpticalRates {
                t: ii.iter().map(|&i| t[i]).collect(),
                omega: ii.iter().map(|&i| rates[i]).collect(),
                quality: ii
                    .iter()
                    .map(|&i| if bq[i] > 0.5 && cq[i] > 0.5 { 1. } else { 0. })
                    .collect(),
            };
            let old = CalibrationData::prepare(&data, &tm, &gyro, fs)
                .and_then(|d| d.score(&al))
                .ok_or("reference score unavailable")?;
            let new =
                audit::CalibrationData::prepare(&data, &tm, &gyro, fs).ok_or("audit prepare")?;
            let ns = new.score(&al).ok_or("audit score")?;
            if ns.samples != old.samples || (ns.rms_deg_s - old.rms_deg_s).abs() > 1e-12 {
                return Err("scorer parity failed".into());
            }
            scores.push(json!({"samples":old.samples,"rms":old.rms_deg_s,"runs":old.runs}));
            streams.push(new.residual_rows(&al));
        }
        if streams[0].len() != streams[1].len() {
            return Err("support differs".into());
        }
        for (&(run, tt, p1, g1), &(run2, tt2, p2, g2)) in streams[0].iter().zip(&streams[1]) {
            if run != run2 || tt != tt2 || g1 != g2 {
                return Err("common support/gyro differs".into());
            }
            let i = ii
                .iter()
                .copied()
                .find(|&i| t[i] == tt)
                .ok_or("observation time")?;
            let norm = |v: [f64; 3]| v.iter().map(|x| x * x).sum::<f64>().sqrt();
            rows.push(
                json!({"fold":fold,"run":run,"t":tt,"quality1":bq[i],"quality2":cq[i],
                "span_difference":norm(std::array::from_fn(|k|p1[k]-p2[k])),
                "error1":norm(std::array::from_fn(|k|p1[k]-g1[k])),
                "error2":norm(std::array::from_fn(|k|p2[k]-g1[k])),
                "optical_speed":norm(p2),"gyro_speed":norm(g1)}),
            );
        }
        folds.push(json!({"interval":[lo,hi],"baseline":scores[0],"gap2":scores[1]}));
    }
    std::fs::write(
        &args[4],
        serde_json::to_vec(&json!({"folds":folds,"rows":rows,
        "note":"Frozen leave-section-out baseline calibration; existing common-support contiguous-run 5Hz filtering and 0.15s trim. Per-sample error is against clean gyro, not motion truth. Exact score parity checked with existing CalibrationData. Quality>0.5 preselection restricts confidence range."}))?,
    )?;
    Ok(())
}
