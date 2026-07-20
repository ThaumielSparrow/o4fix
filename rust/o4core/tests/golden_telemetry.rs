mod common;
use self::common::{npz, repo};
use ndarray::{Array1, Array2};

#[test]
#[ignore] // needs test clip + goldens
fn extraction_matches_python() {
    let tel = o4core::telemetry::extract_quats(&repo("sample_vids/DJI_20260711124046_0021_D.MP4"))
        .unwrap();
    let mut z = npz("extract.npz");
    let t: Array1<f64> = z.by_name("t").unwrap();
    let q: Array2<f64> = z.by_name("q").unwrap();
    assert_eq!(tel.t.len(), t.len(), "sample count");
    for i in 0..t.len() {
        assert!((tel.t[i] - t[i]).abs() <= 1e-12, "t[{i}]");
        for k in 0..4 {
            assert!((tel.q[i][k] - q[[i, k]]).abs() <= 1e-12, "q[{i}][{k}]");
        }
    }
    assert_eq!(tel.meta.model.as_str(), "O4P");
    assert!(tel.meta.camera_matrix.is_some() || tel.meta.calib_w.is_none());
}
