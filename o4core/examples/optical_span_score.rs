use o4core::dsp;
use serde_json::{json, Value};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 3 {
        return Err("usage: optical_span_score SPANS OUTPUT".into());
    }
    let d: Value = serde_json::from_str(&std::fs::read_to_string(&a[1])?)?;
    let mut results = vec![];
    for (lo, hi) in [(132., 146.), (55.9, 58.9)] {
        let mut streams = vec![];
        for v in d["variants"].as_array().ok_or("variants")? {
            let t: Vec<f64> = serde_json::from_value(v["t"].clone())?;
            let r: Vec<[f64; 3]> = serde_json::from_value(v["omega"].clone())?;
            let q: Vec<f64> = serde_json::from_value(v["quality"].clone())?;
            let ii: Vec<_> = (0..t.len())
                .filter(|&i| t[i] >= lo - 0.5 && t[i] <= hi + 0.5)
                .collect();
            let times: Vec<_> = ii.iter().map(|&i| t[i]).collect();
            let values: Vec<_> = ii.iter().map(|&i| r[i]).collect();
            let quality: Vec<_> = ii.iter().map(|&i| q[i]).collect();
            let lp = dsp::filtfilt3(&dsp::butter_low(2, 8. / 50.), &values);
            streams.push((times, lp, quality));
        }
        let (t1, r1, q1) = &streams[0];
        let (t2, r2, q2) = &streams[1];
        let cols: Vec<_> = (0..3)
            .map(|k| dsp::interp(t2, t1, &r1.iter().map(|r| r[k]).collect::<Vec<_>>()))
            .collect();
        let mut bins = vec![];
        for bin in 0..((hi - lo) * 4.) as usize {
            let start = lo + bin as f64 / 4.;
            let mut errors = vec![];
            for i in 25..t2.len().saturating_sub(25) {
                if t2[i] < start || t2[i] >= start + 0.25 {
                    continue;
                }
                let j = dsp::searchsorted_left(t1, t2[i]);
                if j < 25
                    || j + 25 >= q1.len()
                    || q1[j - 25..=j + 25].iter().any(|&q| q < 0.3)
                    || q2[i - 25..=i + 25].iter().any(|&q| q < 0.3)
                {
                    continue;
                }
                errors.push(
                    (0..3)
                        .map(|k| (cols[k][i] - r2[i][k]).to_degrees().powi(2))
                        .sum::<f64>(),
                );
            }
            bins.push(json!({"start":start,"pairs":errors.len(),"span_difference_rms_deg_s":if errors.is_empty(){None}else{Some((errors.iter().sum::<f64>()/errors.len()as f64).sqrt())}}));
        }
        results.push(json!({"window":[lo,hi],"bins":bins,"bad_pair_counts":[q1.iter().filter(|&&q|q<0.3).count(),q2.iter().filter(|&&q|q<0.3).count()]}));
    }
    std::fs::write(
        &a[2],
        serde_json::to_vec_pretty(
            &json!({"note":"LP8 single-pair vs two-pair-span optical rates at common midpoint times; optical-coordinate deg/s. Independent accuracy not established. Quality neighborhoods excluded.","windows":results}),
        )?,
    )?;
    Ok(())
}
