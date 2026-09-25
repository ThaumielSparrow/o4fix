//! Experimental contiguous-run calibration. Not used by the default pipeline.
use crate::{
    dsp,
    optical::{Alignment, OpticalRates},
    pipeline::median,
};
use nalgebra::Matrix3;

struct Run {
    t: Vec<f64>,
    optical: Vec<[f64; 3]>,
    ba: dsp::Ba,
    trim: usize,
}

pub struct CalibrationData {
    runs: Vec<Run>,
    tm: Vec<f64>,
    gyro: Vec<[f64; 3]>,
}

#[derive(Clone, Copy, Debug)]
pub struct Score {
    pub rms_deg_s: f64,
    pub r2: f64,
    pub samples: usize,
    pub runs: usize,
}

impl CalibrationData {
    pub fn prepare(opt: &OpticalRates, tm: &[f64], gyro: &[[f64; 3]], fs: f64) -> Option<Self> {
        if opt.t.len() != opt.omega.len()
            || opt.t.len() != opt.quality.len()
            || tm.len() != gyro.len()
            || tm.len() < 20
            || !fs.is_finite()
            || fs <= 0.0
            || tm.iter().any(|v| !v.is_finite())
            || tm.windows(2).any(|w| w[1] <= w[0])
            || gyro.iter().flatten().any(|v| !v.is_finite())
        {
            return None;
        }
        let mut order: Vec<usize> = (0..opt.t.len()).filter(|&i| opt.t[i].is_finite()).collect();
        order.sort_by(|&a, &b| opt.t[a].total_cmp(&opt.t[b]));
        order.dedup_by(|a, b| opt.t[*a] == opt.t[*b]);
        let dts: Vec<f64> = order
            .windows(2)
            .map(|w| opt.t[w[1]] - opt.t[w[0]])
            .collect();
        if dts.is_empty() {
            return None;
        }
        let dt = median(&dts);
        if dt <= 0.0 || !dt.is_finite() {
            return None;
        }
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut group = Vec::new();
        for i in order {
            let good = opt.quality[i].is_finite() && opt.quality[i] > 0.5
                && opt.omega[i].iter().all(|v| v.is_finite())
                // Keep the same samples for every trial shift; never clamp gyro edges.
                && opt.t[i] >= tm[0] + 0.065 && opt.t[i] <= tm[tm.len()-1] - 0.065;
            let gap = group.last().is_some_and(|&last: &usize| {
                let d = opt.t[i] - opt.t[last];
                d > 1.5 * dt || (d - dt).abs() > 0.1 * dt
            });
            if (!good || gap) && !group.is_empty() {
                groups.push(std::mem::take(&mut group));
            }
            if good {
                group.push(i);
            }
        }
        if !group.is_empty() {
            groups.push(group);
        }
        let mut runs = Vec::new();
        for group in groups {
            let t: Vec<f64> = group.iter().map(|&i| opt.t[i]).collect();
            if t.len() < 10 || t[t.len() - 1] - t[0] < 0.6 {
                continue;
            }
            let cadence = median(&t.windows(2).map(|w| w[1] - w[0]).collect::<Vec<_>>());
            let trim = (0.15 / cadence).ceil() as usize;
            if t.len() <= 2 * trim + 9 {
                continue;
            }
            let ba = dsp::butter_low(2, (5.0 * cadence * 2.0).min(0.9));
            let raw: Vec<[f64; 3]> = group
                .iter()
                .map(|&i| opt.omega[i].map(f64::to_degrees))
                .collect();
            runs.push(Run {
                t,
                optical: dsp::filtfilt3(&ba, &raw),
                ba,
                trim,
            });
        }
        if runs.is_empty() {
            return None;
        }
        Some(Self {
            runs,
            tm: tm.to_vec(),
            gyro: dsp::uniform_filter3(gyro, ((fs / 100.0) as usize).max(1)),
        })
    }

