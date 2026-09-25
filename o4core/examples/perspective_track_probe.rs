//! Research-only persistent-track, leave-region-out perspective validation.
use opencv::{
    calib3d,
    core::{self, no_array, Mat, Point2f, Size, TermCriteria, Vector},
    imgproc,
    prelude::*,
    video, videoio,
};
use serde_json::{json, Value};
fn fit(a: &Vector<Point2f>, b: &Vector<Point2f>, projective: bool) -> opencv::Result<Mat> {
    let mut mask = Mat::default();
    if projective {
        calib3d::find_homography_ext(a, b, calib3d::RANSAC, 1.5, &mut mask, 2000, 0.995)
    } else {
        calib3d::estimate_affine_partial_2d(a, b, &mut mask, calib3d::RANSAC, 1.5, 2000, 0.995, 10)
    }
}
fn predict(m: &Mat, p: Point2f) -> Option<[f64; 2]> {
    if m.empty() {
        return None;
    }
    let (x, y) = (p.x as f64, p.y as f64);
    let v = |r, c| *m.at_2d::<f64>(r, c).unwrap();
    let w = if m.rows() == 3 {
        v(2, 0) * x + v(2, 1) * y + v(2, 2)
    } else {
        1.
    };
    if w.abs() < 1e-6 {
        return None;
    }
    let r = [
        (v(0, 0) * x + v(0, 1) * y + v(0, 2)) / w,
        (v(1, 0) * x + v(1, 1) * y + v(1, 2)) / w,
    ];
    if r.iter().all(|v| v.is_finite()) {
        Some(r)
    } else {
        None
    }
}
fn percentile(v: &[f64], q: f64) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    s[((s.len() - 1) as f64 * q).round() as usize]
}
fn score(a: &[Point2f], b: &[Point2f], ids: &[usize], projective: bool) -> opencv::Result<Value> {
    let mut models = vec![];
    for parity in 0..2 {
        let aa: Vector<Point2f> = a
            .iter()
            .zip(ids)
            .filter(|(_, id)| **id % 2 == parity)
            .map(|(p, _)| *p)
            .collect();
        let bb: Vector<Point2f> = b
            .iter()
            .zip(ids)
            .filter(|(_, id)| **id % 2 == parity)
            .map(|(p, _)| *p)
            .collect();
        if aa.len() < 30 {
            return Ok(Value::Null);
        }
        models.push(fit(&aa, &bb, projective)?);
    }
    let mut errors = vec![];
    let mut disagreement = vec![];
    let mut regions: Vec<Vec<[f64; 2]>> = vec![vec![]; 9];
    for i in 0..a.len() {
        let Some(p0) = predict(&models[0], a[i]) else {
            return Ok(Value::Null);
        };
        let Some(p1) = predict(&models[1], a[i]) else {
            return Ok(Value::Null);
        };
        let pred = if ids[i] % 2 == 0 { p1 } else { p0 };
        let d = [pred[0] - b[i].x as f64, pred[1] - b[i].y as f64];
        errors.push(d[0].hypot(d[1]));
        disagreement.push((p0[0] - p1[0]).hypot(p0[1] - p1[1]));
        let col = (a[i].x / 240.).floor().clamp(0., 2.) as usize;
        let row = (a[i].y / 135.).floor().clamp(0., 2.) as usize;
        regions[row * 3 + col].push(d);
    }
    let grid:Vec<Value>=regions.iter().map(|r|if r.len()<8{Value::Null}else{let dx:Vec<_>=r.iter().map(|d|d[0]).collect();let dy:Vec<_>=r.iter().map(|d|d[1]).collect();json!({"count":r.len(),"median_dx":percentile(&dx,0.5),"median_dy":percentile(&dy,0.5),"median_error":percentile(&r.iter().map(|d|d[0].hypot(d[1])).collect::<Vec<_>>(),0.5)})}).collect();
    Ok(
        json!({"heldout_median_px":percentile(&errors,0.5),"heldout_p90_px":percentile(&errors,0.9),"split_disagreement_median_px":percentile(&disagreement,0.5),"grid":grid}),
    )
}
fn cell(p: Point2f) -> usize {
    (p.y / 135.).floor().clamp(0., 2.) as usize * 3 + (p.x / 240.).floor().clamp(0., 2.) as usize
}
// Entire regions are excluded from training; parity alone can share a dominant plane.
fn regional(a: &[Point2f], b: &[Point2f]) -> opencv::Result<Value> {
    let mut out = vec![];
    for region in 0..9 {
        let train: Vec<_> = (0..a.len()).filter(|&i| cell(a[i]) != region).collect();
        let test: Vec<_> = (0..a.len()).filter(|&i| cell(a[i]) == region).collect();
        if train.len() < 60 || test.len() < 8 {
            out.push(Value::Null);
            continue;
        }
        let aa: Vector<Point2f> = train.iter().map(|&i| a[i]).collect();
        let bb: Vector<Point2f> = train.iter().map(|&i| b[i]).collect();
        let model = fit(&aa, &bb, true)?;
        let errors: Option<Vec<f64>> = test
            .iter()
            .map(|&i| {
                predict(&model, a[i]).map(|p| (p[0] - b[i].x as f64).hypot(p[1] - b[i].y as f64))
            })
            .collect();
        out.push(match errors {
            Some(e) => json!({"count":test.len(),"median_px":percentile(&e,0.5),"p90_px":percentile(&e,0.9)}),
            None => Value::Null,
        });
    }
    Ok(json!(out))
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 7 {
        return Err(
            "usage: perspective_track_probe VIDEO SOURCE_OFFSET START END BURSTS_JSON OUTPUT_JSON"
                .into(),
        );
    }
    let offset: f64 = a[2].parse()?;
    let start: f64 = a[3].parse()?;
    let end: f64 = a[4].parse()?;
    let bursts: Vec<(f64, f64)> = serde_json::from_str(&std::fs::read_to_string(&a[5])?)?;
    let mut cap = videoio::VideoCapture::from_file(&a[1], videoio::CAP_ANY)?;
    if !cap.is_opened()? || (cap.get(videoio::CAP_PROP_FPS)? - 100.).abs() > 1e-6 {
        return Err("expected 100fps input".into());
    }
    if !start.is_finite()
        || !end.is_finite()
        || !offset.is_finite()
        || start < offset
        || end <= start
    {
        return Err("invalid time window".into());
    }
    cap.set(
        videoio::CAP_PROP_POS_FRAMES,
        ((start - offset) * 100.).round(),
    )?;
    let count = ((end - start) * 100.).round() as usize;
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
            Size::new(720, 405),
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
                        && z.x < 720.
                        && z.y >= 0.
                        && z.y < 405.
                })
                .collect();
            let aa: Vec<_> = good.iter().map(|&i| points.get(i).unwrap()).collect();
            let bb: Vec<_> = good.iter().map(|&i| forward.get(i).unwrap()).collect();
            let kept: Vec<_> = good.iter().map(|&i| ids[i]).collect();
            let time = start + (f as f64 - 0.5) / 100.;
            let active = bursts.iter().any(|&(s, e)| time >= s && time <= e);
            let (sim, hom) = if good.len() >= 80 {
                (
                    score(&aa, &bb, &kept, false)?,
                    score(&aa, &bb, &kept, true)?,
                )
            } else {
                (Value::Null, Value::Null)
            };
            let kept_ages: Vec<_> = good.iter().map(|&i| ages[i] + 1).collect();
            let region_check = regional(&aa, &bb)?;
            let mature: Vec<_> = (0..aa.len()).filter(|&i| kept_ages[i] >= 10).collect();
            let ma: Vec<_> = mature.iter().map(|&i| aa[i]).collect();
            let mb: Vec<_> = mature.iter().map(|&i| bb[i]).collect();
            let mi: Vec<_> = mature.iter().map(|&i| kept[i]).collect();
            let mature_regions = regional(&ma, &mb)?;
            let mature_hom = if ma.len() >= 80 {
                score(&ma, &mb, &mi, true)?
            } else {
                Value::Null
            };
            rows.push(json!({"mature_leave_region_out":mature_regions,"leave_region_out":region_check,"mature_tracks":ma.len(),"mature_homography":mature_hom,"age_median_frames":if kept_ages.is_empty(){None}else{Some(percentile(&kept_ages.iter().map(|&x|x as f64).collect::<Vec<_>>(),0.5))},"t":time,"active":active,"replenished":replenished,"tracks":good.len(),"similarity":sim,"homography":hom}));
            points = bb.into_iter().collect();
            ids = kept;
            ages = kept_ages;
        }
        prev = Some(gray);
    }
    std::fs::write(
        &a[6],
        serde_json::to_vec(
            &json!({"window":[start,end],"grid":"row-major 3x3, 720x405 coordinates","rows":rows}),
        )?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn excluded_region_exposes_independent_motion_and_missing_coverage() {
        let a: Vec<Point2f> = (0..18)
            .flat_map(|r| {
                (0..30).map(move |c| Point2f::new(12. + c as f32 * 24., 10. + r as f32 * 22.))
            })
            .collect();
        let b: Vec<_> = a
            .iter()
            .map(|&p| Point2f::new(p.x + 2. + if cell(p) == 2 { 5. } else { 0. }, p.y - 3.))
            .collect();
        let result = regional(&a, &b).unwrap();
        assert!(result[2]["median_px"].as_f64().unwrap() > 4.9);
        let sparse: Vec<_> = a.iter().copied().filter(|&p| cell(p) != 2).collect();
        assert!(regional(&sparse, &sparse).unwrap()[2].is_null());
    }
    #[test]
    fn heldout_projective_motion_is_not_mistaken_for_similarity() {
        let a: Vec<Point2f> = (0..12)
            .flat_map(|r| {
                (0..20).map(move |c| Point2f::new(20.0 + c as f32 * 35.0, 20.0 + r as f32 * 32.0))
            })
            .collect();
        let b: Vec<_> = a
            .iter()
            .map(|p| {
                let w = 1.0 + 0.0005 * p.x - 0.0004 * p.y;
                Point2f::new((p.x + 0.01 * p.y + 2.0) / w, (-0.01 * p.x + p.y - 3.0) / w)
            })
            .collect();
        let ids: Vec<_> = (0..a.len()).collect();
        let sim = score(&a, &b, &ids, false).unwrap();
        let hom = score(&a, &b, &ids, true).unwrap();
        assert!(hom["heldout_median_px"].as_f64().unwrap() < 0.001);
        assert!(hom["split_disagreement_median_px"].as_f64().unwrap() < 0.001);
        assert!(sim["heldout_median_px"].as_f64().unwrap() > 1.0);
    }
    #[test]
    fn simple_translation_fits_both_models_on_heldout_points() {
        let a: Vec<Point2f> = (0..12)
            .flat_map(|r| {
                (0..20).map(move |c| Point2f::new(20.0 + c as f32 * 35.0, 20.0 + r as f32 * 32.0))
            })
            .collect();
        let b: Vec<_> = a
            .iter()
            .map(|p| Point2f::new(p.x + 2.0, p.y - 3.0))
            .collect();
        let ids: Vec<_> = (0..a.len()).collect();
        for projective in [false, true] {
            let result = score(&a, &b, &ids, projective).unwrap();
            assert!(result["heldout_median_px"].as_f64().unwrap() < 0.001);
            assert!(result["grid"]
                .as_array()
                .unwrap()
                .iter()
                .all(|v| !v.is_null()));
        }
    }
}
