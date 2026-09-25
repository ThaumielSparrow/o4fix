//! Post-burst gyro/optical consistency; optical is a proxy, not motion truth.
use o4core::{config::Config, detect, dsp, optical, pipeline, quat, telemetry};
use serde_json::{json, Value};
use std::{path::Path, sync::atomic::AtomicBool};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 4 {
        return Err("usage: settling_probe SOURCE CALIBRATION OUTPUT".into());
    }
    let tel = telemetry::extract_quats(Path::new(&a[1]))?;
    let cfg = Config::default();
    let fs = pipeline::fs(&tel.t);
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
    let cal: Value = serde_json::from_str(&std::fs::read_to_string(&a[2])?)?;
    let shift = cal["baseline"]["shift_ms"].as_f64().ok_or("shift")? / 1000.;
    let matrix: [[f64; 3]; 3] = serde_json::from_value(cal["baseline"]["matrix"].clone())?;
    let raw_lp = dsp::filtfilt3(&dsp::butter_low(2, 8. / (fs / 2.)), &raw);
    let ends = [
        106.61010759764649,
        226.17924825294278,
        249.3159796205548,
        309.27912198448456,
        19.0,
    ];
    let mut results = vec![];
    for (wi, end) in ends.iter().enumerate() {
        let duration = if wi == 4 { 3. } else { 10. };
        let opt = optical::video_rates(
            Path::new(&a[1]),
            &[(*end - 0.5, *end + duration + 0.5)],
            &tel.meta,
            &AtomicBool::new(false),
            &|d, n| println!("window {wi}: {d}/{n}"),
        )?;
        let mapped: Vec<[f64; 3]> = opt
            .omega
            .iter()
            .map(|r| {
                std::array::from_fn(|k| (0..3).map(|j| r[j].to_degrees() * matrix[j][k]).sum())
            })
            .collect();
        let filtered = dsp::filtfilt3(&dsp::butter_low(2, 8. / 50.), &mapped);
        let query: Vec<_> = opt.t.iter().map(|v| v + shift).collect();
        let cols: Vec<_> = (0..3)
            .map(|k| {
                dsp::interp(
                    &query,
                    &tm,
                    &raw_lp.iter().map(|r| r[k].to_degrees()).collect::<Vec<_>>(),
                )
            })
            .collect();
        let noise = dsp::interp(&query, &tm, &diag.noise);
        let mut rows = vec![];
        for sec in 0..duration as usize {
            let mut residuals: Vec<[f64; 3]> = vec![];
            let mut excluded = 0;
            for i in 25..query.len().saturating_sub(25) {
                if query[i] < end + sec as f64 || query[i] >= end + sec as f64 + 1. {
                    continue;
                }
                if intervals
                    .iter()
                    .any(|&(s, e)| query[i] >= s && query[i] <= e)
                    || noise[i] > 8.
                    || opt.quality[i - 25..=i + 25].iter().any(|&q| q < 0.3)
                {
                    excluded += 1;
                    continue;
                }
                residuals.push(std::array::from_fn(|k| cols[k][i] - filtered[i][k]));
            }
            let count = residuals.len();
            let rms = if count == 0 {
                None
            } else {
                Some((residuals.iter().flatten().map(|v| v * v).sum::<f64>() / count as f64).sqrt())
            };
            let mean: Option<[f64; 3]> = if count == 0 {
                None
            } else {
                Some(std::array::from_fn(|k| {
                    residuals.iter().map(|r| r[k]).sum::<f64>() / count as f64
                }))
            };
            rows.push(json!({"seconds_after_end":sec,"accepted_pairs":count,"excluded_pairs":excluded,"vector_residual_rms_deg_s":rms,"mean_residual_deg_s":mean}));
        }
        results.push(json!({"end":end,"is_clean_control":wi==4,"bins":rows}));
    }
    std::fs::write(
        &a[3],
        serde_json::to_vec_pretty(
            &json!({"note":"Raw body rates and calibrated optical rates both LP8; saved production alignment, 0.5s tracking context. Excludes padded severe intervals and +/-0.25s optical low-quality neighborhoods. Optical/model error remains confounded; no proof of fusion settling.","windows":results}),
        )?,
    )?;
    Ok(())
}
