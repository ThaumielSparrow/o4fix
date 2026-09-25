//! Remeasure exported panels, rather than infer quality from the commanded correction.
#[allow(dead_code)]
#[path = "local_render_smooth.rs"]
mod smooth;
use opencv::{
    core::{Mat, Rect},
    imgproc,
    prelude::*,
    videoio,
};
use serde_json::json;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 5 {
        return Err("usage: local_render_measure COMPARISON START BURSTS_JSON OUTPUT_JSON".into());
    }
    let start: f64 = a[2].parse()?;
    let bursts: Vec<(f64, f64)> = serde_json::from_str(&std::fs::read_to_string(&a[3])?)?;
    let mut cap = videoio::VideoCapture::from_file(&a[1], videoio::CAP_ANY)?;
    if !cap.is_opened()? {
        return Err("cannot open comparison".into());
    }
    let fps = cap.get(videoio::CAP_PROP_FPS)?;
    if (fps - 100.).abs() > 1e-6 {
        return Err("wrong cadence".into());
    }
    let mut prev: Vec<Option<Mat>> = vec![None, None, None];
    let mut rates = [vec![], vec![], vec![]];
    let mut quality = [vec![], vec![], vec![]];
    let mut frame = Mat::default();
    while cap.read(&mut frame)? {
        for k in 0..3 {
            let roi = Mat::roi(&frame, Rect::new(k as i32 * 720, 0, 720, 405))?;
            let mut gray = Mat::default();
            imgproc::cvt_color_def(&roi, &mut gray, imgproc::COLOR_BGR2GRAY)?;
            if let Some(p) = &prev[k] {
                let d = smooth::pair(p, &gray)?;
                rates[k].push([d.x * fps, d.y * fps, d.a.to_degrees() * fps]);
                quality[k].push(d.q);
            }
            prev[k] = Some(gray);
        }
    }
    let n = rates[0].len();
    if n < 100 {
        return Err("too short".into());
    }
    // Shared reliable support, discarding quarter-second neighborhoods of failed fits.
    let valid: Vec<bool> = (0..n)
        .map(|i| {
            i >= 50
                && i + 50 < n
                && (i - 25..=i + 25).all(|j| quality.iter().all(|q| q[j] >= 0.3))
                && smooth::gate(start + (i as f64 + 0.5) / fps, &bursts) > 0.9
        })
        .collect();
    let count = valid.iter().filter(|&&v| v).count();
    if count < 30 {
        return Err("insufficient common reliable active support".into());
    }
    let mut panels = vec![];
    for k in 0..3 {
        let mut bands = vec![];
        for (lo, hi) in [(2., 8.), (8., 30.)] {
            let filtered = o4core::dsp::filtfilt3(
                &o4core::dsp::butter_band(2, lo / (fps / 2.), hi / (fps / 2.)),
                &rates[k],
            );
            let mut xy = 0.;
            let mut roll = 0.;
            for (i, r) in filtered.iter().enumerate() {
                if valid[i] {
                    xy += r[0] * r[0] + r[1] * r[1];
                    roll += r[2] * r[2];
                }
            }
            bands.push(json!({"hz":[lo,hi],"translation_rms_half_px_s":(xy/count as f64).sqrt(),"roll_rms_deg_s":(roll/count as f64).sqrt()}));
        }
        panels.push(json!({"panel":k,"bad_pairs":quality[k].iter().filter(|&&q|q<0.3).count(),"bands":bands}));
    }
    std::fs::write(
        &a[4],
        serde_json::to_vec_pretty(
            &json!({"window_start":start,"common_active_pairs":count,"panels":panels,"note":"pixel-motion proxy, not ground truth; common 8% zoom; confidence neighborhoods and 0.5s edges excluded; no low-frequency pan claim"}),
        )?,
    )?;
    Ok(())
}
