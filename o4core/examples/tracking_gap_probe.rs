//! Two-frame optical measurement; baseline resampled to common midpoint times.
#[path = "support/fb_tracker.rs"]
mod fb;
use serde_json::{json, Value};
use std::{path::Path, sync::atomic::AtomicBool};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 4 {
        return Err("usage: tracking_gap_probe VIDEO OBSERVATIONS_JSON OUTPUT_JSON".into());
    }
    let obs: Value = serde_json::from_str(&std::fs::read_to_string(&a[2])?)?;
    let intervals: Vec<(f64, f64)> = serde_json::from_value(obs["intervals"].clone())?;
    let tel = o4core::telemetry::extract_quats(Path::new(&a[1]))?;
    let other = fb::video_rates(
        Path::new(&a[1]),
        &intervals,
        &tel.meta,
        &AtomicBool::new(false),
        &|d, n| println!("two-frame {d}/{n}"),
        false,
        0,
        2,
    )?;
    let times: Vec<f64> = serde_json::from_value(obs["t"].clone())?;
    let rates: Vec<[f64; 3]> = serde_json::from_value(obs["baseline"].clone())?;
    let quality: Vec<f64> = serde_json::from_value(obs["baseline_quality"].clone())?;
    // Original interval ordering is by calibration score, not time. Sort for interpolation.
    let mut order: Vec<usize> = (0..times.len()).collect();
    order.sort_by(|&a, &b| times[a].total_cmp(&times[b]));
    let sorted: Vec<_> = order.iter().map(|&i| times[i]).collect();
    let cols: Vec<_> = (0..3)
        .map(|k| {
            o4core::dsp::interp(
                &other.t,
                &sorted,
                &order.iter().map(|&i| rates[i][k]).collect::<Vec<_>>(),
            )
        })
        .collect();
    let baseline: Vec<[f64; 3]> = (0..other.t.len())
        .map(|i| [cols[0][i], cols[1][i], cols[2][i]])
        .collect();
    let q: Vec<_> = other
        .t
        .iter()
        .map(|&t| {
            let i = o4core::dsp::searchsorted_left(&sorted, t);
            assert!(i > 0 && i < sorted.len());
            assert!(sorted[i] - sorted[i - 1] < 0.011);
            quality[order[i]].min(quality[order[i - 1]])
        })
        .collect();
    std::fs::write(
        &a[3],
        serde_json::to_vec(
            &json!({"intervals":intervals,"t":other.t,"baseline":baseline,"baseline_quality":q,"candidate":other.omega,"candidate_quality":other.quality,"frame_gap":2,"note":"baseline linearly resampled to two-frame midpoint times; quality minimum of bracketing pair"}),
        )?,
    )?;
    Ok(())
}
