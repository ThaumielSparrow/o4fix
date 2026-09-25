//! Research-only bounded regional acceleration correction atop v1 translation.
use opencv::{
    core::{self, Mat, Rect, Scalar},
    imgproc,
    prelude::*,
    videoio,
};
use serde_json::{json, Value};
use std::{
    io::Write,
    process::{Command, Stdio},
};
fn corrections(acc: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let n = acc.len() + 2;
    let mut path = vec![[0.; 2]; n];
    for i in 1..n - 1 {
        for k in 0..2 {
            path[i + 1][k] = 2. * path[i][k] - path[i - 1][k] + acc[i - 1][k];
        }
    }
    // Remove arbitrary integrated linear drift before linear boundary extrapolation.
    for k in 0..2 {
        let slope = path[n - 1][k] / (n - 1) as f64;
        for (i, p) in path.iter_mut().enumerate() {
            p[k] -= slope * i as f64;
        }
    }
    (0..n)
        .map(|i| {
            let (mut mean, mut total) = ([0.; 2], 0.);
            for d in -12i32..=12 {
                let j = i as i32 + d;
                let sample: [f64; 2] = if j < 0 {
                    std::array::from_fn(|k| path[0][k] + j as f64 * (path[1][k] - path[0][k]))
                } else if j >= n as i32 {
                    std::array::from_fn(|k| {
                        path[n - 1][k]
                            + (j - (n as i32 - 1)) as f64 * (path[n - 1][k] - path[n - 2][k])
                    })
                } else {
                    path[j as usize]
                };
                let w = (-0.5 * (d as f64 / 4.).powi(2)).exp();
                total += w;
                for (k, v) in mean.iter_mut().enumerate() {
                    *v += w * sample[k];
                }
            }
            [mean[0] / total - path[i][0], mean[1] / total - path[i][1]]
        })
        .collect()
}
fn field(nodes: &[[f64; 2]; 9], x: f64, y: f64) -> [f64; 2] {
    // Nodes at centers of the 3x3 regions. Clamp outside outer node centers.
    let fx = (x / 480. - 0.5).clamp(0., 2.);
    let fy = (y / 270. - 0.5).clamp(0., 2.);
    let ix = (fx.floor() as usize).min(1);
    let iy = (fy.floor() as usize).min(1);
    let ux = fx - ix as f64;
    let uy = fy - iy as f64;
    std::array::from_fn(|k| {
        nodes[iy * 3 + ix][k] * (1. - ux) * (1. - uy)
            + nodes[iy * 3 + ix + 1][k] * ux * (1. - uy)
            + nodes[(iy + 1) * 3 + ix][k] * (1. - ux) * uy
            + nodes[(iy + 1) * 3 + ix + 1][k] * ux * uy
    })
}
fn warp(src: &Mat, c: [f64; 2], nodes: &[[f64; 2]; 9]) -> opencv::Result<Mat> {
    let z = 1.04;
    let mut mx = Mat::new_rows_cols_with_default(810, 1440, core::CV_32FC1, Scalar::all(0.))?;
    let mut my = Mat::new_rows_cols_with_default(810, 1440, core::CV_32FC1, Scalar::all(0.))?;
    let xx = mx.data_typed_mut::<f32>()?;
    let yy = my.data_typed_mut::<f32>()?;
    for y in 0..810 {
        for x in 0..1440 {
            let (dx, dy) = (x as f64, y as f64);
            let mut q = [dx, dy];
            for _ in 0..3 {
                let f = field(nodes, q[0], q[1]);
                q = [dx - 2. * f[0], dy - 2. * f[1]];
            }
            let i = y * 1440 + x;
            xx[i] = (720. + (q[0] - 720.) / z - 2. * c[0]) as f32;
            yy[i] = (405. + (q[1] - 405.) / z - 2. * c[1]) as f32;
        }
    }
    let mut out = Mat::default();
    imgproc::remap(
        src,
        &mut out,
        &mx,
        &my,
        imgproc::INTER_LANCZOS4,
        core::BORDER_CONSTANT,
        Scalar::all(0.),
    )?;
    Ok(out)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 5 {
        return Err(
            "usage: render_regional_review FULL_VIDEO PROBE_JSON OUTPUT_MP4 PRIOR_CORRECTIONS_JSON"
                .into(),
        );
    }
    if std::path::Path::new(&a[3]).exists() {
        return Err("output exists".into());
    }
    let data: Value = serde_json::from_str(&std::fs::read_to_string(&a[2])?)?;
    let rows = data["rows"].as_array().ok_or("rows")?;
    let prior: Value = serde_json::from_str(&std::fs::read_to_string(&a[4])?)?;
    let old: Vec<[f64; 2]> = serde_json::from_value(prior["corrections"].clone())?;
    let n = old.len();
    if rows.len() + 2 != n {
        return Err("length mismatch".into());
    }
    let mut c = vec![[[0.; 2]; 9]; n];
    for node in 0..9 {
        let mut acc = vec![];
        let mut support = vec![];
        for r in rows {
            let score = &r["score"];
            let shared: [f64; 2] = serde_json::from_value(score["shared"].clone())?;
            let region = &score["regions"][node];
            // Unsupported nodes use shared acceleration; local deviation requires persistent support.
            let local: [f64; 2] = if region.is_null() {
                shared
            } else {
                serde_json::from_value(region["acc"].clone())?
            };
            acc.push([local[0] - shared[0], local[1] - shared[1]]);
            support.push(!region.is_null());
        }
        let mut delta = corrections(&acc);
        let mut gate = vec![0.; n];
        for i in 1..n - 1 {
            if i >= 13 && i + 13 < n && support[i - 13..=i + 11].iter().all(|&v| v) {
                gate[i] = 1.;
            }
        }
        // Smooth support transitions independently; no threshold tuned from output scores.
        let gate = o4core::dsp::filtfilt(&o4core::dsp::butter_low(2, 3. / 50.), &gate);
        for k in 0..2 {
            let raw: Vec<_> = delta.iter().map(|v| v[k]).collect();
            let band =
                o4core::dsp::filtfilt(&o4core::dsp::butter_band(2, 4. / 50., 30. / 50.), &raw);
            for i in 0..n {
                delta[i][k] = band[i] * gate[i].clamp(0., 1.);
            }
        }
        for i in 0..n {
            c[i][node] = delta[i];
        }
    }
    // Only spatially varying residual: v1 already addresses shared translation.
    let max = c
        .iter()
        .flatten()
        .flatten()
        .fold(0f64, |a, &v| a.max(v.abs()));
    let mut gradient = 0f64;
    for nodes in &c {
        for y in 0..3 {
            for x in 0..3 {
                let i = y * 3 + x;
                for k in 0..2 {
                    if x < 2 {
                        gradient = gradient.max(2. * (nodes[i + 1][k] - nodes[i][k]).abs() / 480.);
                    }
                    if y < 2 {
                        gradient = gradient.max(2. * (nodes[i + 3][k] - nodes[i][k]).abs() / 270.);
                    }
                }
            }
        }
    }
    if max > 6. || gradient > 0.03 {
        return Err(format!("unsafe field: {max}px, gradient {gradient}").into());
    }
    // Sample inverse source support around the complete output perimeter.
    let mut margin = f64::INFINITY;
    for (i, nodes) in c.iter().enumerate() {
        for j in 0..=60 {
            for (x, y) in [
                (1439. * j as f64 / 60., 0.),
                (1439. * j as f64 / 60., 809.),
                (0., 809. * j as f64 / 60.),
                (1439., 809. * j as f64 / 60.),
            ] {
                let mut q = [x, y];
                for _ in 0..3 {
                    let f = field(nodes, q[0], q[1]);
                    q = [x - 2. * f[0], y - 2. * f[1]];
                }
                let sx = 720. + (q[0] - 720.) / 1.04 - 2. * old[i][0];
                let sy = 405. + (q[1] - 405.) / 1.04 - 2. * old[i][1];
                margin = margin.min(sx).min(1439. - sx).min(sy).min(809. - sy);
            }
        }
    }
    if margin < 4. {
        return Err(format!("source margin {margin}").into());
    }
    let mut cap = videoio::VideoCapture::from_file(&a[1], videoio::CAP_ANY)?;
    let mut encoder = Command::new("C:/ffmpeg/bin/ffmpeg.exe")
        .args([
            "-v",
            "error",
            "-n",
            "-f",
            "rawvideo",
            "-pixel_format",
            "bgr24",
            "-video_size",
            "2880x810",
            "-framerate",
            "100",
            "-i",
            "pipe:0",
            "-an",
            "-c:v",
            "libx264",
            "-preset",
            "fast",
            "-crf",
            "17",
            "-pix_fmt",
            "yuv420p",
            &a[3],
        ])
        .stdin(Stdio::piped())
        .spawn()?;
    let mut pipe = encoder.stdin.take().ok_or("pipe")?;
    let mut raw = Mat::default();
    for (i, &correction) in c.iter().enumerate() {
        if !cap.read(&mut raw)? {
            return Err("early EOF".into());
        }
        let src = Mat::roi(&raw, Rect::new(0, 0, 1440, 810))?.try_clone()?;
        let left = warp(&src, old[i], &[[0.; 2]; 9])?;
        let right = warp(&src, old[i], &correction)?;
        let mut joined = Mat::default();
        core::hconcat2(&left, &right, &mut joined)?;
        let mut padded = Mat::default();
        core::copy_make_border(
            &joined,
            &mut padded,
            0,
            0,
            0,
            0,
            core::BORDER_CONSTANT,
            Scalar::all(0.),
        )?;
        pipe.write_all(padded.data_bytes()?)?;
    }
    if cap.read(&mut raw)? {
        return Err("extra input frames".into());
    }
    drop(pipe);
    if !encoder.wait()?.success() {
        return Err("encoder failed".into());
    }
    std::fs::write(
        format!("{}.json", a[3]),
        serde_json::to_vec(
            &json!({"fields":c,"max_px":max,"max_gradient":gradient,"source_margin":margin,"sigma_seconds":0.04,"zoom":1.04,"note":"LEFT prior v1 translation; RIGHT same translation plus regional residual field. 4-30Hz, persistent-support gate. Both from same original render in one warp. No telemetry change. First/last 0.5s not for scoring."}),
        )?,
    )?;
    println!("{} frames; max correction {max:.4}px", c.len());
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn correction_sign_reduces_oscillation() {
        let path: Vec<f64> = (0..400).map(|i| 2. * (i as f64 * 0.3).sin()).collect();
        let a: Vec<_> = (1..399)
            .map(|i| [path[i - 1] + path[i + 1] - 2. * path[i], 0.])
            .collect();
        let c = corrections(&a);
        let before: f64 = (50..350).map(|i| path[i].powi(2)).sum();
        let after: f64 = (50..350).map(|i| (path[i] + c[i][0]).powi(2)).sum();
        assert!(after < before * 0.3);
    }
    #[test]
    fn field_interpolation_and_inverse_are_consistent() {
        let nodes = std::array::from_fn(|i| [0.005 * (240. + 480. * (i % 3) as f64), 0.]);
        let mut q = [720., 405.];
        for _ in 0..3 {
            let f = field(&nodes, q[0], q[1]);
            q = [720. - 2. * f[0], 405. - 2. * f[1]];
        }
        assert!((q[0] - 720. / 1.01).abs() < 0.0001);
        assert_eq!(q[1], 405.);
        assert_eq!(field(&[[1., -2.]; 9], 0., 809.), [1., -2.]);
    }
    #[test]
    fn steady_pan_needs_no_correction() {
        assert!(corrections(&vec![[0., 0.]; 100])
            .iter()
            .flatten()
            .all(|&v| v == 0.));
    }
}
