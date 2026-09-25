//! Read-only temporal-span consistency on the residual event and a clean control.
#[allow(clippy::too_many_arguments)] // Existing experimental tracker API.
#[path = "support/fb_tracker.rs"]
mod tracker;
use o4core::telemetry;
use serde_json::json;
use std::{path::Path, sync::atomic::AtomicBool};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 3 {
        return Err("usage: optical_span_probe SOURCE OUTPUT".into());
    }
    let tel = telemetry::extract_quats(Path::new(&a[1]))?;
    let intervals = [(131.5, 146.5), (55.4, 59.4)];
    let mut result = vec![];
    for gap in [1, 2] {
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
        result.push(json!({"gap":gap,"t":r.t,"omega":r.omega,"quality":r.quality}));
    }
    std::fs::write(
        &a[2],
        serde_json::to_vec(
            &json!({"intervals":intervals,"variants":result,"note":"Existing tracker with FB gate off, original lens handling unchanged, only pair span 10 vs 20ms. Read-only consistency, not motion truth."}),
        )?,
    )?;
    Ok(())
}
