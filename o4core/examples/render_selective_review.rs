//! Rotation-only residual ablation atop prior v1 translation. Full-resolution single resampling.
use opencv::{
    core::{self, Mat, Rect, Scalar, Size},
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
fn warp(src: &Mat, c: [f64; 2], angle: f64) -> opencv::Result<Mat> {
    let z = 1.06;
    let (sin, cos) = angle.sin_cos();
    let m = Mat::from_slice_2d(&[
        [
            z * cos,
            -z * sin,
            720. - z * cos * 720. + z * sin * 405. + 2. * z * (cos * c[0] - sin * c[1]),
        ],
        [
            z * sin,
            z * cos,
            405. - z * sin * 720. - z * cos * 405. + 2. * z * (sin * c[0] + cos * c[1]),
        ],
    ])?;
    let mut dst = Mat::default();
    imgproc::warp_affine(
        src,
        &mut dst,
        &m,
        Size::new(1440, 810),
        imgproc::INTER_LANCZOS4,
        core::BORDER_CONSTANT,
        Scalar::all(0.),
    )?;
    Ok(dst)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 6 {
        return Err(
            "usage: render_selective_review FULL_VIDEO ROTATION_PROBE OUTPUT_MP4 PRIOR_CORRECTIONS_JSON GATE_JSON".into(),
        );
    }
    if std::path::Path::new(&a[3]).exists() {
        return Err("output exists".into());
    }
    let data: Value = serde_json::from_str(&std::fs::read_to_string(&a[2])?)?;
    let acc: Vec<[f64; 2]> = data["rows"]
        .as_array()
        .ok_or("rows")?
        .iter()
        .map(|r| {
            r["score"]["rigid_fit"][2]
                .as_f64()
                .map(|v| [v, 0.])
                .ok_or("missing angular acceleration")
        })
        .collect::<Result<_, _>>()?;
    let raw = corrections(&acc);
    let mut angle = o4core::dsp::filtfilt(
        &o4core::dsp::butter_band(2, 4. / 50., 30. / 50.),
        &raw.iter().map(|v| v[0]).collect::<Vec<_>>(),
    );
    let gating: Value = serde_json::from_str(&std::fs::read_to_string(&a[5])?)?;
    let weights: Vec<f64> = serde_json::from_value(gating["weights"].clone())?;
    if weights.len() != angle.len() || weights.iter().any(|v| !v.is_finite() || *v < 0. || *v > 1.)
    {
        return Err("invalid gate".into());
    }
    for (v, w) in angle.iter_mut().zip(&weights) {
        *v *= w;
    }
    let c: Vec<[f64; 2]> = angle.iter().map(|&v| [v, 0.]).collect();
    let prior: Value = serde_json::from_str(&std::fs::read_to_string(&a[4])?)?;
    let old: Vec<[f64; 2]> = serde_json::from_value(prior["corrections"].clone())?;
    if old.len() != c.len() {
        return Err("prior length mismatch".into());
    }
    // Fixed safety envelope inside common 4% crop. Refuse rather than clip the path.
    let max = c.iter().flatten().fold(0f64, |a, &v| a.max(v.abs()));
    if max > 0.5f64.to_radians() {
        return Err(format!("rotation {max}rad exceeds 0.5deg envelope").into());
    }
    // Validate inverse-warp corners including interpolation support for every frame.
    let mut margin = f64::INFINITY;
    for (i, &theta) in angle.iter().enumerate() {
        let (sn, cs) = theta.sin_cos();
        for x in [0., 1439.] {
            for y in [0., 809.] {
                let dx = (x - 720.) / 1.06;
                let dy = (y - 405.) / 1.06;
                let sx = 720. + cs * dx + sn * dy - 2. * old[i][0];
                let sy = 405. - sn * dx + cs * dy - 2. * old[i][1];
                margin = margin.min(sx).min(1439. - sx).min(sy).min(809. - sy);
            }
        }
    }
    if margin < 4. {
        return Err(format!("insufficient source margin {margin}").into());
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
        let left = warp(&src, old[i], 0.)?;
        let right = warp(&src, old[i], correction[0])?;
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
            &json!({"angles":angle,"weights":weights,"max_angle_rad":max,"source_margin":margin,"sigma_seconds":0.04,"band_hz":[4,30],"zoom":1.06,"note":"LEFT prior v1 translation; RIGHT same translation plus evidence-gated rotation during supported gyro-corruption bursts. Both from same original render in one warp. No telemetry change. First/last 0.5s not for scoring."}),
        )?,
    )?;
    println!("{} frames; max rotation {max:.6}rad", c.len());
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
    fn steady_pan_needs_no_correction() {
        assert!(corrections(&vec![[0., 0.]; 100])
            .iter()
            .flatten()
            .all(|&v| v == 0.));
    }
}
