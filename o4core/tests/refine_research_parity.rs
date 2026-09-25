//! Production refine() vs research wp1c (feedback-v1). Needs local artifacts under target/experiments.
mod common;
use o4core::refine::{refine, RefineConfig};
use o4core::telemetry::extract_quats;
use std::sync::atomic::AtomicBool;

fn bursts(stages: &str) -> Vec<(f64, f64)> {
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(common::repo(stages)).unwrap()).unwrap();
    v["bursts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            (
                b["interval"][0].as_f64().unwrap(),
                b["interval"][1].as_f64().unwrap(),
            )
        })
        .collect()
}

fn research_angle(adjust: &str, t: &[f64]) -> Vec<[f64; 3]> {
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(common::repo(adjust)).unwrap()).unwrap();
    let ta: Vec<f64> = serde_json::from_value(v["t"].clone()).unwrap();
    let aa: Vec<[f64; 3]> = serde_json::from_value(v["angle"].clone()).unwrap();
    t.iter()
        .map(|&x| {
            if x < ta[0] {
                return [0.0; 3];
            }
            let i = ta.partition_point(|&y| y <= x).clamp(1, ta.len() - 1);
            let f = ((x - ta[i - 1]) / (ta[i] - ta[i - 1])).clamp(0.0, 1.0);
            std::array::from_fn(|k| aa[i - 1][k] + f * (aa[i][k] - aa[i - 1][k]))
        })
        .collect()
}

#[allow(clippy::needless_range_loop)] // parallel indexing of angle_t, angle and research
fn case(base_mp4: &str, stages: &str, adjust: &str) {
    let tel = extract_quats(&common::repo(base_mp4)).unwrap();
    let iv = bursts(stages);
    let r = refine(
        &common::repo(base_mp4),
        &tel.t,
        &tel.q,
        &iv,
        &tel.meta,
        &RefineConfig::default(),
        &|s| println!("{s}"),
        &AtomicBool::new(false),
    )
    .unwrap();
    let st = r.stats.as_ref().unwrap();
    println!(
        "{base_mp4}: floor {:.2} deg/s, motion ratio {:?}, pairs {}, skipped {:?}",
        st.hp_rms_deg, st.motion_ratio, st.pairs, r.skipped_reason
    );
    assert!(r.skipped_reason.is_none());
    // compare applied correction-angle tracks (sum of body increments), per burst,
    // relative to each burst's start; carried offsets from other bursts cancel
    let research = research_angle(adjust, &r.angle_t);
    let mut worst = 0.0_f64;
    for b in r.bursts.iter().filter(|b| b.applied) {
        let (i0, i1) = (
            r.angle_t.partition_point(|&x| x < b.start - 0.3),
            r.angle_t.partition_point(|&x| x <= b.end + 0.3),
        );
        if i1 <= i0 + 1 {
            continue;
        }
        let (p0, q0) = (r.angle[i0], research[i0]);
        // research did not refine this burst (outside its windows): skip
        if (0..3).all(|k| (research[i1 - 1][k] - q0[k]).abs() < 1e-12) {
            continue;
        }
        let mut bw = 0.0_f64;
        for i in i0..i1 {
            let diff = (0..3)
                .map(|k| ((r.angle[i][k] - p0[k]) - (research[i][k] - q0[k])).powi(2))
                .sum::<f64>()
                .sqrt()
                .to_degrees();
            bw = bw.max(diff);
        }
        worst = worst.max(bw);
        println!(
            "  burst {:.2}-{:.2} max {:.3} deg, diff vs research {bw:.4} deg",
            b.start, b.end, b.max_deg
        );
    }
    for b in r.bursts.iter().filter(|b| !b.applied) {
        println!(
            "  burst {:.2}-{:.2} not applied: {:?}",
            b.start, b.end, b.note
        );
    }
    println!("  worst in-burst difference vs research {worst:.4} deg");
    assert!(
        worst < 0.15,
        "production diverges from reviewed wp1c by {worst} deg"
    );
}

#[test]
#[ignore]
fn parity_0073() {
    case(
        "target/experiments/generalization-v1/0073/edgeoffset.MP4",
        "target/experiments/residual-stage-v1/0073/stages.json",
        "target/experiments/feedback-v1/wp1c-adjust.json",
    );
}
#[test]
#[ignore]
fn parity_0071() {
    case(
        "target/experiments/generalization-v1/0071/edgeoffset.MP4",
        "target/experiments/residual-stage-v1/0071/stages.json",
        "target/experiments/feedback-v1/0071/wp1c-adjust.json",
    );
}
#[test]
#[ignore]
fn parity_0060() {
    case(
        "target/experiments/gyro-trace-v1/edgeoffset.MP4",
        "target/experiments/gyro-trace-v1/edgeoffset-metrics.json",
        "target/experiments/feedback-v1/0060/wp1c-adjust.json",
    );
}

#[test]
#[ignore]
fn refine_honours_cancel() {
    let base = "target/experiments/generalization-v1/0073/edgeoffset.MP4";
    let tel = extract_quats(&common::repo(base)).unwrap();
    let cancel = AtomicBool::new(true);
    let r = refine(
        &common::repo(base),
        &tel.t,
        &tel.q,
        &[(141.25, 144.45)],
        &tel.meta,
        &RefineConfig::default(),
        &|_| {},
        &cancel,
    );
    assert!(matches!(r, Err(o4core::error::O4Error::Cancelled)));
}
