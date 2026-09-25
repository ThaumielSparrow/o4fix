# Calibrated source-frame pose experiment

September 10, 2026. This completes the source-image next step from perspective-v2. No candidate passes the clean-data checks. Production settings, core and shipping binaries are unchanged; no repair or correction render was made.

## What was tested

`o4core/examples/source_pose_probe.rs` tracks source MP4 frames at 100 fps, retaining surviving feature IDs and replenishing at ten-frame intervals or below 150 tracks. Use up to 600 features, ten half-resolution-pixel spacing, and one-pixel forward/backward agreement. Decode at actual source dimensions; halve with INTER_AREA and convert to grayscale. Thus the acquisition differs from production in persistence, reverse gating, spacing and resampling. The cached production comparison measures this combined acquisition change; it does not isolate persistence alone. All three experimental estimators share exactly the same observations.

Require actual camera matrix, distortion and calibration dimensions from telemetry; no fallback lens constants. All three clips report 1440x1080 calibration, focal length 546.4027099609375, principal point (720,540), and fisheye coefficients [0.15513110160827637, 0.1371408998966217, -0.0938614010810852, 0.0041704000905156136]. Scale K to decoded dimensions, restore tracked coordinates to full resolution and fisheye-undistort to normalized rays using the same OpenCV call as production.

Compare:

1. Rotation-only bearing alignment using SVD and up to five 0.002-radian residual rejection/refit iterations; at least 30 accepted points. This bounded robust candidate is not a globally optimal rotation-only fit.
2. Essential matrix using the production 0.002 normalized-coordinate RANSAC threshold, then the smaller-angle rotation branch, on the persistent observations. The experimental minimum support is 30 rather than the production confidence ramp; it is a diagnostic reference.
3. The same essential matrix with OpenCV positive-depth pose recovery and at least 30 accepted points.
4. Cached original production observations, requiring their original quality >0.5.

Fits to disjoint persistent-ID parity groups provide rotation disagreement and held-out per-region angular rotation residuals, with cells below eight observations marked missing. These are diagnostics, not truth: rotation residual includes legitimate translation, and ray errors can be correlated. No new acceptance gate is selected from these statistics.

## Clean held-out telemetry validation

Use the existing 16 calibration intervals: four on 0021, six each on 0027/0060. Freeze each saved leave-one-interval-out baseline time shift and axis matrix; do not refit a candidate to its test interval. Compare against the existing adaptively cleaned telemetry-rate reference through CalibrationData's contiguous-run filtering and edge trimming. Compare each pair of methods on identical available timestamps. Missing methods remain null in the observations; internal zero placeholders have quality zero and are excluded. Runs too short for the scorer remain unscored. This is a telemetry consistency test, not ground-truth visual quality.

| Clip | Rotation-only comparison: essential RMS -> rotation RMS (deg/s) | Scored samples / folds | Experimental essential RMS -> cached production RMS on their common support |
|---|---:|---:|---:|
| 0021 | 2.257 -> 3.192 (+41.4%) | 294 / 1 of 4 | 2.178 -> 2.354 |
| 0027 | 4.013 -> 10.866 (+170.8%) | 1760 / 6 of 6 | 3.917 -> 3.197 |
| 0060 | 3.759 -> 10.031 (+166.9%) | 1488 / 6 of 6 | 3.564 -> 3.293 |

RMS values are pooled by scored sample count. Each comparison has its own common support; do not compare RMS between columns as if observations were identical. The production comparison has 1463/2128/2052 scored samples respectively and all 16 folds. Experimental acquisition improves 0021 but worsens 0027 and 0060. No general gain is established.

Rotation-only returns 664/1583, 2049/2318 and 1939/2294 pairs on 0021/0027/0060. It frequently fails the rigid-rotation residual criterion, especially on 0021. Its lower median split-rate disagreements (1.42/0.74/1.17 deg/s versus 5.85/7.02/6.97 for essential fitting) coexist with higher telemetry error. Independent groups can agree on a biased rotation-only approximation when translation/depth matters. Do not loosen its gate merely to turn missing observations into a numerical score.

## Pose ambiguity and distance-filter ablation

Default positive-depth recovery accepts 1243/1583, 526/2318 and 739/2294 pairs. On scored common support it chooses exactly the same rotation rates as the smaller-angle convention; it supplies no rotation improvement.

Replay saved normalized correspondences with the triangulation distance cutoff set to 1e6 translation-baseline units, solely to diagnose rejection. All 6195 pairs then pass the count requirement, and none chooses a rotation differing from the smaller-angle result by more than 1e-5 radians. Numerical trace-angle differences peak below 0.000003 degrees. The replay reconstructs the reference rate to 1e-10 rad/s tolerance on every pair. This demonstrates the original missing-pose counts depend on the depth-distance filter, not solely on an absence of positive-depth geometry. It does not validate distant triangulation or make translation observable in pure rotation. Do not promote this enormous distance threshold as a confidence setting.

The five synthetic tests verify stationary/small/fast pure rotation, rotation plus translation over varying depth, deterministic outliers, insufficient/concentrated support, and translation-direction non-identifiability under pure rotation. The initial stationary test found a trace/acos round-off NaN; the research angle calculation now clamps its cosine and rate conversion uses a quaternion. All final rates were checked finite. Synthetic tests concern known correspondences, not LK tracking through real fast turns.

## Decision and next step

Reject this rotation-only candidate and the combined persistent acquisition as global replacements. Positive-depth selection also offers no new rotation estimate on these clean sections. Do not render a candidate that already loses on clean controls. These results do not prove that parallax alone causes the complaint-window judder, or rule out better rotational/translation estimators.

Next bounded research direction: **joint estimation over several source frames**, allowing separate feature depths and camera translation while estimating rotation. Use saved persistent normalized tracks to form tracks across frames; fit on disjoint feature groups and evaluate against the same held-out clean telemetry. Start with synthetic observability/degeneracy checks and a short clean interval before implementing an expensive multi-frame solve. A simple wider-gap essential measurement was already tested with only weak visual benefit; this next hypothesis must exploit shared multi-frame geometry, not repeat that pair-gap experiment or add temporal smoothing. Preserve fast intentional turns and missing coverage. No such joint solver is implemented yet.

## Reproduction and durable artifacts

Use the Cargo/OpenCV environment in SESSION_HANDOFF.md:

```powershell
cargo test --release --offline -p o4core --example source_pose_probe
cargo build --release --offline -p o4core --example source_pose_probe
target/release/examples/source_pose_probe.exe sample_vids/DJI_20260808151831_0060_D.MP4 docs/experiments/calibration-v1/0060.json target/experiments/calibration-v1/0060/observations.json target/experiments/source-pose-v1/0060.json
target/release/examples/source_pose_probe.exe --depth-check target/experiments/source-pose-v1/0060.json target/experiments/source-pose-v1/0060-depth.json
python docs/experiments/source-pose-v1/summarize.py
```

Repeat the acquisition and depth check for 0021/0027 before running the summary. Raw files contain normalized paired observations, persistent IDs, image regions, estimator support, null failures and split diagnostics. They live in target/experiments/source-pose-v1; durable folds, aggregates, summary code and fingerprints live here. All 6195 source pairs across 16 intervals completed, as did the three depth replays. Five tests and targeted Clippy with warnings denied passed. The final source adds one cfg(test)-only degeneracy regression after the replay executable build; acquisition logic is unchanged by that test. No production regression suite or shipping rebuild was needed. All earlier artifacts and working-tree changes remain preserved.
