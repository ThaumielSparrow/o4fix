# Stabilization follow-up tasks and experiment backlog

**Status:** active research, with no candidate promoted. Read [the current session handoff](../../SESSION_HANDOFF.md) for the consolidated experiment ledger, user verdicts, verification, and next step. This document remains the broader provisional backlog.

**Objective:** reduce the remaining small judders and unwanted panning without sacrificing the current clean-flight behavior, fast-motion trade-offs, embedded-telemetry workflow, or output-file safety.

## Starting point

The accepted v0.1.2 behavior uses M2 by default, optional M4, segment-peak monster-burst gyro trust, and drift rebase above 30 deg/s with 1.5 deg/s decay. The September review added safeguards and verified existing numerical behavior; it did not establish a visual-quality improvement.

The working hypothesis is that remaining artifacts come mainly from optical low-frequency error and contaminated gyro content handed back during motion. That is not a complete diagnosis. Timing error, splice transitions, post-burst DJI fusion settling, and evaluation artifacts should be distinguished before choosing another correction mechanism.

Read the September code-review report and the latest `CLAUDE.md`/Superpowers progress entries before starting. The older task-5 report's negative rebase assessment used superseded renders; later zoom-corrected comparisons and user feedback supersede that conclusion.

## Proposed order

1. Make the evaluation evidence trustworthy and localize the residuals.
2. Address calibration timing, tracking confidence, and dropout handling.
3. Test optical estimation improvements in small, isolated experiments.
4. Investigate post-burst motion and transition behavior if localization points there.
5. Consider a different fusion method only after source uncertainty is measurable.

Parser safety, filename collisions, and cancellation are important product work that can proceed independently of quality research. They should not be represented as judder fixes.

## Remaining engineering priorities

| ID | Priority | Rough task and intended outcome |
|---|---|---|
| R1 | High | Harden MP4/protobuf parsing: bounded reads, checked arithmetic, valid table/sample extents, bounded allocation, actionable errors, malformed-input tests and fuzzing. Invalid files should not panic or abort processing. |
| R2 | High for research | Make evaluation clip-aware and reproducible: explicit clip/window manifests, input/configuration/build fingerprints, matching Gyroflow video metadata, quality reporting, and adequate context for low-frequency measurements. |
| R3 | High for quality | Fix calibration's treatment of time: chronological contiguous runs, actual sampling cadence, no filtering across gaps, final transform refit after shift selection, and held-out validation. Preserve the established ability to fit a reflected coordinate mapping. |
| R4 | High for quality | Distinguish accepted segments from reliable observations. Measure continuous dropouts, limit interpolation/extrapolation appropriately, and propagate confidence to the repair decision. Decide explicitly how to report unrepairable bursts; never silently rebase filtered-gyro fallback. |
| R5 | Medium | Reserve/disambiguate batch destination paths before dispatch, including same basenames in different input folders and path aliases. Atomic writes alone do not prevent one completed job replacing another. |
| R6 | Medium | Carry cancellation through telemetry extraction, copying, quaternion writing, and verification, with a final check before publication. Preserve the previous destination on cancellation. |
| R7 | Before Python repair experiments | Bring Python research tools into agreement with Rust's coverage, finite-calibration, and output-transaction safeguards. Keep the historical numerical reference identifiable rather than replacing its fixtures implicitly. |
| R8 | Medium | Make settings writes resilient and surface save failures. Complete packaged GUI repair/cancel/batch/error verification. Strengthen release version/build provenance and repeatable checks. |
| R9 | Later | Profile repeated parsing, individual payload writes, moving-window filters, and nested concurrency. Optimize measured bottlenecks while preserving numerical gates; validate public DSP input assumptions. |

Each selected task should later receive its own scope, tests, compatibility decisions, and completion criteria. This backlog does not prescribe a broad refactor.

## Experiment candidates

### E0 — Localize the remaining artifact before changing the algorithm

**Question:** is each visible judder/pan caused inside the repaired burst, at its boundary, or in the following nominally clean span?

Create a small set of user-relevant examples with sufficient lead-in and tail. Overlay raw and patched body rates, optical observations/confidence, handback weight, severe/optical interval boundaries, carried offset, and Gyroflow body-frame correction. Classify intended camera motion separately from unwanted motion. Compare bridge/rebase only where useful for attribution, not as another default-threshold sweep.

**Useful result:** an artifact map with timestamps, plausible causes, contradictory evidence, and unresolved cases. This should determine which of E1–E7 deserves effort. An optical-derived diagnostic is not independent ground truth for an optical-derived repair.

