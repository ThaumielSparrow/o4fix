//! Optical rotation measurement. Ports o4fix.video_rates/_pair_rotation
//! (o4fix.py:273-349). RNG seeded per frame pair: set_rng_seed(1_000_000+fidx)
//! — OpenCV's theRNG is thread-local, so this is deterministic under rayon.
use crate::error::O4Error;
use crate::telemetry::Meta;
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
) -> Result<OpticalRates, O4Error> {
    // probe fps/size once
    let cap = videoio::VideoCapture::from_file(video_path.to_str().unwrap(), videoio::CAP_ANY)?;
    let fps = cap.get(videoio::CAP_PROP_FPS)?;
    let w = cap.get(videoio::CAP_PROP_FRAME_WIDTH)? as i32;
    let h = cap.get(videoio::CAP_PROP_FRAME_HEIGHT)? as i32;
    drop(cap);
    let done = std::sync::atomic::AtomicUsize::new(0);
    let results: Result<Vec<_>, O4Error> = intervals
        .par_iter()
        .map(|&(a, b)| {
            if cancel.load(Ordering::Relaxed) {
                return Err(O4Error::Cancelled);
            }
            let r = interval_rates(video_path, a, b, fps, w, h, meta);
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

fn interval_rates(
    video_path: &Path,
    a: f64,
    b: f64,
    fps: f64,
    w: i32,
    h: i32,
    meta: &Meta,
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
    let mut prev: Option<Mat> = None;
    let mut frame = Mat::default();
    for fidx in f0..=f1 {
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
        if let Some(p) = &prev {
            core::set_rng_seed(1_000_000 + fidx as i32)?;
            let (rvec, q) = pair_rotation(p, &half, &k, &d)?;
            out.t.push((fidx as f64 - 0.5) / fps);
            out.omega
                .push([rvec[0] * fps, rvec[1] * fps, rvec[2] * fps]);
            out.quality.push(q);
        }
        prev = Some(half);
    }
    Ok(out)
}

/// Ports o4fix._pair_rotation (o4fix.py:321-349).
fn pair_rotation(prev: &Mat, gray: &Mat, k: &Mat, d: &Mat) -> Result<([f64; 3], f64), O4Error> {
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
    let good: Vec<usize> = (0..st.len()).filter(|&i| st.get(i).unwrap() == 1).collect();
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
    let quality = (((inl - 60) as f64) / 150.0).min(1.0).max(0.0);
    Ok((rvec, quality))
}

use nalgebra::Matrix3;

pub struct Alignment {
    pub shift: f64,
    pub n: [[f64; 3]; 3],
    pub r2: f64,
}

/// Ports o4fix.fit_video_alignment (o4fix.py:352-400).
pub fn fit_video_alignment(
    opt: &OpticalRates,
    tm: &[f64],
    gyro_deg: &[[f64; 3]],
    fs: f64,
) -> Option<Alignment> {
    use crate::dsp;
    let good: Vec<usize> = (0..opt.quality.len())
        .filter(|&i| opt.quality[i] > 0.5)
        .collect();
    if good.len() < 200 {
        return None;
    }
    const R2D: f64 = 180.0 / std::f64::consts::PI;
    let ovd: Vec<[f64; 3]> = good
        .iter()
        .map(|&i| std::array::from_fn(|k| opt.omega[i][k] * R2D))
        .collect();
    let tv: Vec<f64> = good.iter().map(|&i| opt.t[i]).collect();

    // gyro_at: 10 ms-smoothed gyro sampled at query times (o4fix.py:363-366)
    let sm = dsp::uniform_filter3(gyro_deg, ((fs / 100.0) as usize).max(1));
    let cols: Vec<Vec<f64>> = (0..3).map(|k| sm.iter().map(|r| r[k]).collect()).collect();
    let gyro_at = |tq: &[f64]| -> Vec<[f64; 3]> {
        let per: Vec<Vec<f64>> = (0..3).map(|k| dsp::interp(tq, tm, &cols[k])).collect();
        (0..tq.len())
            .map(|i| [per[0][i], per[1][i], per[2][i]])
            .collect()
    };
    let ba = dsp::butter_low(2, 5.0 / 50.0);
    let lowf = |x: &[[f64; 3]]| dsp::filtfilt3(&ba, x);

    let procrustes = |b: &[[f64; 3]], a: &[[f64; 3]]| -> [[f64; 3]; 3] {
        let mut m = Matrix3::<f64>::zeros(); // B^T A
        for i in 0..b.len() {
            for r in 0..3 {
                for c in 0..3 {
                    m[(r, c)] += b[i][r] * a[i][c];
                }
            }
        }
        let svd = m.svd(true, true);
        let n = svd.u.unwrap() * svd.v_t.unwrap();
        std::array::from_fn(|r| std::array::from_fn(|c| n[(r, c)]))
    };
    let apply = |b: &[[f64; 3]], n: &[[f64; 3]; 3]| -> Vec<[f64; 3]> {
        b.iter()
            .map(|row| std::array::from_fn(|c| (0..3).map(|r| row[r] * n[r][c]).sum()))
            .collect()
    };
    let pearson = |x: &[f64], y: &[f64]| -> f64 {
        let n = x.len() as f64;
        let (mx, my) = (x.iter().sum::<f64>() / n, y.iter().sum::<f64>() / n);
        let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
        for i in 0..x.len() {
            let (dx, dy) = (x[i] - mx, y[i] - my);
            sxy += dx * dy;
            sxx += dx * dx;
            syy += dy * dy;
        }
        sxy / (sxx.sqrt() * syy.sqrt())
    };

    let b_low = lowf(&ovd);
    let mut shift = 0.0f64;
    let mut n_mat = [[0.0; 3]; 3];
    for _ in 0..3 {
        let tq: Vec<f64> = tv.iter().map(|t| t + shift).collect();
        let a_low = lowf(&gyro_at(&tq));
        n_mat = procrustes(&b_low, &a_low);
        let p = apply(&b_low, &n_mat);
        let mut best = (f64::NEG_INFINITY, shift);
        for k in 0..=60 {
            // scan shift-0.06 .. +0.06 by 2 ms
            let sh = shift - 0.06 + 0.002 * k as f64;
            let tq: Vec<f64> = tv.iter().map(|t| t + sh).collect();
            let g = lowf(&gyro_at(&tq));
            let r: f64 = (0..3)
                .map(|ax| {
                    pearson(
                        &g.iter().map(|r| r[ax]).collect::<Vec<_>>(),
                        &p.iter().map(|r| r[ax]).collect::<Vec<_>>(),
                    )
                })
                .sum::<f64>()
                / 3.0;
            if r > best.0 {
                best = (r, sh);
            }
        }
        shift = best.1;
    }
    let tq: Vec<f64> = tv.iter().map(|t| t + shift).collect();
    let g = lowf(&gyro_at(&tq));
    let p = apply(&b_low, &n_mat);
    let (mut ss_res, mut ss_tot) = (0.0, 0.0);
    let mean: [f64; 3] =
        std::array::from_fn(|k| g.iter().map(|r| r[k]).sum::<f64>() / g.len() as f64);
    for i in 0..g.len() {
        for k in 0..3 {
            ss_res += (g[i][k] - p[i][k]).powi(2);
            ss_tot += (g[i][k] - mean[k]).powi(2);
        }
    }
    Some(Alignment {
        shift,
        n: n_mat,
        r2: 1.0 - ss_res / ss_tot,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_video_alignment_returns_none_below_good_gate() {
        let opt = OpticalRates {
            t: vec![0.0; 10],
            omega: vec![[0.0; 3]; 10],
            quality: vec![0.0; 10],
        };
        let result = fit_video_alignment(&opt, &[0.0, 0.001], &[[0.0; 3]; 2], 1000.0);
        assert!(result.is_none());
    }
}
