//! Research-only local optical stabilization of already rendered footage.
//! No telemetry or shipping settings are changed.
use opencv::{
    calib3d,
    core::{self, no_array, Mat, Point2f, Scalar, Size, TermCriteria, Vector},
    imgproc,
    prelude::*,
    video, videoio,
};
use serde_json::json;
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};
#[derive(Clone, Copy)]
pub(crate) struct Pose {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) a: f64,
    pub(crate) q: f64,
}
fn gray(frame: &Mat) -> opencv::Result<Mat> {
    let mut g = Mat::default();
    imgproc::cvt_color_def(frame, &mut g, imgproc::COLOR_BGR2GRAY)?;
    Ok(g)
}
fn small(frame: &Mat) -> opencv::Result<Mat> {
    let mut s = Mat::default();
    imgproc::resize(
        frame,
        &mut s,
        Size::new(720, 405),
        0.,
        0.,
        imgproc::INTER_AREA,
    )?;
    Ok(s)
}
pub(crate) fn pair(p: &Mat, g: &Mat) -> opencv::Result<Pose> {
    let mut a = Vector::<Point2f>::new();
    imgproc::good_features_to_track(p, &mut a, 400, 0.01, 12., &no_array(), 7, false, 0.04)?;
    if a.len() < 40 {
        return Ok(Pose {
            x: 0.,
            y: 0.,
            a: 0.,
            q: 0.,
        });
    }
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
        TermCriteria::new(core::TermCriteria_COUNT + core::TermCriteria_EPS, 30, 0.01)?,
        0,
        1e-4,
    )?;
    let idx: Vec<_> = (0..a.len()).filter(|&i| st.get(i).unwrap() == 1).collect();
    if idx.len() < 40 {
        return Ok(Pose {
            x: 0.,
            y: 0.,
            a: 0.,
            q: 0.,
        });
    }
    let aa: Vector<Point2f> = idx.iter().map(|&i| a.get(i).unwrap()).collect();
    let bb: Vector<Point2f> = idx.iter().map(|&i| b.get(i).unwrap()).collect();
    let mut inl = Mat::default();
    let m = calib3d::estimate_affine_partial_2d(
        &aa,
        &bb,
        &mut inl,
        calib3d::RANSAC,
        2.,
        2000,
        0.99,
        10,
    )?;
    if m.empty() {
        return Ok(Pose {
            x: 0.,
            y: 0.,
            a: 0.,
            q: 0.,
        });
    }
    let n = (0..inl.rows())
        .filter(|&i| *inl.at::<u8>(i).unwrap() != 0)
        .count();
    let v = |r, c| *m.at_2d::<f64>(r, c).unwrap();
    // Measure displacement at image center; omit fitted scale to avoid chasing zoom.
    Ok(Pose {
        x: v(0, 0) * 360. + v(0, 1) * 202.5 + v(0, 2) - 360.,
        y: v(1, 0) * 360. + v(1, 1) * 202.5 + v(1, 2) - 202.5,
        a: v(1, 0).atan2(v(0, 0)),
        q: if n >= 80 {
            n as f64 / idx.len() as f64
        } else {
            0.0
        },
    })
}
fn cap(path: &str, start: f64) -> opencv::Result<videoio::VideoCapture> {
    let mut c = videoio::VideoCapture::from_file(path, videoio::CAP_ANY)?;
    if !c.is_opened()? {
        return Err(opencv::Error::new(0, "cannot open render"));
    }
    let fps = c.get(videoio::CAP_PROP_FPS)?;
    if (fps - 100.).abs() > 1e-6 {
        return Err(opencv::Error::new(0, "expected measured 100fps render"));
    }
    c.set(videoio::CAP_PROP_POS_FRAMES, (start * 100.).round())?;
    Ok(c)
}
pub(crate) fn gate(t: f64, bursts: &[(f64, f64)]) -> f64 {
    bursts
        .iter()
        .map(|&(a, b)| {
            let up = ((t - (a - 0.4)) / 0.3).clamp(0., 1.);
            let down = (((b + 0.6) - t) / 0.3).clamp(0., 1.);
            o4core::quat::smoothstep(up) * o4core::quat::smoothstep(down)
        })
        .fold(0., f64::max)
}
fn correction(path: &[Pose], i: usize, sigma: f64) -> Pose {
    let radius = (sigma * 3.).ceil() as usize;
    let (mut x, mut y, mut a, mut w) = (0., 0., 0., 0.);
    for j in i.saturating_sub(radius)..=(i + radius).min(path.len() - 1) {
        let v = (-0.5 * ((j as f64 - i as f64) / sigma).powi(2)).exp();
        x += path[j].x * v;
        y += path[j].y * v;
        a += path[j].a * v;
        w += v;
    }
    let da = a / w - path[i].a;
    let (c, s) = (da.cos(), da.sin());
    Pose {
        x: x / w - (c * path[i].x - s * path[i].y),
        y: y / w - (s * path[i].x + c * path[i].y),
        a: da,
        q: 1.,
    }
}
fn warp(frame: &Mat, c: Pose) -> opencv::Result<Mat> {
    let zoom = 1.08;
    let (a, b) = (zoom * c.a.cos(), zoom * c.a.sin());
    let m = Mat::from_slice_2d(&[
        [a, -b, 360. - a * 360. + b * 202.5 + zoom * c.x],
        [b, a, 202.5 - b * 360. - a * 202.5 + zoom * c.y],
    ])?;
    let mut out = Mat::default();
    imgproc::warp_affine(
        frame,
        &mut out,
        &m,
        Size::new(720, 405),
        imgproc::INTER_LANCZOS4,
        core::BORDER_CONSTANT,
        Scalar::all(0.),
    )?;
    let mut padded = Mat::default();
    core::copy_make_border(
        &out,
        &mut padded,
        0,
        1,
        0,
        0,
        core::BORDER_CONSTANT,
        Scalar::all(0.),
    )?;
    Ok(padded)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 6 && a.len() != 7 {
        return Err(
            "usage: local_render_smooth BASELINE_RENDER START END OUTPUT_MP4 BURSTS_JSON [SOURCE_OFFSET_SECONDS]".into(),
        );
    }
    let start: f64 = a[2].parse()?;
    let end: f64 = a[3].parse()?;
    if start < 1. || end <= start {
        return Err("invalid window".into());
    }
    let bursts: Vec<(f64, f64)> = serde_json::from_str(&std::fs::read_to_string(&a[5])?)?;
    let source_offset: f64 = a.get(6).map(|s| s.parse()).transpose()?.unwrap_or(0.0);
    let context = start - 1.;
    let count = ((end - start + 2.) * 100.).round() as usize;
    let mut c = cap(&a[1], context - source_offset)?;
    let mut frame = Mat::default();
    let mut prev = None;
    let mut poses = vec![];
    let mut pairs = vec![];
    let mut current = Pose {
        x: 0.,
        y: 0.,
        a: 0.,
        q: 1.,
    };
    for i in 0..count {
        if !c.read(&mut frame)? {
            return Err(format!(
                "unexpected end of render at local frame {i}, source frame {}",
                (context * 100.) as usize + i
            )
            .into());
        }
        let g = gray(&small(&frame)?)?;
        if let Some(p) = &prev {
            core::set_rng_seed(1000000 + (context * 100.) as i32 + i as i32)?;
            let d = pair(p, &g)?;
            pairs.push(d);
            let (co, si) = (d.a.cos(), d.a.sin());
            current = Pose {
                x: co * current.x - si * current.y + d.x,
                y: si * current.x + co * current.y + d.y,
                a: current.a + d.a,
                q: d.q,
            };
        }
        poses.push(current);
        prev = Some(g);
    }
    let bad = pairs.iter().filter(|p| p.q < 0.3).count();
    let mut quality: Vec<f64> = pairs.iter().map(|p| p.q).collect();
    quality.sort_by(f64::total_cmp);
    println!(
        "tracking pairs {}, below30pct {}, quality min {} p10 {} median {}",
        pairs.len(),
        bad,
        quality[0],
        quality[quality.len() / 10],
        quality[quality.len() / 2]
    );
    std::fs::write(
        Path::new(&a[4]).with_extension("tracking.json"),
        serde_json::to_vec(
            &json!({"t0":context,"pairs":pairs.iter().map(|p|[p.x,p.y,p.a,p.q]).collect::<Vec<_>>()}),
        )?,
    )?;

    if bad as f64 / pairs.len() as f64 > 0.05 {
        return Err("too many low-confidence motion estimates; no output".into());
    }
    let mut process = Command::new("C:/ffmpeg/bin/ffmpeg.exe")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "bgr24",
            "-s",
            "2160x406",
            "-r",
            "100",
            "-i",
            "pipe:0",
            "-an",
            "-c:v",
            "libx264",
            "-preset",
            "fast",
            "-crf",
            "18",
            "-pix_fmt",
            "yuv420p",
            "-movflags",
            "+faststart",
            &a[4],
        ])
        .stdin(Stdio::piped())
        .spawn()?;
    let mut stdin = process.stdin.take().unwrap();
    let mut c = cap(&a[1], start - source_offset)?;
    let n = ((end - start) * 100.).round() as usize;
    let mut rows = vec![];
    let mut max_shift = [0.0_f64; 2];
    let mut max_roll = [0.0_f64; 2];
    let mut limited = [0usize; 2];
    for f in 0..n {
        if !c.read(&mut frame)? {
            return Err("unexpected end of second pass".into());
        }
        let input = small(&frame)?;
        let i = f + 100;
        let weight = gate(start + f as f64 / 100., &bursts);
        let mut parts = Vector::<Mat>::new();
        parts.push(warp(
            &input,
            Pose {
                x: 0.,
                y: 0.,
                a: 0.,
                q: 1.,
            },
        )?);
        let mut changes = vec![];
        for (k, sigma) in [4., 8.].iter().enumerate() {
            let mut d = correction(&poses, i, *sigma);
            d.x *= weight;
            d.y *= weight;
            d.a *= weight;
            let norm = d.x.hypot(d.y);
            let factor = (11. / norm.max(1e-12)).min(1.);
            if factor < 1. || d.a.abs() > 0.5_f64.to_radians() {
                limited[k] += 1;
            }
            d.x *= factor;
            d.y *= factor;
            d.a = d.a.clamp(-0.5_f64.to_radians(), 0.5_f64.to_radians());
            max_shift[k] = max_shift[k].max(d.x.hypot(d.y));
            max_roll[k] = max_roll[k].max(d.a.abs().to_degrees());
            parts.push(warp(&input, d)?);
            changes.push(json!({"dx_half_px":d.x,"dy_half_px":d.y,"roll_deg":d.a.to_degrees()}));
        }
        let mut joined = Mat::default();
        core::hconcat(&parts, &mut joined)?;
        stdin.write_all(joined.data_bytes()?)?;
        rows.push(json!({"t":start+f as f64/100.,"gate":weight,"confidence":poses[i].q,"corrections":changes}));
    }
    drop(stdin);
    if !process.wait()?.success() {
        return Err("encoder failed".into());
    }
    std::fs::write(
        Path::new(&a[4]).with_extension("json"),
        serde_json::to_vec_pretty(
            &json!({"window":[start,end],"panels":["baseline","sigma40ms","sigma80ms"],"common_zoom":1.08,"max_translation_half_px":max_shift,"max_roll_deg":max_roll,"limited_frames":limited,"low_confidence_pairs":bad,"rows":rows}),
        )?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn smoothing_attenuates_ten_hz_without_lagging_linear_pan() {
        let path: Vec<Pose> = (0..400)
            .map(|i| Pose {
                x: 0.2 * i as f64
                    + 2.0 * (2.0 * std::f64::consts::PI * 10.0 * i as f64 / 100.0).sin(),
                y: 0.0,
                a: 0.0,
                q: 1.0,
            })
            .collect();
        let mut residual = 0.0;
        for i in 100..300 {
            let c = correction(&path, i, 4.0);
            let error = path[i].x + c.x - 0.2 * i as f64;
            residual += error * error;
        }
        assert!((residual / 200.0).sqrt() < 0.1);
        let straight: Vec<Pose> = (0..400)
            .map(|i| Pose {
                x: 0.2 * i as f64,
                y: -0.1 * i as f64,
                a: 0.0,
                q: 1.0,
            })
            .collect();
        let c = correction(&straight, 200, 8.0);
        assert!(c.x.abs() < 1e-10 && c.y.abs() < 1e-10);
    }
    #[test]
    fn finite_gate_and_crop_cover_bounded_corrections() {
        let bursts = [(10.0, 11.0)];
        assert_eq!(gate(9.5, &bursts), 0.0);
        assert_eq!(gate(11.7, &bursts), 0.0);
        assert_eq!(gate(10.5, &bursts), 1.0);
        // Inverse-map viewport corners for every limiting translation direction and rotation sign.
        for i in 0..360 {
            for angle in [-0.5_f64, 0.5] {
                let dir = (i as f64).to_radians();
                let (dx, dy) = (11.0 * dir.cos(), 11.0 * dir.sin());
                let (c, s) = (angle.to_radians().cos(), angle.to_radians().sin());
                for (x, y) in [(0.0, 0.0), (719.0, 0.0), (0.0, 404.0), (719.0, 404.0)] {
                    let (xx, yy) = ((x - 360.0) / 1.08 - dx, (y - 202.5) / 1.08 - dy);
                    let (sx, sy) = (c * xx + s * yy + 360.0, -s * xx + c * yy + 202.5);
                    assert!(sx >= 0.0 && sx <= 719.0 && sy >= 0.0 && sy <= 404.0);
                }
            }
        }
    }
}
