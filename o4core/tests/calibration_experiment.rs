use o4core::{
    alignment::CalibrationData,
    optical::{fit_video_alignment, OpticalRates},
};

fn motion(t: f64) -> [f64; 3] {
    [
        50.0 * (2.3 * t).sin() + 18.0 * (13.1 * t).cos(),
        35.0 * (3.7 * t).cos() + 13.0 * (9.4 * t).sin(),
        40.0 * (1.3 * t).sin() + 22.0 * (17.3 * t).cos(),
    ]
}

#[test]
fn reflected_motion_with_gaps_and_different_fps() {
    let shift = 0.017;
    let matrix = nalgebra::Rotation3::from_euler_angles(0.2, -0.4, 0.7).into_inner()
        * nalgebra::Matrix3::from_diagonal(&nalgebra::Vector3::new(-1.0, 1.0, 1.0));
    let tm: Vec<f64> = (0..25000).map(|i| i as f64 / 1000.0).collect();
    let gyro: Vec<_> = tm.iter().map(|&t| motion(t)).collect();
    for fps in [50.0, 60.0, 100.0, 120.0] {
        let mut opt = OpticalRates {
            t: vec![],
            omega: vec![],
            quality: vec![],
        };
        // Intentionally out-of-order intervals. A 0.4 s quality gap must not be bridged.
        for start in [18.0, 2.0, 10.0] {
            for i in 0..(4.0 * fps) as usize {
                let t = start + i as f64 / fps;
                let g = motion(t + shift);
                opt.t.push(t);
                opt.omega.push(std::array::from_fn(|r| {
                    (0..3)
                        .map(|c| g[c] * matrix[(r, c)])
                        .sum::<f64>()
                        .to_radians()
                }));
                opt.quality
                    .push(if t > 11.2 && t < 11.6 { 0.0 } else { 1.0 });
            }
        }
        let fit = CalibrationData::prepare(&opt, &tm, &gyro, 1000.0)
            .unwrap()
            .fit()
            .unwrap();
        let error = (0..3)
            .flat_map(|r| (0..3).map(move |c| (r, c)))
            .map(|(r, c)| (fit.n[r][c] - matrix[(r, c)]).powi(2))
            .sum::<f64>()
            .sqrt();
        let old = fit_video_alignment(&opt, &tm, &gyro, 1000.0).unwrap();
        println!(
            "fps={fps}: old shift={} candidate={} matrix error={error}",
            old.shift, fit.shift
        );
        assert!((fit.shift - shift).abs() < 0.002);
        assert!(error < 0.01);
        // Reverse every observation: sorting must recover exactly the same fit.
        opt.t.reverse();
        opt.omega.reverse();
        opt.quality.reverse();
        let reverse = CalibrationData::prepare(&opt, &tm, &gyro, 1000.0)
            .unwrap()
            .fit()
            .unwrap();
        assert_eq!(fit.shift, reverse.shift);
        assert_eq!(fit.n, reverse.n);
    }
}

#[test]
fn stationary_or_unobservable_motion_is_not_a_calibration() {
    let tm: Vec<f64> = (0..4000).map(|i| i as f64 / 1000.0).collect();
    for single_axis in [false, true] {
        let gyro: Vec<_> = tm
            .iter()
            .map(|&t| {
                if single_axis {
                    [t.sin(), 0.0, 0.0]
                } else {
                    [0.0; 3]
                }
            })
            .collect();
        let t: Vec<f64> = (100..300).map(|i| i as f64 / 100.0).collect();
        let opt = OpticalRates {
            omega: t
                .iter()
                .map(|t| {
                    if single_axis {
                        [t.sin().to_radians(), 0.0, 0.0]
                    } else {
                        [0.0; 3]
                    }
                })
                .collect(),
            quality: vec![1.0; t.len()],
            t,
        };
        assert!(CalibrationData::prepare(&opt, &tm, &gyro, 1000.0)
            .unwrap()
            .fit()
            .is_none());
    }
}
