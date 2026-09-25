# Calibration experiment, September 9, 2026

Decision: keep the candidate experimental. No production stabilization defaults changed and no candidate video was produced. Manual viewing is not needed for this experiment.

The candidate filters chronological contiguous quality runs at measured cadence and refits the reflected orthogonal mapping for each trial shift. It uses the existing calibration intervals, tracker, and cleaned gyro. Both fits are scored on identical held-out observations with the contiguous-run evaluator. Each fold excludes one whole original calibration interval. Pooled RMS weights squared residuals by retained sample count.

| Clip | Folds | Baseline held-out RMS (deg/s) | Candidate | Change |
|---|---:|---:|---:|---:|
| 0021 | 4 | 2.353813 | 2.354654 | +0.036% |
| 0027 | 6 | 3.197408 | 3.180470 | -0.530% |
| 0060 | 6 | 3.293079 | 3.299000 | +0.180% |

All-data fitted clock shifts changed from -4 to -2 ms, +6 to +2.6 ms, and 0 to -0.2 ms respectively. The slight training improvements do not generalize consistently. These are gyro/optical calibration residuals, not rendered judder measurements, and folds from one clip are not independent flights. This experiment does not prove that calibration is irrelevant; it does not justify promoting this particular candidate.

Synthetic reflected motion at 50/60/100/120 fps recovered a known 17 ms offset as 17.4 ms and mapping Frobenius error about 0.000063. Reversing observation order gave identical fits. Stationary and single-axis motion were rejected. Good synthetic behavior alone is insufficient for promotion.

## Reproduction and evidence

Build `cargo build --release --offline -p o4core --example calibration_probe`, then run `target/release/examples/calibration_probe.exe VIDEO OUTPUT_DIRECTORY` for each original source. Requires the existing OpenCV build/runtime configuration. This probe reads source video and writes JSON; it does not repair footage. Run `cargo test --offline -p o4core --test calibration_experiment` for synthetic cases.

The adjacent 0021/0027/0060 JSON reports retain every fit, original interval, sample/run count, and held-out score. `manifest.json` records source hashes, actual ffprobe metadata, default configuration, baseline executable hashes, optical observation hashes, and relevant source fingerprints. Raw observations, preserved baseline binaries, and logs are local in `target/experiments/calibration-v1/`. Acquisition preceded a helper refactor consolidating identical coarse/fine SVD fitting; the final source applies the observability guard to both stages.

## Next slice

E0 remains open: acquire tracking confidence and body-frame correction diagnostics around 0060 at 106, 225, 249, and 308 seconds, with clean control 117-128 seconds. Separate optical dropouts, splice/handback transitions, rebase motion, and zoom effects before changing another mechanism. Use matched full-context Gyroflow renders only once a candidate has evidence worth human comparison. The user confirmed these four complaint moments and offered manual better/worse judgments.

## Verification

33 regular tests and all 14 private regressions passed, including full M2/M4 reference comparisons and exact sign-folded 0060 quaternion parity with five rebases. Both release binaries were rebuilt successfully; hashes are in the manifest. The existing nom 6.1.2 future-compatibility warning remains.
