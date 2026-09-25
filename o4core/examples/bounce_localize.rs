//! Read-only time localization; apparent image motion and intended camera motion retain separate units.
#[allow(dead_code, clippy::needless_range_loop)]
#[path = "local_render_smooth.rs"]
mod tracking;
use o4core::{dsp, quat};
use opencv::{
    core::{Mat, Rect},
    imgproc,
    prelude::*,
    videoio,
};
use serde_json::{json, Value};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 6 {
        return Err(
            "usage: bounce_localize COMPARISON START CAMERA_JSON STAGES_JSON OUTPUT".into(),
        );
    }
    let start: f64 = a[2].parse()?;
    let camera: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(&a[3])?)?;
    let stages: Value = serde_json::from_str(&std::fs::read_to_string(&a[4])?)?;
    let times: Vec<f64> = camera
        .iter()
        .map(|r| r["timestamp_ms"].as_f64().unwrap() / 1000.)
        .collect();
    let quats: Vec<[f64; 4]> = camera
        .iter()
        .map(|r| serde_json::from_value(r["stab_quat"].clone()).unwrap())
        .collect();
    let (qt, qr) = quat::quats_to_rates(&times, &quats);
    let intended = dsp::filtfilt3(&dsp::butter_band(2, 2. / 50., 8. / 50.), &qr);
    let mut cap = videoio::VideoCapture::from_file(&a[1], videoio::CAP_ANY)?;
    if !cap.is_opened()? || (cap.get(videoio::CAP_PROP_FPS)? - 100.).abs() > 1e-6 {
        return Err("bad video cadence".into());
    }
    let mut previous: Option<Mat> = None;
    let mut frame = Mat::default();
    let mut observed = vec![];
    let mut quality = vec![];
    while cap.read(&mut frame)? {
        let roi = Mat::roi(&frame, Rect::new(720, 0, 720, 405))?;
        let mut gray = Mat::default();
        imgproc::cvt_color_def(&roi, &mut gray, imgproc::COLOR_BGR2GRAY)?;
        if let Some(ref p) = previous {
            let motion = tracking::pair(p, &gray)?;
            observed.push([
                motion.x * 100.,
                motion.y * 100.,
                motion.a.to_degrees() * 100.,
            ]);
            quality.push(motion.q);
        }
        previous = Some(gray);
    }
    let n = observed.len();
    if n < 200 {
        return Err("too short".into());
    }
    let low = dsp::filtfilt3(&dsp::butter_band(2, 2. / 50., 8. / 50.), &observed);
    let high = dsp::filtfilt3(&dsp::butter_band(2, 8. / 50., 30. / 50.), &observed);
    let mut rows = vec![];
    for i in 50..n - 50 {
        let t = start + (i as f64 + 0.5) / 100.;
        let mut phase = "outside";
        for b in stages["bursts"].as_array().ok_or("bursts")? {
            let lo = b["interval"][0].as_f64().ok_or("start")?;
            let hi = b["interval"][1].as_f64().ok_or("end")?;
            if t >= lo && t <= hi {
                phase = if t - lo < 0.19 || hi - t < 0.19 {
                    "edge"
                } else if b["rebased"].as_bool().unwrap() {
                    "rebased_interior"
                } else {
                    "bridged_interior"
                };
                break;
            }
        }
        let j = dsp::searchsorted_left(&qt, t).min(qt.len() - 1);
        rows.push(json!({"t":t,"phase":phase,"reliable":quality[i-25..=i+25].iter().all(|&q|q>=0.3),"image_low":low[i],"image_high":high[i],"intended_low_deg_s":intended[j].map(f64::to_degrees)}));
    }
    std::fs::write(
        &a[5],
        serde_json::to_vec(
            &json!({"note":"Right accepted panel only. Image center apparent velocity: half-res px/s x,y and deg/s roll. Intended camera body rotation in deg/s, not subtracted or mapped to pixel motion. High-frequency image motion includes parallax/model error. 0.5s filter edges and +/-0.25s low-quality neighborhoods excluded. Phase edges approximate by saved detection times within one gyro sample.","rows":rows}),
        )?,
    )?;
    Ok(())
}
