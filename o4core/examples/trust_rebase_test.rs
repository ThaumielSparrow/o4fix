#[path = "support/edge_offset_patch.rs"]
mod accepted;
#[path = "support/trust_rebase_patch.rs"]
mod candidate;
fn main() {}
#[cfg(test)]
mod tests {
    use o4core::quat;
    #[test]
    fn selected_small_drift_removes_bridge_and_unselected_path_is_identical() {
        let t: Vec<_> = (0..8001).map(|i| i as f64 / 1000.).collect();
        let raw: Vec<_> = t
            .iter()
            .map(|&s| {
                quat::qexp([
                    0.,
                    24_f64.to_radians() * ((s - 2.3) / 2.4).clamp(0., 1.),
                    0.,
                ])
            })
            .collect();
        let rates = vec![[0.; 3]; t.len() - 1];
        let intervals = [(2., 5.)];
        let (base, bs) =
            super::accepted::splice_orientation(&t, &raw, &rates, &intervals, 0.19, 30., 0.);
        let (unchanged, _) =
            super::candidate::splice_orientation(&[], &t, &raw, &rates, &intervals, 0.19, 30., 0.);
        assert_eq!(base, unchanged);
        assert!(!bs[0].rebased);
        let (candidate, cs) = super::candidate::splice_orientation(
            &intervals, &t, &raw, &rates, &intervals, 0.19, 30., 0.,
        );
        assert!(cs[0].rebased);
        let (tm, br) = quat::quats_to_rates(&t, &base);
        let (_, cr) = quat::quats_to_rates(&t, &candidate);
        let peak = |r: &Vec<[f64; 3]>| {
            tm.iter()
                .zip(r)
                .filter(|(s, _)| **s > 2.3 && **s < 4.7)
                .map(|(_, r)| r[1].abs().to_degrees())
                .fold(0., f64::max)
        };
        println!("bridge peak {} -> {} deg/s", peak(&br), peak(&cr));
        assert!(peak(&br) > 11.9);
        assert!(peak(&cr) < 1e-8);
    }
}
