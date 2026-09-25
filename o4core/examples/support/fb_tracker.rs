//! Research-only copy of the established tracker, adding a 1px half-resolution round-trip gate.
//! Optical rotation measurement. Ports o4fix.video_rates/_pair_rotation
//! (o4fix.py:273-349). RNG seeded per frame pair: set_rng_seed(1_000_000+fidx)
//! — OpenCV's theRNG is thread-local, so this is deterministic under rayon.
use o4core::error::O4Error;
use o4core::telemetry::Meta;
use opencv::core::{self, no_array, Mat, Point2f, Size, TermCriteria, Vector};
use opencv::prelude::*;
use opencv::{calib3d, imgproc, video, videoio};
use rayon::prelude::*;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct OpticalRates {
    pub t: Vec<f64>,
    pub omega: Vec<[f64; 3]>, // rad/s, camera frame
    pub quality: Vec<f64>,
}

fn k_d(meta: &Meta, w: i32, h: i32) -> (Mat, Mat) {
    let km = meta.camera_matrix.unwrap_or([
        [546.4027, 0.0, w as f64 / 2.0],
        [0.0, 546.4027, h as f64 / 2.0],
        [0.0, 0.0, 1.0],
    ]);
    let d = meta
        .distortion
        .unwrap_or([0.1551311, 0.1371409, -0.0938614, 0.0041704]);
    let (cw, ch) = (
        meta.calib_w.unwrap_or(w as f64),
        meta.calib_h.unwrap_or(h as f64),
    );
    let mut k = km;
    for c in 0..3 {
        k[0][c] *= w as f64 / cw;
        k[1][c] *= h as f64 / ch;
    }
    let k_mat = Mat::from_slice_2d(&k).unwrap();
    // Mat::from_slice borrows its input (returns BoxedRef tied to `d`'s lifetime);
    // `d` is a local that would be dropped at function exit, so copy into an
    // owned Mat via try_clone() before returning.
    let d_mat = Mat::from_slice(&d).unwrap().try_clone().unwrap();
    (k_mat, d_mat)
}

pub fn video_rates(
    video_path: &Path,
    intervals: &[(f64, f64)],
    meta: &Meta,
    cancel: &AtomicBool,
    on_interval: &(dyn Fn(usize, usize) + Sync),
    fb_gate: bool,
    seed_delta: i32,
    frame_gap: usize,
) -> Result<OpticalRates, O4Error> {
    assert!(frame_gap > 0 && frame_gap <= 4);
    // probe fps/size once
    let cap = videoio::VideoCapture::from_file(video_path.to_str().unwrap(), videoio::CAP_ANY)?;
    let fps = cap.get(videoio::CAP_PROP_FPS)?;
    let w = cap.get(videoio::CAP_PROP_FRAME_WIDTH)? as i32;
    let h = cap.get(videoio::CAP_PROP_FRAME_HEIGHT)? as i32;
    if !cap.is_opened()? || !fps.is_finite() || fps <= 0.0 || w < 2 || h < 2 {
        return Err(O4Error::Cv(
            "cannot decode video or invalid frame rate/dimensions".into(),
        ));
    }
    drop(cap);
    let done = std::sync::atomic::AtomicUsize::new(0);
    let results: Result<Vec<_>, O4Error> = intervals
        .par_iter()
        .map(|&(a, b)| {
            if cancel.load(Ordering::Relaxed) {
                return Err(O4Error::Cancelled);
            }
            let r = interval_rates(
                video_path, a, b, fps, w, h, meta, cancel, fb_gate, seed_delta, frame_gap,
            );
            on_interval(done.fetch_add(1, Ordering::Relaxed) + 1, intervals.len());
            r
        })
        .collect();
    let mut out = OpticalRates {
        t: vec![],
        omega: vec![],
        quality: vec![],
    };
    for part in results? {
        // interval order preserved
        out.t.extend(part.t);
        out.omega.extend(part.omega);
        out.quality.extend(part.quality);
    }
    Ok(out)
}

// cancel is a Rust-only addition over o4fix.py's interval helper (no
// cancellation in Python); pushes the arg count past clippy's default limit.
#[allow(clippy::too_many_arguments)]
fn interval_rates(
    video_path: &Path,
    a: f64,
    b: f64,
    fps: f64,
    w: i32,
    h: i32,
    meta: &Meta,
    cancel: &AtomicBool,
    fb_gate: bool,
    seed_delta: i32,
    frame_gap: usize,
) -> Result<OpticalRates, O4Error> {
    let (k, d) = k_d(meta, w, h);
    let mut cap = videoio::VideoCapture::from_file(video_path.to_str().unwrap(), videoio::CAP_ANY)?;
    let f0 = (a * fps).max(0.0) as i64;
    let f1 = (b * fps) as i64 + 1;
    cap.set(videoio::CAP_PROP_POS_FRAMES, f0 as f64)?;
    let mut out = OpticalRates {
        t: vec![],
        omega: vec![],
        quality: vec![],
    };
    let mut prev: Vec<Mat> = Vec::new();
    let mut frame = Mat::default();
    for fidx in f0..=f1 {
        if cancel.load(Ordering::Relaxed) {
            return Err(O4Error::Cancelled);
        }
        if !cap.read(&mut frame)? {
            break;
        }
        let mut gray = Mat::default();
        // API-fix: generated cvt_color() takes (src, dst, code, dst_cn, hint);
        // cvt_color_def() is the 3-arg form with dst_cn=0 / hint=ALGO_HINT_DEFAULT,
        // matching Python's cv2.cvtColor(frame, cv2.COLOR_BGR2GRAY) exactly.
        imgproc::cvt_color_def(&frame, &mut gray, imgproc::COLOR_BGR2GRAY)?;
        let mut half = Mat::default();
        imgproc::resize(
            &gray,
            &mut half,
            Size::new(w / 2, h / 2),
            0.0,
            0.0,
            imgproc::INTER_LINEAR,
        )?;
        if prev.len() == frame_gap {
            let p = &prev[0];
            core::set_rng_seed(1_000_000 + fidx as i32 + seed_delta)?;
            let (rvec, q) = pair_rotation(p, &half, &k, &d, fb_gate)?;
            out.t.push((fidx as f64 - frame_gap as f64 * 0.5) / fps);
            out.omega.push([
                rvec[0] * fps / frame_gap as f64,
                rvec[1] * fps / frame_gap as f64,
                rvec[2] * fps / frame_gap as f64,
            ]);
            out.quality.push(q);
        }
        prev.push(half);
        if prev.len() > frame_gap {
            prev.remove(0);
        }
    }
    Ok(out)
}

