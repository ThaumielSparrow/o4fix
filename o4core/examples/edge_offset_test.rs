#[path = "support/edge_offset_patch.rs"]
mod edge;
fn main() {}
#[cfg(test)]
mod tests {
    use o4core::{patch, quat};
    #[test]
    fn preserves_known_multi_axis_motion_and_non_rebased_path() {
        let t: Vec<_> = (0..5001).map(|i| i as f64 / 1000.).collect();
        let mut truth = vec![[1., 0., 0., 0.]];
        for i in 0..t.len() - 1 {
            truth.push(quat::qmul(
                truth[i],
                quat::qexp([0.0002 * (t[i] * 2.).sin(), 0.0003, 0.0001]),
            ));
        }
        let (_, rates) = quat::quats_to_rates(&t, &truth);
        for drift in [3_f64, 60.] {
            let raw: Vec<_> = t
                .iter()
                .enumerate()
                .map(|(i, &s)| {
                    quat::qmul(
                        quat::qexp([drift.to_radians() * ((s - 2.3) / 0.4).clamp(0., 1.), 0., 0.]),
                        truth[i],
                    )
                })
                .collect();
            let (base, bs) =
                patch::splice_orientation(&t, &raw, &rates, &[(2., 3.)], 0.19, 30., 0.);
            let (candidate, cs) =
                super::edge::splice_orientation(&t, &raw, &rates, &[(2., 3.)], 0.19, 30., 0.);
            assert_eq!(bs[0].rebased, cs[0].rebased);
            assert_eq!(bs[0].drift_deg, cs[0].drift_deg);
            if !cs[0].rebased {
                assert_eq!(base, candidate);
            } else {
                let error = candidate
                    .iter()
                    .zip(&truth)
                    .map(|(a, b)| {
                        quat::qlog(quat::qmul(quat::qconj(*a), *b))
                            .iter()
                            .map(|v| v * v)
                            .sum::<f64>()
                            .sqrt()
                    })
                    .fold(0., f64::max);
                assert!(error < 1e-9, "known moving truth error {error}");
            }
        }
    }
    #[test]
    fn stationary_camera_with_middle_drift_should_not_move_in_healthy_edges() {
        let t: Vec<_> = (0..5001).map(|i| i as f64 / 1000.).collect();
        let q: Vec<_> = t
            .iter()
            .map(|&t| {
                quat::qexp([
                    0.,
                    0.,
                    60_f64.to_radians() * ((t - 2.3) / 0.4).clamp(0., 1.),
                ])
            })
            .collect();
        let rates = vec![[0.; 3]; t.len() - 1];
        let (base, _) = patch::splice_orientation(&t, &q, &rates, &[(2., 3.)], 0.19, 30., 0.);
        let (candidate, _) =
            super::edge::splice_orientation(&t, &q, &rates, &[(2., 3.)], 0.19, 30., 0.);
        let (_, br) = quat::quats_to_rates(&t, &base);
        let (_, cr) = quat::quats_to_rates(&t, &candidate);
        let peak = |r: &Vec<[f64; 3]>| r.iter().map(|v| v[2].abs().to_degrees()).fold(0., f64::max);
        println!(
            "stationary synthetic peak rate: base={} fixed_edge={}",
            peak(&br),
            peak(&cr)
        );
        assert!(peak(&br) > 1.);
        assert!(peak(&cr) < 1e-8);
    }
}
