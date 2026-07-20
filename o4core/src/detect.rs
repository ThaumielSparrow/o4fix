//! adaptive_clean (o4fix.py:209-241) + find_intervals (o4fix.py:253-270).
use crate::config::Config;
use crate::dsp;

pub struct CleanDiag {
    pub alpha: Vec<f64>,
    pub noise: Vec<f64>,
    pub light: Vec<[f64; 3]>,  // deg/s
    pub strong: Vec<[f64; 3]>, // deg/s
    pub spike_frac: f64,
}

const R2D: f64 = 180.0 / std::f64::consts::PI;
const D2R: f64 = std::f64::consts::PI / 180.0;

pub fn adaptive_clean(omega: &[[f64; 3]], fs: f64, cfg: &Config) -> (Vec<[f64; 3]>, CleanDiag) {
    let deg: Vec<[f64; 3]> = omega
        .iter()
        .map(|r| [r[0] * R2D, r[1] * R2D, r[2] * R2D])
        .collect();
    let (x, spike_frac) = dsp::hampel(&deg, cfg.hampel_window, cfg.hampel_sigma);

    // 30-180 Hz band-RMS noise estimate, max across axes (o4fix.py:222-229)
    let ba = dsp::butter_band(
        2,
        cfg.noise_band.0 / (fs / 2.0),
        (cfg.noise_band.1).min(0.95 * fs / 2.0) / (fs / 2.0),
    );
    let hf = dsp::filtfilt3(&ba, &x);
    let win = ((cfg.noise_window_ms * fs / 1000.0).round() as usize).max(3);
    let mut noise = vec![0.0f64; x.len()];
    for ax in 0..3 {
        let sq: Vec<f64> = hf.iter().map(|r| r[ax] * r[ax]).collect();
        let sm = dsp::uniform_filter1d(&sq, win);
        for i in 0..noise.len() {
            noise[i] = noise[i].max(sm[i].sqrt());
        }
    }

    let mut alpha: Vec<f64> = noise
        .iter()
        .map(|&n| ((n - cfg.noise_low) / (cfg.noise_high - cfg.noise_low)).clamp(0.0, 1.0))
        .collect();
    alpha = dsp::uniform_filter1d(&alpha, ((0.2 * fs) as usize).max(3));

    let light = dsp::filtfilt3(&dsp::butter_low(2, cfg.light_cutoff / (fs / 2.0)), &x);
    let strong = dsp::filtfilt3(&dsp::butter_low(2, cfg.strong_cutoff / (fs / 2.0)), &x);
    let out: Vec<[f64; 3]> = (0..x.len())
        .map(|i| {
            core::array::from_fn(|k| {
                (light[i][k] * (1.0 - alpha[i]) + strong[i][k] * alpha[i]) * D2R
            })
        })
        .collect();
    (
        out,
        CleanDiag {
            alpha,
            noise,
            light,
            strong,
            spike_frac,
        },
    )
}

/// Time intervals where mask is true, padded/merged/pruned (o4fix.py:253-270).
pub fn find_intervals(
    mask: &[bool],
    t: &[f64],
    pad_s: f64,
    merge_s: f64,
    min_s: f64,
) -> Vec<(f64, f64)> {
    let n = mask.len();
    let mut starts = Vec::new();
    let mut ends = Vec::new();
    if mask[0] {
        starts.push(0usize);
    }
    for i in 1..n {
        if mask[i] && !mask[i - 1] {
            starts.push(i);
        }
        if !mask[i] && mask[i - 1] {
            ends.push(i);
        }
    }
    if mask[n - 1] {
        ends.push(n);
    }
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for (s, e) in starts.iter().zip(&ends) {
        let a = t[*s] - pad_s;
        let b = t[(*e).min(n - 1)] + pad_s;
        if let Some(last) = merged.last_mut() {
            if a - last.1 < merge_s {
                last.1 = b;
                continue;
            }
        }
        merged.push((a, b));
    }
    merged.into_iter().filter(|(a, b)| b - a >= min_s).collect()
}
