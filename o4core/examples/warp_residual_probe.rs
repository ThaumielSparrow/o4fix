//! Research-only, Gyroflow-free residual measurement. Track features between consecutive
//! SOURCE frames, fisheye-undistort them with the telemetry lens model, and rotate each
//! point's bearing into the world frame with the file's own telemetry orientation sampled
//! at that point's rolling-shutter row time. With perfect telemetry (and no translation)
//! consecutive world bearings coincide; the leftover small rotation is the telemetry error.
//!
//! usage: warp_residual_probe MP4 START DURATION READOUT_MS LAG_S AXES OUTPUT_JSON
//!   AXES = index 0..23 of the camera->body signed-permutation mount, or "search"
//!   (scores all 24 on the window and prints the ranking; writes nothing).
//! Output pairs: t (pair midpoint, container clock), body residual rate rad/s, inliers.
use o4core::{quat, telemetry};
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

type V3 = [f64; 3];

fn mounts() -> Vec<[[f64; 3]; 3]> {
    let mut out = vec![];
    let perms = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    for p in perms {
        for s in 0..8 {
            let sg = [
                1. - 2. * (s & 1) as f64,
                1. - 2. * ((s >> 1) & 1) as f64,
                1. - 2. * ((s >> 2) & 1) as f64,
            ];
            let mut m = [[0.; 3]; 3];
            for r in 0..3 {
                m[r][p[r]] = sg[r];
            }
            let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
                - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
                + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
            if det > 0. {
                out.push(m);
            }
        }
    }
    out
}

fn mv(m: &[[f64; 3]; 3], v: V3) -> V3 {
    std::array::from_fn(|r| m[r][0] * v[0] + m[r][1] * v[1] + m[r][2] * v[2])
}

fn qrot(q: [f64; 4], v: V3) -> V3 {
    let p = quat::qmul(quat::qmul(q, [0., v[0], v[1], v[2]]), quat::qconj(q));
    [p[1], p[2], p[3]]
}

struct Tel {
    t: Vec<f64>,
    q: Vec<[f64; 4]>,
}
impl Tel {
    fn at(&self, t: f64) -> [f64; 4] {
        let i = self
            .t
            .partition_point(|&x| x <= t)
            .clamp(1, self.t.len() - 1);
        let f = ((t - self.t[i - 1]) / (self.t[i] - self.t[i - 1])).clamp(0., 1.);
        quat::slerp(self.q[i - 1], self.q[i], f)
    }
}

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn solve3(a: [[f64; 3]; 3], b: V3) -> V3 {
    let det = a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1])
        - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
        + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]);
    std::array::from_fn(|k| {
        let mut m = a;
        for r in 0..3 {
            m[r][k] = b[r];
        }
        (m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]))
            / det
    })
}

/// Small rotation d with b ~= a + d x a, robust (iterative trimming). Returns (d, inliers, rms).
fn fit_rotation(a: &[V3], b: &[V3]) -> Option<(V3, usize, f64)> {
    let mut use_: Vec<bool> = vec![true; a.len()];
    let mut d = [0.; 3];
    let mut n = 0;
    let mut rms = 0.;
    for _ in 0..4 {
        let mut ata = [[0.; 3]; 3];
        let mut atb = [0.; 3];
        n = 0;
        for i in 0..a.len() {
            if !use_[i] {
                continue;
            }
            n += 1;
            // d x a = -[a]x d ; residual r = (b - a) + [a]x d
            let ax = [
                [0., -a[i][2], a[i][1]],
                [a[i][2], 0., -a[i][0]],
                [-a[i][1], a[i][0], 0.],
            ];
            let r0: V3 = std::array::from_fn(|k| b[i][k] - a[i][k]);
            for p in 0..3 {
                for q in 0..3 {
                    ata[p][q] += (0..3).map(|k| ax[k][p] * ax[k][q]).sum::<f64>();
                }
                atb[p] -= (0..3).map(|k| ax[k][p] * r0[k]).sum::<f64>();
            }
        }
        if n < 30 {
            return None;
        }
        d = solve3(ata, atb);
        let res: Vec<f64> = (0..a.len())
            .map(|i| {
                let c = cross(d, a[i]);
                (0..3)
                    .map(|k| (b[i][k] - a[i][k] - c[k]).powi(2))
                    .sum::<f64>()
                    .sqrt()
            })
            .collect();
        let mut s: Vec<f64> = (0..a.len()).filter(|&i| use_[i]).map(|i| res[i]).collect();
        s.sort_by(f64::total_cmp);
        let med = s[s.len() / 2];
        rms = (s.iter().map(|x| x * x).sum::<f64>() / s.len() as f64).sqrt();
        let thr = (3. * med).max(0.0008);
        for i in 0..a.len() {
            use_[i] = res[i] < thr;
        }
    }
    Some((d, n, rms))
}

