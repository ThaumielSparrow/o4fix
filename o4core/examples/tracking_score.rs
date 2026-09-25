//! Score fixed held-out baseline calibrations on common accepted observations.
use o4core::{
    alignment::CalibrationData,
    config::Config,
    detect,
    optical::{Alignment, OpticalRates},
    pipeline, quat, telemetry,
};
use serde_json::{json, Value};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 5 {
        return Err(
            "usage: tracking_score VIDEO CALIBRATION_JSON OBSERVATIONS_JSON OUTPUT_JSON".into(),
        );
    }
    let cal: Value = serde_json::from_str(&std::fs::read_to_string(&args[2])?)?;
    let obs: Value = serde_json::from_str(&std::fs::read_to_string(&args[3])?)?;
    let tel = telemetry::extract_quats(std::path::Path::new(&args[1]))?;
    let fs = pipeline::fs(&tel.t);
    let (tm, om) = quat::quats_to_rates(&tel.t, &tel.q);
    let (clean, _) = detect::adaptive_clean(&om, fs, &Config::default());
    let gyro: Vec<_> = clean.iter().map(|r| r.map(f64::to_degrees)).collect();
    let t: Vec<f64> = serde_json::from_value(obs["t"].clone())?;
    let base: Vec<[f64; 3]> = serde_json::from_value(obs["baseline"].clone())?;
    let candidate: Vec<[f64; 3]> = serde_json::from_value(obs["candidate"].clone())?;
    let bq: Vec<f64> = serde_json::from_value(obs["baseline_quality"].clone())?;
    let cq: Vec<f64> = serde_json::from_value(obs["candidate_quality"].clone())?;
    let mut folds = vec![];
    for f in cal["held_out"].as_array().unwrap() {
        let a = f["interval"][0].as_f64().unwrap();
        let b = f["interval"][1].as_f64().unwrap();
        let fit = &f["baseline"];
        let al = Alignment {
            shift: fit["shift_ms"].as_f64().unwrap() / 1000.,
            n: serde_json::from_value(fit["matrix"].clone())?,
            r2: 0.,
        };
        let indices: Vec<_> = (0..t.len())
            .filter(|&i| t[i] >= a - 0.02 && t[i] <= b + 0.02)
            .collect();
        let mut scores = vec![];
        for rates in [&base, &candidate] {
            let data = OpticalRates {
                t: indices.iter().map(|&i| t[i]).collect(),
                omega: indices.iter().map(|&i| rates[i]).collect(),
                quality: indices
                    .iter()
                    .map(|&i| if bq[i] > 0.5 && cq[i] > 0.5 { 1. } else { 0. })
                    .collect(),
            };
            let score = CalibrationData::prepare(&data, &tm, &gyro, fs).and_then(|d| d.score(&al));
            scores
                .push(score.map(|s| json!({"rms":s.rms_deg_s,"samples":s.samples,"runs":s.runs})));
        }
        folds.push(json!({"interval":[a,b],"baseline":scores[0],"candidate":scores[1]}));
    }
    let mut windows = vec![];
    for interval in obs["intervals"].as_array().unwrap() {
        let a = interval[0].as_f64().unwrap();
        let b = interval[1].as_f64().unwrap();
        let ii: Vec<_> = (0..t.len()).filter(|&i| t[i] >= a && t[i] <= b).collect();
        let good: Vec<_> = ii
            .iter()
            .copied()
            .filter(|&i| bq[i] >= 0.3 && cq[i] >= 0.3)
            .collect();
        let rms = (good
            .iter()
            .map(|&i| {
                (0..3)
                    .map(|k| (base[i][k] - candidate[i][k]).to_degrees().powi(2))
                    .sum::<f64>()
            })
            .sum::<f64>()
            / good.len() as f64)
            .sqrt();
        windows.push(json!({"interval":interval,"samples":ii.len(),"baseline_bad":ii.iter().filter(|&&i|bq[i]<0.3).count(),"candidate_bad":ii.iter().filter(|&&i|cq[i]<0.3).count(),"raw_rate_difference_rms_deg_s":rms}));
    }
    std::fs::write(
        &args[4],
        serde_json::to_vec_pretty(&json!({"held_out_common_support":folds,"windows":windows}))?,
    )?;
    Ok(())
}
