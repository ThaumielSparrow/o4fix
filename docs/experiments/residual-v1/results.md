# Residual localization and decay ablation

The four confirmed 0060 complaint windows (102-110, 221-229, 245-253, 304-312 s) contain zero optical pairs below the current quality threshold 0.3, across 3,200 pairs. This does not establish tracking accuracy: confidence is an inlier-count score, not independent ground truth. It does make threshold-detected dropouts a poor first explanation for these moments.

All four overlap severe bursts inside optical segments whose peak noise exceeds 300 deg/s, so segment-level gyro trust is zero. This does not mean handback is zero: the existing rate-aware minimum still permits handback during fast motion.

The proposed 117-128 s control was incorrectly described as clean. It contains two severe intervals and a 0.20 s low-confidence run around 118.04 s. Treat it as a secondary repaired-motion comparison. Do not generalize the older 0021 clean labels to 0060.

The read-only Rust `residual_probe` example writes optical observations, full-context LP8 source/repaired rates sampled at 100 Hz, repair membership and segment trust. Source-to-fixed orientation angle is explicitly not Gyroflow's stabilization correction. Adjacent summary.json contains intervals and counts; raw artifacts live in target/experiments/residual-v1.

## Isolated next experiment: hold rebase offset

Use the existing validated CLI parameter `--drift-decay-rate 0` instead of default 1.5 deg/s. Everything else remains M2 default. Holding the world-frame offset removes its slow return as a possible panning contributor. This is an ablation, not a default change or a demonstrated fix. Horizon lock remains off. Existing offset composition tests cover the zero-decay case.

Candidate repair verified embedded timestamps and quaternion writeback with zero difference; five rebases remain. Baseline is the preserved default repair previously checked for exact parity. Both Gyroflow projects use actual 0060 metadata (1440x1080, 100 fps, 38269 frames, 382690 ms), identical lens/smoothing/zoom/output settings and empty offsets. Render the entire clip before extracting review windows to preserve smoothing context. Projects and outputs live in target/experiments/decay-v1.

Human gate: compare judder, slow panning, turn crispness and edge/zoom artifacts. Indistinguishable or worse is a valid result; no production promotion before viewing. Parser/product backlog remains separate.

## Review artifacts ready

Both full renders validated at 1440x810, 100 fps, 38269 frames. compare_106, compare_225, compare_249 and compare_308.mp4 each contain 8 seconds/800 frames; compare_secondary.mp4 contains 11 seconds/1100 frames. Left is default, right holds the rebase offset. Comparison exports are 1440x406, with a one-pixel bottom pad for H.264 compatibility, no audio and unchanged cadence. A decoded frame was visually checked for layout. User visual judgment is recorded below; no meaningful stabilization improvement was found. No production core code changed in this slice.

## User verdict

User judged both versions extremely similar, with judder still present and differences visible only on close side-by-side inspection. Classify zero-decay as no meaningful visual improvement; retain the 1.5 deg/s default and investigate other causes.
