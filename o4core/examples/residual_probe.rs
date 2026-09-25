//! Read-only residual localization; no stabilization parameters are changed.
use o4core::{config::Config, detect, dsp, optical, pipeline, quat, telemetry};
use serde_json::json;
use std::{io::Write, path::PathBuf, sync::atomic::AtomicBool};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("usage: residual_probe ORIGINAL PRESERVED_FIXED OUTPUT_DIRECTORY".into());
    }
    let out = PathBuf::from(&args[3]);
    std::fs::create_dir_all(&out)?;
    let source = telemetry::extract_quats(std::path::Path::new(&args[1]))?;
    let fixed = telemetry::extract_quats(std::path::Path::new(&args[2]))?;
    if source.t != fixed.t || source.q.len() != fixed.q.len() {
        return Err("reference timestamps/length mismatch".into());
    }
    let fs = pipeline::fs(&source.t);
    let cfg = Config::default();
    let (tm, raw) = quat::quats_to_rates(&source.t, &source.q);
    let (_, repaired) = quat::quats_to_rates(&fixed.t, &fixed.q);
    let (_, diag) = detect::adaptive_clean(&raw, fs, &cfg);
    let severe = detect::find_intervals(
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
    let noisy = detect::find_intervals(
        &diag.alpha.iter().map(|&a| a > 0.15).collect::<Vec<_>>(),
        &tm,
        cfg.patch_pad,
        cfg.patch_merge,
        0.2,
    );
    let windows = [
        (102., 110.),
        (221., 229.),
        (245., 253.),
        (304., 312.),
        (117., 128.),
    ];
    let opt = optical::video_rates(
        std::path::Path::new(&args[1]),
        &windows,
        &source.meta,
        &AtomicBool::new(false),
        &|d, n| println!("tracking {d}/{n}"),
    )?;
    std::fs::write(
        out.join("optical.json"),
        serde_json::to_vec(&json!({"t":opt.t,"omega":opt.omega,"quality":opt.quality}))?,
    )?;
    let raw_lp = dsp::filtfilt3(&dsp::butter_low(2, 8.0 / (fs / 2.0)), &raw);
    let fixed_lp = dsp::filtfilt3(&dsp::butter_low(2, 8.0 / (fs / 2.0)), &repaired);
    let mut csv = std::io::BufWriter::new(std::fs::File::create(out.join("telemetry.csv"))?);
    writeln!(csv,"t,noise_deg_s,alpha,raw_lp8_x,raw_lp8_y,raw_lp8_z,fixed_lp8_x,fixed_lp8_y,fixed_lp8_z,source_to_fixed_angle_deg")?;
    for i in (0..tm.len()).step_by(10) {
        if windows.iter().any(|&(a, b)| tm[i] >= a && tm[i] <= b) {
            let rel = quat::qlog(quat::qmul(quat::qconj(source.q[i]), fixed.q[i]));
            let angle = rel.iter().map(|v| v * v).sum::<f64>().sqrt().to_degrees();
            writeln!(
                csv,
                "{},{},{},{},{},{},{},{},{},{}",
                tm[i],
                diag.noise[i],
                diag.alpha[i],
                raw_lp[i][0].to_degrees(),
                raw_lp[i][1].to_degrees(),
                raw_lp[i][2].to_degrees(),
                fixed_lp[i][0].to_degrees(),
                fixed_lp[i][1].to_degrees(),
                fixed_lp[i][2].to_degrees(),
                angle
            )?;
        }
    }
    let mut summaries = vec![];
    for &(a, b) in &windows {
        let indices: Vec<_> = (0..opt.t.len())
            .filter(|&i| opt.t[i] >= a && opt.t[i] <= b)
            .collect();
        let mut gaps = vec![];
        let mut begin = None;
        for &i in &indices {
            if opt.quality[i] < 0.3 {
                begin.get_or_insert(opt.t[i]);
            } else if let Some(start) = begin.take() {
                gaps.push((start, opt.t[i]));
            }
        }
        if let Some(start) = begin {
            gaps.push((start, opt.t[*indices.last().unwrap()] + 0.01));
        }
        let nearby: Vec<_> = severe.iter().filter(|&&(s, e)| e >= a && s <= b).collect();
        let patches:Vec<_>=noisy.iter().filter(|&&(s,e)|e>=a&&s<=b).map(|&(s,e)|{let peak=tm.iter().zip(&diag.noise).filter(|(&t,_)|t>=s&&t<=e).map(|(_,n)|*n).fold(0.0,f64::max);json!({"start":s,"end":e,"peak_noise":peak,"gyro_trust":(1.0-(peak-200.)/100.).clamp(0.,1.)})}).collect();
        summaries.push(json!({"window":[a,b],"samples":indices.len(),"bad_fraction":indices.iter().filter(|&&i|opt.quality[i]<0.3).count() as f64/indices.len() as f64,"longest_bad_seconds":gaps.iter().map(|(s,e)|e-s).fold(0.,f64::max),"bad_runs":gaps,"severe_intervals":nearby,"optical_intervals":patches}));
    }
    std::fs::write(
        out.join("summary.json"),
        serde_json::to_vec_pretty(
            &json!({"windows":summaries,"config":format!("{cfg:?}"),"note":"source-to-fixed orientation difference is not Gyroflow stabilization correction; LP8 rates are diagnostic only"}),
        )?,
    )?;
    Ok(())
}
