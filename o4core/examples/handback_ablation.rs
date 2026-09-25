//! Cached exact-trace ablation: suppress handback only in zero-trust optical segments.
use o4core::{config::Config, detect, mp4, patch, pipeline, quat, telemetry};
use serde_json::{json, Value};
use std::path::Path;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 6 {
        return Err(
            "usage: handback_ablation SOURCE RAMP019_REFERENCE TRACE DESTINATION METRICS".into(),
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
    let (base, bs) = patch::splice_orientation(
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
    let mut changed = baseline.rates.clone();
    let mut touched = 0;
    for row in trace["rows"].as_array().ok_or("rows")? {
        if row["trust"].as_f64().ok_or("trust")? != 0. {
            continue;
        }
        let i = row["index"].as_u64().ok_or("index")? as usize;
        let w =
            row["optical_weight"].as_f64().ok_or("w")? * row["handback"].as_f64().ok_or("wf")?;
        if w == 0. {
            continue;
        }
        if i >= changed.len() || (row["t"].as_f64().ok_or("t")? - tm[i]).abs() > 1e-9 {
            return Err("trace index mismatch".into());
        }
        for k in 0..3 {
            changed[i][k] -= (w
                * (row["medium_deg_s"][k].as_f64().ok_or("medium")?
                    - row["optical_deg_s"][k].as_f64().ok_or("optical")?))
            .to_radians();
        }
        touched += 1;
    }
    let (candidate, cs) = patch::splice_orientation(
        &tel.t,
        &tel.q,
        &changed,
        &intervals,
        0.19,
        cfg.drift_rebase_above,
        cfg.drift_decay_rate,
    );
    let mut rows = vec![];
    for (b, c) in bs.iter().zip(&cs) {
        rows.push(json!({"interval":[b.start,b.end],"baseline_drift":b.drift_deg,"candidate_drift":c.drift_deg,"baseline_rebased":b.rebased,"candidate_rebased":c.rebased}));
    }
    let windows: Vec<_> = [
        (102., 110.),
        (221., 229.),
        (245., 253.),
        (304., 312.),
        (19., 22.),
    ]
    .iter()
    .map(|&(a, b)| {
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
            &json!({"parity_max_component":parity,"changed_rate_samples":touched,"bursts":rows,"windows":windows,"note":"Only zero-trust-segment handback removed; ramp remains 0.19. Drift/rebase decisions may change because integrated rates change. Research only."}),
        )?,
    )?;
    if !mp4::inject_and_check(source, out, &candidate, &|s| println!("{s}"))? {
        return Err("verification failed".into());
    }
    Ok(())
}