    fn pairs(&self, shift: f64) -> (Vec<[f64; 3]>, Vec<[f64; 3]>) {
        let mut b = Vec::new();
        let mut a = Vec::new();
        let cols: Vec<Vec<f64>> = (0..3)
            .map(|k| self.gyro.iter().map(|r| r[k]).collect())
            .collect();
        for run in &self.runs {
            let query: Vec<f64> = run.t.iter().map(|t| t + shift).collect();
            let values: Vec<Vec<f64>> = cols
                .iter()
                .map(|c| dsp::interp(&query, &self.tm, c))
                .collect();
            let sampled: Vec<[f64; 3]> = (0..query.len())
                .map(|i| [values[0][i], values[1][i], values[2][i]])
                .collect();
            let filtered = dsp::filtfilt3(&run.ba, &sampled);
            b.extend_from_slice(&run.optical[run.trim..run.t.len() - run.trim]);
            a.extend_from_slice(&filtered[run.trim..run.t.len() - run.trim]);
        }
        (b, a)
    }

    pub fn score(&self, alignment: &Alignment) -> Option<Score> {
        let (b, a) = self.pairs(alignment.shift);
        score(&b, &a, &alignment.n, self.runs.len())
    }

    fn fit_at(&self, shift: f64) -> Option<(Alignment, f64)> {
        let (b, a) = self.pairs(shift);
        let mut m = Matrix3::<f64>::zeros();
        for (b, a) in b.iter().zip(&a) {
            for r in 0..3 {
                for c in 0..3 {
                    m[(r, c)] += b[r] * a[c];
                }
            }
        }
        let svd = m.svd(true, true);
        // All three axes must be observable when reflections are allowed.
        if svd.singular_values[0] <= f64::EPSILON
            || svd.singular_values[2] < 1e-6 * svd.singular_values[0]
        {
            return None;
        }
        let matrix = svd.u? * svd.v_t?;
        let n = std::array::from_fn(|r| std::array::from_fn(|c| matrix[(r, c)]));
        let s = score(&b, &a, &n, self.runs.len())?;
        Some((Alignment { shift, n, r2: s.r2 }, s.rms_deg_s))
    }

    pub fn fit(&self) -> Option<Alignment> {
        let mut best: Option<(Alignment, f64)> = None;
        for k in -30..=30 {
            if let Some(candidate) = self.fit_at(k as f64 * 0.002) {
                if best.as_ref().is_none_or(|(_, v)| candidate.1 < *v) {
                    best = Some(candidate);
                }
            }
        }
        let coarse = best.as_ref()?.0.shift;
        for k in -10..=10 {
            let shift = coarse + k as f64 * 0.0002;
            if shift.abs() > 0.0600001 {
                continue;
            }
            if let Some(candidate) = self.fit_at(shift) {
                if candidate.1 < best.as_ref()?.1 {
                    best = Some(candidate);
                }
            }
        }
        Some(best?.0)
    }
}

fn score(b: &[[f64; 3]], a: &[[f64; 3]], n: &[[f64; 3]; 3], runs: usize) -> Option<Score> {
    if a.len() < 30 {
        return None;
    }
    let mean: [f64; 3] =
        std::array::from_fn(|k| a.iter().map(|r| r[k]).sum::<f64>() / a.len() as f64);
    let mut residual = 0.0;
    let mut variance = 0.0;
    for (b, a) in b.iter().zip(a) {
        for c in 0..3 {
            let p: f64 = (0..3).map(|r| b[r] * n[r][c]).sum();
            residual += (p - a[c]).powi(2);
            variance += (a[c] - mean[c]).powi(2);
        }
    }
    if !residual.is_finite() || !variance.is_finite() || variance <= f64::EPSILON {
        return None;
    }
    Some(Score {
        rms_deg_s: (residual / a.len() as f64).sqrt(),
        r2: 1.0 - residual / variance,
        samples: a.len(),
        runs,
    })
}
