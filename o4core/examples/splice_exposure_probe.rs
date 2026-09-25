//! Read-only audit of raw-orientation exposure in the production edge ramps.
use o4core::{config::Config, detect, dsp, pipeline, quat, telemetry};
use serde_json::json;

#[cfg(test)]
mod tests {
    use o4core::{patch, quat};
    #[test]
    fn perfect_replacement_still_inherits_raw_error_inside_edge_ramp() {
        let t: Vec<_> = (0..5001).map(|i| i as f64 / 1000.).collect();
        let raw: Vec<_> = t
            .iter()
            .map(|&s| {
                let angle = if (2.2..=2.8).contains(&s) {
                    0.5_f64.to_radians() * (2. * std::f64::consts::PI * 25. * (s - 2.2)).sin()
                } else {
                    0.
                };
                quat::qexp([0., 0., angle])
            })
            .collect();
        let (out, _) = patch::splice_orientation(
            &t,
            &raw,
            &vec![[0.; 3]; t.len() - 1],
            &[(2., 3.)],
            0.3,
            30.,
            1.5,
        );
        let (tm, rates) = quat::quats_to_rates(&t, &out);
        let peak = |a: f64, b: f64| {
            tm.iter()
                .zip(&rates)
                .filter(|(s, _)| **s > a && **s < b)
                .map(|(_, r)| r[2].abs().to_degrees())
                .fold(0., f64::max)
        };
        assert!(
            peak(2.2, 2.3) > 1.,
            "raw corruption survives despite perfect replacement rates"
        );
        assert!(
            peak(2.31, 2.69) < 1e-8,
            "fully replaced interior remains still"
        );
        let (candidate, _) = patch::splice_orientation(
            &t,
            &raw,
            &vec![[0.; 3]; t.len() - 1],
            &[(2., 3.)],
            0.19,
            30.,
            1.5,
        );
        let (_, candidate_rates) = quat::quats_to_rates(&t, &candidate);
        assert!(
            candidate_rates.iter().flatten().all(|v| v.abs() < 1e-8),
            "shortened ramp excludes this corruption without introducing boundary motion"
        );
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 3 {
        return Err("usage: splice_exposure_probe SOURCE OUTPUT_JSON".into());
    }
    let tel = telemetry::extract_quats(std::path::Path::new(&a[1]))?;
    let cfg = Config::default();
    let fs = pipeline::fs(&tel.t);
    let (tm, raw) = quat::quats_to_rates(&tel.t, &tel.q);
    let (_, diag) = detect::adaptive_clean(&raw, fs, &cfg);
    let intervals = detect::find_intervals(
        &diag
            .noise
            .iter()
            .map(|&n| n > cfg.severe)
            .collect::<Vec<_>>(),
        &tm,
        cfg.severe_pad,
        cfg.severe_merge,
        0.2,
    );
    let mut rows = vec![];
    for &(a, b) in &intervals {
        if ![(102., 110.), (221., 229.), (245., 253.), (304., 312.)]
            .iter()
            .any(|&(s, e)| b >= s && a <= e)
        {
            continue;
        }
        // Same quaternion-index selection and ramp formula as splice_orientation.
        let i0 = dsp::searchsorted_left(&tel.t, a);
        let i1 = dsp::searchsorted_right(&tel.t, b)
            .saturating_sub(1)
            .min(tel.t.len() - 1);
        let mut exposed = vec![];
        let mut max_partner: f64 = 0.;
        for i in i0..i1 {
            let r = quat::smoothstep((tel.t[i] - tel.t[i0]) / cfg.ramp)
                .min(quat::smoothstep((tel.t[i1] - tel.t[i]) / cfg.ramp));
            if diag.noise[i] > cfg.severe {
                max_partner = max_partner.max(1. - (diag.alpha[i] / 0.35).clamp(0., 1.));
                if r < 1. - 1e-12 {
                    exposed.push((tel.t[i], 1. - r, diag.noise[i]));
                }
            }
        }
        rows.push(json!({"interval":[a,b],"snapped_interval":[tel.t[i0],tel.t[i1]],"detected_noisy_samples_in_raw_orientation_ramp":exposed.len(),"max_raw_orientation_slerp_weight_when_noisy":exposed.iter().map(|v|v.1).fold(0.,f64::max),"max_noise_in_exposed_samples":exposed.iter().map(|v|v.2).fold(0.,f64::max),"max_light_rate_partner_weight_when_noisy":max_partner,"exposed_samples_t_weight_noise":exposed}));
    }
    std::fs::write(
        &a[2],
        serde_json::to_vec_pretty(
            &json!({"source":a[1],"sample_rate":fs,"note":"Exact production interval and orientation-ramp formulas on original telemetry. Slerp weight is not an angular-rate contribution or proof of visible error. Rate partner weight excludes fast handback. No repair or render generated.","bursts":rows}),
        )?,
    )?;
    Ok(())
}
