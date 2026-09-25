# Remaining bounce: localization and optical measurement span

2026-09-12. Preferred reference remains fixed edge offsets, 0.19 s ramps, original 0.20 s padding, 30 deg/s rebase threshold and 1.5 deg/s decay. Extra zero-trust rebasing was visually negative. Production unchanged.

## Read-only findings

0073 source and accepted full render have 1400 frames in 132–146 s, with every timestamp step 10 ms. Source decoded frame hashes have zero adjacent exact duplicates. This excludes those particular cadence failures in this window, not all possible synchronization errors.

Gyroflow 1.6.3 `--export-metadata 3:PATH` exported the accepted project's full intended camera path without rendering. `bounce_localize` measures the right accepted panel of generalization-v1's late comparison and tags samples with saved severe-splice phases. Image similarity velocity and intended body rotation are reported separately, in different units; they are not subtracted. Neither is ground-truth residual shake. Translation, parallax and tracking/model errors remain possible. Filtering spreads energy across boundaries, so phase labels cannot uniquely attribute filtered peaks.

Several largest apparent horizontal 2–8 Hz peaks coincide with substantial intended path motion (e.g. 140.25 s). A distinct 143.50–143.75 s bin lies well inside the rebased 141.253–144.451 s burst: apparent horizontal RMS 16.72 half-resolution px/s, intended rotation norm RMS 0.112 deg/s. This motivates examining supplied optical rates but does not establish bad gyro as that peak's cause.

`optical_span_probe` reuses the original tracker/lens model, seed zero, no forward/backward gate, measuring 10 ms and 20 ms pairs in 131.5–146.5 s and 55.4–59.4 s. `optical_span_score` compares LP8 rates at common midpoint times with 0.5 s context and +/-0.25 s quality exclusions. No low-quality pairs occur in the main window. Quarter-second vector RMS differences reach 12.13 deg/s at 137.50 s and 8.81 deg/s at 143.25 s; the clean control maximum is 3.92 deg/s with uneven accepted coverage. This demonstrates span sensitivity, not which estimate is accurate. See span-scores.json for all bins and coverage.

## Controlled follow-up

Revisit the existing two-frame optical measurement **combined with the accepted edge fix**, not as a newly discovered estimator. Its earlier 0060 visual benefit before the edge fix was weak/uncertain; clean held-out gyro consistency had improved across 0021/0027/0060. The new comparison targets whether it helps the remaining measurement-sensitive motion after the major splice problem is removed.

`gap_edge_repair` uses the existing gap_patch optical helper and accepted edge_offset_patch splice. A source diff confirms gap_patch's optical function differs from production only in the noisy tracking call: original one-frame calibration is retained, noisy pairs span two frames, output still 100 Hz at pair midpoint. Coverage refusal and verified transactional MP4 injection remain required. The original rebase rule is retained; numerical rebase decisions can change because the supplied rates change. No forced rebases or new smoothing parameters. On this completed repair, all burst intervals and rebase decisions match the accepted reference (six rebases). Calibration R2 rounds to 0.995, shift 0 ms. MP4 verification reports zero timestamp and sign-folded quaternion writeback difference.

Stored-rate differences from accepted repair are 3.36 deg/s RMS for 14–30 s and 2.40 for 132–146 s. Clean-control difference is 0.114 deg/s, with effectively unchanged 2–8 and 8–30 Hz magnitudes; this is still not a visual verdict. A constant world-angle difference is not direct stabilization error. See event-rates.json.

Artifacts live under target/experiments/bounce-localize-v1. Candidate gap2edge.MP4 and matching full-render project use original 0073 frame count/duration, empty offsets, no autosync, horizon off and fresh embedded gyro metadata. Full render precedes excerpts. Comparisons: left accepted edgeoffset, right gap2edge; early 14–30 s, late 132–146 s, control 55.9–58.9 s. User verdict (2026-09-13): “the subtle jello effect and vibration remains in both clips. i think its a small improvement though.” Record a small perceived improvement with unresolved residuals; no separate clean-control verdict.

## Reproduction and checks

Use the OpenCV environment in SESSION_HANDOFF.md. Build/run examples bounce_localize, optical_span_probe, optical_span_score and gap_edge_repair; their argument errors describe positional usage. All four pass release offline Clippy with warnings denied (existing nom future-compatibility warning remains). The imported tracker has a local allowance for its existing eight-argument API.

Inputs for localization: generalization-v1/0073/compare_late.mp4, start 132, bounce-localize-v1/camera.json, residual-stage-v1/0073/stages.json. Source for probe/repair: sample_vids/DJI_20260829141435_0073_D.MP4. Export comparisons using this directory's export_reviews.py from the repository root after the full render completes. Cadence and quarter-second summaries are preserved here; raw observations, camera metadata and logs remain in target.


Full render and all three comparisons completed and passed frame-count/cadence checks: full 37594 frames at 100 fps, early 1600, late 1400, control 300. Decoded late-event side-by-side layout inspected. User reports a small improvement, with subtle jello/vibration remaining in both clips; no promotion.


Next: reuse the exact traces and localization to distinguish remaining contaminated raw-gyro contribution in ramps from motion inside fully replaced optical intervals. This is not yet a demonstrated explanation for the residual. Keep lens/scaling deferred, preserve the accepted baseline and this incremental candidate, and avoid another identical parameter comparison without new evidence.
