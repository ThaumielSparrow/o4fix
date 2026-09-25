# Rendered acceleration experiment — 2026-09-14

The user requested higher-confidence experiments aimed at smooth, flowy post-Gyroflow footage after finding subset averaging left late judder essentially unchanged. This experiment measures the delivered images directly. Production, shipping binaries, source clips and accepted research baseline are unchanged. User visual verdict is recorded below.

## Hypothesis and measurement

Feature turnover in pairwise motion estimation can introduce artificial path jitter. A three-frame measurement tracks the same middle-frame feature backward and forward and measures `previous + next - 2*middle` position. This is an acceleration proxy, not proof of unwanted camera motion. No lens/scaling or geometric-model expansion is introduced.

Use the LEFT previous gap2-edge panel of subset-ensemble-v1's saved comparisons, at 720x405 and 100 fps. Detect up to 600 middle-frame corners; use 21x21, three-level pyramidal LK in both directions and reverse checks <=0.5 px. Require both endpoints inside the image margin. Divide retained tracks into two spatially balanced groups, fit componentwise median acceleration to each, and test on the opposite group. Separately leave each of nine entire image regions out and test its acceleration against the median from the other regions. No affine/homography fitting or temporal prior is used in this detector.

All accelerations are half-resolution pixels/frame squared. Regional holdout still shares the same images/LK estimator and is not ground truth. True camera maneuvers, parallax, image deformation, motion blur, resampling and common tracking bias can contribute. No gyro-cause attribution follows from these numbers.

| Window | Valid triplets | Held-out feature energy removed | Held-out region energy removed |
|---|---:|---:|---:|
| Late 132–146 | 1398/1398 | 34.11% | 20.28% |
| Clean 55.9–58.9 | 298/298 | 2.45% | -0.51% |
| Early 14–30 | 1592/1598 | See summary | 6.71% |
| Additional 264–272 | 798/798 | See summary | 2.72% |

Late shared acceleration RMS is 0.3151 px/frame², versus 0.04682 in clean footage. Split disagreement RMS is 0.03195 versus 0.02261. This supports a late translation-correction ablation; it does not establish an automatic detector threshold.

## Bounded correction trial

Integrate shared acceleration twice, remove arbitrary linear integration drift, and compute Gaussian-smoothed path minus path. Final fixed sigma is 40 ms, radius 120 ms. Linear boundary extrapolation is used; exclude first/last 0.5 s from measurement/review conclusions. Translate images only, with Lanczos resampling and common 1.04 zoom on BOTH panels (3.85% linear field-of-view cost). No roll correction, image blur, frame interpolation or telemetry modification. This isolates a different estimator from the rejected local-smooth-v1 pairwise similarity smoother; it is not a claim that output smoothing is generally safe.

Refuse unsupported triplets or any correction exceeding 6 px per axis; do not fill or clip the correction. Early rendering refused six unsupported triplets. Final late/control/additional maximum per-axis correction: 4.9836 / 0.2003 / 1.4621 px. This is a review-resolution ablation, not a full-resolution integration or deployable automatic pass. It has no confidence gate and can alter clean footage.

An initial 100 ms preflight refused before encoding: reflected integration boundaries produced large artifacts, and even the interior late correction reached 27.59 px. Replaced the boundary rule with linear extrapolation and restricted the experiment to the shorter 40 ms scale. No 100 ms candidate was rendered or reviewed. Do not hide this design adaptation or count it as an independent successful trial.

## Actual encoded-image checks

Rerun the three-frame detector independently on both encoded panels, plus the existing pairwise similarity velocity-band measurement. Both panels receive the same crop/encode path. Band scores use common reliable support, discard 0.5 s edges and exclude +/-0.25 s around failed fits (no failed pairwise fits in late/control). This is independent of the correction's triplet statistic, but still optical estimation, not perceptual ground truth.

| Window | 2–8 Hz translation | 8–30 Hz translation | 2–8 Hz roll | 8–30 Hz roll |
|---|---:|---:|---:|---:|
| Late | -6.23% | **-39.53%** | -4.41% | -1.20% |
| Clean | +2.88% | -2.22% | +21.07% | -1.56% |
| Additional | -28.77% | **+4.35%** | -36.97% | **+25.67%** |

Late common-support shared acceleration RMS falls 0.3310 to 0.1483 (-55.18%); all-feature acceleration RMS falls 0.5365 to 0.4550 (-15.18%). These are not percentages of perceived judder removed. Clean low-band roll changes from 0.1375 to 0.1665 deg/s: small absolute motion but a regression must remain visible in reporting. Additional high-band regression rejects unconditional use, despite the promising late result. No roll transform is applied, so measured roll differences include changes in selected features and residual motion under translation/resampling.

## Artifacts, checks and next decision

`target/experiments/render-acceleration-v1/compare_late.mp4`, `compare_control.mp4`, `compare_additional.mp4`: **LEFT gap2-edge with common crop; RIGHT triplet-acceleration translation correction**. Late priority 10.5–11.75 seconds into the excerpt. All are 1440x406, 100 fps; ffprobe counted 1400/300/800 frames. Decoded late frame at 10.6 s inspected for panel layout and image bounds. No early candidate exists. All processes completed.

Five synthetic tests pass: shared acceleration predicts withheld tracks, opposed regions are not removed by translation, constant velocity has zero acceleration, correction sign attenuates oscillation, steady pan needs no correction. These test mathematical behavior, not photometric tracking accuracy. Both new examples pass release offline Clippy with warnings denied and release builds. Existing nom future-compatibility warning remains. No shipping/core edit or rebuild was needed. See summary.json and SHA256 manifest.json for compact results and provenance.

Reproduction: build examples `render_acceleration_probe`, `render_acceleration_review` with the handoff OpenCV environment. Their usage errors describe positional arguments; both refuse existing outputs. Probe saved comparisons with source start and panel 0; feed its JSON to the review renderer. Probe both encoded output panels, then use `splice_render_measure` with `whole-window.json`. Run `python docs/experiments/render-acceleration-v1/summarize.py` for validation/hashes.

## User verdict and next decision

User verdict (2026-09-14): late looks better, but the remaining vibration/shake/judder is still noticeable. Additional may be slightly worse, but the perceived difference is not significant. No separate clean-control verdict was supplied. Record a visible partial late improvement, not a smooth/flowy result or a solved residual. Preserve the measured additional regression without equating its percentage with perceptual severity.

Retain this candidate as a useful research direction, without promotion. Next characterize the residual in the corrected late output at original render resolution: whether acceleration remains coherent across regions, whether roll or spatially inconsistent motion dominates, and how it varies over time. Pair any targeted correction with a spatial-coherence/support gate evaluated on held-out windows and clean footage, including explicit off/fade behavior through tracking failures. Do not simply increase smoothing strength or tune thresholds solely to accept late and reject additional. The acceptance criterion is that the remaining disturbances cease to stand out visually, alongside protection of other footage; lower average numerical scores alone are insufficient. Lens/scaling and geometric expansion remain deferred.