fn track(p: &Mat, g: &Mat) -> opencv::Result<Vec<(Point2f, Point2f)>> {
    let mut a = Vector::<Point2f>::new();
    imgproc::good_features_to_track(
        p,
        &mut a,
        1200,
        0.005,
        18.,
        &core::no_array(),
        7,
        false,
        0.04,
    )?;
    if a.len() < 40 {
        return Ok(vec![]);
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
        4,
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
        4,
        crit,
        0,
        1e-4,
    )?;
    let mut out = vec![];
    for i in 0..a.len() {
        let (x, y, z) = (a.get(i)?, b.get(i)?, r.get(i)?);
        if st.get(i)? == 1 && sr.get(i)? == 1 && (z.x - x.x).hypot(z.y - x.y) < 0.5 {
            out.push((x, y));
        }
    }
    Ok(out)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 8 {
        return Err("usage: warp_residual_probe MP4 START DURATION READOUT_MS LAG_S AXES|search OUTPUT_JSON".into());
    }
    let (start, dur, readout, lag): (f64, f64, f64, f64) = (
        a[2].parse()?,
        a[3].parse()?,
        a[4].parse::<f64>()? / 1000.,
        a[5].parse()?,
    );
    let tel = telemetry::extract_quats(std::path::Path::new(&a[1]))?;
    let km = tel.meta.camera_matrix.ok_or("no lens matrix")?;
    let dd = tel.meta.distortion.ok_or("no distortion")?;
    let (w, h) = (1440i32, 1080i32);
    let sx = w as f64 / tel.meta.calib_w.unwrap_or(w as f64);
    let sy = h as f64 / tel.meta.calib_h.unwrap_or(h as f64);
    let k = Mat::from_slice_2d(&[
        [km[0][0] * sx, 0., km[0][2] * sx],
        [0., km[1][1] * sy, km[1][2] * sy],
        [0., 0., 1.],
    ])?;
    let d = Mat::from_slice(&dd)?.try_clone()?;
    let tq = Tel {
        t: tel.t.clone(),
        q: tel.q.clone(),
    };
    let search = a[6] == "search";
    let crop: Option<(f64, f64)> = std::env::var("O4_CROP").ok().map(|v| {
        let p: Vec<f64> = v.split(',').map(|x| x.parse().unwrap()).collect();
        (p[0], p[1])
    });
    let all = mounts();
    let cands: Vec<usize> = if search {
        (0..all.len()).collect()
    } else {
        vec![a[6].parse()?]
    };

    let f0 = (start * 100.).round() as usize;
    let frames = (dur * 100.).round() as usize;
    let mut child = Command::new("C:/ffmpeg/bin/ffmpeg.exe")
        .args([
            "-v",
            "error",
            "-ss",
            &format!("{}", f0 as f64 / 100.),
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
    let mut buf = vec![0u8; (w * h) as usize];
    let mut prev: Option<Mat> = None;
    let mut rows = vec![];
    let mut score: Vec<Vec<V3>> = vec![vec![]; all.len()];
    let mut kf = 0usize;
    let bearing = |pts: &[Point2f]| -> opencv::Result<Vec<V3>> {
        let src: Vector<Point2f> = pts.iter().copied().collect();
        let mut u = Vector::<Point2f>::new();
        calib3d::fisheye_undistort_points_def(&src, &mut u, &k, &d)?;
        Ok(u.iter()
            .map(|p| {
                let v = [p.x as f64, p.y as f64, 1.];
                let n = (v[0] * v[0] + v[1] * v[1] + 1.).sqrt();
                [v[0] / n, v[1] / n, v[2] / n]
            })
            .collect())
    };
    while out.read_exact(&mut buf).is_ok() {
        let g = Mat::new_rows_cols_with_data(h, w, &buf)?.try_clone()?;
        if let Some(p) = &prev {
            let fi = f0 + kf - 1; // index of the earlier frame
            let tr = track(p, &g)?;
            let tmid = (fi as f64 + 1.) / 100. + readout * 0.5;
            if tr.len() >= 40 {
                let ba = bearing(&tr.iter().map(|x| x.0).collect::<Vec<_>>())?;
                let bb = bearing(&tr.iter().map(|x| x.1).collect::<Vec<_>>())?;
                // Optional: keep only features inside the rectilinear field Gyroflow renders
                // (|x/z| < XMAX, |y/z| < YMAX in normalized coords), env O4_CROP="XMAX,YMAX".
                let keep: Vec<usize> = (0..tr.len())
                    .filter(|&i| match crop {
                        Some((xm, ym)) => [ba[i], bb[i]].iter().all(|b| {
                            b[2] > 0. && (b[0] / b[2]).abs() < xm && (b[1] / b[2]).abs() < ym
                        }),
                        None => true,
                    })
                    .collect();
                let tr: Vec<_> = keep.iter().map(|&i| tr[i]).collect();
                let ba: Vec<V3> = keep.iter().map(|&i| ba[i]).collect();
                let bb: Vec<V3> = keep.iter().map(|&i| bb[i]).collect();
                let qa: Vec<_> = tr
                    .iter()
                    .map(|x| tq.at(fi as f64 / 100. + x.0.y as f64 / h as f64 * readout + lag))
                    .collect();
                let qb: Vec<_> = tr
                    .iter()
                    .map(|x| {
                        tq.at((fi + 1) as f64 / 100. + x.1.y as f64 / h as f64 * readout + lag)
                    })
                    .collect();
                let qm = tq.at(tmid + lag);
                for &ci in &cands {
                    let m = &all[ci];
                    let wa: Vec<V3> = (0..tr.len()).map(|i| qrot(qa[i], mv(m, ba[i]))).collect();
                    let wb: Vec<V3> = (0..tr.len()).map(|i| qrot(qb[i], mv(m, bb[i]))).collect();
                    if let Some((dw, n, rms)) = fit_rotation(&wa, &wb) {
                        if search {
                            score[ci].push(dw);
                        } else {
                            // world-frame residual rotation -> telemetry body frame, per second
                            let db = qrot(quat::qconj(qm), dw);
                            rows.push(json!({"t": tmid, "rate": [db[0]*100., db[1]*100., db[2]*100.], "n": n, "rms": rms}));
                        }
                    } else if !search {
                        rows.push(json!({"t": tmid, "rate": null, "n": 0}));
                    }
                }
            } else if !search {
                rows.push(json!({"t": tmid, "rate": null, "n": 0}));
            }
        }
        prev = Some(g);
        kf += 1;
    }
    child.wait()?;
    if search {
        let mut r: Vec<(usize, f64)> = score
            .iter()
            .enumerate()
            .map(|(i, s)| {
                // frame-to-frame change of the residual: cancels slowly varying translation flow
                let mut s: Vec<f64> = s
                    .windows(2)
                    .map(|w| {
                        (0..3)
                            .map(|k| (w[1][k] - w[0][k]).powi(2))
                            .sum::<f64>()
                            .sqrt()
                    })
                    .collect();
                s.sort_by(f64::total_cmp);
                (
                    i,
                    if s.is_empty() {
                        f64::INFINITY
                    } else {
                        s[s.len() / 2]
                    },
                )
            })
            .collect();
        r.sort_by(|x, y| x.1.total_cmp(&y.1));
        for (i, s) in r.iter().take(5) {
            println!(
                "mount {i}: median |frame-to-frame residual change| {:.6} rad  {:?}",
                s, all[*i]
            );
        }
        return Ok(());
    }
    std::fs::write(
        &a[7],
        serde_json::to_vec(
            &json!({"mp4": a[1], "start": start, "readout_ms": readout * 1000., "lag": lag, "mount": a[6], "pairs": rows}),
        )?,
    )?;
    println!("{} frames, {} pairs", kf, rows.len());
    Ok(())
}
