//! Cached exact-trace ablation: extend severe padding to 0.4 s, retaining 0.19 s ramps.
#[path = "support/edge_offset_patch.rs"]
mod edge_offset;
#[path = "support/trust_rebase_patch.rs"]
mod trust_rebase;
use o4core::{config::Config, detect, mp4, patch, pipeline, quat, telemetry};
use serde_json::{json, Value};
use std::path::Path;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 7 {
        return Err(
            "usage: trust_rebase_repair SOURCE EDGEOFFSET_REFERENCE TRACE DESTINATION METRICS WINDOWS_JSON"
                .into(),
        );
    }
    let source = Path::new(&a[1]);
    let out = Path::new(&a[4]);
    if out.exists() {
        return Err("output exists".into());
    }
    mp4::validate_output(source, out)?;
    let tel = telemetry::extract_quats(source)?;
    let reference = telemetry::extract_quats(Path::new(&a[2]))?;
    if tel.t != reference.t {
        return Err("timestamp mismatch".into());
    }
    let cfg = Config::default();
    cfg.validate()?;
    let fs = pipeline::fs(&tel.t);
    cfg.validate_sample_rate(fs)?;
    let (tm, raw) = quat::quats_to_rates(&tel.t, &tel.q);
    let (_, diag) = detect::adaptive_clean(&raw, fs, &cfg);
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
    let trace: Value = serde_json::from_str(&std::fs::read_to_string(&a[3])?)?;
    let deg: Vec<[f64; 3]> = serde_json::from_value(trace["rates_deg_s"].clone())?;
    let supported: Vec<bool> = serde_json::from_value(trace["supported"].clone())?;
    if deg.len() != tm.len() || deg.iter().flatten().any(|v| !v.is_finite()) {
        return Err("bad trace rates".into());
    }
    let baseline = patch::OpticalPatch {
        rates: deg.iter().map(|r| r.map(f64::to_radians)).collect(),
        supported,
    };
    baseline.require_coverage(&tel.t, &intervals)?;
    let (base, bs) = edge_offset::splice_orientation(
        &tel.t,
        &tel.q,
        &baseline.rates,
        &intervals,
        0.19,
        cfg.drift_rebase_above,
        cfg.drift_decay_rate,
    );
    let parity = base
        .iter()
        .zip(&reference.q)
        .map(|(x, y)| {
            let sign = if x.iter().zip(y).map(|(a, b)| a * b).sum::<f64>() < 0. {
                -1.
            } else {
                1.
            };
            (0..4)
                .map(|k| (x[k] - sign * y[k]).abs())
                .fold(0., f64::max)
        })
        .fold(0., f64::max);
    if parity > 1e-6 {
        return Err(format!("cached baseline parity failed: {parity}").into());
    }
    let trace_rows = trace["rows"].as_array().ok_or("trace rows")?;
    let forced: Vec<_> = intervals
        .iter()
        .copied()
        .filter(|&(a, b)| {
            trace_rows.iter().any(|r| {
                r["trust"].as_f64() == Some(0.)
                    && r["segment"][0].as_f64().is_some_and(|s| s <= a)
                    && r["segment"][1].as_f64().is_some_and(|e| e >= b)
            })
        })
        .collect();
    let (candidate, cs) = trust_rebase::splice_orientation(
        &forced,
        &tel.t,
        &tel.q,
        &baseline.rates,
        &intervals,
        0.19,
        cfg.drift_rebase_above,
        cfg.drift_decay_rate,
    );
    let mut rows = vec![];
    for c in &cs {
        let old: Vec<_> = bs
            .iter()
            .filter(|b| b.end >= c.start && b.start <= c.end)
            .map(|b| json!({"interval":[b.start,b.end],"drift":b.drift_deg,"rebased":b.rebased}))
            .collect();
        rows.push(json!({"interval":[c.start,c.end],"candidate_drift":c.drift_deg,"candidate_rebased":c.rebased,"overlapping_baseline":old}));
    }
    let review_windows: Vec<[f64; 2]> = serde_json::from_str(&std::fs::read_to_string(&a[6])?)?;
    let windows: Vec<_> = review_windows
        .iter()
        .map(|&[a, b]| {
            let max = tel
                .t
                .iter()
                .enumerate()
                .filter(|(_, t)| **t >= a && **t <= b)
                .map(|(i, _)| {
                    quat::qlog(quat::qmul(quat::qconj(base[i]), candidate[i]))
                        .iter()
                        .map(|v| v * v)
                        .sum::<f64>()
                        .sqrt()
                        .to_degrees()
                })
                .fold(0., f64::max);
            json!({"window":[a,b],"max_orientation_difference_deg":max})
        })
        .collect();
    std::fs::write(
        &a[5],
        serde_json::to_vec_pretty(
            &json!({"parity_max_component":parity,"padding":0.2,"ramp":0.19,"forced_intervals":forced,"bursts":rows,"windows":windows,"note":"Accepted edgeoffset baseline; additionally rebase severe intervals wholly inside zero-trust optical segments. Same rates, ramps, original padding and decay. Research ablation; optical bias can be carried forward."}),
        )?,
    )?;
    if !mp4::inject_and_check(source, out, &candidate, &|s| println!("{s}"))? {
        return Err("verification failed".into());
    }
    Ok(())
}
