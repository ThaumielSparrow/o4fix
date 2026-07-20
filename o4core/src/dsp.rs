//! scipy.signal-compatible filtering. butter via scipy's zpk+bilinear path;
//! filtfilt = odd-ext padding, default padlen = 3*max(len(a),len(b)).
use num_complex::Complex64 as C;

#[derive(Clone, Debug)]
pub struct Ba {
    pub b: Vec<f64>,
    pub a: Vec<f64>,
}

/// scipy buttap: p = -exp(1j*pi*arange(-N+1, N, 2)/(2N)), k=1.
fn buttap(n: usize) -> Vec<C> {
    let mut p = Vec::with_capacity(n);
    let mut m = -(n as i64) + 1;
    while m < n as i64 {
        let th = std::f64::consts::PI * m as f64 / (2.0 * n as f64);
        p.push(-C::new(th.cos(), th.sin()));
        m += 2;
    }
    p
}

/// Real coefficients of prod(x - r_i); roots come in conjugate pairs.
fn poly(roots: &[C]) -> Vec<f64> {
    let mut c = vec![C::new(1.0, 0.0)];
    for r in roots {
        c.push(C::new(0.0, 0.0));
        for i in (1..c.len()).rev() {
            let prev = c[i - 1];
            c[i] -= r * prev;
        }
    }
    c.iter().map(|z| z.re).collect()
}

/// scipy bilinear_zpk with fs=2 (its internal digital-design value).
fn bilinear_zpk(z: &[C], p: &[C], k: f64) -> (Vec<C>, Vec<C>, f64) {
    let fs2 = 4.0; // 2*fs
    let mut zd: Vec<C> = z.iter().map(|&x| (fs2 + x) / (fs2 - x)).collect();
    let pd: Vec<C> = p.iter().map(|&x| (fs2 + x) / (fs2 - x)).collect();
    let num = z.iter().fold(C::new(1.0, 0.0), |acc, &x| acc * (fs2 - x));
    let den = p.iter().fold(C::new(1.0, 0.0), |acc, &x| acc * (fs2 - x));
    let kd = k * (num / den).re;
    while zd.len() < pd.len() {
        zd.push(C::new(-1.0, 0.0));
    }
    (zd, pd, kd)
}

pub fn butter_low(order: usize, wn: f64) -> Ba {
    let warped = 4.0 * (std::f64::consts::PI * wn / 2.0).tan();
    let p: Vec<C> = buttap(order).iter().map(|&x| x * warped).collect();
    let (zd, pd, kd) = bilinear_zpk(&[], &p, warped.powi(order as i32));
    Ba {
        b: poly(&zd).iter().map(|c| c * kd).collect(),
        a: poly(&pd),
    }
}

pub fn butter_band(order: usize, wn_lo: f64, wn_hi: f64) -> Ba {
    let w1 = 4.0 * (std::f64::consts::PI * wn_lo / 2.0).tan();
    let w2 = 4.0 * (std::f64::consts::PI * wn_hi / 2.0).tan();
    let (bw, wo) = (w2 - w1, (w1 * w2).sqrt());
    let mut p_bp = Vec::with_capacity(2 * order);
    for &pp in &buttap(order) {
        let pl = pp * (bw / 2.0);
        let disc = (pl * pl - C::new(wo * wo, 0.0)).sqrt();
        p_bp.push(pl + disc);
        p_bp.push(pl - disc);
    }
    let z_bp = vec![C::new(0.0, 0.0); order];
    let (zd, pd, kd) = bilinear_zpk(&z_bp, &p_bp, bw.powi(order as i32));
    Ba {
        b: poly(&zd).iter().map(|c| c * kd).collect(),
        a: poly(&pd),
    }
}

