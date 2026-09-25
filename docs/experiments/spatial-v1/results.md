# Spatial motion diagnosis — September 10, 2026

The user confirmed that localized similarity-based output smoothing turns wobble into worse sharp shake. Both smoothing candidates are retired; production defaults remain unchanged.

## Read-only model comparison

Use persistent LK tracks, replenished every ten frames or when fewer than 150 remain. Require forward/backward round-trip error <=1 half-resolution pixel. Divide tracks by persistent ID parity, fit each model on one half and evaluate on the opposite half. Both models use identical accepted tracks and RANSAC threshold 1.5 pixels. Also compare predictions from the two separately fitted halves on the same observed feature positions. This measures sensitivity to feature choice, not ground-truth camera motion. No correction video is produced by this probe.

Each point is also assigned to a 3x3 image grid. Cells with fewer than eight evaluated features are reported missing, never as zero error. Values below are medians of per-frame metrics inside the severe intervals; units are pixels at 720x405. Models are similarity (translation/rotation/uniform scale) and homography (full perspective mapping).

| Moment | Active pairs | Similarity held-out error | Homography held-out error | Similarity split disagreement | Homography split disagreement |
|---|---:|---:|---:|---:|---:|
| 106 | 78 | 0.830 | 0.226 | 0.387 | 0.049 |
| 225 | 167 | 0.713 | 0.253 | 0.256 | 0.086 |
| 249 | 98 | 0.653 | 0.392 | 0.182 | 0.191 |
| 308 | 127 | 0.592 | 0.328 | 0.217 | 0.153 |

Perspective fitting substantially lowers held-out error in every window. It also reduces split-fit disagreement strongly at 1:46 and 3:45, moderately at 5:08, but does not improve agreement at 4:09. This supports model mismatch as a contributor to the failed similarity smoother. It does not establish that a homography-based stabilizer will look better.

At 4:09, the upper-right grid cell has median homography error about 3.06 half-resolution pixels while most other cells are around 0.3-0.7. A frame at 248.8 s was inspected: that area is predominantly sky. Depth-dependent parallax, ambiguous cloud tracking, and local deformation are possible explanations; this experiment does not separate them. Grid rows in a rendered image are not original sensor readout rows, so this is not evidence proving rolling shutter. One homography can also fit a dominant plane while failing elsewhere.

## Controlled next check

Render the existing default repaired 0060 with rolling-shutter compensation disabled (readout time 0 instead of 5.092569 ms). Keep every other Gyroflow setting, the repaired telemetry, empty offsets, full-clip context and output cadence identical. This is a diagnostic ablation, not a proposed calibration or default change. Compare the four complaint moments and the clean 19-22 s control. A clear response would justify closer examination of readout timing and within-frame orientation; no response would deprioritize that mechanism. Do not infer the camera's physical readout time from a subjective preference for zero compensation.

If needed after that check, test a perspective-aware correction only where separate track groups agree, and validate against held-out features and clean motion before asking for another visual comparison. Do not simply replace similarity with a more flexible warp everywhere.

## Validation and artifacts

Two synthetic tests passed: projective motion is correctly better explained by the perspective model on held-out points, and simple translation is accurately recovered by both models. Raw per-frame grids and diagnostics are in target/experiments/spatial-v1; compact metrics and source/input fingerprints are retained here. Baseline decoded footage is reused; later windows use the prior lossless FFV1 context workaround. No production core or release executable changed.
