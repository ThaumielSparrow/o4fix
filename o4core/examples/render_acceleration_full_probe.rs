//! Research-only full-resolution triplet audit; output normalized to 720x405 coordinates.
use opencv::{
    core::{self, Mat, Point2f, Rect, Size, TermCriteria, Vector},
    imgproc,
    prelude::*,
    video, videoio,
};
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
fn center(a: &[[f64; 2]]) -> [f64; 2] {
    [
        median(a.iter().map(|x| x[0]).collect()),
        median(a.iter().map(|x| x[1]).collect()),
    ]
}
fn cell(p: Point2f) -> usize {
    (p.y / 135.).floor().clamp(0., 2.) as usize * 3 + (p.x / 240.).floor().clamp(0., 2.) as usize
}
// Read-only linear translation + rotational acceleration field, not a camera pose fit.
fn rigid_fit(points: &[Point2f], acc: &[[f64; 2]], ids: &[usize]) -> [f64; 3] {
    let n = ids.len() as f64;
    let mean: [f64; 4] = std::array::from_fn(|k| {
        ids.iter()
            .map(|&i| match k {
                0 => points[i].x as f64 - 360.,
                1 => points[i].y as f64 - 202.5,
                2 => acc[i][0],
                _ => acc[i][1],
            })
            .sum::<f64>()
            / n
    });
    let mut num = 0.;
    let mut den = 0.;
    for &i in ids {
        let x = points[i].x as f64 - 360. - mean[0];
        let y = points[i].y as f64 - 202.5 - mean[1];
        num += x * (acc[i][1] - mean[3]) - y * (acc[i][0] - mean[2]);
        den += x * x + y * y;
    }
    let alpha = if den > 1e-9 { num / den } else { 0. };
    [mean[2] + alpha * mean[1], mean[3] - alpha * mean[0], alpha]
}
fn score(points: &[Point2f], acc: &[[f64; 2]]) -> Value {
    let mut groups = [vec![], vec![]];
    let mut counts = [0usize; 9];
    let mut region = vec![vec![]; 9];
    let mut labels = vec![];
    for (&p, &a) in points.iter().zip(acc) {
        let c = cell(p);
        let g = (counts[c] + c) % 2;
        counts[c] += 1;
        groups[g].push(a);
        labels.push(g);
        region[c].push(a);
    }
    if groups.iter().any(|g| g.len() < 30) {
        return Value::Null;
    }
    let estimates = [center(&groups[0]), center(&groups[1])];
    let mut before = 0.;
    let mut after = 0.;
    for (a, g) in acc.iter().zip(labels) {
        for (k, &v) in a.iter().enumerate() {
            before += v * v;
            after += (v - estimates[1 - g][k]).powi(2);
        }
    }
    let regions: Vec<_> = region
        .iter()
        .map(|r| {
            if r.len() >= 8 {
                json!({"n":r.len(),"acc":center(r)})
            } else {
                Value::Null
            }
        })
        .collect();
    let mut region_after = 0.;
    let mut region_before = 0.;
    let mut rigid_after = 0.;
    for c in 0..9 {
        if region[c].len() < 8 {
            continue;
        }
        let train: Vec<_> = region
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != c)
            .flat_map(|(_, v)| v.iter().copied())
            .collect();
        if train.len() < 60 {
            continue;
        }
        let ids: Vec<_> = (0..points.len())
            .filter(|&i| cell(points[i]) != c)
            .collect();
        let fit = rigid_fit(points, acc, &ids);
        for i in 0..points.len() {
            if cell(points[i]) == c {
                let pred = [
                    fit[0] - fit[2] * (points[i].y as f64 - 202.5),
                    fit[1] + fit[2] * (points[i].x as f64 - 360.),
                ];
                for (k, value) in pred.iter().enumerate() {
                    rigid_after += (acc[i][k] - value).powi(2);
                }
            }
        }
        let estimate = center(&train);
        for a in &region[c] {
            for k in 0..2 {
                region_before += a[k] * a[k];
                region_after += (a[k] - estimate[k]).powi(2);
            }
        }
    }
    json!({"n":acc.len(),"shared":center(acc),"split":estimates,"regions":regions,"heldout_before_ss":before,"heldout_after_ss":after,"region_before_ss":region_before,"region_after_ss":region_after,"rigid_region_after_ss":rigid_after,"rigid_fit":rigid_fit(points,acc,&(0..points.len()).collect::<Vec<_>>())})
}
fn flow(a: &Mat, b: &Mat, p: &Vector<Point2f>) -> opencv::Result<(Vector<Point2f>, Vector<u8>)> {
    let (mut q, mut status, mut err) = (Vector::new(), Vector::new(), Vector::<f32>::new());
    video::calc_optical_flow_pyr_lk(
        a,
        b,
        p,
        &mut q,
        &mut status,
        &mut err,
        Size::new(41, 41),
        3,
        TermCriteria::new(core::TermCriteria_COUNT + core::TermCriteria_EPS, 30, 0.01)?,
        0,
        1e-4,
    )?;
    Ok((q, status))
}
fn measure(prev: &Mat, cur: &Mat, next: &Mat) -> opencv::Result<Value> {
    let mut p = Vector::<Point2f>::new();
    imgproc::good_features_to_track(
        cur,
        &mut p,
        1200,
        0.01,
        20.,
        &core::no_array(),
        7,
        false,
        0.04,
    )?;
    if p.len() < 60 {
        return Ok(Value::Null);
    }
    let (a, sa) = flow(cur, prev, &p)?;
    let (b, sb) = flow(cur, next, &p)?;
    let (ar, sar) = flow(prev, cur, &a)?;
    let (br, sbr) = flow(next, cur, &b)?;
    let (mut pts, mut acc) = (vec![], vec![]);
    for i in 0..p.len() {
        let q = p.get(i)?;
        let x = a.get(i)?;
        let y = b.get(i)?;
        let ra = ar.get(i)?;
        let rb = br.get(i)?;
        if [sa.get(i)?, sb.get(i)?, sar.get(i)?, sbr.get(i)?].contains(&0)
            || (ra.x - q.x).hypot(ra.y - q.y) > 1.0
            || (rb.x - q.x).hypot(rb.y - q.y) > 1.0
            || [x, y]
                .iter()
                .any(|v| v.x < 10. || v.x > 1430. || v.y < 10. || v.y > 800.)
        {
            continue;
        }
        pts.push(Point2f::new(q.x * 0.5, q.y * 0.5));
        acc.push([
            ((x.x as f64 + y.x as f64) - 2. * q.x as f64) * 0.5,
            ((x.y as f64 + y.y as f64) - 2. * q.y as f64) * 0.5,
        ]);
    }
    Ok(score(&pts, &acc))
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 5 {
        return Err("usage: render_acceleration_full_probe FULL_VIDEO_OR_COMPARISON START PANEL(0/1) OUTPUT".into());
    }
    if std::path::Path::new(&args[4]).exists() {
        return Err("output already exists".into());
    }
    let start: f64 = args[2].parse()?;
    let panel: i32 = args[3].parse()?;
    if !(0..=1).contains(&panel) {
        return Err("invalid panel".into());
    }
    let mut cap = videoio::VideoCapture::from_file(&args[1], videoio::CAP_ANY)?;
    if !cap.is_opened()?
        || (cap.get(videoio::CAP_PROP_FPS)? - 100.).abs() > 1e-6
        || cap.get(videoio::CAP_PROP_FRAME_WIDTH)? < (panel + 1) as f64 * 1440.
        || cap.get(videoio::CAP_PROP_FRAME_HEIGHT)? != 810.
    {
        return Err("unexpected comparison format".into());
    }
    let expected = cap.get(videoio::CAP_PROP_FRAME_COUNT)? as usize;
    let mut frames = std::collections::VecDeque::new();
    let mut rows = vec![];
    let mut raw = Mat::default();
    let mut n = 0usize;
    while cap.read(&mut raw)? {
        let roi = Mat::roi(&raw, Rect::new(panel * 1440, 0, 1440, 810))?;
        let mut gray = Mat::default();
        imgproc::cvt_color_def(&roi, &mut gray, imgproc::COLOR_BGR2GRAY)?;
        frames.push_back(gray);
        n += 1;
        if frames.len() == 3 {
            rows.push(json!({"t":start+(n-2) as f64/100.,"score":measure(&frames[0],&frames[1],&frames[2])?}));
            frames.pop_front();
        }
    }
    if n != expected {
        return Err(format!("decoded {n}, expected {expected}").into());
    }
    std::fs::write(
        &args[4],
        serde_json::to_vec(
            &json!({"input":args[1],"panel":panel,"frames":n,"units":"half-resolution pixels per frame squared; multiply by 10000 for px/s^2","rows":rows}),
        )?,
    )?;
    println!("decoded {n} frames, {} triplets", rows.len());
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_shake_is_predicted_on_heldout_features() {
        let p: Vec<_> = (0..180)
            .map(|i| Point2f::new((i % 18) as f32 * 39. + 10., (i / 18) as f32 * 39. + 10.))
            .collect();
        let a = vec![[0.3, -0.2]; 180];
        let s = score(&p, &a);
        assert!(s["heldout_after_ss"].as_f64().unwrap() < 1e-20);
        assert!(s["heldout_before_ss"].as_f64().unwrap() > 20.);
    }
    #[test]
    fn rotational_acceleration_recovers_known_field() {
        let p: Vec<_> = (0..180)
            .map(|i| Point2f::new((i % 18) as f32 * 39. + 10., (i / 18) as f32 * 39. + 10.))
            .collect();
        let a: Vec<_> = p
            .iter()
            .map(|p| {
                [
                    0.2 - 0.001 * (p.y as f64 - 202.5),
                    -0.1 + 0.001 * (p.x as f64 - 360.),
                ]
            })
            .collect();
        let s = score(&p, &a);
        assert!(s["rigid_region_after_ss"].as_f64().unwrap() < 1e-20);
        assert!((s["rigid_fit"][2].as_f64().unwrap() - 0.001).abs() < 1e-12);
    }
    #[test]
    fn image_tracking_distinguishes_pan_from_translation_shake() -> opencv::Result<()> {
        let mut texture =
            Mat::new_rows_cols_with_default(810, 1440, core::CV_8UC1, core::Scalar::all(0.))?;
        // Deterministic local texture without dependence on OpenCV's shared RNG.
        let mut seed = 12345u32;
        for v in texture.data_bytes_mut()? {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            *v = (seed >> 24) as u8;
        }
        let mut smooth = Mat::default();
        imgproc::gaussian_blur(
            &texture,
            &mut smooth,
            Size::new(5, 5),
            1.,
            1.,
            core::BORDER_REFLECT101,
            core::AlgorithmHint::ALGO_HINT_DEFAULT,
        )?;
        let shift = |dx: f64| -> opencv::Result<Mat> {
            let mut out = Mat::default();
            let m = Mat::from_slice_2d(&[[1., 0., dx], [0., 1., 0.]])?;
            imgproc::warp_affine(
                &smooth,
                &mut out,
                &m,
                Size::new(1440, 810),
                imgproc::INTER_LINEAR,
                core::BORDER_REFLECT101,
                core::Scalar::all(0.),
            )?;
            Ok(out)
        };
        let plus = shift(0.75)?;
        let minus = shift(-0.75)?;
        let pan = measure(&minus, &smooth, &plus)?;
        let shake = measure(&plus, &smooth, &plus)?;
        assert!(pan["shared"][0].as_f64().unwrap().abs() < 0.05);
        assert!((shake["shared"][0].as_f64().unwrap() - 0.75).abs() < 0.1);
        Ok(())
    }
    #[test]
    fn constant_velocity_has_zero_acceleration() {
        let (p, v) = ([20., 30.], [3., -2.]);
        for k in 0..2 {
            assert_eq!((p[k] - v[k]) + (p[k] + v[k]) - 2. * p[k], 0.);
        }
    }
    #[test]
    fn opposing_regions_are_not_removed_by_shared_translation() {
        let p: Vec<_> = (0..180)
            .map(|i| Point2f::new(if i < 90 { 50. } else { 650. }, 100.))
            .collect();
        let a: Vec<_> = (0..180)
            .map(|i| [if i < 90 { 0.3 } else { -0.3 }, 0.])
            .collect();
        let s = score(&p, &a);
        assert_eq!(s["shared"], json!([0., 0.]));
        assert_eq!(s["heldout_before_ss"], s["heldout_after_ss"]);
    }
}
