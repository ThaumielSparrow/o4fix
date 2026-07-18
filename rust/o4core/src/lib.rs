pub mod config;
pub mod error;
pub mod detect;
pub mod dsp;
pub mod mp4; // added in Task 9
pub mod patch; // added in Task 13
// pub mod pipeline; // added in Task 15
pub mod quat; // added in Task 4
pub mod telemetry; // added in Task 7
pub mod optical; // added in Task 11 (requires local OpenCV install)

/// np.median(np.diff(t)) reciprocal — sampling rate estimate.
// replaced by pipeline::fs in Task 15
pub fn pipeline_fs_placeholder(t: &[f64]) -> f64 {
    let mut d: Vec<f64> = t.windows(2).map(|w| w[1] - w[0]).collect();
    d.sort_by(f64::total_cmp);
    let n = d.len();
    let med = if n % 2 == 0 { (d[n / 2 - 1] + d[n / 2]) / 2.0 } else { d[n / 2] };
    1.0 / med
}
