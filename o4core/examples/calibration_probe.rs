//! Read-only acquisition and leave-one-interval-out calibration experiment.
use o4core::{
    alignment::CalibrationData,
    config::Config,
    detect,
    optical::{self, Alignment, OpticalRates},
    patch, pipeline, quat, telemetry,
};
use serde_json::{json, Value};
use std::{path::PathBuf, sync::atomic::AtomicBool};

fn select(opt: &OpticalRates, interval: (f64, f64), inside: bool) -> OpticalRates {
    let indices: Vec<usize> = (0..opt.t.len())
        .filter(|&i| ((opt.t[i] >= interval.0 - 0.02) && (opt.t[i] <= interval.1 + 0.02)) == inside)
        .collect();
    OpticalRates {
        t: indices.iter().map(|&i| opt.t[i]).collect(),
        omega: indices.iter().map(|&i| opt.omega[i]).collect(),
        quality: indices.iter().map(|&i| opt.quality[i]).collect(),
    }
}
fn summarize(fit: Option<&Alignment>, data: Option<&CalibrationData>) -> Value {
    match fit {
        None => Value::Null,
        Some(f) => json!({"shift_ms":f.shift*1000.0,"matrix":f.n,"training_r2":f.r2,
        "score":data.and_then(|d|d.score(f)).map(|s|json!({"rms_deg_s":s.rms_deg_s,"r2":s.r2,"samples":s.samples,"runs":s.runs}))}),
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 {
        return Err("usage: calibration_probe VIDEO OUTPUT_DIRECTORY".into());
    }
    let video = PathBuf::from(&args[1]);
    let out = PathBuf::from(&args[2]);
    std::fs::create_dir_all(&out)?;
    let tel = telemetry::extract_quats(&video)?;
    let fs = pipeline::fs(&tel.t);
    let (tm, om) = quat::quats_to_rates(&tel.t, &tel.q);
    let cfg = Config::default();
    let (clean, diag) = detect::adaptive_clean(&om, fs, &cfg);
    let intervals = patch::calibration_intervals(&tm, &clean, &diag);
    let opt = optical::video_rates(
        &video,
        &intervals,
        &tel.meta,
        &AtomicBool::new(false),
        &|done, total| println!("calibration {done}/{total}"),
    )?;
    let gyro: Vec<[f64; 3]> = clean.iter().map(|r| r.map(f64::to_degrees)).collect();
    let data = CalibrationData::prepare(&opt, &tm, &gyro, fs);
    let old = optical::fit_video_alignment(&opt, &tm, &gyro, fs);
    let new = data.as_ref().and_then(CalibrationData::fit);
    let mut folds = Vec::new();
    for &interval in &intervals {
        let train = select(&opt, interval, false);
        let test = select(&opt, interval, true);
        let held = CalibrationData::prepare(&test, &tm, &gyro, fs);
        let baseline = optical::fit_video_alignment(&train, &tm, &gyro, fs);
        let candidate = CalibrationData::prepare(&train, &tm, &gyro, fs).and_then(|d| d.fit());
        folds.push(json!({"interval":interval,"baseline":summarize(baseline.as_ref(),held.as_ref()),"candidate":summarize(candidate.as_ref(),held.as_ref())}));
    }
    let report = json!({"video":video.canonicalize()?.display().to_string(),"config":format!("{cfg:?}"),"intervals":intervals,
        "baseline":summarize(old.as_ref(),data.as_ref()),"candidate":summarize(new.as_ref(),data.as_ref()),"held_out":folds});
    std::fs::write(
        out.join("calibration.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    std::fs::write(
        out.join("observations.json"),
        serde_json::to_vec(&json!({"t":opt.t,"omega":opt.omega,"quality":opt.quality}))?,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
