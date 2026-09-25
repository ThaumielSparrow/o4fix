//! Disjoint, cell-balanced subset diagnostic using the existing essential estimator.
use o4core::{error::O4Error, quat};
use opencv::{
    calib3d,
    core::{self, no_array, Mat, Point2f, Vector},
    prelude::*,
};
use serde_json::{json, Value};

pub fn partition(pixels: &[Point2f], w: f64, h: f64) -> [Vec<usize>; 2] {
    let mut counts = [0usize; 12];
    let mut out = [vec![], vec![]];
    for (i, p) in pixels.iter().enumerate() {
        let col = (p.x as f64 / w * 4.).floor().clamp(0., 3.) as usize;
        let row = (p.y as f64 / h * 3.).floor().clamp(0., 2.) as usize;
        let cell = row * 4 + col;
        let side = (counts[cell] + cell) % 2;
        out[side].push(i);
        counts[cell] += 1;
    }
    out
}

pub fn fit(a: &Vector<Point2f>, b: &Vector<Point2f>) -> Result<Option<([f64; 3], i32)>, O4Error> {
    if a.len() < 60 {
        return Ok(None);
    }
    let eye = Mat::eye(3, 3, core::CV_64F)?.to_mat()?;
    let mut mask = Mat::default();
    let e =
        calib3d::find_essential_mat(a, b, &eye, calib3d::RANSAC, 0.999, 0.002, 1000, &mut mask)?;
    if e.rows() != 3 || e.cols() != 3 {
        return Ok(None);
    }
    let n = (0..mask.rows())
        .map(|i| *mask.at::<u8>(i).unwrap() as i32)
        .sum::<i32>();
    if n < 60 {
        return Ok(None);
    }
    let (mut r1, mut r2, mut t) = (Mat::default(), Mat::default(), Mat::default());
    calib3d::decompose_essential_mat(&e, &mut r1, &mut r2, &mut t)?;
    let rv = |r: &Mat| -> Result<[f64; 3], O4Error> {
        let mut v = Mat::default();
        calib3d::rodrigues(r, &mut v, &mut no_array())?;
        Ok([*v.at::<f64>(0)?, *v.at::<f64>(1)?, *v.at::<f64>(2)?])
    };
    let (a, b) = (rv(&r1)?, rv(&r2)?);
    let norm = |r: [f64; 3]| r.iter().map(|v| v * v).sum::<f64>();
    let r = if norm(a) <= norm(b) { a } else { b };
    if r.iter().any(|v| !v.is_finite()) {
        return Ok(None);
    }
    Ok(Some((r, n)))
}

pub fn angle(a: [f64; 3], b: [f64; 3]) -> f64 {
    quat::qlog(quat::qmul(quat::qconj(quat::qexp(a)), quat::qexp(b)))
        .iter()
        .map(|v| v * v)
        .sum::<f64>()
        .sqrt()
}

pub fn diagnose(
    pixels: &[Point2f],
    a: &Vector<Point2f>,
    b: &Vector<Point2f>,
    w: f64,
    h: f64,
    full: [f64; 3],
) -> Result<Value, O4Error> {
    let ids = partition(pixels, w, h);
    let mut halves = vec![];
    let mut rotations = vec![];
    for ii in &ids {
        let aa: Vector<Point2f> = ii.iter().map(|&i| a.get(i).unwrap()).collect();
        let bb: Vector<Point2f> = ii.iter().map(|&i| b.get(i).unwrap()).collect();
        // Reset to the same seed for each half. Full fit already finished; next pair resets its own seed.
        core::set_rng_seed(271828)?;
        let result = fit(&aa, &bb)?;
        halves.push(json!({"tracks":ii.len(),"inliers":result.map(|v|v.1),"rotation_rad":result.map(|v|v.0),"full_angle_rad":result.map(|v|angle(full,v.0))}));
        rotations.push(result.map(|v| v.0));
    }
    Ok(
        json!({"halves":halves,"angle_rad":match(rotations[0],rotations[1]){(Some(a),Some(b))=>Some(angle(a,b)),_=>None}}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::{Rotation3, Vector3};
    fn scene(angle: f64, bias: f64) -> (Vec<Point2f>, Vector<Point2f>, Vector<Point2f>, [f64; 3]) {
        let truth = Rotation3::from_scaled_axis(Vector3::new(angle, -0.4 * angle, 0.2 * angle));
        let observed = Rotation3::from_scaled_axis(Vector3::new(bias, 0., 0.)) * truth;
        let mut pixels = vec![];
        let mut a = Vector::new();
        let mut b = Vector::new();
        for y in 0..18 {
            for x in 0..24 {
                let px = (x as f64 - 11.5) / 20.;
                let py = (y as f64 - 8.5) / 20.;
                let z = 3. + ((y * 24 + x) * 17 % 31) as f64 * 0.13;
                let p = Vector3::new(px * z, py * z, z);
                let q = observed * p + Vector3::new(0.12, -0.03, 0.02);
                a.push(Point2f::new(px as f32, py as f32));
                b.push(Point2f::new((q.x / q.z) as f32, (q.y / q.z) as f32));
                pixels.push(Point2f::new((x as f32 + 0.5) * 30., (y as f32 + 0.5) * 30.));
            }
        }
        (pixels, a, b, [angle, -0.4 * angle, 0.2 * angle])
    }
    #[test]
    fn disjoint_balanced_partition_covers_every_track() {
        let (p, _, _, _) = scene(0.01, 0.);
        let ids = partition(&p, 720., 540.);
        assert!(ids[0].iter().all(|i| !ids[1].contains(i)));
        let mut all = ids[0].iter().chain(&ids[1]).copied().collect::<Vec<_>>();
        all.sort_unstable();
        assert_eq!(all, (0..p.len()).collect::<Vec<_>>());
        assert_eq!(ids[0].len(), ids[1].len());
    }
    #[test]
    fn known_rotation_translation_and_fast_turn() {
        for turn in [0.003, 0.08] {
            let (p, a, b, truth) = scene(turn, 0.);
            let d = diagnose(&p, &a, &b, 720., 540., truth).unwrap();
            for h in d["halves"].as_array().unwrap() {
                assert!(h["full_angle_rad"].as_f64().unwrap() < 1e-4, "{d}");
            }
        }
    }
    #[test]
    fn agreement_cannot_detect_shared_bias() {
        let (p, a, b, truth) = scene(0.01, 0.01);
        let d = diagnose(&p, &a, &b, 720., 540., truth).unwrap();
        assert!(d["angle_rad"].as_f64().unwrap() < 1e-4, "{d}");
        for h in d["halves"].as_array().unwrap() {
            assert!(h["full_angle_rad"].as_f64().unwrap() > 0.005, "{d}");
        }
    }
    #[test]
    fn insufficient_subset_is_missing() {
        let a: Vector<Point2f> = (0..40).map(|i| Point2f::new(i as f32, 1.)).collect();
        assert!(fit(&a, &a).unwrap().is_none());
    }
}
