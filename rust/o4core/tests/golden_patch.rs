#[path = "golden_telemetry.rs"]
mod gt;
use ndarray::Array2;
use std::sync::atomic::AtomicBool;

#[test]
#[ignore]
fn patched_rates_match_python() {
    let video = gt::repo("sample_vids/DJI_20260711124046_0021_D.MP4");
    let tel = o4core::telemetry::extract_quats(&video).unwrap();
    let cfg = o4core::config::Config::default();
    let fs = o4core::pipeline::fs(&tel.t);
    let (tm, omega) = o4core::quat::quats_to_rates(&tel.t, &tel.q);
    let (cleaned, diag) = o4core::detect::adaptive_clean(&omega, fs, &cfg);
    let patched = o4core::patch::optical_patch(
        &video,
        &tm,
        &cleaned,
        &diag,
        fs,
        &cfg,
        &tel.meta,
        &|s: &str| println!("{s}"),
        &AtomicBool::new(false),
    )
    .unwrap();
    let mut z = gt::npz("patched.npz");
    let rates_g: Array2<f64> = z.by_name("rates").unwrap();
    assert_eq!(patched.len(), rates_g.nrows());
    for i in 0..patched.len() {
        for k in 0..3 {
            assert!(
                (patched[i][k] - rates_g[[i, k]]).abs() <= 1e-9,
                "patched[{i}][{k}]: {} vs {}",
                patched[i][k],
                rates_g[[i, k]]
            );
        }
    }
}