### E1 — Calibration and clock alignment

**Hypothesis:** filtering noncontiguous observations as one 100 Hz sequence biases the transform or time shift enough to leave residual motion.

First use synthetic known rotations and offsets with missing observations, shuffled calibration segments, and different frame rates. Compare the present fit with filtering/resampling within contiguous runs and refitting at the final shift. Select calibration windows using the motion actually contained in the selected window, not a longer interval that is subsequently truncated. Assess directional excitation and reserve a clean segment for validation.

Only if residuals suggest it, test whether one constant time offset is adequate across a long clip. Avoid per-burst timing fits on corrupted gyro; they could “explain” phantom motion by choosing the wrong clock offset.

**Advance if:** known parameters are recovered more reliably and held-out real-clip alignment improves without clean/flick regressions. A better training R² alone is insufficient.

### E2 — Better feature validation and spatial coverage

**Hypothesis:** some high-count optical fits are confidently wrong because their tracks are clustered, mistracked, or dominated by moving objects.

Test forward/backward LK consistency first. Separately examine LK error, reprojection residuals, feature distribution across the image, and robust background-motion selection. Compare one addition at a time. Audit lens calibration and image-coordinate scaling so an incorrect camera model is not mistaken for a tracker problem.

**Advance if:** endpoint/rate errors and visible artifacts improve on difficult bursts while enough tracks survive fast motion. Rejecting more observations is not automatically an improvement; record rejection rates and newly unrepairable spans.

### E3 — Continuous dropout handling

**Hypothesis:** an acceptable overall bad-frame percentage hides long continuous gaps whose interpolation creates false low-frequency motion.

Measure dropout-duration distributions on current clips. Inject controlled gaps into otherwise reliable optical sequences and compare bounded interpolation, segment splitting, and explicit refusal. Establish a defensible maximum gap using recovery error, motion regime, and filter bandwidth; do not pick it solely to preserve fixture acceptance.

**Advance if:** fabricated motion is reduced without excessive false refusals. Specify how interval edges and a carried offset behave when part of a burst lacks support.

### E4 — Rolling-shutter-aware optical motion

**Hypothesis:** fitting one rigid rotation to a whole frame pair leaves systematic low-frequency bias during fast motion and vibration.

Start with diagnostics: do fit residuals vary with image row and motion rate? Establish readout direction/timing and whether metadata supplies trustworthy values. If the evidence supports it, prototype row-time-aware correspondence fitting or dewarping only for difficult spans.

Use clean spans and synthetic rolling-shutter motion to isolate model error. Do not use the corrupted gyro as an unquestioned dewarping prior inside monster bursts. Keep translation/parallax separable from camera rotation.

**Advance if:** the extra model reduces optical error and improves the same visible burst at tolerable cost. Stop if readout timing is unidentifiable or model complexity simply absorbs noise.

### E5 — Tracking resolution, blur, and temporal support

**Hypothesis:** half-resolution, adjacent-frame tracking loses useful information in some blurred or very fast spans.

Try full-resolution or targeted multiscale tracking before introducing learned deblurring. Consider a short multi-frame fit where adjacent-frame estimates are unstable, using a motion model that can represent acceleration. Compare against the existing essential-matrix approach; do not replace it with a rotation-only fit that absorbs parallax into pan/tilt.

**Advance if:** improvements persist on unseen spans without suppressing real flicks or becoming too slow. Treat deblurring/dense-flow methods as later, higher-cost options; generated image detail is not reliable motion evidence by itself.

### E6 — Post-burst DJI fusion settling

**Question:** does nominally clean embedded orientation contain a slow artificial return motion after severe vibration?

Measure raw-versus-optical relative motion for roughly 5–10 seconds after selected monster bursts, extending the window where needed. Use high-confidence optical spans and matched clean controls. Separate actual camera panning, carried-offset decay, and Gyroflow's intended smoothed trajectory.

**Useful result:** evidence that settling is present, absent, or not identifiable. Only a reproducible signal should lead to a correction proposal. This would affect currently preserved motion outside bursts and therefore needs stricter regression review.

### E7 — Splice boundaries and offset-decay attribution

**Hypothesis:** a subset of residuals is tied to rate/acceleration changes at splice entry, exit, or decay completion rather than poor in-burst optical estimates.

Inspect synthetic and real body-rate continuity at those points, including multiple rebases about different axes. For diagnosis only, compare the current decay with zero decay while holding the burst trajectory fixed and horizon lock off. A constant carried offset and a changing offset have different effects; do not infer decay behavior from whole-clip constant-offset invariance.

