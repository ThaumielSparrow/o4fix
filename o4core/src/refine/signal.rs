//! Residual conditioning, burst gating and orientation update (feedback-v1).
use super::geometry::V3;
use crate::dsp;
use crate::quat::{qconj, qexp, qmul, qnorm, smoothstep};

pub struct PairSeries {
    pub t: Vec<f64>,
    pub resid: Vec<Option<V3>>,
    pub inliers: Vec<usize>,
    pub tel_rate: Vec<f64>,
}

#[derive(Clone)]
pub struct Conditioned {
    pub t: Vec<f64>,
    pub hp: Vec<V3>,
    pub raw: Vec<V3>,
    pub conf: Vec<f64>,
    pub inliers: Vec<usize>,
    pub tel_rate: Vec<f64>,
    pub valid: Vec<bool>,
}

/// Fill missing pairs linearly, zero-phase high-pass, confidence weights.
/// None when fewer than 60% of pairs are valid or the window is too short.
pub fn condition(s: &PairSeries, fps: f64, hp_hz: f64) -> Option<Conditioned> {
    let n = s.t.len();
    let valid: Vec<bool> = s.resid.iter().map(|r| r.is_some()).collect();
    let nv = valid.iter().filter(|&&v| v).count();
    if n < 50 || (nv as f64) < 0.6 * n as f64 {
        return None;
    }
    let known: Vec<usize> = (0..n).filter(|&i| valid[i]).collect();
    let tk: Vec<f64> = known.iter().map(|&i| s.t[i]).collect();
    let mut raw = vec![[0.0; 3]; n];
    for k in 0..3 {
        let fk: Vec<f64> = known.iter().map(|&i| s.resid[i].unwrap()[k]).collect();
        let filled = dsp::interp(&s.t, &tk, &fk);
        for i in 0..n {
            raw[i][k] = filled[i];
        }
    }
    let ba = dsp::butter_high(2, hp_hz / (fps / 2.0));
    let pad = 150.min(n - 1);
    let cols: Vec<Vec<f64>> = (0..3)
        .map(|k| dsp::filtfilt_padlen(&ba, &raw.iter().map(|r| r[k]).collect::<Vec<_>>(), pad))
        .collect();
    let hp: Vec<V3> = (0..n)
        .map(|i| [cols[0][i], cols[1][i], cols[2][i]])
        .collect();
    let c0: Vec<f64> = (0..n)
        .map(|i| {
            if valid[i] {
                ((s.inliers[i] as f64 - 60.0) / 140.0).clamp(0.0, 1.0)
            } else {
                0.0
            }
        })
        .collect();
    // numpy convolve(c0, ones(9)/9, 'same')
    let conf: Vec<f64> = (0..n)
        .map(|i| {
            (i.saturating_sub(4)..(i + 5).min(n))
                .map(|j| c0[j])
                .sum::<f64>()
                / 9.0
        })
        .collect();
    Some(Conditioned {
        t: s.t.clone(),
        hp,
        raw,
        conf,
        inliers: s.inliers.clone(),
        tel_rate: s.tel_rate.clone(),
        valid,
    })
}

fn env(t: f64, a: f64, b: f64, fade: f64) -> f64 {
    smoothstep(((t - a).min(b - t) / fade).clamp(0.0, 1.0))
}

pub fn gate(t: &[f64], bursts: &[(f64, f64)], pad: f64, fade: f64) -> Vec<f64> {
    t.iter()
        .map(|&x| {
            bursts
                .iter()
                .map(|&(a, b)| env(x, a - pad, b + pad, fade))
                .fold(0.0, f64::max)
        })
        .collect()
}

/// Cumulative correction angle (rad) for one burst: integral of −gate·conf·hp.
pub fn burst_correction(
    c: &Conditioned,
    burst: (f64, f64),
    pad: f64,
    fade: f64,
    dt: f64,
) -> Vec<V3> {
    correction(c, &[burst], pad, fade, dt)
}

/// Cumulative correction angle (rad) for a set of bursts sharing one window:
/// integral of −gate·conf·hp with ONE union gate (max over bursts), as the
/// research `correct_from_warp` does, so overlapping gates never weigh > 1.
pub fn correction(c: &Conditioned, bursts: &[(f64, f64)], pad: f64, fade: f64, dt: f64) -> Vec<V3> {
    let g = gate(&c.t, bursts, pad, fade);
    let mut acc = [0.0; 3];
    (0..c.t.len())
        .map(|i| {
            let w = g[i] * c.conf[i];
            for (a, h) in acc.iter_mut().zip(c.hp[i]) {
                *a -= w * h * dt;
            }
            acc
        })
        .collect()
}

pub fn max_angle_deg(ang: &[V3]) -> f64 {
    ang.iter()
        .map(|v| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt())
        .fold(0.0, f64::max)
        .to_degrees()
}

