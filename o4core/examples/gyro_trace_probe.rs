//! Isolated orientation edge-ramp ablation; production optics and splice implementation.
#[path = "support/trace_patch.rs"]
mod trace_patch;
use o4core::{config::Config, detect, patch, pipeline, quat, telemetry};
use std::{path::Path, sync::atomic::AtomicBool};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 5 {
        return Err("usage: gyro_trace_probe SOURCE PRESERVED_FIXED TRACE_JSON METRICS".into());
    }
    let source = Path::new(&a[1]);
    let out = Path::new(&a[3]);
    if out.exists() {
        return Err("candidate already exists".into());
    }
    let reference = telemetry::extract_quats(Path::new(&a[2]))?;
    let cfg = Config::default();
    cfg.validate()?;
    let tel = telemetry::extract_quats(source)?;
    if tel.t.len() < 100
        || tel.t.iter().any(|v| !v.is_finite())
        || tel.t.windows(2).any(|w| w[1] <= w[0])
        || tel.q.iter().flatten().any(|v| !v.is_finite())
    {
        return Err("invalid telemetry".into());
    }
    let fs = pipeline::fs(&tel.t);
    cfg.validate_sample_rate(fs)?;
    let (tm, om) = quat::quats_to_rates(&tel.t, &tel.q);
    let (clean, diag) = detect::adaptive_clean(&om, fs, &cfg);
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
    if intervals.is_empty() {
        return Err("no severe intervals; no output needed".into());
    }
    let patched = trace_patch::optical_patch(
        Path::new(&a[3]),
        source,
        &tm,
        &clean,
        &diag,
        fs,
        &cfg,
        &tel.meta,
        &|s| println!("{s}"),
        &|_, d, n| println!("tracking {d}/{n}"),
        &AtomicBool::new(false),
    )?;
    patched.require_coverage(&tel.t, &intervals)?;
    let (q, stats) = patch::splice_orientation(
        &tel.t,
        &tel.q,
        &patched.rates,
        &intervals,
        cfg.ramp,
        cfg.drift_rebase_above,
        cfg.drift_decay_rate,
    );
    if reference.t != tel.t {
        return Err("reference timestamps differ".into());
    }
    let parity = q
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
    println!("default parity max component error: {parity}");
    if parity > 1e-6 {
        return Err("default parity failed; refusing candidate".into());
    }
    // Same intervals, integration, endpoint correction and decay. Only ramp duration changes.
    let (candidate, candidate_stats) = patch::splice_orientation(
        &tel.t,
        &tel.q,
        &patched.rates,
        &intervals,
        0.19,
        cfg.drift_rebase_above,
        cfg.drift_decay_rate,
    );
    assert!(stats
        .iter()
        .zip(&candidate_stats)
        .all(|(a, b)| a.rebased == b.rebased && a.drift_deg == b.drift_deg));
    let (_, baseline_rates) = quat::quats_to_rates(&tel.t, &q);
    let (_, candidate_rates) = quat::quats_to_rates(&tel.t, &candidate);
    let mut rows = vec![];
    for &(start, end) in &[
        (102., 110.),
        (221., 229.),
        (245., 253.),
        (304., 312.),
        (19., 22.),
    ] {
        let mut variants = vec![];
        for rates in [&baseline_rates, &candidate_rates] {
            let mut err = vec![];
            let mut acceleration = vec![];
            for i in 1..tm.len() {
                if tm[i] < start || tm[i] > end {
                    continue;
                }
                if diag.noise[i] > cfg.severe {
                    err.push(
                        (0..3)
                            .map(|k| (rates[i][k] - patched.rates[i][k]).to_degrees().powi(2))
                            .sum::<f64>(),
                    );
                }
                acceleration.push(
                    (0..3)
                        .map(|k| {
                            ((rates[i][k] - rates[i - 1][k]) / (tm[i] - tm[i - 1]))
                                .to_degrees()
                                .powi(2)
                        })
                        .sum::<f64>()
                        .sqrt(),
                );
            }
            variants.push(serde_json::json!({"noisy_rate_difference_from_patch_rms":if err.is_empty(){None}else{Some((err.iter().sum::<f64>()/err.len() as f64).sqrt())},"peak_acceleration_deg_s2":acceleration.into_iter().fold(0.,f64::max)}));
        }
        let max_change = tel
            .t
            .iter()
            .enumerate()
            .filter(|(_, t)| **t >= start && **t <= end)
            .map(|(i, _)| {
                quat::qlog(quat::qmul(quat::qconj(q[i]), candidate[i]))
                    .iter()
                    .map(|x| x * x)
                    .sum::<f64>()
                    .sqrt()
                    .to_degrees()
            })
            .fold(0., f64::max);
        rows.push(serde_json::json!({"window":[start,end],"baseline":variants[0],"candidate":variants[1],"maximum_orientation_change_deg":max_change}));
    }
    std::fs::write(
        &a[4],
        serde_json::to_vec_pretty(
            &serde_json::json!({"parity_max_component":parity,"baseline_ramp_s":cfg.ramp,"candidate_ramp_s":0.19,"same_rebase_decisions_and_drift":true,"windows":rows,"note":"Difference from patch is a mechanism diagnostic, not ground truth; includes endpoint bridge and discretization."}),
        )?,
    )?;
    Ok(())
}
