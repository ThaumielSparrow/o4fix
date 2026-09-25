# Landmark refinement, observability and initialization sensitivity

September 10, 2026. Completed the next checks from multiframe-v1 on the same clean 0060 control. Refining landmark bearings does not rescue the six-frame solver. No production change, repair or render was made.

## Changes and controls

New isolated `o4core/examples/landmark_bundle_probe.rs` preserves the prior six-frame windows, feature subsampling, two independent parity groups, Huber scale, endpoint initialization, frozen calibration and maximum 60 iterations. Each landmark now has three variables: two coordinates for its anchor bearing and log inverse depth. Include the first-frame observation in the objective instead of treating its measured bearing as exact. All five non-anchor camera poses remain independent; there is no temporal smoothing or constant-velocity penalty.

The damped solver eliminates each 3x3 landmark block and solves for camera updates. A separate diagnostic rebuilds the robust-weighted normal matrix **without damping**, eliminates landmarks using eigenvalue pseudoinverses and examines a diagonally scaled pose Schur matrix. Landmark pseudoinverse relative cutoff is 1e-10; pose rank threshold is 1e-8 of the largest eigenvalue. These are numerical diagnostics, not calibrated uncertainty or selected acceptance thresholds.

Monocular scale is an unavoidable gauge. The first diagnostic could spuriously count it as observable because of numerical cancellation/pseudoinverse truncation. The final diagnostic explicitly projects out the known translation-scale direction, reporting its preprojection residual and any other weak directions. Generic six-frame geometry has expected pose rank 29 of 30 after fixing the first camera. Pure-rotation landmark depth null directions are counted explicitly. Do not infer reliable physical motion just from a rank count; robust model selection, correlated measurement error and local minima remain outside this diagnostic.

Run the default initialization and a fixed perturbation: rotations are left-multiplied by an angle vector reaching [0.002,-0.001,0.000667] radians at the endpoint; translations receive [0.01,-0.005,0.002] endpoint-baseline units. Earlier frames receive proportional perturbations. Features, RANSAC seeds and observations are unchanged. This is an initialization sensitivity check, not parameter tuning against telemetry.

## Clean-control results

Same 399 requested central pairs in the existing 18.9438-22.9438 s clean calibration section; use the prior leave-one-interval-out axis matrix and time shift. The table's RMS values include **finite unconverged iterates** and are exploratory, as in multiframe-v1.

| Method / initialization | Finite pairs | Converged | Longest converged run | Prototype RMS vs production on identical scored support |
|---|---:|---:|---:|---:|
| Prior fixed-bearing prototype | 390 | 339 | 58 pairs | 4.278 vs 2.903 deg/s |
| Refined landmarks / default | 390 | 322 | 48 pairs | 7.569 vs 2.903 deg/s |
| Refined landmarks / perturbed | 390 | 317 | 45 pairs | 6.978 vs 2.903 deg/s |

All three production comparisons have 313 scorer samples, with the same common support for these runs. The new solver also worsens the common-support persistent-essential comparison: 7.544/6.961 versus 3.741 deg/s on 318 samples. Neither initialization provides converged runs long enough for the established 0.6-second scoring requirement. Scores requiring convergence therefore remain null for both reference and candidate; missing observations are never inserted as zero motion.

For the default fit, 378/390 finite windows have rank 29 at the stated numerical threshold. The remainder have rank 28 (6), 26 (1), 25 (1), 24 (3), or 22 (1). The perturbed run has 382 rank-29 windows. Median second-smallest eigenvalue ratio is approximately 0.0012 in both. Preprojection scale residual ratios reach 9.81e-6 and 3.36e-5, so diagnostics near numerical cutoffs deserve caution. The explicit gauge projection changes diagnostics only; it does not constrain optimization or alter estimated rates.

Matched initialization sensitivity is small in the median but has a material tail:

| Common support | Pairs | Median rate difference | 90th percentile | Maximum |
|---|---:|---:|---:|---:|
| Both finite | 390 | 0.00135 deg/s | 2.166 deg/s | 64.788 deg/s |
| Both converged | 305 | 0.000133 deg/s | 0.805 deg/s | 6.836 deg/s |

These are Euclidean differences between central rotation-rate vectors, not differences against telemetry. Even convergence of both solves does not establish a unique useful rotation. Numerical rank is likewise insufficient to certify correctness. The experiments do not establish whether local minima, measurement correlations, rolling shutter, scene motion, lens-model error or another mismatch dominates the remaining errors.

## Decision and next bounded step

Reject this refined-landmark prototype for repair/render use. The first-bearing approximation was worth checking, but removing it worsens this control. Do not add more solver flexibility or tune confidence thresholds solely to obtain a lower training objective. These results do not rule out every multi-frame method.

The next useful work is a **read-only source residual diagnostic**, before another estimator change: measure signed residuals on held-out source features, recover their actual distorted source-row/radius locations using the known lens model, and test whether errors have a repeatable spatial pattern or correlate with angular speed. Compare temporal/spatial patterns across more than one clean section and include negative controls; translation/depth and scene composition can confound row correlations. Only a repeatable new pattern would justify a targeted readout-time or lens-model experiment. The earlier rolling-shutter-off render was inconclusive and is not superseded by this solver failure. Do not feed clean gyro into a correction and then claim accuracy from agreement with that same gyro. No such residual diagnostic is implemented yet.

## Verification and reproduction

Nine release-mode tests pass: the previous seven motion/robustness/gap/gauge checks plus finite-difference validation of the new landmark derivatives (including the anchor frame) and unregularized-rank/depth-degeneracy checks. Targeted Clippy with warnings denied passes. Both 399-pair runs completed and all returned rate vectors are finite. Production core, settings, shipping executables and reference videos are unchanged; no production regression rerun or shipping rebuild was needed.

Use the Cargo/OpenCV environment in SESSION_HANDOFF.md:

```powershell
cargo test --release --offline -p o4core --example landmark_bundle_probe
cargo build --release --offline -p o4core --example landmark_bundle_probe
target/release/examples/landmark_bundle_probe.exe target/experiments/source-pose-v1/0060.json 18.94 22.95 target/experiments/landmark-v1/default.json 0
target/release/examples/landmark_bundle_probe.exe target/experiments/source-pose-v1/0060.json 18.94 22.95 target/experiments/landmark-v1/perturbed.json 0.002
target/release/examples/multiframe_score.exe sample_vids/DJI_20260808151831_0060_D.MP4 docs/experiments/calibration-v1/0060.json target/experiments/landmark-v1/default.json target/experiments/landmark-v1/default-scores.json
```

Repeat the score command for perturbed.json, then run `python docs/experiments/landmark-v1/summarize.py`. Raw fits/eigenvalues and scores are in target/experiments/landmark-v1. The pre-gauge-projection source snapshot is retained there; its preliminary raw diagnostics were superseded by the final reruns. Prior multiframe-v1 code and results remain untouched. Durable metrics, summary code and fingerprints are stored here. No commit or publication was requested.
