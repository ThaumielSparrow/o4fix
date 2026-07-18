#[path = "golden_telemetry.rs"] mod gt;
use ndarray::{Array0, Array1, Array2};
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

#[test]
#[ignore]
fn alignment_fit_matches_python() {
    // rebuild inputs from goldens (independent of Task 11's runtime)
    let mut zc = gt::npz("optical_calib.npz");
    let t: Array1<f64> = zc.by_name("t").unwrap();
    let om: Array2<f64> = zc.by_name("omega").unwrap();
    let q: Array1<f64> = zc.by_name("quality").unwrap();
    let opt = o4core::optical::OpticalRates {
        t: t.to_vec(),
        omega: (0..om.nrows()).map(|i| [om[[i,0]], om[[i,1]], om[[i,2]]]).collect(),
        quality: q.to_vec(),
    };
    let mut zl = gt::npz("clean.npz");
    let tm: Array1<f64> = zl.by_name("tm").unwrap();
    let cleaned: Array2<f64> = zl.by_name("cleaned").unwrap();
    const R2D: f64 = 180.0 / std::f64::consts::PI;
    let gyro_deg: Vec<[f64; 3]> = (0..cleaned.nrows())
        .map(|i| [cleaned[[i,0]]*R2D, cleaned[[i,1]]*R2D, cleaned[[i,2]]*R2D]).collect();
    // python used fs from np.median(diff(t_extract)); recompute identically:
    let te = { let mut z = gt::npz("extract.npz"); let t: Array1<f64> = z.by_name("t").unwrap(); t };
    let mut d: Vec<f64> = te.windows(2).into_iter().map(|w| w[1] - w[0]).collect();
    d.sort_by(f64::total_cmp);
    let fs = 1.0 / ((d[d.len()/2 - 1] + d[d.len()/2]) / 2.0); // np.median, even n
    let al = o4core::optical::fit_video_alignment(&opt, &tm.to_vec(), &gyro_deg, fs).unwrap();
    let mut zf = gt::npz("fit.npz");
    // shift/r2 are bare python scalars in dump_goldens.py (np.savez(shift=shift, r2=r2)),
    // so numpy stores them as 0-d arrays, not length-1 1-d arrays; Array0 + into_scalar()
    // reads them correctly (Array1::by_name on a 0-d array fails with WrongNdim).
    let shift_g: Array0<f64> = zf.by_name("shift").unwrap();
    let n_g: Array2<f64> = zf.by_name("n").unwrap();
    let r2_g: Array0<f64> = zf.by_name("r2").unwrap();
    assert!((al.shift - shift_g.into_scalar()).abs() <= 1e-9, "shift");
    for r in 0..3 { for c in 0..3 {
        assert!((al.n[r][c] - n_g[[r, c]]).abs() <= 1e-9, "N[{r}][{c}]");
    }}
    assert!((al.r2 - r2_g.into_scalar()).abs() <= 1e-9, "r2");
}
