#[path = "golden_telemetry.rs"] mod gt;
use ndarray::{Array1, Array2};
use std::sync::atomic::AtomicBool;

#[test]
#[ignore]
fn seeded_optical_rates_match_python_on_calib_intervals() {
    let video = gt::repo("sample_vids/DJI_20260711124046_0021_D.MP4");
    let tel = o4core::telemetry::extract_quats(&video).unwrap();
    let mut zi = gt::npz("intervals.npz");
    let calib_g: Array2<f64> = zi.by_name("calib").unwrap();
    let calib: Vec<(f64, f64)> = (0..calib_g.nrows())
        .map(|i| (calib_g[[i, 0]], calib_g[[i, 1]])).collect();
    let opt = o4core::optical::video_rates(
        &video, &calib, &tel.meta, &AtomicBool::new(false), &|_, _| ()).unwrap();
    let mut z = gt::npz("optical_calib.npz");
    let t_g: Array1<f64> = z.by_name("t").unwrap();
    let om_g: Array2<f64> = z.by_name("omega").unwrap();
    let q_g: Array1<f64> = z.by_name("quality").unwrap();
    assert_eq!(opt.t.len(), t_g.len(), "sample count");
    for i in 0..opt.t.len() {
        assert!((opt.t[i] - t_g[i]).abs() <= 1e-9, "t[{i}]");
        assert!((opt.quality[i] - q_g[i]).abs() <= 1e-9, "quality[{i}]");
        for k in 0..3 {
            assert!((opt.omega[i][k] - om_g[[i, k]]).abs() <= 1e-9,
                    "omega[{i}][{k}]: {} vs {}", opt.omega[i][k], om_g[[i, k]]);
        }
    }
}
