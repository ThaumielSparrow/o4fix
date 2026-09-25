//! OpenCV measurement of per-pair residual rotation against an orientation track.
use super::geometry::{fit_small_rotation, mat_vec, rotate, Orientation, O4P_MOUNT, V3};
use super::signal::PairSeries;
use super::RefineConfig;
use crate::error::O4Error;
use crate::quat::{qconj, qlog, qmul};
use crate::telemetry::Meta;
use opencv::core::{self, Mat, Point2f, Size, TermCriteria, Vector};
use opencv::prelude::*;
use opencv::{calib3d, imgproc, video, videoio};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct VideoInfo { pub fps: f64, pub width: i32, pub height: i32, pub frames: i64 }

pub fn video_info(video: &Path) -> Result<VideoInfo, O4Error> {
    let cap = videoio::VideoCapture::from_file(video.to_str().unwrap_or_default(), videoio::CAP_ANY)?;
    let info = VideoInfo {
        fps: cap.get(videoio::CAP_PROP_FPS)?,
        width: cap.get(videoio::CAP_PROP_FRAME_WIDTH)? as i32,
        height: cap.get(videoio::CAP_PROP_FRAME_HEIGHT)? as i32,
        frames: cap.get(videoio::CAP_PROP_FRAME_COUNT)? as i64,
    };
    if !cap.is_opened()? || !info.fps.is_finite() || info.fps <= 0.0 || info.width < 2 || info.height < 2 {
        return Err(O4Error::Cv("cannot decode video for refinement".into()));
    }
    Ok(info)
}

pub fn window_frames(a: f64, b: f64, info: &VideoInfo) -> (i64, i64) {
    let last = (info.frames - 1).max(0);
    (((a * info.fps).round() as i64).clamp(0, last), ((b * info.fps).round() as i64).clamp(0, last))
}

fn track(p: &Mat, g: &Mat, max_features: i32) -> opencv::Result<Vec<(Point2f, Point2f)>> {
    let mut a = Vector::<Point2f>::new();
    imgproc::good_features_to_track(p, &mut a, max_features, 0.005, 18.0, &core::no_array(), 7, false, 0.04)?;
    if a.len() < 40 {
        return Ok(vec![]);
    }
    let crit = TermCriteria::new(core::TermCriteria_COUNT + core::TermCriteria_EPS, 30, 0.01)?;
    let (mut b, mut st, mut err) = (Vector::<Point2f>::new(), Vector::<u8>::new(), Vector::<f32>::new());
    video::calc_optical_flow_pyr_lk(p, g, &a, &mut b, &mut st, &mut err, Size::new(21, 21), 4, crit, 0, 1e-4)?;
    let (mut r, mut sr, mut er) = (Vector::<Point2f>::new(), Vector::<u8>::new(), Vector::<f32>::new());
    video::calc_optical_flow_pyr_lk(g, p, &b, &mut r, &mut sr, &mut er, Size::new(21, 21), 4, crit, 0, 1e-4)?;
    let mut out = vec![];
    for i in 0..a.len() {
        let (x, y, z) = (a.get(i)?, b.get(i)?, r.get(i)?);
        if st.get(i)? == 1 && sr.get(i)? == 1 && (z.x - x.x).hypot(z.y - x.y) < 0.5 {
            out.push((x, y));
        }
    }
    Ok(out)
}

fn bearings(pts: &[Point2f], k: &Mat, d: &Mat) -> opencv::Result<Vec<V3>> {
    let src: Vector<Point2f> = pts.iter().copied().collect();
    let mut u = Vector::<Point2f>::new();
    calib3d::fisheye_undistort_points_def(&src, &mut u, k, d)?;
    Ok(u.iter()
        .map(|p| {
            let m = ((p.x as f64).powi(2) + (p.y as f64).powi(2) + 1.0).sqrt();
            [p.x as f64 / m, p.y as f64 / m, 1.0 / m]
        })
        .collect())
}

