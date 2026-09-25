# Localized output smoothing experiment — September 10, 2026

Decision: do not promote either setting. All four comparison clips are ready, but neither candidate passes the sharp-shake numerical gate. This is a research post-render stage, not a change to embedded telemetry or the production application.

## Method

Measure frame-to-frame similarity motion on the existing default 0060 render at 720x405. Fit robust partial affine motion, retain center translation and rotation, and omit fitted scale to avoid chasing zoom. Compose a cumulative SE(2) path and smooth it with centered Gaussian sigma 40 ms (conservative) or 80 ms (strong), truncated at three sigma. The correction maps the measured path to that target path. It fades around the selected severe bursts (0.4 seconds before, 0.6 after, 0.3 second smoothstep transitions). Corrections are exactly zero outside that support.

Translation is bounded at 11 half-resolution pixels (22 original-render pixels), rotation at 0.5 degrees. All panels, including baseline, receive the same fixed 1.08 zoom and interpolation. This costs about 7.4% linear field of view. No image blur is applied. Frames remain at 100 fps; panels are baseline / conservative / strong from left to right.

Tracking requires at least 80 inliers; frames below 30% inlier agreement are flagged. All four generated windows have zero such failures. An initial 50% fraction guard rejected 8.7% of the first window despite its minimum agreement of 38.6%; that heuristic was revised after inspecting the distribution and adding the absolute inlier requirement. This is evidence of model limitations, not proof that the accepted tracks are ground truth.

OpenCV could not seek into the later HEVC windows although FFmpeg decoded them. Those windows were extracted with one second of context on each side into lossless FFV1/BGR intermediates. A source-time offset preserves the original burst timing. No interpolation of frame cadence was performed.

## Remeasured results

Re-track the actual encoded panels using common reliable temporal support around the bursts. Filter apparent translation and roll into 2-8 Hz and 8-30 Hz bands; exclude half-second clip edges and quarter-second neighborhoods around failed fits. Translation is measured in half-resolution pixels/s; roll in degrees/s. These are optical proxies, not perceptual ground truth. The estimator shares its model family with the correction stage, so this is not an independent ground-truth test. Reprojection, parallax, feature selection and resampling can affect the scores. No short-window low-frequency panning conclusion is drawn.

Changes relative to the equally cropped baseline (negative is lower):

| Moment (s) | Setting | 2-8 Hz roll | 8-30 Hz translation | 8-30 Hz roll |
|---|---|---:|---:|---:|
| 106 | conservative | -11.2% | +12.7% | +5.5% |
| 106 | strong | -20.5% | +25.8% | +20.6% |
| 225 | conservative | -19.8% | +54.9% | +58.2% |
| 225 | strong | -31.2% | +70.7% | +61.8% |
| 249 | conservative | -8.3% | +75.9% | +31.1% |
| 249 | strong | -8.4% | +11.6% | +114.2% |
| 308 | conservative | -20.1% | +73.5% | +37.9% |
| 308 | strong | -20.9% | +53.6% | +25.6% |

The lower-frequency roll reduction is consistent, but both settings increase the high-frequency proxies in every window. Increasing this smoothing is therefore not the supported next move. Rotation/translation limits also activate: conservative/strong limited-frame counts are 0/0, 0/32, 44/96, and 13/94 respectively (each clip has 800 frames). A larger correction allowance would require more crop and might soften intended turns; it would not address estimator-induced jitter by itself.

## Handback diagnosis

Reconstruct default handback weights from cached optical observations with the same filtering/ramp equations. This is approximate because the cached eight-second context differs from exact production optical segment boundaries. The selected segments all have zero segment-level gyro trust, which still permits min(gyro,optical) handback at high rates.

The reconstruction gives zero weight at 105.83-106.61, 223.44-223.96 and 225.03-226.18 seconds. At 248.34-249.32 it averages 5.7%, briefly reaching 85%; only 5.5% of samples exceed half weight. At 308.01-309.28 it averages 0.75%, peaking at 24%. Handback is consequently a weak explanation for the common remaining judder across all four moments; it is not ruled out for individual fast-motion samples.

## Assessment and next step

Keep the production defaults. The user's visual verdict is recorded below and confirms a regression. If the high-frequency degradation is visible or neither setting clearly helps, retire this naive global post-render smoothing approach rather than increasing its strength.

The next useful investigation is a spatial motion diagnostic: track persistent features in several image regions, measure agreement and reprojection residuals, and distinguish coherent whole-frame shake from depth-dependent parallax or row-dependent deformation. Compare a second measurement method to determine whether the increased high-frequency scores represent added visible shake or tracker noise. Only then choose between a more stable background-motion estimator, a localized warp model, or rolling-shutter treatment. Existing data do not yet establish which of those is the cause. Do not infer that more gyro low-pass filtering will solve it.

## Verification and artifacts

The synthetic test attenuates a 10 Hz disturbance while preserving a linear pan without lag. The inverse-warp corner test covers the bounded translation/rotation envelope under the common zoom. Both tests passed. Every comparison has exactly 800 frames at 100 fps, 2160x406 pixels (three 720x405 panels plus a one-pixel encoding pad). Zero correction outside burst support was checked from the recorded transforms. A decoded first-window frame was visually checked for layout. Full transforms, tracking data, FFV1 context clips and review MP4s live in target/experiments/local-smooth-v1; compact metrics, handback estimates and hashes are retained beside this report. Production core and release applications were not changed.

## User verdict

User confirms the measured tradeoff: wobbly judder becomes sharper shake, and the sharper shake looks worse. Retire both settings as visual regressions. Keep production defaults and investigate spatial motion/model mismatch.
