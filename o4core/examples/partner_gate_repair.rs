//! Cached partner-only ablation on zero-trust severe intervals, accepted edge splice.
#[path = "support/edge_offset_patch.rs"]
mod edge;
#[path = "support/partner_gate.rs"]
mod partner;
use o4core::{config::Config, detect, dsp, mp4, patch, pipeline, quat, telemetry};
use serde_json::{json, Value};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 6 {
        return Err(
            "usage: partner_gate_repair SOURCE ACCEPTED_EDGEOFFSET TRACE NEW_MP4 NEW_METRICS"
                .into(),
        );
    }
    let source = Path::new(&a[1]);
    let dest = Path::new(&a[4]);
    if dest.exists() || Path::new(&a[5]).exists() {
        return Err("output exists".into());
    }
    mp4::validate_output(source, dest)?;
    let tel = telemetry::extract_quats(source)?;
    let reference = telemetry::extract_quats(Path::new(&a[2]))?;
    if tel.t != reference.t || tel.q.len() != reference.q.len() {
        return Err("reference timestamps differ".into());
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
            .map(|&n| n > cfg.severe)
            .collect::<Vec<_>>(),
        &tm,
        cfg.severe_pad,
        cfg.severe_merge,
        0.2,
    );
    let trace: Value = serde_json::from_slice(&std::fs::read(&a[3])?)?;
    let deg: Vec<[f64; 3]> = serde_json::from_value(trace["rates_deg_s"].clone())?;
    let supported: Vec<bool> = serde_json::from_value(trace["supported"].clone())?;
    if deg.len() != tm.len()
        || supported.len() != tm.len()
        || deg.iter().flatten().any(|v| !v.is_finite())
    {
        return Err("bad cache shape/rates".into());
    }
    let baseline = patch::OpticalPatch {
        rates: deg.iter().map(|v| v.map(f64::to_radians)).collect(),
        supported,
    };
    baseline.require_coverage(&tel.t, &intervals)?;
    let (base, bs) = edge::splice_orientation(
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
        return Err(format!("accepted parity failed: {parity}").into());
    }
    let rows = trace["rows"].as_array().ok_or("rows")?;
    let selected: Vec<_> = intervals
        .iter()
        .copied()
        .filter(|&(lo, hi)| {
            rows.iter().any(|r| {
                r["trust"].as_f64() == Some(0.)
                    && r["segment"][0].as_f64().is_some_and(|s| s <= lo)
                    && r["segment"][1].as_f64().is_some_and(|e| e >= hi)
            })
        })
        .collect();
    let snapped: Vec<_> = selected
        .iter()
        .map(|&(lo, hi)| {
            let i = dsp::searchsorted_left(&tel.t, lo);
            let j = dsp::searchsorted_right(&tel.t, hi)
                .saturating_sub(1)
                .min(tel.t.len() - 1);
            (i, j, tel.t[i], tel.t[j])
        })
        .collect();
    let mut changed = baseline.rates.clone();
    let mut touched = 0;
    let mut formula_error: f64 = 0.;
    let mut changes = vec![];
    for row in rows {
        let i = row["index"].as_u64().ok_or("index")? as usize;
        let t = row["t"].as_f64().ok_or("time")?;
        if i >= tm.len() || (t - tm[i]).abs() > 1e-9 {
            return Err("trace timestamps differ".into());
        }
        let w = row["optical_weight"].as_f64().ok_or("weight")?;
        let hb = row["handback"].as_f64().ok_or("handback")?;
        let optical: [f64; 3] = serde_json::from_value(row["optical_deg_s"].clone())?;
        let medium: [f64; 3] = serde_json::from_value(row["medium_deg_s"].clone())?;
        for k in 0..3 {
            let expected =
                (1. - w) * diag.light[i][k] + w * ((1. - hb) * optical[k] + hb * medium[k]);
            formula_error = formula_error.max((expected - deg[i][k]).abs());
        }
        let gate = snapped
            .iter()
            .find(|&&(lo, hi, _, _)| i >= lo && i < hi)
            .map_or(0., |&(_, _, lo, hi)| {
                quat::smoothstep((t - lo) / 0.19).min(quat::smoothstep((hi - t) / 0.19))
            });
        if gate == 0. || w == 1. {
            continue;
        }
        if row["trust"].as_f64() != Some(0.) || !baseline.supported[i] {
            return Err("unsupported/nonzero-trust selection".into());
        }
        let new = partner::suppress_partner(deg[i], diag.light[i], optical, medium, w, hb, gate);
        changed[i] = new.map(f64::to_radians);
        let delta = (0..3)
            .map(|k| (new[k] - deg[i][k]).powi(2))
            .sum::<f64>()
            .sqrt();
        changes.push(
            json!({"t":t,"gate":gate,"old_light_weight":1.-w,"handback":hb,"delta_deg_s":delta}),
        );
        touched += 1;
    }
    if formula_error > 1e-8 {
        return Err(format!("cached source-mixture formula differs: {formula_error}").into());
    }
    let modified = patch::OpticalPatch {
        rates: changed,
        supported: baseline.supported.clone(),
    };
    modified.require_coverage(&tel.t, &intervals)?;
    let (candidate, cs) = edge::splice_orientation(
        &tel.t,
        &tel.q,
        &modified.rates,
        &intervals,
        0.19,
        cfg.drift_rebase_above,
        cfg.drift_decay_rate,
    );
    if candidate.iter().flatten().any(|v| !v.is_finite()) {
        return Err("nonfinite candidate".into());
    }
    let bursts:Vec<_>=bs.iter().zip(&cs).map(|(b,c)|json!({"interval":[b.start,b.end],"baseline_drift":b.drift_deg,"candidate_drift":c.drift_deg,"baseline_rebased":b.rebased,"candidate_rebased":c.rebased})).collect();
    std::fs::write(
        &a[5],
        serde_json::to_vec_pretty(
            &json!({"parity_max_component":parity,"mixture_formula_max_error_deg_s":formula_error,"selected_intervals":selected,"changed_rate_samples":touched,"bursts":bursts,"changes":changes,
        "note":"Only light-gyro partner suppressed in zero-trust severe intervals using 0.19s smoothstep transitions. Original fast handback retained in replacement burst. Same accepted edge splice, original rebase gate/decay; resulting decisions may change."}),
        )?,
    )?;
    if !mp4::inject_and_check(source, dest, &candidate, &|s| println!("{s}"))? {
        return Err("writeback verification failed".into());
    }
    println!(
        "Accepted parity {parity}; mixture formula error {formula_error}; changed {touched} rates"
    );
    Ok(())
}
