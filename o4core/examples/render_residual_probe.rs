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

fn pair(p: &Mat, g: &Mat, w: f32, h: f32) -> opencv::Result<Option<([f64; 4], usize)>> {
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
    video::calc_optical_flow_pyr_lk(
        p,
        g,
        &a,
        &mut b,
        &mut st,
        &mut err,
        Size::new(21, 21),
        3,
        crit,
        0,
        1e-4,
    )?;
    let (mut r, mut sr, mut er) = (
        Vector::<Point2f>::new(),
        Vector::<u8>::new(),
        Vector::<f32>::new(),
    );
    video::calc_optical_flow_pyr_lk(
        g,
        p,
        &b,
        &mut r,
        &mut sr,
        &mut er,
        Size::new(21, 21),
        3,
        crit,
        0,
        1e-4,
    )?;
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 5 {
        return Err("usage: render_residual_probe RENDER START DURATION OUTPUT_JSON".into());
    }
    let (start, dur): (f64, f64) = (a[2].parse()?, a[3].parse()?);
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
    let mut child = Command::new("C:/ffmpeg/bin/ffmpeg.exe")
        .args([
            "-v",
            "error",
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
    let mut k = 0usize;
    while out.read_exact(&mut buf).is_ok() {
        let m = Mat::new_rows_cols_with_data(h, w, &buf)?.try_clone()?;
        if let Some(p) = &prev {
            let t = start + (k as f64 - 0.5) / 100.;
            match pair(p, &m, w as f32, h as f32)? {
                Some((d, n)) => {
                    rows.push(json!({"t":t,"rate":[d[0]*100.,d[1]*100.,d[2]*100.,d[3]*100.],"n":n}))
                }
                None => rows.push(json!({"t":t,"rate":null,"n":0})),
            }
        }
        prev = Some(m);
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