/// Direct-form II transposed; scipy lfilter with initial conditions.
pub fn lfilter(ba: &Ba, x: &[f64], zi: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let n = ba.b.len().max(ba.a.len());
    let g = |v: &[f64], i: usize| v.get(i).copied().unwrap_or(0.0);
    let mut z: Vec<f64> = (0..n - 1).map(|i| g(zi, i)).collect();
    let mut y = Vec::with_capacity(x.len());
    for &xi in x {
        let yi = g(&ba.b, 0) * xi + z[0];
        for i in 0..n - 2 {
            z[i] = g(&ba.b, i + 1) * xi + z[i + 1] - g(&ba.a, i + 1) * yi;
        }
        z[n - 2] = g(&ba.b, n - 1) * xi - g(&ba.a, n - 1) * yi;
        y.push(yi);
    }
    (y, z)
}

/// scipy lfilter_zi: solve (I - companion(a)^T) zi = b[1:] - a[1:]*b[0].
pub fn lfilter_zi(ba: &Ba) -> Vec<f64> {
    let n = ba.b.len().max(ba.a.len());
    let g = |v: &[f64], i: usize| v.get(i).copied().unwrap_or(0.0);
    let m = n - 1;
    // companion(a)^T[i][jj] = companion(a)[jj][i]:
    // = -a[i+1]/a[0] when jj == 0; = 1 when i == jj - 1; else 0.
    let mut mat = vec![vec![0.0; m]; m];
    for (i, row) in mat.iter_mut().enumerate() {
        for (jj, cell) in row.iter_mut().enumerate() {
            let ct = if jj == 0 {
                -g(&ba.a, i + 1) / g(&ba.a, 0)
            } else if i == jj - 1 {
                1.0
            } else {
                0.0
            };
            *cell = if i == jj { 1.0 - ct } else { -ct };
        }
    }
    let mut rhs: Vec<f64> = (0..m)
        .map(|i| g(&ba.b, i + 1) - g(&ba.a, i + 1) * g(&ba.b, 0))
        .collect();
    // gaussian elimination with partial pivoting (m <= 4)
    for c in 0..m {
        let piv = (c..m)
            .max_by(|&i, &jj| mat[i][c].abs().total_cmp(&mat[jj][c].abs()))
            .unwrap();
        mat.swap(c, piv);
        rhs.swap(c, piv);
        for r in c + 1..m {
            let f = mat[r][c] / mat[c][c];
            for k in c..m {
                let v = mat[c][k];
                mat[r][k] -= f * v;
            }
            rhs[r] -= f * rhs[c];
        }
    }
    let mut zi = vec![0.0; m];
    for r in (0..m).rev() {
        let s: f64 = (r + 1..m).map(|k| mat[r][k] * zi[k]).sum();
        zi[r] = (rhs[r] - s) / mat[r][r];
    }
    zi
}

pub fn filtfilt(ba: &Ba, x: &[f64]) -> Vec<f64> {
    filtfilt_padlen(ba, x, 3 * ba.b.len().max(ba.a.len()))
}

/// scipy filtfilt, method="pad", padtype="odd".
pub fn filtfilt_padlen(ba: &Ba, x: &[f64], padlen: usize) -> Vec<f64> {
    let n = x.len();
    assert!(padlen < n, "padlen {padlen} >= signal len {n}");
    let mut ext = Vec::with_capacity(n + 2 * padlen);
    for i in (1..=padlen).rev() {
        ext.push(2.0 * x[0] - x[i]);
    }
    ext.extend_from_slice(x);
    for i in 1..=padlen {
        ext.push(2.0 * x[n - 1] - x[n - 1 - i]);
    }
    let zi = lfilter_zi(ba);
    let zi0: Vec<f64> = zi.iter().map(|z| z * ext[0]).collect();
    let (fwd, _) = lfilter(ba, &ext, &zi0);
    let rev: Vec<f64> = fwd.into_iter().rev().collect();
    let zi1: Vec<f64> = zi.iter().map(|z| z * rev[0]).collect();
    let (bwd, _) = lfilter(ba, &rev, &zi1);
    let out: Vec<f64> = bwd.into_iter().rev().collect();
    out[padlen..padlen + n].to_vec()
}

