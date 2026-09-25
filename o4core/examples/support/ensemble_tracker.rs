//! Equal SO(3) midpoint of disjoint subset fits; existing full-fit fallback and quality.
#[path = "subset_tracker.rs"]
mod tracker;
use o4core::{error::O4Error, quat, telemetry::Meta};
use opencv::{prelude::*, videoio};
use std::{path::Path, sync::atomic::AtomicBool};
pub use tracker::OpticalRates;
#[allow(clippy::too_many_arguments)]
pub fn video_rates(
    video_path: &Path,
    intervals: &[(f64, f64)],
    meta: &Meta,
    cancel: &AtomicBool,
    on_interval: &(dyn Fn(usize, usize) + Sync),
    fb_gate: bool,
    seed_delta: i32,
    frame_gap: usize,
) -> Result<OpticalRates, O4Error> {
    let cap = videoio::VideoCapture::from_file(video_path.to_str().unwrap(), videoio::CAP_ANY)?;
    let fps = cap.get(videoio::CAP_PROP_FPS)?;
    if !fps.is_finite() || (fps - 100.).abs() > 1e-6 {
        return Err(O4Error::Cv(
            "subset ensemble research requires 100fps".into(),
        ));
    }
    drop(cap);
    let mut r = tracker::video_rates(
        video_path,
        intervals,
        meta,
        cancel,
        on_interval,
        fb_gate,
        seed_delta,
        frame_gap,
    )?;
    let mut averaged = 0;
    for i in 0..r.t.len() {
        let h = &r.diagnostics[i]["subset"]["halves"];
        if h[0]["rotation_rad"].is_array() && h[1]["rotation_rad"].is_array() {
            let a: [f64; 3] = std::array::from_fn(|k| h[0]["rotation_rad"][k].as_f64().unwrap());
            let b: [f64; 3] = std::array::from_fn(|k| h[1]["rotation_rad"][k].as_f64().unwrap());
            r.omega[i] = quat::qlog(quat::slerp(quat::qexp(a), quat::qexp(b), 0.5))
                .map(|x| x * fps / frame_gap as f64);
            averaged += 1;
        }
    }
    println!(
        "subset average {averaged}/{} pairs; {} full-fit fallbacks",
        r.t.len(),
        r.t.len() - averaged
    );
    Ok(r)
}
