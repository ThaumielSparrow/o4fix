//! Approximate diagnostic from cached window observations; no repair output.
use o4core::{config::Config, detect, dsp, pipeline, quat, telemetry};
use serde_json::{json, Value};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 6 {
        return Err("usage: handback_probe SOURCE OBSERVATIONS CALIBRATION BURSTS OUTPUT".into());
    }
    let obs: Value = serde_json::from_str(&std::fs::read_to_string(&a[2])?)?;
    let cal: Value = serde_json::from_str(&std::fs::read_to_string(&a[3])?)?;
    let bursts: Vec<(f64, f64)> = serde_json::from_str(&std::fs::read_to_string(&a[4])?)?;
    let t: Vec<f64> = serde_json::from_value(obs["t"].clone())?;
    let rates: Vec<[f64; 3]> = serde_json::from_value(obs["baseline"].clone())?;
    let n: [[f64; 3]; 3] = serde_json::from_value(cal["baseline"]["matrix"].clone())?;
    let shift = cal["baseline"]["shift_ms"].as_f64().unwrap() / 1000.;
    let tel = telemetry::extract_quats(std::path::Path::new(&a[1]))?;
    let fs = pipeline::fs(&tel.t);
    let (tm, om) = quat::quats_to_rates(&tel.t, &tel.q);
    let (_, diag) = detect::adaptive_clean(&om, fs, &Config::default());
    let medium = dsp::filtfilt3(&dsp::butter_low(2, 8. / (fs / 2.)), &diag.light);
    let mag = dsp::uniform_filter1d(
        &medium
            .iter()
            .map(|r| r.iter().map(|v| v * v).sum::<f64>().sqrt())
            .collect::<Vec<_>>(),
        (0.1 * fs) as usize,
    );
    let mut rows = vec![];
    for &(start, end) in &[(102., 110.), (221., 229.), (245., 253.), (304., 312.)] {
        let ii: Vec<_> = (0..t.len())
            .filter(|&i| t[i] >= start && t[i] <= end)
            .collect();
        let tv: Vec<_> = ii.iter().map(|&i| t[i] + shift).collect();
        let v: Vec<[f64; 3]> = ii
            .iter()
            .map(|&i| {
                std::array::from_fn(|c| (0..3).map(|r| rates[i][r].to_degrees() * n[r][c]).sum())
            })
            .collect();
        let v = dsp::filtfilt3(&dsp::butter_low(2, 8. / 50.), &v);
        let jj: Vec<_> = (0..tm.len())
            .filter(|&i| tm[i] >= start && tm[i] <= end)
            .collect();
        let query: Vec<_> = jj.iter().map(|&i| tm[i]).collect();
        let cols: Vec<_> = (0..3)
            .map(|k| dsp::interp(&query, &tv, &v.iter().map(|r| r[k]).collect::<Vec<_>>()))
            .collect();
        let omag = dsp::uniform_filter1d(
            &(0..query.len())
                .map(|i| (0..3).map(|k| cols[k][i].powi(2)).sum::<f64>().sqrt())
                .collect::<Vec<_>>(),
            (0.1 * fs) as usize,
        );
        // Each selected burst's production optical segment has peak noise >300 deg/s (trust=0).
        let w = dsp::uniform_filter1d(
            &jj.iter()
                .enumerate()
                .map(|(j, &i)| ((mag[i].min(omag[j]) - 100.) / 150.).clamp(0., 1.))
                .collect::<Vec<_>>(),
            (0.15 * fs) as usize,
        );
        for &(b, e) in &bursts {
            if b < start || e > end {
                continue;
            }
            let values: Vec<_> = query
                .iter()
                .zip(&w)
                .filter(|(&t, _)| t >= b && t <= e)
                .map(|(_, v)| *v)
                .collect();
            rows.push(json!({"burst":[b,e],"mean_weight":values.iter().sum::<f64>()/values.len()as f64,"max_weight":values.iter().copied().fold(0.,f64::max),"fraction_above_half":values.iter().filter(|&&v|v>0.5).count()as f64/values.len()as f64}));
        }
    }
    std::fs::write(
        &a[5],
        serde_json::to_vec_pretty(
            &json!({"approximate":true,"note":"same default handback formula, cached eight-second optical context rather than exact production segment edges; all selected segments have zero gyro trust; no claim of causality","bursts":rows}),
        )?,
    )?;
    Ok(())
}