pub fn filtfilt3(ba: &Ba, x: &[[f64; 3]]) -> Vec<[f64; 3]> {
    let cols: Vec<Vec<f64>> = (0..3)
        .map(|k| filtfilt(ba, &x.iter().map(|r| r[k]).collect::<Vec<_>>()))
        .collect();
    (0..x.len())
        .map(|i| [cols[0][i], cols[1][i], cols[2][i]])
        .collect()
}

/// scipy.ndimage.median_filter, 1-D, odd size, mode="nearest".
pub fn median_filter(x: &[f64], size: usize) -> Vec<f64> {
    assert!(size % 2 == 1);
    let n = x.len() as isize;
    let h = (size / 2) as isize;
    let mut win = vec![0.0; size];
    (0..n)
        .map(|i| {
            for (w, j) in win.iter_mut().zip(i - h..=i + h) {
                *w = x[j.clamp(0, n - 1) as usize];
            }
            win.sort_by(f64::total_cmp);
            win[size / 2]
        })
        .collect()
}

/// scipy.ndimage.uniform_filter1d, mode="nearest", origin=0.
/// Even size: window [i - size/2, i + size/2 - 1] (left-heavy), per scipy.
pub fn uniform_filter1d(x: &[f64], size: usize) -> Vec<f64> {
    let n = x.len() as isize;
    let s = size as isize;
    let lo = -(s / 2);
    (0..n)
        .map(|i| {
            let mut acc = 0.0;
            for k in 0..s {
                acc += x[(i + lo + k).clamp(0, n - 1) as usize];
            }
            acc / size as f64
        })
        .collect()
}

pub fn uniform_filter3(x: &[[f64; 3]], size: usize) -> Vec<[f64; 3]> {
    let cols: Vec<Vec<f64>> = (0..3)
        .map(|k| uniform_filter1d(&x.iter().map(|r| r[k]).collect::<Vec<_>>(), size))
        .collect();
    (0..x.len())
        .map(|i| [cols[0][i], cols[1][i], cols[2][i]])
        .collect()
}

/// o4fix.hampel (o4fix.py:193-201), per axis. Returns (cleaned, spike frac).
pub fn hampel(x: &[[f64; 3]], k: usize, nsig: f64) -> (Vec<[f64; 3]>, f64) {
    let size = 2 * k + 1;
    let mut out = x.to_vec();
    let mut bad = 0usize;
    for ax in 0..3 {
        let col: Vec<f64> = x.iter().map(|r| r[ax]).collect();
        let med = median_filter(&col, size);
        let dev: Vec<f64> = col.iter().zip(&med).map(|(a, m)| (a - m).abs()).collect();
        let sig = median_filter(&dev, size);
        for i in 0..col.len() {
            if dev[i] > nsig * (1.4826 * sig[i] + 1e-9) {
                out[i][ax] = med[i];
                bad += 1;
            }
        }
    }
    (out, bad as f64 / (x.len() * 3) as f64)
}

/// np.interp with edge clamping.
pub fn interp(xq: &[f64], xp: &[f64], fp: &[f64]) -> Vec<f64> {
    xq.iter()
        .map(|&q| {
            if q <= xp[0] {
                return fp[0];
            }
            if q >= xp[xp.len() - 1] {
                return fp[fp.len() - 1];
            }
            let j = searchsorted_right(xp, q) - 1;
            let t = (q - xp[j]) / (xp[j + 1] - xp[j]);
            fp[j] + t * (fp[j + 1] - fp[j])
        })
        .collect()
}

/// np.gradient, unit spacing: central diffs, one-sided edges.
pub fn gradient(x: &[f64]) -> Vec<f64> {
    let n = x.len();
    (0..n)
        .map(|i| {
            if i == 0 {
                x[1] - x[0]
            } else if i == n - 1 {
                x[n - 1] - x[n - 2]
            } else {
                (x[i + 1] - x[i - 1]) / 2.0
            }
        })
        .collect()
}

pub fn searchsorted_left(a: &[f64], v: f64) -> usize {
    a.partition_point(|&e| e < v)
}
pub fn searchsorted_right(a: &[f64], v: f64) -> usize {
    a.partition_point(|&e| e <= v)
}
