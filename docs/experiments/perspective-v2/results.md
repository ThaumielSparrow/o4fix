# Perspective conditioning and common-support validation

September 10, 2026. Continued perspective-v1 with an isolated read-only example, `o4core/examples/perspective_condition_probe.rs`. No production core or shipping binary changed; no correction video was generated.

## Method

Preserve the perspective-v1 survivor-retaining LK tracker and its thresholds. For each of nine excluded regions, fit a RANSAC homography on the other regions. Record accepted-inlier bounding-box area, occupancy counts, local geometric conditioning, projective denominator extrema at the image corners, and prediction disagreement between two fits to alternating training observations.

The sensitivity calculation uses centered isotropic coordinates `(x-360)/720, (y-202.5)/720`, normalizes the homography to center denominator 1, and forms the 2N-by-8 Jacobian of predicted coordinates with respect to its eight free coefficients. Report largest/smallest singular-value ratio and the square root of half the trace of `J_eval (J_train^T J_train)^-1 J_eval^T`. The latter is a dimensionless per-coordinate prediction noise gain under independent equal-variance destination-coordinate noise and fixed source points, chosen inliers and model. It is not calibrated uncertainty, ignores correlated LK/source-coordinate errors, and cannot detect a consistently wrong scene model. Rank-deficient/invalid geometry stays explicitly invalid rather than being given zero uncertainty. Denominator corner signs detect a projective pole crossing the rectangular image.

Compare all-track and mature-track fits on **identical withheld features in identical frame pairs**. Mature means at least ten surviving pairs, as in v1. A mature fit may still be evaluated where no mature test tracks remain, provided it has sufficient training support elsewhere. This avoids conflating feature removal with prediction improvement. Preserve null fits and coverage counts. Separately retain the original mature-only evaluation in the raw files for audit.

Use the same five contexts and masks as v1: complaint windows 102-110, 221-229, 245-253 and 304-312 s; clean evaluation 19-22 s within an 18-23 s context. Units are pixels at 720x405. Report severe intervals only for complaint windows. Split fits are a sensitivity diagnostic, not statistically independent ground truth.

## Findings

| Window / withheld region | Matched pairs | Median error all -> mature | Median split disagreement all -> mature | Median p90 noise gain all -> mature |
|---|---:|---:|---:|---:|
| 249 / upper-right | 64 | 2.835 -> 0.564 | 0.529 -> 0.548 | 0.275 -> 0.327 |
| 249 / middle-left | 88 | 0.374 -> 3.089 | 0.212 -> 0.947 | 0.125 -> 0.396 |
| 249 / bottom-left | 98 | 3.178 -> 6.696 | 1.227 -> 1.286 | 0.393 -> 0.676 |
| 249 / bottom-middle | 85 | 0.684 -> 2.580 | 0.345 -> 0.437 | 0.135 -> 0.358 |
| 308 / bottom-right | 95 | 0.991 -> 3.226 | 1.236 -> 1.105 | 0.304 -> 0.879 |
| Clean / bottom-right | 299 | 1.191 -> 1.560 | 0.847 -> 0.876 | 0.335 -> 0.374 |

These are medians of each series on common frames. They are not medians of paired changes. The latter are respectively -0.665, +0.618, +0.370, +0.213, +0.949 and +0.025 pixels, included in metrics.json. Mature fits are worse in 67/88 middle-left pairs at 249 and 72/95 bottom-right pairs at 308; the finding is not based solely on a difference between aggregate medians.

The sky-region benefit at 249 persists on identical observations, resolving the v1 coverage ambiguity there. It comes with substantial errors elsewhere. At 106, all regional median errors remain approximately 0.24-0.38 pixels; the mechanism has no broad improvement. At 225, bottom-right worsens from 0.631 to 1.011. More mature tracks are not a validated selection rule.

No horizon crossings or rank-deficient fits occurred among the common-support regional evaluations in these selected intervals. The extrema of corner denominators across all windows were 0.926-1.074, including the clean control. This rules out a projective pole in those evaluated fits, not all numerical sensitivity or missing-fit problems. Condition numbers and conditional gains rise with mature selection: e.g. bottom-right at 308 has median condition 34.4 -> 62.2 and noise gain 0.304 -> 0.879. Fits become more sensitive but are not simply singular. Inlier fractions can remain high despite regional failure (249 bottom-left: 0.912 -> 0.971), so inlier count alone is inadequate.

The evidence supports spatial/model-selection instability. It does **not** identify depth parallax, moving scene elements, cloud tracking error, rolling shutter or output deformation as the sole cause. In particular, local sensitivity does not model RANSAC switching or systematic flow bias. The clean control's own errors prevent declaring an arbitrary global residual threshold to be a valid correction gate.

## Synthetic validation

Eight release-mode tests passed: translation; held-out projective/similarity discrimination; independent region motion with missing coverage; matched evaluation after feature removal; small and fast known pinhole rotation with deterministic outliers; clustered/collinear support; a projective pole; and camera translation viewing two depth planes. The two-depth test produces exactly 6.75 pixels of withheld-region error despite stable split fits and nondegenerate geometry. This demonstrates why good numerical conditioning alone cannot certify global scene agreement. These are correspondence-level tests, not an end-to-end guarantee of tracking fast video motion.

All five video runs completed: 799 pairs per complaint context and 499 in the clean context. The final additional two-depth test changed only cfg(test) code; the release executable was rebuilt afterward. Runtime experiment code was unchanged by that addition. No production regression suite was rerun.

## Decision and next bounded step

Retain production M2/M4 and all existing invariants. Reject mature-track selection as a global optical improvement. Do not attempt projective smoothing or a crop/render correction from these fits.

The next useful experiment is **calibrated source-image rotation estimation**, not more rendered-homography gate tuning: retain persistent source-frame tracks, undistort using actual telemetry camera matrix and fisheye coefficients, and compare a rotation-only bearing fit against a translation-aware robust pose fit on independent features/regions. Test known rotation plus translation/depth, pure-rotation degeneracy, outliers and fast turns; score against trustworthy clean telemetry with held-out time ranges before considering severe bursts. Preserve model disagreement and missing coverage. A homography fit to the rendered output must not be interpreted directly as a camera quaternion. Keep the measured production calibration/fusion frozen if a candidate eventually reaches a repair comparison.

## Reproduction and artifacts

Use the Cargo/OpenCV environment in SESSION_HANDOFF.md:

```powershell
cargo test --release --offline -p o4core --example perspective_condition_probe
cargo build --release --offline -p o4core --example perspective_condition_probe
target/release/examples/perspective_condition_probe.exe target/experiments/local-smooth-v1/context_249.avi 244 245 253 target/experiments/local-smooth-v1/bursts.json target/experiments/perspective-v2/249.json
python docs/experiments/perspective-v2/summarize.py
```

The summary script requires all five raw result files. 106 and control use baseline.mp4 with offset 0; 225/249/308 use context_225/249/308.avi with offsets 220/244/303, respectively. Source windows are stated above. Raw files live under target/experiments/perspective-v2; durable metrics, this report, summary code and fingerprints live here. Prior examples and result files are preserved. No commit or publication was made.
