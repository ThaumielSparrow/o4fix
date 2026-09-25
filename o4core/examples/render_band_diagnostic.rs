//! Band-specific regional transfer using saved triplet medians; diagnostic only.
use serde_json::{json, Value};
fn median(mut x: Vec<f64>) -> f64 {
    x.sort_by(f64::total_cmp);
    let n = x.len();
    if n % 2 == 0 {
        (x[n / 2 - 1] + x[n / 2]) * 0.5
    } else {
        x[n / 2]
    }
}
fn filtered(a: &[[f64; 2]], lo: f64, hi: f64) -> Vec<[f64; 2]> {
    let n = a.len();
    let mut c = vec![[0.; 2]; n];
    let z: f64 = (-24i32..=24)
        .map(|d| (-0.5 * (d as f64 / 8.).powi(2)).exp())
        .sum();
    for (i, v) in c.iter_mut().enumerate() {
        for d in 1i32..=24 {
            let w = (-0.5 * (d as f64 / 8.).powi(2)).exp() / z;
            for k in -d + 1..d {
                let j = i as i32 + k;
                if j >= 0 && j < n as i32 {
                    for (axis, value) in v.iter_mut().enumerate() {
                        *value += w * (d - k.abs()) as f64 * a[j as usize][axis];
                    }
                }
            }
        }
    }
    for k in 0..2 {
        let f = o4core::dsp::filtfilt(
            &o4core::dsp::butter_band(2, lo / 50., hi / 50.),
            &c.iter().map(|v| v[k]).collect::<Vec<_>>(),
        );
        for i in 0..n {
            c[i][k] = f[i];
        }
    }
    c
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 3 {
        return Err("usage: render_band_diagnostic PROBE OUTPUT".into());
    }
    if std::path::Path::new(&a[2]).exists() {
        return Err("exists".into());
    }
    let d: Value = serde_json::from_str(&std::fs::read_to_string(&a[1])?)?;
    let rows = d["rows"].as_array().ok_or("rows")?;
    let n = rows.len();
    let mut out = vec![];
    for (lo, hi) in [(2., 4.), (4., 8.), (8., 30.)] {
        let mut regions = vec![];
        for region in 0..9 {
            let mut target = vec![[0.; 2]; n];
            let mut pred = target.clone();
            let mut valid = vec![false; n];
            for (i, r) in rows.iter().enumerate() {
                let rs = &r["score"]["regions"];
                if rs[region].is_null() {
                    continue;
                }
                let others: Vec<_> = (0..9)
                    .filter(|&k| k != region && !rs[k].is_null())
                    .collect();
                if others.len() < 4 {
                    continue;
                }
                target[i] = serde_json::from_value(rs[region]["acc"].clone())?;
                pred[i] = std::array::from_fn(|axis| {
                    median(
                        others
                            .iter()
                            .map(|&k| rs[k]["acc"][axis].as_f64().unwrap())
                            .collect(),
                    )
                });
                valid[i] = true;
            }
            // Zero-filled invalid samples are never scored within 0.5s; record coverage explicitly.
            let x = filtered(&target, lo, hi);
            let p = filtered(&pred, lo, hi);
            let (mut b, mut e, mut count) = (0., 0., 0);
            for i in 100..n.saturating_sub(100) {
                if valid[i - 50..=i + 50].iter().all(|&v| v) {
                    count += 1;
                    for k in 0..2 {
                        b += x[i][k] * x[i][k];
                        e += (x[i][k] - p[i][k]).powi(2);
                    }
                }
            }
            regions.push(json!({"region":region,"count":count,"before":b,"after":e,"reduction":if b>0.{Some(1.-e/b)}else{None}}));
        }
        let b: f64 = regions.iter().map(|r| r["before"].as_f64().unwrap()).sum();
        let e: f64 = regions.iter().map(|r| r["after"].as_f64().unwrap()).sum();
        out.push(
            json!({"hz":[lo,hi],"regions":regions,"reduction":if b>0.{Some(1.-e/b)}else{None}}),
        );
    }
    std::fs::write(
        &a[2],
        serde_json::to_vec_pretty(
            &json!({"bands":out,"note":"Filtered regional-median position proxy; leave-region-out prediction with >=4 other supported regions. 1s edges,0.5s missing-support neighborhoods excluded. Shared images and tracking bias remain. Not ground truth."}),
        )?,
    )?;
    Ok(())
}
