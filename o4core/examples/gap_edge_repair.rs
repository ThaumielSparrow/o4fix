//! Experimental repair; all production defaults remain unchanged.
#[path = "support/edge_offset_patch.rs"]
mod edge;
#[path = "support/fb_tracker.rs"]
#[allow(clippy::too_many_arguments)]
mod fb;
#[path = "support/gap_patch.rs"]
mod gap_patch;
use o4core::{config::Config, detect, mp4, pipeline, quat, telemetry};
use std::{path::Path, sync::atomic::AtomicBool};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 3 {
        return Err("usage: gap_edge_repair SOURCE DESTINATION".into());
    }
    let source = Path::new(&a[1]);
    let out = Path::new(&a[2]);
    mp4::validate_output(source, out)?;
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
    let patched = gap_patch::optical_patch(
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
    for b in stats {
        println!(
            "burst {}-{} drift {} rebased {}",
            b.start, b.end, b.drift_deg, b.rebased
        );
    }
    if !mp4::inject_and_check(source, out, &q, &|s| println!("{s}"))? {
        return Err("writeback verification failed".into());
    }
    Ok(())
}
