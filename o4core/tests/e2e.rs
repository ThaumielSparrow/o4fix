mod common;
use self::common as gt;
use o4core::config::Config;
use o4core::pipeline::{process, Outcome, Progress};
use std::sync::atomic::AtomicBool;

/// Settings the committed Python goldens were produced with.
fn legacy(c: Config) -> Config {
    Config { ramp: 0.3, refine: false, ..c }
}

fn run(cfg: &Config, out: &std::path::Path) -> Result<Outcome, o4core::error::O4Error> {
    let video = gt::repo("sample_vids/DJI_20260711124046_0021_D.MP4");
    process(
        &video,
        Some(out),
        cfg,
        &|p: Progress| {
            if !p.message.is_empty() {
                println!("{}", p.message)
            }
        },
        &AtomicBool::new(false),
    )
}

#[test]
#[ignore]
fn healthy_clip_short_circuits() {
    let out = std::env::temp_dir().join("o4fix_healthy_test.MP4");
    let _ = std::fs::remove_file(&out);
    let cfg = Config {
        severe: f64::MAX,
        ..Config::default()
    };
    let r = run(&cfg, &out).unwrap();
    assert!(matches!(r, Outcome::Healthy));
    assert!(!out.exists(), "healthy path must not write an output file");
}

#[test]
#[ignore]
fn no_calibration_sections_is_actionable_error() {
    let out = std::env::temp_dir().join("o4fix_nocalib_test.MP4");
    let _ = std::fs::remove_file(&out);
    // noise_low/high < 0 forces alpha == 1 everywhere -> no alpha<0.02 calib windows
    let cfg = Config {
        noise_low: -2.0,
        noise_high: -1.0,
        ..Config::default()
    };
    let e = run(&cfg, &out).unwrap_err();
    assert!(matches!(
        e,
        o4core::error::O4Error::CalibrationFailed { .. }
    ));
    assert!(!out.exists());
}

#[test]
#[ignore] // ~10 min: full pipeline incl. optical
fn e2e_matches_seeded_python_reference() {
    let out = std::env::temp_dir().join("o4fix_e2e_test.MP4");
    let _ = std::fs::remove_file(&out);
    let r = run(&legacy(Config::default()), &out).unwrap();
    let Outcome::Repaired { bursts, .. } = r else {
        panic!("expected Repaired")
    };
    assert!(!bursts.is_empty());

    // compare against the SEEDED python reference written by dump_goldens.py
    let (t_r, q_r) = stream(&out);
    let (t_p, q_p) = stream(&gt::repo("goldens/ref_fixed.MP4"));
    assert_eq!(t_r, t_p, "timestamps must be identical");
    let mut zi = gt::npz("intervals.npz");
    let sev: ndarray::Array2<f64> = zi.by_name("severe").unwrap();
    for i in 0..q_r.len() {
        let d = (0..4)
            .map(|k| (q_r[i][k] - q_p[i][k]).abs())
            .fold(0.0, f64::max);
        let dn = (0..4)
            .map(|k| (q_r[i][k] + q_p[i][k]).abs())
            .fold(0.0, f64::max);
        let t_s = t_r[i] / 1000.0;
        let inside = (0..sev.nrows()).any(|j| t_s >= sev[[j, 0]] && t_s <= sev[[j, 1]]);
        if inside {
            assert!(d.min(dn) <= 1e-6, "repaired sample {i}: {}", d.min(dn));
        } else {
            assert_eq!(d.min(dn), 0.0, "clean-zone sample {i} must be bit-exact");
        }
    }
    std::fs::remove_file(&out).ok();
}

fn stream(p: &std::path::Path) -> (Vec<f64>, Vec<[f64; 4]>) {
    o4core::telemetry::flat_quat_stream(p).unwrap()
}

#[test]
#[ignore] // ~12 min
fn refine_off_is_bit_identical_to_splice() {
    // with refinement off, output equals the splice-only pipeline exactly;
    // with it on, only in-burst windows change and the file still verifies
    let off = std::env::temp_dir().join("o4fix_refine_off.MP4");
    let on = std::env::temp_dir().join("o4fix_refine_on.MP4");
    for p in [&off, &on] {
        let _ = std::fs::remove_file(p);
    }
    run(&Config { refine: false, ..Config::default() }, &off).unwrap();
    let r = run(&Config::default(), &on).unwrap();
    assert!(matches!(r, Outcome::Repaired { .. }));
    let (t0, q0) = stream(&off);
    let (t1, q1) = stream(&on);
    assert_eq!(t0, t1);
    let mut zi = gt::npz("intervals.npz");
    let sev: ndarray::Array2<f64> = zi.by_name("severe").unwrap();
    let first = sev[[0, 0]] - 1.5; // before the first refinement window
    let n_before = t0.iter().filter(|&&x| x / 1000.0 < first).count();
    assert_eq!(&q0[..n_before], &q1[..n_before], "samples before the first gate must be identical");
    assert!(q0 != q1, "refinement changed nothing on 0021");
    std::fs::remove_file(&off).ok();
    std::fs::remove_file(&on).ok();
}

#[test]
#[ignore] // ~10 min: full pipeline incl. optical, M4 profile
fn e2e_m4_matches_seeded_python_reference() {
    let out = std::env::temp_dir().join("o4fix_e2e_m4_test.MP4");
    let _ = std::fs::remove_file(&out);
    let r = run(&legacy(Config::m4()), &out).unwrap();
    let Outcome::Repaired { bursts, .. } = r else {
        panic!("expected Repaired")
    };
    assert!(!bursts.is_empty());
    let (t_r, q_r) = stream(&out);
    let (t_p, q_p) = stream(&gt::repo("goldens/ref_fixed_m4.MP4"));
    assert_eq!(t_r, t_p, "timestamps must be identical");
    let mut zi = gt::npz("intervals.npz");
    let sev: ndarray::Array2<f64> = zi.by_name("severe").unwrap();
    for i in 0..q_r.len() {
        let d = (0..4)
            .map(|k| (q_r[i][k] - q_p[i][k]).abs())
            .fold(0.0, f64::max);
        let dn = (0..4)
            .map(|k| (q_r[i][k] + q_p[i][k]).abs())
            .fold(0.0, f64::max);
        let t_s = t_r[i] / 1000.0;
        let inside = (0..sev.nrows()).any(|j| t_s >= sev[[j, 0]] && t_s <= sev[[j, 1]]);
        if inside {
            assert!(d.min(dn) <= 1e-6, "repaired sample {i}: {}", d.min(dn));
        } else {
            assert_eq!(d.min(dn), 0.0, "clean-zone sample {i} must be bit-exact");
        }
    }
    std::fs::remove_file(&out).ok();
}