/// Ports o4fix._pair_rotation (o4fix.py:321-349).
fn pair_rotation(
    prev: &Mat,
    gray: &Mat,
    k: &Mat,
    d: &Mat,
    fb_gate: bool,
) -> Result<([f64; 3], f64), O4Error> {
    let mut p0 = Vector::<Point2f>::new();
    imgproc::good_features_to_track(prev, &mut p0, 600, 0.01, 12.0, &no_array(), 7, false, 0.04)?;
    if p0.len() < 40 {
        return Ok(([0.0; 3], 0.0));
    }
    let mut p1 = Vector::<Point2f>::new();
    let mut st = Vector::<u8>::new();
    let mut err = Vector::<f32>::new();
    video::calc_optical_flow_pyr_lk(
        prev,
        gray,
        &p0,
        &mut p1,
        &mut st,
        &mut err,
        Size::new(21, 21),
        3,
        TermCriteria::new(core::TermCriteria_COUNT + core::TermCriteria_EPS, 30, 0.01)?,
        0,
        1e-4,
    )?;
    let good: Vec<usize> = if fb_gate {
        let mut back = Vector::<Point2f>::new();
        let mut back_status = Vector::<u8>::new();
        let mut back_error = Vector::<f32>::new();
        video::calc_optical_flow_pyr_lk(
            gray,
            prev,
            &p1,
            &mut back,
            &mut back_status,
            &mut back_error,
            Size::new(21, 21),
            3,
            TermCriteria::new(core::TermCriteria_COUNT + core::TermCriteria_EPS, 30, 0.01)?,
            0,
            1e-4,
        )?;
        let good: Vec<usize> = (0..st.len())
            .filter(|&i| {
                if st.get(i).unwrap() != 1 || back_status.get(i).unwrap() != 1 {
                    return false;
                }
                let a = p0.get(i).unwrap();
                let b = back.get(i).unwrap();
                let end = p1.get(i).unwrap();
                end.x.is_finite()
                    && end.y.is_finite()
                    && b.x.is_finite()
                    && b.y.is_finite()
                    && (a.x - b.x).powi(2) + (a.y - b.y).powi(2) <= 1.0
            })
            .collect();
        good
    } else {
        (0..st.len()).filter(|&i| st.get(i).unwrap() == 1).collect()
    };
    if good.len() < 40 {
        return Ok(([0.0; 3], 0.0));
    }
    // x2 back to full res, then fisheye-undistort to normalized coords
    let mk = |src: &Vector<Point2f>| -> Vector<Point2f> {
        good.iter()
            .map(|&i| {
                let p = src.get(i).unwrap();
                Point2f::new(p.x * 2.0, p.y * 2.0)
            })
            .collect()
    };
    let (g0, g1) = (mk(&p0), mk(&p1));
    let mut u0 = Vector::<Point2f>::new();
    let mut u1 = Vector::<Point2f>::new();
    // API-fix: fisheye_undistort_points() (7-arg, with R/P/criteria) has no
    // matching call here; fisheye_undistort_points_def() is the 4-arg form
    // (r=noArray(), p=noArray(), criteria=default) — matches Python's
    // cv2.fisheye.undistortPoints(pts, K, D) (no R/P passed) exactly.
    calib3d::fisheye_undistort_points_def(&g0, &mut u0, k, d)?;
    calib3d::fisheye_undistort_points_def(&g1, &mut u1, k, d)?;
    let eye = Mat::eye(3, 3, core::CV_64F)?.to_mat()?;
    let mut inliers = Mat::default();
    let e = calib3d::find_essential_mat(
        &u0,
        &u1,
        &eye,
        calib3d::RANSAC,
        0.999,
        0.002,
        1000,
        &mut inliers,
    )?;
    if e.rows() != 3 || e.cols() != 3 {
        return Ok(([0.0; 3], 0.0));
    }
    let inl: i32 = (0..inliers.rows())
        .map(|i| *inliers.at::<u8>(i).unwrap() as i32)
        .sum();
    if inl < 60 {
        return Ok(([0.0; 3], 0.0));
    }
    let (mut r1, mut r2, mut t) = (Mat::default(), Mat::default(), Mat::default());
    calib3d::decompose_essential_mat(&e, &mut r1, &mut r2, &mut t)?;
    let rod = |r: &Mat| -> Result<[f64; 3], O4Error> {
        let mut rv = Mat::default();
        calib3d::rodrigues(r, &mut rv, &mut no_array())?;
        Ok([*rv.at::<f64>(0)?, *rv.at::<f64>(1)?, *rv.at::<f64>(2)?])
    };
    let (v1, v2) = (rod(&r1)?, rod(&r2)?);
    let n = |v: &[f64; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    let rvec = if n(&v1) <= n(&v2) { v1 } else { v2 };
    let quality = (((inl - 60) as f64) / 150.0).clamp(0.0, 1.0);
    Ok((rvec, quality))
}
