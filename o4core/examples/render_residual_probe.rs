//! Research-only: per-frame-pair residual motion of a rendered (stabilized) video.
//! Decodes a window with ffmpeg (grayscale, native size), tracks features between
//! consecutive frames and fits a robust similarity. Output rates are about the image
//! center: [dx px/s, dy px/s, roll rad/s, log-scale /s] plus inlier count per pair.
use opencv::{
    calib3d,
    core::{self, Mat, Point2f, Size, TermCriteria, Vector},
    imgproc,
    prelude::*,
    video,
};
use serde_json::json;
use std::io::Read;
use std::process::{Command, Stdio};

/// LK pyramid of one frame (with derivatives), built once and reused by both pairs the frame is in.
fn pyramid(img: &Mat) -> opencv::Result<Vector<Mat>> {
    let mut pyr = Vector::<Mat>::new();
    video::build_optical_flow_pyramid(
        img,
        &mut pyr,
        Size::new(21, 21),
        3,
        true,
        core::BORDER_REFLECT_101,
        core::BORDER_CONSTANT,
        true,
    )?;
    Ok(pyr)
}

fn pair(p: &Mat, g: &Mat, w: f32, h: f32) -> opencv::Result<Option<([f64; 4], usize)>> {
    pair_with(p, g, None, w, h)
}

fn pair_with(
    p: &Mat,
    g: &Mat,
    pyrs: Option<(&Vector<Mat>, &Vector<Mat>)>,
    w: f32,
    h: f32,
) -> opencv::Result<Option<([f64; 4], usize)>> {
    let mut a = Vector::<Point2f>::new();
    imgproc::good_features_to_track(
        p,
        &mut a,
        1500,
        0.005,
        14.,
        &core::no_array(),
        7,
        false,
        0.04,
    )?;
    if a.len() < 60 {
        return Ok(None);
    }
    let crit = TermCriteria::new(core::TermCriteria_COUNT + core::TermCriteria_EPS, 30, 0.01)?;
    let (mut b, mut st, mut err) = (
        Vector::<Point2f>::new(),
        Vector::<u8>::new(),
        Vector::<f32>::new(),
    );
    let flow = |fwd: bool,
                from: &Vector<Point2f>,
                to: &mut Vector<Point2f>,
                st: &mut Vector<u8>,
                err: &mut Vector<f32>|
     -> opencv::Result<()> {
        let win = Size::new(21, 21);
        match pyrs {
            Some((pp, gp)) => {
                let (x, y) = if fwd { (pp, gp) } else { (gp, pp) };
                video::calc_optical_flow_pyr_lk(x, y, from, to, st, err, win, 3, crit, 0, 1e-4)
            }
            None => {
                let (x, y) = if fwd { (p, g) } else { (g, p) };
                video::calc_optical_flow_pyr_lk(x, y, from, to, st, err, win, 3, crit, 0, 1e-4)
            }
        }
    };
    flow(true, &a, &mut b, &mut st, &mut err)?;
    let (mut r, mut sr, mut er) = (
        Vector::<Point2f>::new(),
        Vector::<u8>::new(),
        Vector::<f32>::new(),
    );
    flow(false, &b, &mut r, &mut sr, &mut er)?;
    let (mut aa, mut bb) = (Vector::<Point2f>::new(), Vector::<Point2f>::new());
    for i in 0..a.len() {
        let (x, y, z) = (a.get(i)?, b.get(i)?, r.get(i)?);
        if st.get(i)? == 0 || sr.get(i)? == 0 || (z.x - x.x).hypot(z.y - x.y) > 0.3 {
            continue;
        }
        if y.x < 4. || y.y < 4. || y.x > w - 4. || y.y > h - 4. {
            continue;
        }
        aa.push(x);
        bb.push(y);
    }
    if aa.len() < 60 {
        return Ok(None);
    }
    let mut inl = Mat::default();
    let m = calib3d::estimate_affine_partial_2d(
        &aa,
        &bb,
        &mut inl,
        calib3d::RANSAC,
        0.7,
        4000,
        0.995,
        20,
    )?;
    if m.empty() {
        return Ok(None);
    }
    let n = core::count_non_zero(&inl)? as usize;
    let (ma, mb) = (*m.at_2d::<f64>(0, 0)?, *m.at_2d::<f64>(1, 0)?);
    let (tx, ty) = (*m.at_2d::<f64>(0, 2)?, *m.at_2d::<f64>(1, 2)?);
    let (cx, cy) = (w as f64 * 0.5, h as f64 * 0.5);
    // Displacement of the image center, rotation and log scale.
    let dx = ma * cx - mb * cy + tx - cx;
    let dy = mb * cx + ma * cy + ty - cy;
    Ok(Some(([dx, dy, mb.atan2(ma), ma.hypot(mb).ln()], n)))
}