**Advance if:** a repeatable boundary-localized artifact is isolated. Any proposed ramp or decay change must improve that artifact without moving it elsewhere or increasing panning. Do not revisit rate-weighted drift spreading as a default candidate.

### E8 — Uncertainty-weighted optical/gyro fusion

**Hypothesis:** once optical uncertainty is credible, a smoother confidence-aware blend can outperform the current rate-aware handback in specific motion regimes.

Begin with a simple interpretable blend before an EKF. Retain segment-peak monster-burst distrust, avoid assuming gyro and optical errors are independent, and distinguish uncertainty from merely having many inliers. Compare optical-only, current handback, and proposed fusion as separate ablations.

**Advance if:** it beats the current method on held-out clips without recreating monster-burst gyro leakage or softening genuine fast motion. A more elaborate estimator cannot recover information absent from both sources.

## Evaluation outline for later specifications

Use 0021 as the established regression clip, 0027 and 0060 as monster-burst cases, and reserve selected 0057–0059 spans for confirmation rather than tuning everything on 0060. Additional footage or a healthy unit could help if conveniently available; new hardware is not a prerequisite.

Useful existing anchors include 0060 near 100, 106, 225, 248, and 308 seconds, plus neighboring controls and post-burst spans. On 0021, verified clean windows include 67–72.5, 94–98, and 120–128 seconds. Keep flicks and fast patched motion explicitly represented. Have the user identify remaining objectionable moments before treating these anchors as a complete artifact inventory.

For each experiment, plan to retain:

- A frozen baseline, a single changed mechanism, the input/build/settings identifiers, and enough diagnostics to explain the result.
- Correct clip-specific `video_info`, lens/zoom settings, matching render settings, and empty Gyroflow autosync offsets.
- Full-series or adequately padded filtering before extracting windows, with minimum-duration rules for low-frequency measurements.
- Body-frame correction comparisons (`org⁻¹ · stab`), clean controls, optical-quality/dropout summaries, and context around burst boundaries.
- Matched perceptual A/B, preferably with variant labels hidden, when metric differences approach the known tracker floor. Record “inconclusive” when neither measurement nor viewing resolves a difference.
- Separate judgments for judder, unwanted panning, flick fidelity, zoom/lens-edge artifacts, repair coverage, and runtime.

Do not use the same optical estimator as the only judge of its own improvement. Do not promote a candidate solely because its R² rises, drift magnitude falls, or it produces a lower number on a short low-frequency window. Likewise, numerical parity is a regression safeguard, not proof of better visual stabilization.

## Guardrails and explicit deferrals

Preserve M2/M4 defaults until a candidate earns promotion. Keep embedded telemetry delivery, byte-identical null patching, exact injection round trips, and identity-offset clean-sample preservation. Carrying drift remains incompatible with horizon lock as currently documented.

Do not repeat previously measured dead ends without new evidence: global temporal filtering, gcsv variants as a quality fix, autosync on patched data, unconditional min-rate handback, instantaneous-noise trust gating, rate-weighted drift spreading, or the previously tested Gyroflow glitch-filter settings. The dropped fast-motion rebase guard should not return solely because of the obsolete task-5 metrics.

Blackbox gyro was rejected for hardware/workflow reasons and stays outside the proposed work. Learned deblurring, dense flow, a full image-warping stabilizer, and EKF redesign are deferred until simpler experiments show a specific need. A full image stabilizer would be a separate product scope from repairing embedded orientation.

## Suggested first planning slice

Refine **R2 + E0** into a small diagnostic/evaluation specification, and **R3 + E1** into the first bounded numerical experiment. Address **R1** as an independent safety task. Add **R7** before executing Python-based repair experiments. Use the localization results to choose between correspondence quality, dropout handling, rolling shutter, post-burst settling, and transition work.

For each selected item, the next document should settle the exact scope, data, acceptance criteria, runtime budget, failure behavior, and compatibility decisions. This document intentionally leaves those implementation choices open.

## Latest investigation update (September 10)

The user confirmed that both localized output smoothers turn wobble into worse sharp shake; retire them. Spatial held-out fitting shows perspective-model improvement in all four windows but unresolved regional disagreement at 4:09. See `../../experiments/spatial-v1/results.md`. A controlled rolling-shutter-off comparison is ready for visual review; see `../../experiments/rs-v1/results.md`. No default change is proposed from either diagnostic alone.

Latest user verdict: rolling-shutter compensation off may be slightly better but is close, not a substantial improvement. Retain the current setting. The next direction is perspective-aware motion estimation with persistent tracks and regional agreement checks; see the session handoff.
