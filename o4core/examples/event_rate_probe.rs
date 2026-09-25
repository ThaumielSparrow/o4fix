//! Read-only event-level gyro diagnostics for additional validation clips.
use o4core::{config::Config, detect, dsp, pipeline, quat, telemetry};
use serde_json::json;
use std::path::Path;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 6 {
        return Err("usage: event_rate_probe SOURCE BASELINE CANDIDATE WINDOWS_JSON OUTPUT".into());
    }
    let source = telemetry::extract_quats(Path::new(&a[1]))?;
    let base = telemetry::extract_quats(Path::new(&a[2]))?;
    let candidate = telemetry::extract_quats(Path::new(&a[3]))?;
    if source.t != base.t || source.t != candidate.t {
        return Err("timestamp mismatch".into());
    }
    let windows: Vec<[f64; 2]> = serde_json::from_str(&std::fs::read_to_string(&a[4])?)?;
    let (tm, raw) = quat::quats_to_rates(&source.t, &source.q);
    let (_, br) = quat::quats_to_rates(&base.t, &base.q);
    let (_, cr) = quat::quats_to_rates(&candidate.t, &candidate.q);
    let fs = pipeline::fs(&source.t);
    let (_, diag) = detect::adaptive_clean(&raw, fs, &Config::default());
    let bands: Vec<_> = [(2., 8.), (8., 30.)]
        .iter()
        .map(|&(lo, hi)| {
            let filter = dsp::butter_band(2, lo / (fs / 2.), hi / (fs / 2.));
            [dsp::filtfilt3(&filter, &br), dsp::filtfilt3(&filter, &cr)]
        })
        .collect();
    let mut rows = vec![];
    for [a, b] in windows {
        let ii: Vec<_> = (0..tm.len())
            .filter(|&i| tm[i] >= a && tm[i] <= b)
            .collect();
        if ii.is_empty() {
            return Err("empty window".into());
        }
        let n = ii.len() as f64;
        let delta = ii
            .iter()
            .map(|&i| {
                quat::qlog(quat::qmul(quat::qconj(base.q[i]), candidate.q[i]))
                    .iter()
                    .map(|v| v * v)
                    .sum::<f64>()
                    .sqrt()
                    .to_degrees()
            })
            .fold(0., f64::max);
        let band_rows: Vec<_> = bands
            .iter()
            .map(|pair| {
                pair.iter()
                    .map(|r| {
                        (ii.iter()
                            .map(|&i| r[i].iter().map(|v| v.to_degrees().powi(2)).sum::<f64>())
                            .sum::<f64>()
                            / n)
                            .sqrt()
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        rows.push(json!({"window":[a,b],"noise_peak_deg_s":ii.iter().map(|&i|diag.noise[i]).fold(0.,f64::max),"fraction_noise_above_severe":ii.iter().filter(|&&i|diag.noise[i]>8.).count()as f64/n,"max_baseline_to_candidate_orientation_deg":delta,"rate_rms_change_deg_s":(ii.iter().map(|&i|(0..3).map(|k|(br[i][k]-cr[i][k]).to_degrees().powi(2)).sum::<f64>()).sum::<f64>()/n).sqrt(),"band_rms_deg_s_baseline_candidate":band_rows}));
    }
    std::fs::write(
        &a[5],
        serde_json::to_vec_pretty(
            &json!({"bands_hz":[[2,8],[8,30]],"note":"Stored gyro motion magnitudes, not physical motion error or render-quality scores. Full-clip filtering context. Raw detector is 30-180 Hz band RMS.","windows":rows}),
        )?,
    )?;
    Ok(())
}
