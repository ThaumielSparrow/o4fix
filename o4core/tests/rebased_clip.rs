//! Multi-clip regression beyond the original 0021 Python golden fixture.
//! Reference is the reviewed edge-offset research repair
//! (target/experiments/gyro-trace-v1/edgeoffset.MP4), not the raw source
//! clip; comparisons use a float32-level tolerance because that reference
//! file was written from cached rates. Never overwrites either file.
mod common;
use std::sync::atomic::AtomicBool;

#[test]
#[ignore]
fn monster_bursts_match_edge_offset_research_repair() {
    let source = common::repo("sample_vids/DJI_20260808151831_0060_D.MP4");
    let reference = common::repo("target/experiments/gyro-trace-v1/edgeoffset.MP4");
    let out = std::env::temp_dir().join(format!("o4fix-review-0060-{}.MP4", std::process::id()));
    let result = o4core::pipeline::process(
        &source,
        Some(&out),
        &o4core::config::Config { refine: false, ..o4core::config::Config::default() },
        &|p| {
            if !p.message.is_empty() {
                println!("{}", p.message);
            }
        },
        &AtomicBool::new(false),
    )
    .unwrap();
    let o4core::pipeline::Outcome::Repaired { bursts, .. } = result else {
        panic!("expected repair");
    };
    assert_eq!(bursts.iter().filter(|b| b.rebased).count(), 5);
    let (actual_t, actual_q) = o4core::telemetry::flat_quat_stream(&out).unwrap();
    let (reference_t, reference_q) = o4core::telemetry::flat_quat_stream(&reference).unwrap();
    assert_eq!(actual_t, reference_t);
    let mut max_error = 0.0_f64;
    for (a, b) in actual_q.iter().zip(&reference_q) {
        let same = (0..4).map(|k| (a[k] - b[k]).abs()).fold(0.0_f64, f64::max);
        let flipped = (0..4).map(|k| (a[k] + b[k]).abs()).fold(0.0_f64, f64::max);
        max_error = max_error.max(same.min(flipped));
    }
    println!(
        "0060: {} slots, max sign-folded error {max_error}",
        actual_q.len()
    );
    assert!(
        max_error <= 1e-6,
        "default splice must reproduce the reviewed edge-offset repair (max {max_error})"
    );
    std::fs::remove_file(out).unwrap();
}
