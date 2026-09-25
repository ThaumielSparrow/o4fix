# Full-resolution residual audit — 2026-09-14

Follows the positive-but-incomplete render-acceleration-v1 verdict. The user still notices vibration in corrected late footage and wants smooth, flowy output. Production/core/source clips and the accepted v1 output correction remain unchanged. No visual verdict exists for v2.

## Question and controlled trial

Does the remaining motion come from measuring correction on a reduced-resolution comparison, and does a shared translation or rotational pattern predict residual movement in withheld image regions?

Extract original 1440x810, 100 fps gap2-edge rendered frames losslessly to FFV1: late 132–146 s, clean 55.9–58.9 s, additional 264–272 s. This uses the original rendered output, not upscaled comparisons. No new Gyroflow settings, lens/scaling calibration, source optical pose model, or telemetry changes.

Full-resolution triplet tracking uses up to 1200 corners, 20 px minimum spacing, 41x41 LK windows, three pyramid levels, 1 px forward/backward checks and 10 px margins. Coordinates/accelerations are divided by two for common 720x405-equivalent units. The previous probe used 600 corners, 10 px spacing, 21x21 windows and 0.5 px checks. This is a resolution-appropriate tracking configuration, not proof that resolution alone causes any change.

LEFT: saved v1 translation corrections, doubled and applied directly to the original-resolution source. RIGHT: newly measured full-resolution corrections, using the identical 40 ms Gaussian scale, 120 ms support, 1.04 zoom and 6 half-pixel per-axis refusal bound. Both panels use one full-resolution Lanczos warp and common encoding. Thus the left is a full-resolution realization of v1, not the previously encoded 720x405 panel. No stronger smoothing, rotation correction, or confidence gate is applied.

Probe both full-resolution encoded panels with the full-resolution tracker. Separately downsample each comparison for the existing pairwise velocity-band measurement. The latter retains comparable units and an estimator different from the correction statistic; it is still an optical proxy. All analyses exclude 0.5 s at each boundary. See summary.json for coverage, full numeric results, quarter-second localization, frame counts and warp-support margins.

## Residual field diagnostic

Retain the original spatially balanced split and entire-region holdouts. Add a read-only least-squares field `ax = tx - alpha*y`, `ay = ty + alpha*x` at image center, fitted outside each of nine regions and tested within the excluded region. It represents a possible small rotational acceleration pattern, not a physical camera-pose estimate. Real maneuvers, centripetal acceleration, depth/parallax, deformation, blur, resampling and correlated tracking error remain possible contributors.

Before correction, full-resolution late translation predicts 22.28% of held-out-region acceleration energy; translation plus rotation predicts 34.63%. After applying the prior v1 correction at full resolution, translation transfer becomes **-8.83%**, whereas the combined field predicts **8.41%**. With newly estimated full-resolution correction, the corresponding numbers are **-10.14% / +6.75%**. The remaining field is not reliably explained by a shared translation.

Do not attribute the entire difference between these fits to rotation: the translation diagnostic uses a robust component median, whereas the combined field uses ordinary least squares. A matched least-squares translation-only control is required before a rotation-specific benefit claim or correction. Also, held-out regions share the image/LK pipeline. These are predictive diagnostics, not ground truth or a validated correction gate.

## Encoded comparison results

Changes relative to the full-resolution realization of v1:

| Window | 2–8 Hz translation | 2–8 Hz roll | 8–30 Hz translation | 8–30 Hz roll |
|---|---:|---:|---:|---:|
| late | -4.55% | +2.15% | +0.96% | -2.26% |
| control | -1.64% | -18.00% | -4.10% | -0.90% |
| additional | +2.79% | +0.33% | +7.58% | +8.59% |

The prior-corrected clean/additional combined-field holdout reductions are -19.92% / -11.82%; these controls do not support a blanket extra correction. Keep these negative results alongside the late +8.41% result.

## Decision

Full-resolution estimation does not show a meaningful late high-frequency gain: 8–30 Hz translation increases about 1% relative to the prior correction. Low-band translation falls about 5%, with mixed roll changes. Preserve this as a completed controlled comparison, without promoting it or requesting stronger smoothing based on those small changes.

The useful finding is the residual translation transfer failure at both resolutions. Keep v1 as the visually supported partial improvement. Next use a matched estimator control to isolate whether rotational acceleration predicts residuals, assess temporal persistence and uncertainty across clean/held-out windows, then consider a bounded targeted correction only if supported. Do not train a gate solely to accept this late window and reject the known additional/control windows. Lens/scaling and source geometric-model expansion remain deferred. No production or shipping rebuild is required.

## Verification and artifacts

Seven tests pass across the two new examples: the inherited translation/holdout/integration cases, a known rotational field, and a synthetic textured-image experiment where bidirectional LK distinguishes steady pan from shared translation shake. These verify limited mathematical/tracking behavior, not all real-scene failure modes. Release Clippy passes with warnings denied; the existing nom future-compatibility warning remains.

Artifacts: `target/experiments/render-acceleration-v2/compare_{late,control,additional}_full.mp4` (2880x810), matching `compare_{late,control,additional}.mp4` previews (1440x406), lossless source excerpts, input/prior/full diagnostics, correction arrays and band results. LEFT prior v1 correction; RIGHT full-resolution measured correction. No early-window correction/failure handling is introduced in this experiment. Full-resolution late frame at 10.6 seconds was inspected for layout. Both full and preview outputs have validated 1400/300/800 decoded frames at 100 fps; correction source-support margins include the Lanczos4 footprint. All rendering/probing processes are complete.

Reproduction: use the handoff OpenCV environment; build `render_acceleration_full_probe` and `render_acceleration_full_review`. Their positional usage messages describe inputs. Extract exact source windows with FFmpeg `-ss START -frames:v COUNT -c:v ffv1 -level 3`, probe each at panel 0, then run `python docs/experiments/render-acceleration-v2/run_reviews.py`. Both new examples refuse overwriting output. Run `summarize.py` after completion for validated counts and SHA256 provenance. Scripts operate only on experiment outputs; the original render and prior corrections are retained.
