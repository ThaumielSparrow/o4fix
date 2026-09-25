//! Read-only exact splice-weight audit using previously validated cached traces.
use o4core::{dsp, quat, telemetry};
use serde_json::{json, Value};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 5 {
        return Err("usage: residual_exposure_probe SOURCE TRACE STAGES NEW_OUTPUT".into());
    }
    if Path::new(&a[4]).exists() {
        return Err("output already exists".into());
    }
    let tel = telemetry::extract_quats(Path::new(&a[1]))?;
    let trace: Value = serde_json::from_slice(&std::fs::read(&a[2])?)?;
    let stages: Value = serde_json::from_slice(&std::fs::read(&a[3])?)?;
    let mut bursts = vec![];
    for b in stages["bursts"].as_array().ok_or("bursts")? {
        let lo = b["interval"][0].as_f64().ok_or("start")?;
        let hi = b["interval"][1].as_f64().ok_or("end")?;
        let i0 = dsp::searchsorted_left(&tel.t, lo);
        let i1 = dsp::searchsorted_right(&tel.t, hi)
            .saturating_sub(1)
            .min(tel.t.len() - 1);
        bursts.push((i0, i1, b["rebased"].as_bool().ok_or("rebase")?));
    }
    let mut rows = vec![];
    for r in trace["rows"].as_array().ok_or("rows")? {
        let i = r["index"].as_u64().ok_or("index")? as usize;
        let t = r["t"].as_f64().ok_or("time")?;
        if (t - (tel.t[i] + tel.t[i + 1]) / 2.).abs() > 1e-9 {
            return Err("cache/source timestamp mismatch".into());
        }
        let mut phase = "outside";
        let mut raw_weight = [1., 1.];
        let mut interior_margin = 0.;
        for &(i0, i1, rebased) in &bursts {
            if i >= i0 && i < i1 {
                raw_weight = [i, i + 1].map(|k| {
                    1. - quat::smoothstep((tel.t[k] - tel.t[i0]) / 0.19)
                        .min(quat::smoothstep((tel.t[i1] - tel.t[k]) / 0.19))
                });
                phase = if raw_weight.iter().any(|&w| w > 0.) {
                    "edge"
                } else if rebased {
                    "rebased_interior"
                } else {
                    "bridged_interior"
                };
                interior_margin =
                    (tel.t[i] - tel.t[i0] - 0.19).min(tel.t[i1] - tel.t[i + 1] - 0.19);
                break;
            }
        }
        let optical = r["optical_weight"].as_f64().ok_or("optical weight")?;
        let hb = r["handback"].as_f64().ok_or("handback")?;
        let diff: f64 = (0..3)
            .map(|k| {
                let d =
                    r["final_deg_s"][k].as_f64().unwrap() - r["optical_deg_s"][k].as_f64().unwrap();
                d * d
            })
            .sum();
        rows.push(json!({"t":t,"phase":phase,"raw_slerp_weights":raw_weight,
            "interior_margin_s":interior_margin,"noise":r["noise"],
            "patch_gyro_weight":1.-optical+optical*hb,
            "light_partner_weight":1.-optical,"medium_handback_weight":optical*hb,
            "patch_minus_optical_norm_deg_s":diff.sqrt()}));
    }
    std::fs::write(
        &a[4],
        serde_json::to_vec(&json!({"rows":rows,
        "bursts":bursts.iter().map(|&(i,j,r)|json!({"snapped":[tel.t[i],tel.t[j]],"rebased":r})).collect::<Vec<_>>(),
        "note":"Both endpoint SLERP weights are exact; they are not angular-rate fractions. Patch gyro weight is the light/medium rate mixture coefficient. Interior margin measures distance from edge ramps; filtering/render smoothing can have longer influence. Cached timestamps checked against source."}))?,
    )?;
    Ok(())
}
