//! Pure geometry for residual refinement (feedback-v1 `warp_residual_probe`).
use crate::quat;

pub type V3 = [f64; 3];
pub type M3 = [[f64; 3]; 3];

/// Camera axes (x right, y down, z forward) -> telemetry body axes of DJI O4P
/// quaternions as decoded by `telemetry::extract_quats` (exhaustive
/// signed-permutation search, feedback-v1).
pub const O4P_MOUNT: M3 = [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, -1.0]];

pub fn mat_vec(m: &M3, v: V3) -> V3 {
    std::array::from_fn(|r| m[r][0] * v[0] + m[r][1] * v[1] + m[r][2] * v[2])
}

pub fn rotate(q: [f64; 4], v: V3) -> V3 {
    let p = quat::qmul(quat::qmul(q, [0.0, v[0], v[1], v[2]]), quat::qconj(q));
    [p[1], p[2], p[3]]
}

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn det3(m: &M3) -> f64 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

fn solve3(a: M3, b: V3) -> Option<V3> {
    let det = det3(&a);
    if det.abs() < 1e-18 {
        return None;
    }
    Some(std::array::from_fn(|k| {
        let mut m = a;
        for r in 0..3 {
            m[r][k] = b[r];
        }
        det3(&m) / det
    }))
}

/// Small rotation `d` (rad) with `b ≈ a + d × a`, by linear least squares with
/// four trimming rounds (keep residual < max(3·median, floor)).
/// Returns None when fewer than `min_inliers` points survive.
pub fn fit_small_rotation(
    a: &[V3],
    b: &[V3],
    min_inliers: usize,
    floor: f64,
) -> Option<(V3, usize)> {
    let mut keep = vec![true; a.len()];
    let mut d = [0.0; 3];
    let mut n = 0;
    for _ in 0..4 {
        let mut ata = [[0.0; 3]; 3];
        let mut atb = [0.0; 3];
        n = 0;
        for i in (0..a.len()).filter(|&i| keep[i]) {
            n += 1;
            let ax = [
                [0.0, -a[i][2], a[i][1]],
                [a[i][2], 0.0, -a[i][0]],
                [-a[i][1], a[i][0], 0.0],
            ];
            let r0: V3 = std::array::from_fn(|k| b[i][k] - a[i][k]);
            for p in 0..3 {
                for q in 0..3 {
                    ata[p][q] += (0..3).map(|k| ax[k][p] * ax[k][q]).sum::<f64>();
                }
                atb[p] -= (0..3).map(|k| ax[k][p] * r0[k]).sum::<f64>();
            }
        }
        if n < min_inliers {
            return None;
        }
        d = solve3(ata, atb)?;
        let res: Vec<f64> = (0..a.len())
            .map(|i| {
                let c = cross(d, a[i]);
                (0..3)
                    .map(|k| (b[i][k] - a[i][k] - c[k]).powi(2))
                    .sum::<f64>()
                    .sqrt()
            })
            .collect();
        let mut s: Vec<f64> = (0..a.len()).filter(|&i| keep[i]).map(|i| res[i]).collect();
        s.sort_by(f64::total_cmp);
        let thr = (3.0 * s[s.len() / 2]).max(floor);
        for i in 0..a.len() {
            keep[i] = res[i] < thr;
        }
    }
    (n >= min_inliers).then_some((d, n))
}

/// Orientation track with slerp lookup, clamped to its ends.
pub struct Orientation<'a> {
    pub t: &'a [f64],
    pub q: &'a [[f64; 4]],
}

impl Orientation<'_> {
    pub fn at(&self, t: f64) -> [f64; 4] {
        let i = self
            .t
            .partition_point(|&x| x <= t)
            .clamp(1, self.t.len() - 1);
        let f = ((t - self.t[i - 1]) / (self.t[i] - self.t[i - 1])).clamp(0.0, 1.0);
        quat::slerp(self.q[i - 1], self.q[i], f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quat::qexp;
    fn lcg(seed: &mut u64) -> f64 {
        *seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((*seed >> 11) as f64) / ((1u64 << 53) as f64)
    }
    fn bearings(n: usize, seed: &mut u64) -> Vec<V3> {
        (0..n)
            .map(|_| {
                let (x, y) = (lcg(seed) * 2.6 - 1.3, lcg(seed) * 1.44 - 0.72);
                let m = (x * x + y * y + 1.0).sqrt();
                [x / m, y / m, 1.0 / m]
            })
            .collect()
    }
    #[test]
    fn fit_recovers_small_rotation_with_outliers() {
        let mut s = 7;
        let a = bearings(400, &mut s);
        let d = [0.0012, -0.0021, 0.0006];
        let q = qexp(d);
        let mut b: Vec<V3> = a.iter().map(|&v| rotate(q, v)).collect();
        for v in b.iter_mut().step_by(5) {
            v[0] += 0.02; // 20% gross outliers
        }
        let (e, n) = fit_small_rotation(&a, &b, 30, 0.0008).unwrap();
        for k in 0..3 {
            assert!((e[k] - d[k]).abs() < 2e-6, "{e:?} vs {d:?}");
        }
        assert!(n >= 300);
    }
    #[test]
    fn fit_refuses_too_few_points() {
        let mut s = 3;
        let a = bearings(20, &mut s);
        assert!(fit_small_rotation(&a, &a, 30, 0.0008).is_none());
    }
    #[test]
    fn mount_is_proper_rotation_and_rotate_identity() {
        let m = O4P_MOUNT;
        let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
        assert_eq!(det, 1.0);
        assert_eq!(rotate([1., 0., 0., 0.], [0.3, 0.4, 0.5]), [0.3, 0.4, 0.5]);
        assert_eq!(mat_vec(&m, [1., 2., 3.]), [1., -2., -3.]);
    }
    #[test]
    fn orientation_interpolates_and_clamps() {
        let t = [0.0, 1.0];
        let q = [[1., 0., 0., 0.], qexp([0., 0., 0.2])];
        let o = Orientation { t: &t, q: &q };
        let mid = o.at(0.5);
        let want = qexp([0., 0., 0.1]);
        assert!((0..4).all(|k| (mid[k] - want[k]).abs() < 1e-12));
        assert_eq!(o.at(-5.0), q[0]);
        assert!((0..4).all(|k| (o.at(9.0)[k] - q[1][k]).abs() < 1e-12));
    }
}
