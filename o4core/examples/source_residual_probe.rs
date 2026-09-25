//! Held-out epipolar residual patterns in distorted source-image coordinates.
use o4core::{config::Config, detect, dsp, pipeline, quat, telemetry};
use opencv::{
    calib3d,
    core::{self, Mat, Point2f, Vector},
    prelude::*,
};
use serde_json::{json, Value};
fn median(v: &[f64]) -> Option<f64> {
    if v.is_empty() {
        None
    } else {
        let mut s = v.to_vec();
        s.sort_by(f64::total_cmp);
        Some(s[s.len() / 2])
    }
}
fn slope(x: &[f64], y: &[f64]) -> Option<f64> {
    if x.len() < 20 {
        return None;
    }
    let n = x.len() as f64;
    let mx = x.iter().sum::<f64>() / n;
    let my = y.iter().sum::<f64>() / n;
    let den = x.iter().map(|v| (v - mx).powi(2)).sum::<f64>();
    if den < 1e-10 {
        None
    } else {
        Some(
            x.iter()
                .zip(y)
                .map(|(x, y)| (x - mx) * (y - my))
                .sum::<f64>()
                / den,
        )
    }
}
fn corr(x: &[f64], y: &[f64]) -> Option<f64> {
    let a = slope(x, y)?;
    let b = slope(y, x)?;
    Some(a.signum() * (a * b).max(0.).sqrt())
}
fn shuffled(n: usize, seed: u64) -> Vec<usize> {
    let mut order: Vec<_> = (0..n).collect();
    let mut state = seed | 1;
    for i in (1..n).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        order.swap(i, state as usize % (i + 1));
    }
    order
}
fn distort(p: &[Point2f], k: &Mat, d: &Mat) -> opencv::Result<Vec<Point2f>> {
    if p.is_empty() {
        return Ok(vec![]);
    }
    let src: Vector<Point2f> = p.iter().copied().collect();
    let mut dst = Vector::<Point2f>::new();
    calib3d::fisheye_distort_points_def(&src, &mut dst, k, d)?;
    Ok(dst.to_vec())
}
fn correction(e: &Mat, a: Point2f, b: Point2f) -> Option<Point2f> {
    let x = [a.x as f64, a.y as f64, 1.];
    let l: [f64; 3] = std::array::from_fn(|r| {
        (0..3)
            .map(|c| *e.at_2d::<f64>(r as i32, c as i32).unwrap() * x[c])
            .sum()
    });
    let den = l[0] * l[0] + l[1] * l[1];
    if den < 1e-18 {
        return None;
    }
    let v = (l[0] * b.x as f64 + l[1] * b.y as f64 + l[2]) / den;
    let p = Point2f::new(
        (b.x as f64 - v * l[0]) as f32,
        (b.y as f64 - v * l[1]) as f32,
    );
    if p.x.is_finite() && p.y.is_finite() {
        Some(p)
    } else {
        None
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 5 {
        return Err(
            "usage: source_residual_probe VIDEO SOURCE_OBSERVATIONS CALIBRATION_JSON OUTPUT_JSON"
                .into(),
        );
    }
    let data: Value = serde_json::from_str(&std::fs::read_to_string(&args[2])?)?;
    let cal: Value = serde_json::from_str(&std::fs::read_to_string(&args[3])?)?;
    let tel = telemetry::extract_quats(std::path::Path::new(&args[1]))?;
    let km = tel.meta.camera_matrix.ok_or("missing camera matrix")?;
    let dc = tel.meta.distortion.ok_or("missing distortion")?;
    let w = tel.meta.calib_w.ok_or("missing width")?;
    let h = tel.meta.calib_h.ok_or("missing height")?;
    if (w - 1440.).abs() > 1e-6 || (h - 1080.).abs() > 1e-6 {
        return Err("cached source coordinates require verified 1440x1080 geometry".into());
    }
    let k = Mat::from_slice_2d(&km)?;
    let d = Mat::from_slice(&dc)?.try_clone()?;
    let (tm, om) = quat::quats_to_rates(&tel.t, &tel.q);
    let fs = pipeline::fs(&tel.t);
    let (clean, _) = detect::adaptive_clean(&om, fs, &Config::default());
    let speed: Vec<_> = clean
        .iter()
        .map(|v| v.iter().map(|x| x * x).sum::<f64>().sqrt().to_degrees())
        .collect();
    let mut out = vec![];
    for (index, row) in data["rows"]
        .as_array()
        .ok_or("missing rows")?
        .iter()
        .enumerate()
    {
        let time = row["t"].as_f64().unwrap();
        let fold = cal["held_out"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| {
                time >= f["interval"][0].as_f64().unwrap() - 0.02
                    && time <= f["interval"][1].as_f64().unwrap() + 0.02
            })
            .ok_or("time outside clean folds")?;
        let shift = fold["baseline"]["shift_ms"].as_f64().unwrap() / 1000.;
        let motion = dsp::interp(&[time + shift], &tm, &speed)[0];
        let parse = |v: &Value| -> Vec<Point2f> {
            v.as_array()
                .unwrap()
                .iter()
                .map(|v| Point2f::new(v[0].as_f64().unwrap() as f32, v[1].as_f64().unwrap() as f32))
                .collect()
        };
        let a = parse(&row["normalized_a"]);
        let b = parse(&row["normalized_b"]);
        let ids: Vec<usize> = serde_json::from_value(row["ids"].clone())?;
        if row["source_b"].is_null() {
            return Err("source residual diagnostic requires original distorted source_b points; normalized-only caches cannot verify failed or alternate inverse branches".into());
        }
        let source = parse(&row["source_b"]);
        if source.len() != b.len() {
            return Err("source point count mismatch".into());
        }
        let back = distort(&b, &k, &d)?;
        if source
            .iter()
            .zip(&back)
            .any(|(a, b)| (a.x - b.x).hypot(a.y - b.y) > 0.001)
        {
            return Err(
                "inverse/projection roundtrip failed; do not interpret spatial residuals".into(),
            );
        }
        let mut models = vec![];
        for parity in 0..2 {
            let aa: Vector<Point2f> = a
                .iter()
                .zip(&ids)
                .filter(|(_, id)| **id % 2 == parity)
                .map(|(&p, _)| p)
                .collect();
            let bb: Vector<Point2f> = b
                .iter()
                .zip(&ids)
                .filter(|(_, id)| **id % 2 == parity)
                .map(|(&p, _)| p)
                .collect();
            core::set_rng_seed(4_000_000 + index as i32 * 2 + parity as i32)?;
            let eye = Mat::eye(3, 3, core::CV_64F)?.to_mat()?;
            let mut mask = Mat::default();
            let e = if aa.len() >= 40 {
                calib3d::find_essential_mat(
                    &aa,
                    &bb,
                    &eye,
                    calib3d::RANSAC,
                    0.999,
                    0.002,
                    1000,
                    &mut mask,
                )?
            } else {
                Mat::default()
            };
            models.push(if e.rows() == 3 && e.cols() == 3 {
                Some(e)
            } else {
                None
            });
        }
        let mut valid = vec![];
        let mut predicted = vec![];
        for i in 0..a.len() {
            if let Some(e) = &models[1 - ids[i] % 2] {
                if let Some(p) = correction(e, a[i], b[i]) {
                    valid.push(i);
                    predicted.push(p);
                }
            }
        }
        let predicted = distort(&predicted, &k, &d)?;
        let mut samples = vec![];
        for (&i, p) in valid.iter().zip(&predicted) {
            let actual = source[i];
            let dx = (actual.x - p.x) as f64 / 2.;
            let dy = (actual.y - p.y) as f64 / 2.;
            let x = (actual.x as f64 - km[0][2]) / (w / 2.);
            let y = (actual.y as f64 - km[1][2]) / (h / 2.);
            let radius = x.hypot(y);
            let radial = if radius > 1e-8 {
                (dx * x + dy * y) / radius
            } else {
                0.
            };
            if [dx, dy, x, y, radial].iter().all(|v| v.is_finite()) {
                samples.push([
                    actual.y as f64 / h - 0.5,
                    radius,
                    dx,
                    dy,
                    radial,
                    dx.hypot(dy),
                    actual.x as f64 / w - 0.5,
                ]);
            }
        }
        let col = |c: usize| samples.iter().map(|s| s[c]).collect::<Vec<_>>();
        let yy = col(0);
        let rr = col(1);
        let dy = col(3);
        let rad = col(4);
        let mag = col(5);
        let xx = col(6);
        let mut negative = vec![];
        for trial in 0..32 {
            let order = shuffled(samples.len(), 0x9e3779b9u64 + index as u64 * 37 + trial);
            let sy: Vec<_> = order.iter().map(|&i| yy[i]).collect();
            let sr: Vec<_> = order.iter().map(|&i| rr[i]).collect();
            negative.push(json!({"row_dy_slope":slope(&sy,&dy),"radius_radial_slope":slope(&sr,&rad),"row_magnitude_correlation":corr(&sy,&mag),"radius_magnitude_correlation":corr(&sr,&mag)}));
        }
        let mut bins = vec![];
        for which in 0..2 {
            let count = if which == 0 { 5 } else { 4 };
            let mut groups: Vec<Vec<usize>> = vec![vec![]; count];
            for (i, s) in samples.iter().enumerate() {
                let bin = if which == 0 {
                    ((s[0] + 0.5) * 5.).floor().clamp(0., 4.) as usize
                } else {
                    (s[1] * 3.).floor().clamp(0., 3.) as usize
                };
                groups[bin].push(i);
            }
            bins.push(groups.iter().map(|g|if g.len()<8{Value::Null}else{json!({"count":g.len(),"median_dx":median(&g.iter().map(|&i|samples[i][2]).collect::<Vec<_>>()),"median_dy":median(&g.iter().map(|&i|samples[i][3]).collect::<Vec<_>>()),"median_radial":median(&g.iter().map(|&i|samples[i][4]).collect::<Vec<_>>()),"median_magnitude":median(&g.iter().map(|&i|samples[i][5]).collect::<Vec<_>>())})}).collect::<Vec<_>>());
        }
        out.push(json!({"t":time,"angular_speed_deg_s":motion,"tracks":a.len(),"evaluated":samples.len(),"median_magnitude_px_half":median(&mag),"row_dy_slope":slope(&yy,&dy),"column_dx_slope":slope(&xx,&col(2)),"radius_radial_slope":slope(&rr,&rad),"row_magnitude_correlation":corr(&yy,&mag),"radius_magnitude_correlation":corr(&rr,&mag),"row_bins":bins[0],"radius_bins":bins[1],"shuffled_location":negative}));
    }
    std::fs::write(
        &args[4],
        serde_json::to_vec(
            &json!({"rows":out,"intervals":cal["intervals"],"units":"half-resolution source pixels; epipolar-normal displacement, not full motion error","metadata":format!("{:?}",tel.meta)}),
        )?,
    )?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_edge_inverse_failure_is_detectable() {
        let k = Mat::from_slice_2d(&[[546.40271, 0., 720.], [0., 546.40271, 540.], [0., 0., 1.]])
            .unwrap();
        let d = Mat::from_slice(&[0.1551311, 0.1371409, -0.0938614, 0.0041704])
            .unwrap()
            .try_clone()
            .unwrap();
        let p: Vector<Point2f> = [Point2f::new(1310., 40.)].into_iter().collect();
        let mut u = Vector::<Point2f>::new();
        calib3d::fisheye_undistort_points_def(&p, &mut u, &k, &d).unwrap();
        let back = distort(&u.to_vec(), &k, &d).unwrap();
        assert!((back[0].x - 1310.).hypot(back[0].y - 40.) > 1000.);
    }
    #[test]
    fn central_lens_roundtrip_preserves_source_rows() {
        let k = Mat::from_slice_2d(&[[546.40271, 0., 720.], [0., 546.40271, 540.], [0., 0., 1.]])
            .unwrap();
        let d = Mat::from_slice(&[0.1551311, 0.1371409, -0.0938614, 0.0041704])
            .unwrap()
            .try_clone()
            .unwrap();
        let p: Vector<Point2f> = (0..8)
            .flat_map(|r| {
                (0..10).map(move |c| Point2f::new(320. + c as f32 * 80., 260. + r as f32 * 70.))
            })
            .collect();
        let mut u = Vector::<Point2f>::new();
        calib3d::fisheye_undistort_points_def(&p, &mut u, &k, &d).unwrap();
        let back = distort(&u.to_vec(), &k, &d).unwrap();
        for (a, b) in p.iter().zip(back) {
            assert!(
                (a.x - b.x).hypot(a.y - b.y) < 0.001,
                "source {:?} reconstructed {:?}, error {}",
                a,
                b,
                (a.x - b.x).hypot(a.y - b.y)
            );
        }
    }
    #[test]
    fn epipolar_correction_is_sign_invariant() {
        let e = Mat::from_slice_2d(&[[0., 0., 0.], [0., 0., -1.], [0., 1., 0.]]).unwrap();
        let neg = Mat::from_slice_2d(&[[0., 0., 0.], [0., 0., 1.], [0., -1., 0.]]).unwrap();
        let a = Point2f::new(0.1, 0.2);
        let b = Point2f::new(0.4, 0.23);
        let p = correction(&e, a, b).unwrap();
        let q = correction(&neg, a, b).unwrap();
        assert!((p.y - 0.2).abs() < 1e-7);
        assert_eq!(p, q);
    }
    #[test]
    fn spatial_signal_survives_and_shuffled_control_loses_it() {
        let x: Vec<_> = (0..300).map(|i| i as f64 / 300. - 0.5).collect();
        let y: Vec<_> = x.iter().map(|x| 2. * x + 0.1).collect();
        assert!((slope(&x, &y).unwrap() - 2.).abs() < 1e-10);
        let null: Vec<_> = (0..32)
            .map(|seed| {
                let p = shuffled(x.len(), 123 + seed);
                slope(&p.iter().map(|&i| x[i]).collect::<Vec<_>>(), &y).unwrap()
            })
            .collect();
        assert!(median(&null).unwrap().abs() < 0.1);
        assert!(slope(&[], &[]).is_none());
    }
}
