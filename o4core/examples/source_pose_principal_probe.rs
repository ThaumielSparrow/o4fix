#[path = "support/lens_inverse.rs"]
mod lens_inverse;
// Principal-branch source observations, retaining actual pixel coordinates.
use nalgebra::{Matrix3, Rotation3, Vector3};
use o4core::{
    alignment::CalibrationData,
    config::Config,
    detect,
    optical::{Alignment, OpticalRates},
    pipeline, quat, telemetry,
};
use opencv::{
    calib3d,
    core::{self, no_array, Mat, Point2f, Size, TermCriteria, Vector},
    imgproc,
    prelude::*,
    video, videoio,
};
use serde_json::{json, Value};
fn bearing(p: Point2f) -> Vector3<f64> {
    Vector3::new(p.x as f64, p.y as f64, 1.).normalize()
}
fn median(v: &[f64]) -> f64 {
    let mut x = v.to_vec();
    x.sort_by(f64::total_cmp);
    x[x.len() / 2]
}
fn angular(r: &Matrix3<f64>, a: Vector3<f64>, b: Vector3<f64>) -> f64 {
    let p = r * a;
    p.cross(&b).norm().atan2(p.dot(&b))
}
fn rotation_angle(r: &Matrix3<f64>) -> f64 {
    ((r.trace() - 1.) / 2.).clamp(-1., 1.).acos()
}
fn rvec(r: &Matrix3<f64>) -> [f64; 3] {
    let v = nalgebra::UnitQuaternion::from_rotation_matrix(&Rotation3::from_matrix_unchecked(*r))
        .scaled_axis();
    [v[0], v[1], v[2]]
}
struct Model {
    r: Matrix3<f64>,
    support: usize,
}
fn wahba(a: &[Point2f], b: &[Point2f]) -> Option<Model> {
    if a.len() < 30 {
        return None;
    }
    let mut ids: Vec<_> = (0..a.len()).collect();

    for _ in 0..5 {
        let mut cov = Matrix3::zeros();
        for &i in &ids {
            cov += bearing(b[i]) * bearing(a[i]).transpose();
        }
        let sv = cov.svd(true, true);
        if sv.singular_values[1] < 1e-8 {
            return None;
        }
        let u = sv.u?;
        let vt = sv.v_t?;
        let mut d = Matrix3::identity();
        d[(2, 2)] = (u * vt).determinant();
        let r = u * d * vt;
        let next: Vec<_> = (0..a.len())
            .filter(|&i| angular(&r, bearing(a[i]), bearing(b[i])) < 0.002)
            .collect();
        if next.len() < 30 {
            return None;
        }
        if next == ids {
            break;
        }
        ids = next;
    }
    // Refit the final accepted set before reporting support.
    let mut cov = Matrix3::zeros();
    for &i in &ids {
        cov += bearing(b[i]) * bearing(a[i]).transpose();
    }
    let sv = cov.svd(true, true);
    let u = sv.u?;
    let vt = sv.v_t?;
    let mut d = Matrix3::identity();
    d[(2, 2)] = (u * vt).determinant();
    let r = u * d * vt;
    Some(Model {
        r,
        support: ids.len(),
    })
}
fn mat3(m: &Mat) -> Matrix3<f64> {
    Matrix3::from_fn(|r, c| *m.at_2d::<f64>(r as i32, c as i32).unwrap())
}
// Rotation-only, essential small-angle branch (production convention), cheirality pose.
fn models(a: &[Point2f], b: &[Point2f]) -> opencv::Result<[Option<Model>; 3]> {
    models_at_distance(a, b, None)
}
fn models_at_distance(
    a: &[Point2f],
    b: &[Point2f],
    distance: Option<f64>,
) -> opencv::Result<[Option<Model>; 3]> {
    let rot = wahba(a, b);
    if a.len() < 40 {
        return Ok([rot, None, None]);
    }
    let aa: Vector<Point2f> = a.iter().copied().collect();
    let bb: Vector<Point2f> = b.iter().copied().collect();
    let eye = Mat::eye(3, 3, core::CV_64F)?.to_mat()?;
    let mut mask = Mat::default();
    let e = calib3d::find_essential_mat(
        &aa,
        &bb,
        &eye,
        calib3d::RANSAC,
        0.999,
        0.002,
        1000,
        &mut mask,
    )?;
    if e.rows() != 3 || e.cols() != 3 {
        return Ok([rot, None, None]);
    }
    let support = (0..mask.rows())
        .filter(|&i| *mask.at::<u8>(i).unwrap() != 0)
        .count();
    if support < 30 {
        return Ok([rot, None, None]);
    }
    let (mut r1, mut r2, mut t) = (Mat::default(), Mat::default(), Mat::default());
    calib3d::decompose_essential_mat(&e, &mut r1, &mut r2, &mut t)?;
    let (x, y) = (mat3(&r1), mat3(&r2));
    let r = if rotation_angle(&x) <= rotation_angle(&y) {
        x
    } else {
        y
    };
    let (mut cr, mut ct) = (Mat::default(), Mat::default());
    let count = if let Some(distance) = distance {
        calib3d::recover_pose_triangulated(
            &e,
            &aa,
            &bb,
            &eye,
            &mut cr,
            &mut ct,
            distance,
            &mut mask,
            &mut no_array(),
        )?
    } else {
        calib3d::recover_pose_estimated(&e, &aa, &bb, &eye, &mut cr, &mut ct, &mut mask)?
    };
    let pose = if count >= 30 {
        Some(Model {
            r: mat3(&cr),
            support: count as usize,
        })
    } else {
        None
    };
    Ok([rot, Some(Model { r, support }), pose])
}
fn interval(
    path: &str,
    start: f64,
    end: f64,
    meta: &telemetry::Meta,
) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let mut cap = videoio::VideoCapture::from_file(path, videoio::CAP_ANY)?;
    if !cap.is_opened()? || (cap.get(videoio::CAP_PROP_FPS)? - 100.).abs() > 1e-6 {
        return Err("expected decodable 100 fps source".into());
    }
    let w = cap.get(videoio::CAP_PROP_FRAME_WIDTH)? as i32;
    let h = cap.get(videoio::CAP_PROP_FRAME_HEIGHT)? as i32;
    let mut km = meta.camera_matrix.ok_or("missing actual camera matrix")?;
    let cw = meta.calib_w.ok_or("missing calibration width")?;
    let ch = meta.calib_h.ok_or("missing calibration height")?;
    if cw <= 0. || ch <= 0. {
        return Err("invalid calibration dimensions".into());
    }
    for c in 0..3 {
        km[0][c] *= w as f64 / cw;
        km[1][c] *= h as f64 / ch;
    }
    let k = Mat::from_slice_2d(&km)?;
    let distortion = meta.distortion.ok_or("missing actual distortion")?;
    let d = Mat::from_slice(&distortion)?;
    let f0 = (start * 100.) as usize;
    let count = (end * 100.) as usize + 2 - f0;
    if !cap.set(videoio::CAP_PROP_POS_FRAMES, f0 as f64)? {
        return Err("seek rejected".into());
    }
    let mut prev: Option<Mat> = None;
    let mut points = Vector::<Point2f>::new();
    let mut ids = vec![];
    let mut ages: Vec<usize> = vec![];
    let mut rows = vec![];
    let mut frame = Mat::default();
    let mut next_id = 0;
    for f in 0..count {
        if !cap.read(&mut frame)? {
            return Err(format!("decode stopped at {f}").into());
        }
        let mut small = Mat::default();
        imgproc::resize(
            &frame,
            &mut small,
            Size::new(w / 2, h / 2),
            0.,
            0.,
            imgproc::INTER_AREA,
        )?;
        let mut gray = Mat::default();
        imgproc::cvt_color_def(&small, &mut gray, imgproc::COLOR_BGR2GRAY)?;
        if let Some(p) = &prev {
            let replenished = points.len() < 150 || f % 10 == 1;
            if replenished {
                let mut additions = Vector::<Point2f>::new();
                imgproc::good_features_to_track(
                    p,
                    &mut additions,
                    600,
                    0.01,
                    10.,
                    &no_array(),
                    7,
                    false,
                    0.04,
                )?;
                for candidate in additions {
                    if points.len() >= 600 {
                        break;
                    }
                    if points
                        .iter()
                        .all(|q| (q.x - candidate.x).powi(2) + (q.y - candidate.y).powi(2) >= 100.)
                    {
                        points.push(candidate);
                        ids.push(next_id);
                        ages.push(0);
                        next_id += 1;
                    }
                }
            }
            let (mut forward, mut status, mut error) = (
                Vector::<Point2f>::new(),
                Vector::<u8>::new(),
                Vector::<f32>::new(),
            );
            let criteria =
                TermCriteria::new(core::TermCriteria_COUNT + core::TermCriteria_EPS, 30, 0.01)?;
            video::calc_optical_flow_pyr_lk(
                p,
                &gray,
                &points,
                &mut forward,
                &mut status,
                &mut error,
                Size::new(21, 21),
                3,
                criteria,
                0,
                1e-4,
            )?;
            let (mut back, mut bs, mut be) = (
                Vector::<Point2f>::new(),
                Vector::<u8>::new(),
                Vector::<f32>::new(),
            );
            video::calc_optical_flow_pyr_lk(
                &gray,
                p,
                &forward,
                &mut back,
                &mut bs,
                &mut be,
                Size::new(21, 21),
                3,
                criteria,
                0,
                1e-4,
            )?;
            let good: Vec<_> = (0..points.len())
                .filter(|&i| {
                    if status.get(i).unwrap() != 1 || bs.get(i).unwrap() != 1 {
                        return false;
                    }
                    let x = points.get(i).unwrap();
                    let y = back.get(i).unwrap();
                    let z = forward.get(i).unwrap();
                    (x.x - y.x).powi(2) + (x.y - y.y).powi(2) <= 1.
                        && z.x >= 0.
                        && z.x < (w / 2) as f32
                        && z.y >= 0.
                        && z.y < (h / 2) as f32
                })
                .collect();
            let aa: Vec<_> = good.iter().map(|&i| points.get(i).unwrap()).collect();
            let bb: Vec<_> = good.iter().map(|&i| forward.get(i).unwrap()).collect();
            let kept: Vec<_> = good.iter().map(|&i| ids[i]).collect();
            let time = (f0 as f64 + f as f64 - 0.5) / 100.;
            core::set_rng_seed(1_000_000 + f0 as i32 + f as i32)?;
            let to_full = |v: &[Point2f]| -> Vector<Point2f> {
                v.iter().map(|p| Point2f::new(p.x * 2., p.y * 2.)).collect()
            };
            let (mut u0, mut u1) = (Vector::<Point2f>::new(), Vector::<Point2f>::new());
            let inverse = lens_inverse::Inverse::new(&k, &d.try_clone()?)?;
            for p in to_full(&aa) {
                u0.push(inverse.point(p).unwrap_or(Point2f::new(f32::NAN, f32::NAN)));
            }
            for p in to_full(&bb) {
                u1.push(inverse.point(p).unwrap_or(Point2f::new(f32::NAN, f32::NAN)));
            }
            let valid: Vec<_> = (0..u0.len())
                .filter(|&i| {
                    let x = u0.get(i).unwrap();
                    let y = u1.get(i).unwrap();
                    [x.x, x.y, y.x, y.y]
                        .iter()
                        .all(|v| v.is_finite() && v.abs() < 100.)
                })
                .collect();
            let x: Vec<_> = valid.iter().map(|&i| u0.get(i).unwrap()).collect();
            let y: Vec<_> = valid.iter().map(|&i| u1.get(i).unwrap()).collect();
            let labels: Vec<_> = valid.iter().map(|&i| kept[i]).collect();
            let cells: Vec<_> = valid
                .iter()
                .map(|&i| {
                    ((aa[i].y / (h as f32 / 6.)).floor().clamp(0., 2.) as usize) * 3
                        + (aa[i].x / (w as f32 / 6.)).floor().clamp(0., 2.) as usize
                })
                .collect();
            let fit = models(&x, &y)?;
            let mut split = vec![];
            for parity in 0..2 {
                let xx: Vec<_> = x
                    .iter()
                    .zip(&labels)
                    .filter(|(_, id)| **id % 2 == parity)
                    .map(|(&p, _)| p)
                    .collect();
                let yy: Vec<_> = y
                    .iter()
                    .zip(&labels)
                    .filter(|(_, id)| **id % 2 == parity)
                    .map(|(&p, _)| p)
                    .collect();
                split.push(models(&xx, &yy)?);
            }
            let mut diagnostics = vec![];
            for mode in 0..3 {
                let residuals: Vec<_> = (0..x.len())
                    .map(|i| {
                        split[1 - labels[i] % 2][mode]
                            .as_ref()
                            .map(|m| angular(&m.r, bearing(x[i]), bearing(y[i])))
                    })
                    .collect();
                let grid: Vec<_> = (0..9)
                    .map(|c| {
                        let e: Vec<_> = residuals
                            .iter()
                            .zip(&cells)
                            .filter(|(_, cc)| **cc == c)
                            .filter_map(|(e, _)| *e)
                            .collect();
                        if e.len() < 8 {
                            Value::Null
                        } else {
                            json!({"count":e.len(),"rotation_residual_median_rad":median(&e)})
                        }
                    })
                    .collect();
                let disagree = split[0][mode]
                    .as_ref()
                    .zip(split[1][mode].as_ref())
                    .map(|(a, b)| rotation_angle(&(a.r * b.r.transpose())));
                diagnostics.push(json!({"split_rotation_disagreement_rad":disagree,"heldout_rotation_residual_grid":grid}));
            }
            rows.push(json!({"t":time,"tracks":x.len(),"models":fit.iter().map(|m|m.as_ref().map(|m|json!({"omega":rvec(&m.r).map(|v|v*100.),"support":m.support}))).collect::<Vec<_>>(),"diagnostics":diagnostics,
                "source_a":valid.iter().map(|&i|[aa[i].x*2.,aa[i].y*2.]).collect::<Vec<_>>(),"source_b":valid.iter().map(|&i|[bb[i].x*2.,bb[i].y*2.]).collect::<Vec<_>>(),"normalized_a":x.iter().map(|p|[p.x,p.y]).collect::<Vec<_>>(),"normalized_b":y.iter().map(|p|[p.x,p.y]).collect::<Vec<_>>(),"ids":labels,"regions":cells}));
            let kept_ages: Vec<_> = good.iter().map(|&i| ages[i] + 1).collect();
            points = bb.into_iter().collect();
            ids = kept;
            ages = kept_ages;
        }
        prev = Some(gray);
    }
    Ok(rows)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() == 4 && args[1] == "--depth-check" {
        let data: Value = serde_json::from_str(&std::fs::read_to_string(&args[2])?)?;
        let mut out = vec![];
        for row in data["rows"].as_array().ok_or("missing rows")? {
            let parse = |v: &Value| -> Vec<Point2f> {
                v.as_array()
                    .unwrap()
                    .iter()
                    .map(|p| {
                        Point2f::new(p[0].as_f64().unwrap() as f32, p[1].as_f64().unwrap() as f32)
                    })
                    .collect()
            };
            let a = parse(&row["normalized_a"]);
            let b = parse(&row["normalized_b"]);
            core::set_rng_seed(
                1_000_000 + (row["t"].as_f64().unwrap() * 100. + 0.5).round() as i32,
            )?;
            let models = models_at_distance(&a, &b, Some(1e6))?;
            let replay = models[1].as_ref().map(|m| rvec(&m.r).map(|x| x * 100.));
            let old: Option<[f64; 3]> = if row["models"][1].is_null() {
                None
            } else {
                Some(serde_json::from_value(row["models"][1]["omega"].clone())?)
            };
            if let Some((x, y)) = replay.zip(old) {
                assert!(
                    x.iter().zip(y).all(|(x, y)| (x - y).abs() < 1e-10),
                    "replay differs"
                );
            }
            let angle = models[1]
                .as_ref()
                .zip(models[2].as_ref())
                .map(|(x, y)| rotation_angle(&(x.r * y.r.transpose())));
            out.push(json!({"t":row["t"],"original_valid":!row["models"][2].is_null(),"relaxed_valid":models[2].is_some(),"rotation_disagreement_rad":angle,"relaxed_support":models[2].as_ref().map(|m|m.support)}));
        }
        std::fs::write(
            &args[3],
            serde_json::to_vec(
                &json!({"distance_threshold":1e6,"note":"baseline units; diagnostic only, not validated pose confidence","rows":out}),
            )?,
        )?;
        return Ok(());
    }
    if args.len() != 5 {
        return Err("usage: source_pose_principal_probe VIDEO CALIBRATION_JSON BASELINE_OBSERVATIONS_JSON OUTPUT_JSON".into());
    }
    let cal: Value = serde_json::from_str(&std::fs::read_to_string(&args[2])?)?;
    let tel = telemetry::extract_quats(std::path::Path::new(&args[1]))?;
    let mut rows = vec![];
    for (i, v) in cal["intervals"]
        .as_array()
        .ok_or("missing intervals")?
        .iter()
        .enumerate()
    {
        println!("interval {}", i + 1);
        rows.extend(interval(
            &args[1],
            v[0].as_f64().unwrap(),
            v[1].as_f64().unwrap(),
            &tel.meta,
        )?);
    }
    let base: Value = serde_json::from_str(&std::fs::read_to_string(&args[3])?)?;
    let index: std::collections::HashMap<i64, usize> = base["t"]
        .as_array()
        .ok_or("missing baseline times")?
        .iter()
        .enumerate()
        .map(|(i, t)| ((t.as_f64().unwrap() * 100000.).round() as i64, i))
        .collect();
    for row in &mut rows {
        let key = (row["t"].as_f64().unwrap() * 100000.).round() as i64;
        let entry = index
            .get(&key)
            .filter(|&&i| base["quality"][i].as_f64().unwrap() > 0.5)
            .map(|&i| json!({"omega":base["omega"][i],"quality":base["quality"][i]}))
            .unwrap_or(Value::Null);
        row["models"].as_array_mut().unwrap().push(entry);
    }
    let fs = pipeline::fs(&tel.t);
    let (tm, om) = quat::quats_to_rates(&tel.t, &tel.q);
    let (clean, _) = detect::adaptive_clean(&om, fs, &Config::default());
    let gyro: Vec<_> = clean.iter().map(|r| r.map(f64::to_degrees)).collect();
    let mut folds = vec![];
    for f in cal["held_out"].as_array().unwrap() {
        let start = f["interval"][0].as_f64().unwrap();
        let end = f["interval"][1].as_f64().unwrap();
        let al = Alignment {
            shift: f["baseline"]["shift_ms"].as_f64().unwrap() / 1000.,
            n: serde_json::from_value(f["baseline"]["matrix"].clone())?,
            r2: 0.,
        };
        let rr: Vec<_> = rows
            .iter()
            .filter(|r| {
                r["t"].as_f64().unwrap() >= start - 0.02 && r["t"].as_f64().unwrap() <= end + 0.02
            })
            .collect();
        let mut comparisons = vec![];
        for candidate in [0, 2, 3] {
            let mut scores = vec![];
            for mode in [1, candidate] {
                let mut opt = OpticalRates {
                    t: vec![],
                    omega: vec![],
                    quality: vec![],
                };
                for r in &rr {
                    let good = !r["models"][1].is_null() && !r["models"][candidate].is_null();
                    opt.t.push(r["t"].as_f64().unwrap());
                    opt.omega.push(if r["models"][mode].is_null() {
                        [0.; 3]
                    } else {
                        serde_json::from_value(r["models"][mode]["omega"].clone())?
                    });
                    opt.quality.push(if good { 1. } else { 0. });
                }
                scores.push(
                    CalibrationData::prepare(&opt, &tm, &gyro, fs)
                        .and_then(|d| d.score(&al))
                        .map(
                            |s| json!({"rms_deg_s":s.rms_deg_s,"samples":s.samples,"runs":s.runs}),
                        ),
                );
            }
            comparisons.push(json!({"candidate_index":candidate,"reference":scores[0],"candidate":scores[1],"common_pairs":rr.iter().filter(|r|!r["models"][1].is_null()&&!r["models"][candidate].is_null()).count()}));
        }
        folds.push(json!({"interval":[start,end],"pairs":rr.len(),"valid_counts":(0..4).map(|m|rr.iter().filter(|r|!r["models"][m].is_null()).count()).collect::<Vec<_>>(),"comparisons":comparisons}));
    }
    std::fs::write(
        &args[4],
        serde_json::to_vec(
            &json!({"inverse_model":"first_monotone_fisheye_branch","source_pixels_preserved":true,"video":args[1],"metadata":format!("{:?}",tel.meta),"model_order":["rotation_only","essential_small_angle","essential_cheirality","cached_production"],"folds":folds,"rows":rows}),
        )?,
    )?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn scene(angle: f64, translation: Vector3<f64>) -> (Vec<Point2f>, Vec<Point2f>, Matrix3<f64>) {
        let r = *Rotation3::from_euler_angles(angle / 2., angle, -angle / 3.).matrix();
        let mut a = vec![];
        let mut b = vec![];
        for row in 0..20 {
            for col in 0..30 {
                let x = -0.7 + col as f64 * 0.048;
                let y = -0.5 + row as f64 * 0.052;
                let depth = 4. + ((row * 13 + col * 7) % 19) as f64;
                let q = r * (Vector3::new(x, y, 1.) * depth) + translation;
                a.push(Point2f::new(x as f32, y as f32));
                b.push(Point2f::new((q.x / q.z) as f32, (q.y / q.z) as f32));
            }
        }
        (a, b, r)
    }
    #[test]
    fn pure_rotation_cannot_identify_translation_direction() {
        let (a, b, r) = scene(0.02, Vector3::zeros());
        for t in [Vector3::new(1., 0., 0.), Vector3::new(0., 1., 0.)] {
            let skew = Matrix3::new(0., -t.z, t.y, t.z, 0., -t.x, -t.y, t.x, 0.);
            for (&x, &y) in a.iter().zip(&b) {
                assert!(bearing(y).dot(&(skew * r * bearing(x))).abs() < 1e-7);
            }
        }
    }
    #[test]
    fn pure_rotation_and_fast_turn_recover_known_rotation() {
        for angle in [0., 0.003, 0.08] {
            let (a, b, r) = scene(angle, Vector3::zeros());
            let m = wahba(&a, &b).unwrap();
            assert!(
                rotation_angle(&(m.r * r.transpose())) < 1e-6,
                "angle={angle} error={} matrix={:?}",
                rotation_angle(&(m.r * r.transpose())),
                m.r * r.transpose()
            );
            assert_eq!(m.support, a.len());
            assert!(rvec(&m.r).iter().all(|v| v.is_finite()));
        }
    }
    #[test]
    fn translation_and_depth_require_pose_model() {
        let (a, b, r) = scene(0.008, Vector3::new(0.08, 0.02, 0.01));
        core::set_rng_seed(123).unwrap();
        let m = models(&a, &b).unwrap();
        assert!(m[0]
            .as_ref()
            .is_none_or(|m| rotation_angle(&(m.r * r.transpose())) > 0.001));
        for i in [1, 2] {
            assert!(rotation_angle(&(m[i].as_ref().unwrap().r * r.transpose())) < 1e-4);
        }
    }
    #[test]
    fn robust_pose_handles_correspondence_outliers() {
        let (a, mut b, r) = scene(0.06, Vector3::new(0.15, -0.03, 0.02));
        for (i, p) in b.iter_mut().enumerate() {
            if i % 9 == 0 {
                p.x += 0.1;
                p.y -= 0.05;
            }
        }
        core::set_rng_seed(456).unwrap();
        let m = models(&a, &b).unwrap();
        assert!(rotation_angle(&(m[2].as_ref().unwrap().r * r.transpose())) < 1e-4);
    }
    #[test]
    fn insufficient_and_concentrated_support_remain_missing() {
        assert!(wahba(&[], &[]).is_none());
        let a = vec![Point2f::new(0., 0.); 100];
        assert!(wahba(&a, &a).is_none());
        let m = models(&a[..10], &a[..10]).unwrap();
        assert!(m.iter().all(Option::is_none));
    }
}
