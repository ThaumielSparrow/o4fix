mod common;
use self::common as gt;
use ndarray::Array2;

#[test]
#[ignore] // needs test clip + goldens
fn splice_matches_python() {
    let (t, q) = gt::npz_extract(); // helper added below: (Vec<f64>, Vec<[f64;4]>) from extract.npz
    let mut zp = gt::npz("patched.npz");
    let rates: Array2<f64> = zp.by_name("rates").unwrap();
    let omega: Vec<[f64; 3]> = (0..rates.nrows())
        .map(|i| [rates[[i, 0]], rates[[i, 1]], rates[[i, 2]]])
        .collect();
    let mut zi = gt::npz("intervals.npz");
    let sev: Array2<f64> = zi.by_name("severe").unwrap();
    let intervals: Vec<(f64, f64)> = (0..sev.nrows())
        .map(|i| (sev[[i, 0]], sev[[i, 1]]))
        .collect();

    let ramp = o4core::config::Config::default().ramp;
    let (q_out, stats) = o4core::patch::splice_orientation(&t, &q, &omega, &intervals, ramp);

    let mut zs = gt::npz("splice.npz");
    let qg: Array2<f64> = zs.by_name("q_out").unwrap();
    let drifts: Array2<f64> = zs.by_name("drifts").unwrap(); // rows (a, b, drift_deg)
    assert_eq!(q_out.len(), qg.nrows());
    for i in 0..q_out.len() {
        let d = (0..4)
            .map(|k| (q_out[i][k] - qg[[i, k]]).abs())
            .fold(0.0, f64::max);
        let dn = (0..4)
            .map(|k| (q_out[i][k] + qg[[i, k]]).abs())
            .fold(0.0, f64::max);
        assert!(d.min(dn) <= 1e-9, "q_out[{i}]: folded diff {}", d.min(dn));
    }
    assert_eq!(stats.len(), drifts.nrows());
    for (i, s) in stats.iter().enumerate() {
        assert!((s.start - drifts[[i, 0]]).abs() <= 1e-9);
        assert!((s.end - drifts[[i, 1]]).abs() <= 1e-9);
        assert!(
            (s.drift_deg - drifts[[i, 2]]).abs() <= 1e-6,
            "drift[{i}]: {} vs {}",
            s.drift_deg,
            drifts[[i, 2]]
        );
    }
}
