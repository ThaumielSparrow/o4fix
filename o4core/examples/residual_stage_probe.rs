//! Isolated orientation edge-ramp ablation; production optics and splice implementation.
#[path = "support/edge_offset_patch.rs"]
mod edge;
#[path = "support/trace_patch.rs"]
mod trace_patch;
use o4core::{config::Config, detect, pipeline, quat, telemetry};
use std::{path::Path, sync::atomic::AtomicBool};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 5 {
        return Err(
            "usage: residual_stage_probe SOURCE ACCEPTED_EDGEOFFSET TRACE_JSON METRICS".into(),
        );
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
    let (q, stats) = edge::splice_orientation(
        &tel.t,
        &tel.q,
        &patched.rates,
        &intervals,
        0.19,
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
    let (_, final_rates) = quat::quats_to_rates(&tel.t, &q);
    let mut rows = vec![];
    for burst in &stats {
        let i0 = o4core::dsp::searchsorted_left(&tel.t, burst.start);
        let i1 = o4core::dsp::searchsorted_right(&tel.t, burst.end)
            .saturating_sub(1)
            .min(tel.t.len() - 1);
        let mut phases = vec![];
        for interior in [false, true] {
            let ii: Vec<_> = (i0..i1)
                .filter(|&i| {
                    ((tel.t[i] - tel.t[i0] >= 0.19) && (tel.t[i1] - tel.t[i + 1] >= 0.19))
                        == interior
                })
                .collect();
            let mut errs: Vec<[f64; 3]> = vec![];
            for i in ii {
                errs.push(std::array::from_fn(|k| {
                    (final_rates[i][k] - patched.rates[i][k]).to_degrees()
                }));
            }
            let n = errs.len();
            let rms = if n > 0 {
                Some((errs.iter().flatten().map(|v| v * v).sum::<f64>() / n as f64).sqrt())
            } else {
                None
            };
            phases.push(serde_json::json!({"interior":interior,"samples":n,"rate_difference_rms_deg_s":rms}));
        }
        rows.push(serde_json::json!({"interval":[burst.start,burst.end],"rebased":burst.rebased,"drift_deg":burst.drift_deg,"implied_bridge_peak_deg_s":1.5*burst.drift_deg/(tel.t[i1]-tel.t[i0]),"phases":phases}));
    }
    std::fs::write(
        &a[4],
        serde_json::to_vec_pretty(
            &serde_json::json!({"parity_max_component":parity,"bursts":rows,"note":"Final accepted repair rates minus supplied optical patch: mechanism attribution, not truth. Same frame-index integration convention as production."}),
        )?,
    )?;
    Ok(())
}