#[derive(Debug)]
pub struct GeometryStats {
    pub hp_rms_deg: f64,
    /// None when fewer than 50 calm pairs move faster than 30 deg/s; the
    /// motion-ratio check is then not applicable and counts as passing
    /// (refine() logs that it was skipped).
    pub motion_ratio: Option<f64>,
    pub pairs: usize,
}

/// Non-burst (gate < 0.01, ≥ 300 inliers) residual statistics across windows.
pub fn geometry_stats(
    ws: &[Conditioned],
    bursts: &[(f64, f64)],
    pad: f64,
    fade: f64,
) -> GeometryStats {
    let (mut ss, mut n) = (0.0, 0usize);
    let (mut res_mag, mut tel_mag) = (vec![], vec![]);
    for c in ws {
        let g = gate(&c.t, bursts, pad, fade);
        for (i, &gi) in g.iter().enumerate() {
            if gi >= 0.01 || c.inliers[i] < 300 || !c.valid[i] {
                continue;
            }
            n += 1;
            ss += c.hp[i].iter().map(|v| v * v).sum::<f64>();
            if c.tel_rate[i] > 30f64.to_radians() {
                res_mag.push(c.raw[i].iter().map(|v| v * v).sum::<f64>().sqrt());
                tel_mag.push(c.tel_rate[i]);
            }
        }
    }
    let med = |mut v: Vec<f64>| {
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    };
    GeometryStats {
        hp_rms_deg: if n > 0 {
            (ss / n as f64).sqrt().to_degrees()
        } else {
            f64::NAN
        },
        motion_ratio: (res_mag.len() >= 50).then(|| med(res_mag) / med(tel_mag)),
        pairs: n,
    }
}

/// Sort windows by start time and carry each window's final angle into the next.
pub fn chain_windows(mut ws: Vec<(Vec<f64>, Vec<V3>)>) -> (Vec<f64>, Vec<V3>) {
    ws.sort_by(|a, b| a.0[0].total_cmp(&b.0[0]));
    let (mut t, mut ang, mut total) = (vec![], vec![], [0.0; 3]);
    for (wt, wa) in ws {
        let base = total;
        for (x, a) in wt.into_iter().zip(wa) {
            let v = [base[0] + a[0], base[1] + a[1], base[2] + a[2]];
            t.push(x);
            ang.push(v);
            total = v;
        }
    }
    (t, ang)
}

