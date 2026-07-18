#[path = "golden_telemetry.rs"] mod gt; // reuse repo()/npz() helpers via pub fns
use ndarray::{Array1, Array2};
use o4core::{config::Config, detect, quat};

#[test]
#[ignore]
fn clean_stage_matches_python() {
    let tel = o4core::telemetry::extract_quats(
        &gt::repo("sample_vids/DJI_20260711124046_0021_D.MP4")).unwrap();
    let mut z = gt::npz("clean.npz");
    let tm_g: Array1<f64> = z.by_name("tm").unwrap();
    let omega_g: Array2<f64> = z.by_name("omega").unwrap();
    let cleaned_g: Array2<f64> = z.by_name("cleaned").unwrap();
    let alpha_g: Array1<f64> = z.by_name("alpha").unwrap();
    let noise_g: Array1<f64> = z.by_name("noise").unwrap();
    let light_g: Array2<f64> = z.by_name("light").unwrap();
    let strong_g: Array2<f64> = z.by_name("strong").unwrap();

    let fs = {
        let mut d: Vec<f64> = tel.t.windows(2).map(|w| w[1] - w[0]).collect();
        d.sort_by(f64::total_cmp);
        1.0 / ((d[d.len()/2 - 1] + d[d.len()/2]) / 2.0)  // np.median, even n
    };
    let (tm, omega) = quat::quats_to_rates(&tel.t, &tel.q);
    let cfg = Config::default();
    let (cleaned, diag) = detect::adaptive_clean(&omega, fs, &cfg);

    assert_eq!(tm.len(), tm_g.len());
    for i in 0..tm.len() {
        assert!((tm[i] - tm_g[i]).abs() <= 1e-12);
        for k in 0..3 {
            assert!((omega[i][k] - omega_g[[i, k]]).abs() <= 1e-11, "omega[{i}][{k}]");
            assert!((cleaned[i][k] - cleaned_g[[i, k]]).abs() <= 1e-9, "cleaned[{i}][{k}]");
            assert!((diag.light[i][k] - light_g[[i, k]]).abs() <= 1e-9);
            assert!((diag.strong[i][k] - strong_g[[i, k]]).abs() <= 1e-9);
        }
        assert!((diag.alpha[i] - alpha_g[i]).abs() <= 1e-9, "alpha[{i}]");
        assert!((diag.noise[i] - noise_g[i]).abs() <= 1e-9, "noise[{i}]");
    }

    // intervals: exact same values
    let mut zi = gt::npz("intervals.npz");
    let noisy_g: Array2<f64> = zi.by_name("noisy").unwrap();
    let severe_g: Array2<f64> = zi.by_name("severe").unwrap();
    let noisy = detect::find_intervals(
        &diag.alpha.iter().map(|&a| a > 0.15).collect::<Vec<_>>(),
        &tm, cfg.patch_pad, cfg.patch_merge, 0.2);
    let severe = detect::find_intervals(
        &diag.noise.iter().map(|&n| n > cfg.severe).collect::<Vec<_>>(),
        &tm, cfg.severe_pad, cfg.severe_merge, 0.2);
    assert_eq!(noisy.len(), noisy_g.nrows());
    for (i, (a, b)) in noisy.iter().enumerate() {
        assert!((a - noisy_g[[i, 0]]).abs() <= 1e-12 && (b - noisy_g[[i, 1]]).abs() <= 1e-12);
    }
    assert_eq!(severe.len(), severe_g.nrows());
    for (i, (a, b)) in severe.iter().enumerate() {
        assert!((a - severe_g[[i, 0]]).abs() <= 1e-12 && (b - severe_g[[i, 1]]).abs() <= 1e-12);
    }
}
