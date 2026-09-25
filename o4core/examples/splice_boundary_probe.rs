//! Local derivative audit of the verified repaired MP4s; no optical fitting.
use o4core::{quat, telemetry};
use serde_json::json;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 5 {
        return Err("usage: splice_boundary_probe BASELINE CANDIDATE EXPOSURE_JSON OUTPUT".into());
    }
    let b = telemetry::extract_quats(std::path::Path::new(&a[1]))?;
    let c = telemetry::extract_quats(std::path::Path::new(&a[2]))?;
    if b.t != c.t {
        return Err("timestamp mismatch".into());
    }
    let (tm, br) = quat::quats_to_rates(&b.t, &b.q);
    let (_, cr) = quat::quats_to_rates(&c.t, &c.q);
    let exposure: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&a[3])?)?;
    let mut rows = vec![];
    for burst in exposure["bursts"].as_array().ok_or("missing bursts")? {
        let start = burst["snapped_interval"][0].as_f64().ok_or("start")?;
        let end = burst["snapped_interval"][1].as_f64().ok_or("end")?;
        let mut variants = vec![];
        for rates in [&br, &cr] {
            let mut acc = vec![];
            let mut speed = vec![];
            for i in 1..tm.len() {
                if (tm[i] >= start - 0.01 && tm[i] <= start + 0.31)
                    || (tm[i] >= end - 0.31 && tm[i] <= end + 0.01)
                {
                    speed.push(
                        rates[i]
                            .iter()
                            .map(|v| v.to_degrees().powi(2))
                            .sum::<f64>()
                            .sqrt(),
                    );
                    acc.push(
                        (0..3)
                            .map(|k| {
                                ((rates[i][k] - rates[i - 1][k]) / (tm[i] - tm[i - 1]))
                                    .to_degrees()
                                    .powi(2)
                            })
                            .sum::<f64>()
                            .sqrt(),
                    );
                }
            }
            acc.sort_by(f64::total_cmp);
            variants.push(json!({"edge_peak_speed_deg_s":speed.into_iter().fold(0.,f64::max),"edge_acceleration_rms_deg_s2":(acc.iter().map(|v|v*v).sum::<f64>()/acc.len() as f64).sqrt(),"edge_acceleration_p99_deg_s2":acc[(acc.len()-1)*99/100],"edge_acceleration_peak_deg_s2":acc.last()}));
        }
        rows.push(json!({"interval":[start,end],"baseline":variants[0],"candidate":variants[1]}));
    }
    std::fs::write(
        &a[4],
        serde_json::to_vec_pretty(
            &json!({"note":"Derivatives of stored quaternions in 0.31s inner / 0.01s outer edge windows, not ground-truth error. Stored float precision affects acceleration.","bursts":rows}),
        )?,
    )?;
    Ok(())
}