/// Apply angle increments as extra body rotation per telemetry sample:
/// q'[i+1] = q'[i] · (q[i]⁻¹ q[i+1]) · exp(ΔA). Angle is 0 before t_ang[0],
/// held after the end; untouched samples keep their bits.
pub fn apply_increments(t_tel: &[f64], q: &[[f64; 4]], t_ang: &[f64], ang: &[V3]) -> Vec<[f64; 4]> {
    let cols: Vec<Vec<f64>> = (0..3)
        .map(|k| {
            let a: Vec<f64> = ang.iter().map(|v| v[k]).collect();
            t_tel
                .iter()
                .map(|&x| {
                    if x < t_ang[0] {
                        0.0
                    } else {
                        dsp::interp(&[x], t_ang, &a)[0]
                    }
                })
                .collect()
        })
        .collect();
    let mut out = q.to_vec();
    for i in 0..q.len() - 1 {
        let inc = [
            cols[0][i + 1] - cols[0][i],
            cols[1][i + 1] - cols[1][i],
            cols[2][i + 1] - cols[2][i],
        ];
        if inc == [0.0; 3] && out[i] == q[i] {
            continue;
        }
        let dq = qmul(qconj(q[i]), q[i + 1]);
        out[i + 1] = qnorm(qmul(qmul(out[i], dq), qexp(inc)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quat::{qexp, quats_to_rates};

    fn series(t0: f64, n: usize, f: impl Fn(f64) -> V3) -> PairSeries {
        let t: Vec<f64> = (0..n).map(|i| t0 + i as f64 * 0.01).collect();
        PairSeries {
            resid: t.iter().map(|&x| Some(f(x))).collect(),
            inliers: vec![500; n],
            tel_rate: vec![0.5; n],
            t,
        }
    }

    #[test]
    fn condition_fills_gaps_and_highpasses() {
        let mut s = series(10.0, 1000, |x| {
            [
                0.02 * (2.0 * std::f64::consts::PI * 5.0 * x).sin() + 0.3,
                0.,
                0.,
            ]
        });
        s.resid[400] = None;
        s.inliers[400] = 0;
        let c = condition(&s, 100.0, 1.0).unwrap();
        // DC 0.3 removed, 5 Hz kept
        let mid = &c.hp[200..800];
        let mean = mid.iter().map(|v| v[0]).sum::<f64>() / mid.len() as f64;
        assert!(mean.abs() < 1e-3);
        assert!(c.hp[400][0].is_finite() && !c.valid[400]);
        assert!(c.conf[400] < 1.0 && c.conf[100] == 1.0);
    }

    #[test]
    fn condition_rejects_mostly_missing() {
        let mut s = series(0.0, 300, |_| [0.; 3]);
        for r in s.resid.iter_mut().skip(100) {
            *r = None;
        }
        assert!(condition(&s, 100.0, 1.0).is_none());
    }

    #[test]
    fn gate_zero_outside_and_one_inside() {
        let t: Vec<f64> = (0..600).map(|i| i as f64 * 0.01).collect();
        let g = gate(&t, &[(2.0, 3.0)], 0.25, 0.15);
        assert_eq!(g[100], 0.0);
        assert_eq!(g[250], 1.0);
        assert_eq!(g[500], 0.0);
    }

    #[test]
    fn burst_correction_opposes_residual_and_is_flat_outside() {
        let s = series(0.0, 600, |x| {
            [0.05 * (2.0 * std::f64::consts::PI * 4.0 * x).sin(), 0., 0.]
        });
        let c = condition(&s, 100.0, 1.0).unwrap();
        let ang = burst_correction(&c, (2.0, 4.0), 0.25, 0.15, 0.01);
        assert_eq!(
            ang[..150]
                .iter()
                .map(|v| v[0])
                .fold(0., |a: f64, b| a.max(b.abs())),
            0.0
        );
        // derivative of the angle inside the burst is -residual
        let i = 300;
        let d = (ang[i + 1][0] - ang[i][0]) / 0.01;
        assert!((d + c.hp[i + 1][0]).abs() < 1e-9);
        // constant after the gate
        assert_eq!(ang[500], ang[599]);
    }

    #[test]
    fn zero_correction_returns_input_bits() {
        let t: Vec<f64> = (0..2000).map(|i| i as f64 * 0.001).collect();
        let q: Vec<[f64; 4]> = t.iter().map(|&x| qexp([0.1 * x, 0.02, -0.3 * x])).collect();
        let out = apply_increments(&t, &q, &[0.5, 1.5], &[[0.; 3], [0.; 3]]);
        assert_eq!(out, q);
    }

    #[test]
    fn increments_keep_rates_outside_and_leave_world_offset() {
        let t: Vec<f64> = (0..3001).map(|i| i as f64 * 0.001).collect();
        let q: Vec<[f64; 4]> = t
            .iter()
            .map(|&x| qexp([0.4 * x, 0.1 * (3.0 * x).sin(), 0.2]))
            .collect();
        let t_ang: Vec<f64> = (0..301).map(|i| i as f64 * 0.01).collect();
        let ang: Vec<V3> = t_ang
            .iter()
            .map(|&x| {
                let s = ((x - 1.0) / 1.0).clamp(0.0, 1.0);
                [0.01 * s, -0.02 * s, 0.005 * s]
            })
            .collect();
        let out = apply_increments(&t, &q, &t_ang, &ang);
        assert!(
            out[..1000].iter().zip(&q[..1000]).all(|(a, b)| a == b),
            "pre-gate bits"
        );
        let (_, r0) = quats_to_rates(&t[2100..], &q[2100..]);
        let (_, r1) = quats_to_rates(&t[2100..], &out[2100..]);
        for (a, b) in r0.iter().zip(&r1) {
            for k in 0..3 {
                assert!((a[k] - b[k]).abs() < 1e-9);
            }
        }
        // after the gate, out = W * q with a constant world rotation W
        let w = |i: usize| qmul(out[i], qconj(q[i]));
        let (wa, wb) = (w(2100), w(3000));
        let dot: f64 = (0..4).map(|k| wa[k] * wb[k]).sum();
        assert!(dot.abs() > 1.0 - 1e-12);
    }

    #[test]
    fn merged_windows_are_time_ordered_and_continuous() {
        // two windows supplied out of order must still yield zero angle before the first
        let a = (vec![5.0, 5.01, 5.02], vec![[0.01, 0., 0.]; 3]);
        let b = (vec![1.0, 1.01, 1.02], vec![[0.02, 0., 0.]; 3]);
        let (t, ang) = chain_windows(vec![a, b]);
        assert!(t.windows(2).all(|w| w[1] > w[0]));
        assert_eq!(ang[0], [0.02, 0., 0.]); // first window's own angle, no carried offset from the later one
        assert_eq!(ang[3], [0.03, 0., 0.]); // later window carries the earlier total
    }

    #[test]
    fn geometry_stats_floor_rms() {
        let s = series(0.0, 800, |x| {
            [0.02 * (2.0 * std::f64::consts::PI * 6.0 * x).sin(), 0., 0.]
        });
        let c = condition(&s, 100.0, 1.0).unwrap();
        let g = geometry_stats(&[c], &[(3.0, 4.0)], 0.25, 0.15);
        assert!(g.pairs > 300);
        let expect = 0.02_f64.to_degrees() / 2f64.sqrt();
        assert!((g.hp_rms_deg - expect).abs() < 0.1 * expect);
    }
}