/// Opt this (single worker) thread out of EcoQoS. Harness runs start the probe at below-normal
/// priority to keep the machine usable; without this, Windows 11 also confines such
/// background processes to the efficiency cores of a hybrid CPU (same fix as o4core::threads).
#[cfg(windows)]
fn no_ecoqos() {
    use windows_sys::Win32::System::Threading::{
        GetCurrentThread, SetThreadInformation, ThreadPowerThrottling,
        THREAD_POWER_THROTTLING_CURRENT_VERSION, THREAD_POWER_THROTTLING_EXECUTION_SPEED,
        THREAD_POWER_THROTTLING_STATE,
    };
    let state = THREAD_POWER_THROTTLING_STATE {
        Version: THREAD_POWER_THROTTLING_CURRENT_VERSION,
        ControlMask: THREAD_POWER_THROTTLING_EXECUTION_SPEED,
        StateMask: 0,
    };
    // SAFETY: pseudo-handle of the calling thread; `state` is valid for the size passed.
    unsafe {
        SetThreadInformation(
            GetCurrentThread(),
            ThreadPowerThrottling,
            &state as *const _ as *const std::ffi::c_void,
            std::mem::size_of::<THREAD_POWER_THROTTLING_STATE>() as u32,
        );
    }
}

#[cfg(not(windows))]
fn no_ecoqos() {}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var("O4_ECOQOS").map_or(true, |v| v != "1") {
        no_ecoqos();
    }
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 5 {
        return Err("usage: render_residual_probe RENDER START DURATION OUTPUT_JSON".into());
    }
    let (start, dur): (f64, f64) = (a[2].parse()?, a[3].parse()?);
    // Parallel chunk runs: one OpenCV thread per process avoids oversubscription.
    if let Ok(n) = std::env::var("O4_CV_THREADS") {
        core::set_num_threads(n.parse()?)?;
    }
    let probe = Command::new("C:/ffmpeg/bin/ffprobe.exe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=p=0",
            &a[1],
        ])
        .output()?;
    let dims: Vec<i32> = String::from_utf8(probe.stdout)?
        .trim()
        .split(',')
        .map(|s| s.parse().unwrap())
        .collect();
    let (w, h) = (dims[0], dims[1]);
    // Frame-accurate: input seek to start, frame count derived from duration at 100 fps.
    let frames = (dur * 100.).round() as usize;
    // decoder threads follow O4_CV_THREADS too (default: ffmpeg's own choice)
    let dec_threads = std::env::var("O4_CV_THREADS").unwrap_or_else(|_| "0".into());
    let mut child = Command::new("C:/ffmpeg/bin/ffmpeg.exe")
        .args([
            "-v",
            "error",
            "-threads",
            &dec_threads,
            "-ss",
            &format!("{start}"),
            "-i",
            &a[1],
            "-frames:v",
            &frames.to_string(),
            "-f",
            "rawvideo",
            "-pix_fmt",
            "gray",
            "-",
        ])
        .stdout(Stdio::piped())
        .spawn()?;
    let mut out = child.stdout.take().unwrap();
    let size = (w * h) as usize;
    let mut buf = vec![0u8; size];
    let mut prev: Option<Mat> = None;
    let mut rows = vec![];
    // Optional (env O4_HALVES=1): the same fit on the left and right image halves. Real
    // frame-to-frame motion appears in both; tracking noise is independent between them.
    let halves = std::env::var("O4_HALVES").is_ok_and(|v| v == "1");
    // Env O4_PYR=0 rebuilds the LK pyramids inside every flow call (the original path).
    let reuse = std::env::var("O4_PYR").map_or(true, |v| v != "0");
    let (mut prev_pyr, mut k) = (None::<Vector<Mat>>, 0usize);
    while out.read_exact(&mut buf).is_ok() {
        let m = Mat::new_rows_cols_with_data(h, w, &buf)?.try_clone()?;
        let pyr = if reuse { Some(pyramid(&m)?) } else { None };
        if let Some(p) = &prev {
            let t = start + (k as f64 - 0.5) / 100.;
            let pp = prev_pyr.as_ref().zip(pyr.as_ref());
            let mut row = match pair_with(p, &m, pp, w as f32, h as f32)? {
                Some((d, n)) => {
                    json!({"t":t,"rate":[d[0]*100.,d[1]*100.,d[2]*100.,d[3]*100.],"n":n})
                }
                None => json!({"t":t,"rate":null,"n":0}),
            };
            if halves {
                for (name, x0) in [("left", 0), ("right", w / 2)] {
                    let roi = core::Rect::new(x0, 0, w / 2, h);
                    let (pa, pb) = (
                        Mat::roi(p, roi)?.try_clone()?,
                        Mat::roi(&m, roi)?.try_clone()?,
                    );
                    row[name] = match pair(&pa, &pb, (w / 2) as f32, h as f32)? {
                        Some((d, n)) => {
                            json!({"rate":[d[0]*100.,d[1]*100.,d[2]*100.,d[3]*100.],"n":n})
                        }
                        None => serde_json::Value::Null,
                    };
                }
            }
            rows.push(row);
        }
        prev = Some(m);
        prev_pyr = pyr;
        k += 1;
    }
    child.wait()?;
    if k != frames {
        return Err(format!("decoded {k} frames, expected {frames}").into());
    }
    std::fs::write(
        &a[4],
        serde_json::to_vec(
            &json!({"render":a[1],"start":start,"width":w,"height":h,"frames":k,"pairs":rows}),
        )?,
    )?;
    println!("{k} frames, {} pairs", rows.len());
    Ok(())
}
