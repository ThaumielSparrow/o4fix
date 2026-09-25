# Tracking experiments: forward/backward gate and two-frame measurement

User feedback on zero-decay was no meaningful visual improvement; keep the default 1.5 deg/s decay. Production tracker and application settings are unchanged.

## Completed measurements

A research copy of the current tracker adds a forward/backward LK check at 1 pixel in half-resolution coordinates. This worsened pooled 0060 clean-section error from 3.29356 to 3.41370 deg/s (+3.65%), with three sections improving and three worsening. It did not introduce low-confidence pairs at the four complaint windows, but does not earn promotion.

A control changes only the OpenCV RNG seed by +1: no measurable rate change on these samples. This does not establish how every OpenCV estimator uses randomness; it rules out this seed perturbation as a useful improvement here. The no-gate research tracker matched cached production output within 8.9e-16 rad/s; the original exact assertion was too strict for JSON floating-point round trips and was replaced by a 1e-12 tolerance with lengths/timestamps checked.

The two-frame experiment retains feature detection, LK parameters, lens model, essential-matrix fitting, and quality formula. It estimates rotation between frames i-2 and i, divides by 20 ms, and timestamps it at the pair midpoint. Outputs still arrive every frame at 100 Hz. This increases optical measurement duration, not gyro smoothing. Fast-motion/occlusion risks require viewing.

| Clip | Baseline pooled RMS | Two-frame RMS | Reduction | Improved sections |
|---|---:|---:|---:|---:|
| 0021 | 2.34906 | 1.76409 | 24.90% | 4/4 |
| 0027 | 3.18510 | 1.90555 | 40.17% | 6/6 |
| 0060 | 3.28653 | 2.11937 | 35.51% | 6/6 |

Units are vector deg/s. Each original calibration section uses the saved baseline transform trained on the other sections. Both trackers use identical common accepted support, contiguous-run 5 Hz comparison filtering, and boundary trimming. Baseline observations are linearly interpolated to the new midpoint times; bracketing quality uses the minimum, and interpolation across gaps is rejected. Consequently these baseline values differ slightly from the earlier calibration experiment. These are clean gyro-reference scores, not rendered judder scores or independent-flight statistical confidence. No candidate calibration was fitted to the scored sections.

All four 0060 complaint windows have zero candidate observations below quality 0.3. Their raw baseline/candidate rate differences are approximately 8.7-9.6 deg/s; a difference does not establish which is correct during corrupt gyro bursts.

## Viewing candidate

The research repair uses original one-frame calibration and two-frame tracking only in noisy sections. All other configuration, patch blending, coverage checks, splice/rebase logic, default decay and transactional verified MP4 injection are preserved. Research copies are under o4core/examples/support; do not promote them as duplicated production implementations. Refactor and validate a shared implementation only if visual review supports adoption.

Full-context Gyroflow rendering uses the exact prior baseline project's lens, smoothing, zoom, metadata and empty synchronization offsets. Reuse the validated default baseline render. Compare 102-110, 221-229, 245-253 and 304-312 seconds with default on the left and two-frame candidate on the right. Human gate: less judder, no loss of turn crispness, and no new pan/zoom/edge artifact. User visual verdict is recorded below; no clear benefit was established.

## Reproduction

Research examples: tracking_probe (forward/backward comparison), tracking_seed_probe (parity and seed control), tracking_gap_probe (two-frame tracking), tracking_score (common-support held-out evaluation), tracking_gap_repair (candidate-only MP4). Usage strings document positional arguments. Build with cargo build --release --offline -p o4core --examples and the existing OpenCV runtime environment. Full observations and logs live in target/experiments/tracking-v1; compact scores and source/artifact fingerprints are retained here.

## Artifact verification

All research examples built successfully. Experimental repair passed accepted optical coverage and transactional timestamp/quaternion writeback verification; five rebases remain. Full baseline and candidate renders match at 1440x810, 100 fps, 38269 frames and 382.69 seconds. Four side-by-side exports contain 800 frames each; the clean control at 19-22 seconds contains 300. Layout is default left, two-frame candidate right, with 720x405 panels and one bottom padding pixel for encoding. A decoded comparison frame was checked for layout. Hashes are in manifest.json. Production core and release applications were not changed by these experiments. User visual verdict is recorded below.

## User verdict

User saw little difference: the two-frame candidate may be slightly better for judder, but would be difficult to identify without labels. Classify as weak/uncertain visual benefit, not a clear improvement. Next authorized experiment targets sharp visible shakes with conservative and stronger localized output smoothing.
