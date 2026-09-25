//! Research-only suppression of the light-gyro partner; fast handback is retained.
pub fn suppress_partner(
    original: [f64; 3],
    light: [f64; 3],
    optical: [f64; 3],
    medium: [f64; 3],
    optical_weight: f64,
    handback: f64,
    gate: f64,
) -> [f64; 3] {
    let amount = gate * (1. - optical_weight);
    if amount == 0. {
        return original;
    }
    std::array::from_fn(|k| {
        original[k] + amount * ((1. - handback) * optical[k] + handback * medium[k] - light[k])
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use o4core::{patch::OpticalPatch, quat};

    #[test]
    fn alpha_dip_leakage_removed_without_losing_fast_multiaxis_motion() {
        let mut old_angle = [0.; 3];
        let mut new_angle = [0.; 3];
        let mut true_angle = [0.; 3];
        for i in 0..1000 {
            let t = i as f64 / 1000.;
            let truth = [250. + 80. * (8. * t).sin(), -120. * t, 40. * (5. * t).cos()];
            let light = [truth[0] + 30., truth[1] - 20., truth[2] + 10.];
            let w = if (0.3..0.7).contains(&t) { 0. } else { 1. };
            let old = std::array::from_fn(|k| (1. - w) * light[k] + w * truth[k]);
            let new = suppress_partner(old, light, truth, truth, w, 0., 1.);
            for k in 0..3 {
                old_angle[k] += old[k] / 1000.;
                new_angle[k] += new[k] / 1000.;
                true_angle[k] += truth[k] / 1000.;
                assert!((new[k] - truth[k]).abs() < 1e-12);
            }
        }
        assert!((old_angle[0] - true_angle[0]).abs() > 10.);
        for k in 0..3 {
            assert!((new_angle[k] - true_angle[k]).abs() < 1e-10);
        }
    }

    #[test]
    fn preserves_handback_unselected_rates_and_continuous_gate() {
        let light = [20., 30., 40.];
        let optical = [100., 200., 300.];
        let medium = [500., -300., 250.];
        for hb in [0., 0.4, 1.] {
            let burst = std::array::from_fn(|k| (1. - hb) * optical[k] + hb * medium[k]);
            let old = std::array::from_fn(|k| 0.7 * light[k] + 0.3 * burst[k]);
            assert_eq!(
                suppress_partner(old, light, optical, medium, 0.3, hb, 0.),
                old
            );
            assert_eq!(
                suppress_partner(burst, light, optical, medium, 1., hb, 1.),
                burst
            );
            let new = suppress_partner(old, light, optical, medium, 0.3, hb, 1.);
            for k in 0..3 {
                assert!((new[k] - burst[k]).abs() < 1e-12);
            }
            let near =
                suppress_partner(old, light, optical, medium, 0.3, hb, quat::smoothstep(1e-6));
            for k in 0..3 {
                assert!((near[k] - old[k]).abs() < 1e-8);
            }
        }
    }

    #[test]
    fn missing_coverage_still_refuses_repair() {
        let t: Vec<_> = (0..1001).map(|i| i as f64 / 1000.).collect();
        let mut p = OpticalPatch {
            rates: vec![[0.; 3]; 1000],
            supported: vec![true; 1000],
        };
        assert!(p.require_coverage(&t, &[(0.1, 0.9)]).is_ok());
        p.supported[500] = false;
        assert!(p.require_coverage(&t, &[(0.1, 0.9)]).is_err());
    }
}