#[allow(clippy::too_many_arguments)]
pub fn measure_window(video: &Path, a: f64, b: f64, info: &VideoInfo, meta: &Meta, cfg: &RefineConfig,
                      orient: &Orientation, cancel: &AtomicBool) -> Result<PairSeries, O4Error> {
    let (k, d) = crate::optical::k_d(meta, info.width, info.height);
    let readout = cfg.readout_ms / 1000.0;
    let (f0, f1) = window_frames(a, b, info);
    let mut cap = videoio::VideoCapture::from_file(video.to_str().unwrap_or_default(), videoio::CAP_ANY)?;
    cap.set(videoio::CAP_PROP_POS_FRAMES, f0 as f64)?;
    let mut s = PairSeries { t: vec![], resid: vec![], inliers: vec![], tel_rate: vec![] };
    let (mut prev, mut frame) = (None::<Mat>, Mat::default());
    let row_t = |fi: i64, y: f32| fi as f64 / info.fps + y as f64 / info.height as f64 * readout;
    for fi in f0..=f1 {
        if cancel.load(Ordering::Relaxed) {
            return Err(O4Error::Cancelled);
        }
        if !cap.read(&mut frame)? {
            break;
        }
        let mut g = Mat::default();
        imgproc::cvt_color_def(&frame, &mut g, imgproc::COLOR_BGR2GRAY)?;
        if let Some(p) = &prev {
            let fa = fi - 1;
            let tmid = fi as f64 / info.fps + readout * 0.5;
            let qa_mid = orient.at(fa as f64 / info.fps + readout * 0.5);
            let qb_mid = orient.at(tmid);
            let dv = qlog(qmul(qconj(qa_mid), qb_mid));
            let tel_rate = (dv[0] * dv[0] + dv[1] * dv[1] + dv[2] * dv[2]).sqrt() * info.fps;
            let tr = track(p, &g, cfg.max_features)?;
            let mut resid = None;
            let mut n = 0;
            if tr.len() >= 40 {
                let ba = bearings(&tr.iter().map(|x| x.0).collect::<Vec<_>>(), &k, &d)?;
                let bb = bearings(&tr.iter().map(|x| x.1).collect::<Vec<_>>(), &k, &d)?;
                let inside = |v: &V3| v[2] > 0.0 && (v[0] / v[2]).abs() < cfg.crop.0 && (v[1] / v[2]).abs() < cfg.crop.1;
                let (mut wa, mut wb) = (vec![], vec![]);
                for i in 0..tr.len() {
                    if !(inside(&ba[i]) && inside(&bb[i])) {
                        continue;
                    }
                    wa.push(rotate(orient.at(row_t(fa, tr[i].0.y)), mat_vec(&O4P_MOUNT, ba[i])));
                    wb.push(rotate(orient.at(row_t(fi, tr[i].1.y)), mat_vec(&O4P_MOUNT, bb[i])));
                }
                if let Some((dw, ni)) = fit_small_rotation(&wa, &wb, cfg.min_inliers, 0.0008) {
                    let q_mid = orient.at(tmid);
                    let db = rotate(qconj(q_mid), dw);
                    resid = Some([db[0] * info.fps, db[1] * info.fps, db[2] * info.fps]);
                    n = ni;
                }
            }
            s.t.push(tmid);
            s.resid.push(resid);
            s.inliers.push(n);
            s.tel_rate.push(tel_rate);
        }
        prev = Some(g);
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn window_frames_uses_video_fps() {
        let i100 = VideoInfo { fps: 100.0, width: 1440, height: 1080, frames: 1000 };
        assert_eq!(window_frames(2.0, 3.5, &i100), (200, 350));
        let i60 = VideoInfo { fps: 59.94, width: 3840, height: 2160, frames: 600 };
        assert_eq!(window_frames(2.0, 3.5, &i60), (120, 210));
        // clipped to the video
        assert_eq!(window_frames(-1.0, 50.0, &i100), (0, 999));
    }
}
