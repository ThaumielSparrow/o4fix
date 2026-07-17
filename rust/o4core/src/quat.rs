//! Quaternion ops (wxyz, Hamilton). Ports o4fix.py:39-188.

pub fn qmul(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    let [w1, x1, y1, z1] = a;
    let [w2, x2, y2, z2] = b;
    [w1*w2 - x1*x2 - y1*y2 - z1*z2,
     w1*x2 + x1*w2 + y1*z2 - z1*y2,
     w1*y2 - x1*z2 + y1*w2 + z1*x2,
     w1*z2 + x1*y2 - y1*x2 + z1*w2]
}

pub fn qconj(a: [f64; 4]) -> [f64; 4] { [a[0], -a[1], -a[2], -a[3]] }

pub fn qnorm(q: [f64; 4]) -> [f64; 4] {
    let n = (q[0]*q[0] + q[1]*q[1] + q[2]*q[2] + q[3]*q[3]).sqrt();
    [q[0]/n, q[1]/n, q[2]/n, q[3]/n]
}

/// Rotation vector -> unit quat; sin(t/2)/t -> 0.5 small-angle (o4fix.py:97-103).
pub fn qexp(v: [f64; 3]) -> [f64; 4] {
    let theta = (v[0]*v[0] + v[1]*v[1] + v[2]*v[2]).sqrt();
    let k = if theta > 1e-12 { (theta / 2.0).sin() / theta } else { 0.5 };
    qnorm([(theta / 2.0).cos(), v[0]*k, v[1]*k, v[2]*k])
}

/// Unit quat -> rotation vector via 2*asin(|vec|) (o4fix.py:106-112).
pub fn qlog(q: [f64; 4]) -> [f64; 3] {
    let q = if q[0] < 0.0 { [-q[0], -q[1], -q[2], -q[3]] } else { q };
    let vecn = (q[1]*q[1] + q[2]*q[2] + q[3]*q[3]).sqrt();
    let theta = 2.0 * vecn.clamp(0.0, 1.0).asin();
    let k = if vecn > 1e-12 { theta / vecn } else { 2.0 };
    [q[1]*k, q[2]*k, q[3]*k]
}

/// Element-wise slerp (o4fix.py:115-129).
pub fn slerp(qa: [f64; 4], qb: [f64; 4], w: f64) -> [f64; 4] {
    let dot: f64 = (0..4).map(|i| qa[i] * qb[i]).sum();
    let (qb, dot) = if dot < 0.0 {
        ([-qb[0], -qb[1], -qb[2], -qb[3]], -dot)
    } else { (qb, dot) };
    let theta = dot.clamp(-1.0, 1.0).acos();
    let sin_t = theta.sin();
    let (wa, wb) = if sin_t < 1e-6 {
        (1.0 - w, w)
    } else {
        (((1.0 - w) * theta).sin() / sin_t.max(1e-12),
         (w * theta).sin() / sin_t.max(1e-12))
    };
    qnorm(core::array::from_fn(|i| wa * qa[i] + wb * qb[i]))
}

pub fn smoothstep(x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Body angular rate between consecutive quats (o4fix.py:176-188).
/// Returns (t_mid, omega rad/s), len N-1.
pub fn quats_to_rates(t: &[f64], q: &[[f64; 4]]) -> (Vec<f64>, Vec<[f64; 3]>) {
    let n = q.len() - 1;
    let mut tm = Vec::with_capacity(n);
    let mut om = Vec::with_capacity(n);
    for i in 0..n {
        let mut dq = qmul(qconj(q[i]), q[i + 1]);
        if dq[0] < 0.0 { for c in dq.iter_mut() { *c = -*c; } }
        let vecn = (dq[1]*dq[1] + dq[2]*dq[2] + dq[3]*dq[3]).sqrt();
        let theta = 2.0 * vecn.clamp(0.0, 1.0).asin();
        let scale = if vecn > 1e-12 { theta / vecn } else { 2.0 };
        let dt = t[i + 1] - t[i];
        let s = scale / dt.max(1e-9);
        tm.push(t[i] + dt / 2.0);
        om.push([dq[1]*s, dq[2]*s, dq[3]*s]);
    }
    (tm, om)
}
