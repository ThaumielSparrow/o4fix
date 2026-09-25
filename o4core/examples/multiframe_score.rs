//! Frozen-calibration, common-support multi-frame scoring. No refit.
use o4core::{
    alignment::CalibrationData,
    config::Config,
    detect,
    optical::{Alignment, OpticalRates},
    pipeline, quat, telemetry,
};
use serde_json::{json, Value};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 5 {
        return Err(
            "usage: multiframe_score VIDEO CALIBRATION_JSON MULTIFRAME_JSON OUTPUT_JSON".into(),
        );
    }
    let cal: Value = serde_json::from_str(&std::fs::read_to_string(&a[2])?)?;
    let obs: Value = serde_json::from_str(&std::fs::read_to_string(&a[3])?)?;
    let tel = telemetry::extract_quats(std::path::Path::new(&a[1]))?;
    let fs = pipeline::fs(&tel.t);
    let (tm, om) = quat::quats_to_rates(&tel.t, &tel.q);
    let (clean, _) = detect::adaptive_clean(&om, fs, &Config::default());
    let gyro: Vec<_> = clean.iter().map(|r| r.map(f64::to_degrees)).collect();
    let mut folds = vec![];
    for fold in cal["held_out"].as_array().unwrap() {
        let start = fold["interval"][0].as_f64().unwrap();
        let end = fold["interval"][1].as_f64().unwrap();
        let rows: Vec<_> = obs["rows"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| {
                r["t"].as_f64().unwrap() >= start - 0.02 && r["t"].as_f64().unwrap() <= end + 0.02
            })
            .collect();
        if rows.is_empty() {
            continue;
        }
        let align = Alignment {
            shift: fold["baseline"]["shift_ms"].as_f64().unwrap() / 1000.,
            n: serde_json::from_value(fold["baseline"]["matrix"].clone())?,
            r2: 0.,
        };
        let mut checks = vec![];
        for require_convergence in [false, true] {
            for reference in [1, 3] {
                let valid = |r: &&Value| {
                    !r["fit"].is_null()
                        && !r["reference_models"][reference].is_null()
                        && (!require_convergence || r["fit"]["converged"].as_bool().unwrap())
                };
                let mut scores = vec![];
                for candidate in [false, true] {
                    let opt = OpticalRates {
                        t: rows.iter().map(|r| r["t"].as_f64().unwrap()).collect(),
                        omega: rows
                            .iter()
                            .map(|r| {
                                if valid(r) {
                                    serde_json::from_value(if candidate {
                                        r["fit"]["omega"].clone()
                                    } else {
                                        r["reference_models"][reference]["omega"].clone()
                                    })
                                    .unwrap()
                                } else {
                                    [0.; 3]
                                }
                            })
                            .collect(),
                        quality: rows
                            .iter()
                            .map(|r| if valid(r) { 1. } else { 0. })
                            .collect(),
                    };
                    scores.push(CalibrationData::prepare(&opt,&tm,&gyro,fs).and_then(|d|d.score(&align)).map(|s|json!({"rms_deg_s":s.rms_deg_s,"samples":s.samples,"runs":s.runs})));
                }
                checks.push(json!({"reference_index":reference,"require_convergence":require_convergence,"common_pairs":rows.iter().filter(|r|valid(r)).count(),"reference":scores[0],"candidate":scores[1]}));
            }
        }
        folds.push(json!({"interval":[start,end],"pairs":rows.len(),"checks":checks}));
    }
    std::fs::write(&a[4], serde_json::to_vec_pretty(&json!({"folds":folds}))?)?;
    Ok(())
}
