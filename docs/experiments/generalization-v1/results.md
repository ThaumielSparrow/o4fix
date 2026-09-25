# Additional diagnostic clips: frozen candidate validation

2026-09-12. New user-provided clips 0071 and 0073 validate the accepted fixed-edge-offset research candidate without clip-specific tuning. Production remains unchanged. The exact candidate support-file hash is checked against gyro-trace-v1 and preserved in the manifest, along with source hashes.

## User annotations and review windows

| Clip | User-reported event | Export window |
|---|---|---|
| 0071 | 14–17 s, full-throttle pin with severe shaking | 11–21 s |
| 0071 | 255–257 s, trigger uncertain | 252–260 s |
| 0073 | 17–27 s, mildly shaking powerloop followed by high-throttle severe judder | 14–30 s |
| 0073 | 135–140 s, another shake event | 132–146 s |

Controls: 0071 at 74–77 s, explicitly confirmed clean by the user, and 0073 at 55.9–58.9 s, selected inside its production calibration interval. The later 0073 review also includes a detected burst at 141.25–144.45 s. These are additional scenario tests, not a statistically representative sample of every flight.

## Frozen implementation and verification

`validate_edge_clips SOURCE NEW_OUTPUT_DIRECTORY` runs production optical_patch once and uses those same rates for both repairs. Baseline is current M2 with 0.30 s ramps; candidate is fixed edge offsets with 0.19 s ramps and original 0.20 s padding. Calibration selection/fitting, tracker, handback, peak-noise trust, endpoint correction/rebase decisions and 1.5 deg/s decay are unchanged. No wider padding or other research candidate is included.

Both clips passed optical coverage and embedded timestamp/quaternion writeback checks for both repairs. Baseline/candidate drift and rebase decisions match exactly. Calibration R2 is 0.996 for 0071 and 0.995 for 0073, with shifts displayed as 0 ms by the production logger. 0071 has 7 severe intervals and 1 rebase; 0073 has 31 severe intervals and 6 rebases. Detailed intervals and verification are in the per-clip JSONs. No optical segments were reported rejected.

Both sources are 1440x1080 at 100 fps. Actual frame counts/durations are 38269 / 382.69 s for 0071 and 37594 / 375.94 s for 0073. Their embedded camera matrix and distortion coefficients exactly match the earlier O4P profile. Projects use each clip's actual metadata and the same render recipe, with horizon lock off, no autosync, empty offsets and no stale gyro metadata. Full-clip renders preserve smoothing context. Comparison panels show current production repair on the left and the new candidate on the right; this differs from the prior incremental edgeoffset-versus-ramp019 comparison.

## Corrected 0071 annotation

The user corrected the first throttle event to **14–17 s**, not 74–77 s, and explicitly confirmed 74–77 s is clean. This matches the severe interval at 14.168–17.103 s. The earlier apparent detector mismatch was caused by the annotation error and is not evidence of a missed burst. The 11–21 s review includes the correct event; the 74–77 s interval becomes the clean control. Its peak detector noise is 3.190 deg/s and candidate orientation matches baseline to round-off, as expected. No detection threshold or candidate setting changed in response to the correction.

The later 0071 event has peak noise 319.8 deg/s and a 123.1-degree rebased drift. 0073's reported early/late windows reach 347.6/370.3 deg/s and include multiple rebases. These directly exercise the corrected mechanism. Both clean-control orientations are unchanged to round-off. Stored-gyro band magnitudes in `event-rates.json` are diagnostic motion magnitudes, not ground-truth error or perceptual scores.

## Reproduction

Use the OpenCV/LLVM environment in the handoff. `validate_edge_clips` creates separate baseline/candidate MP4s and refuses existing repair destinations. `event_rate_probe` is read-only. Targeted Clippy passed for the validation runner. `export_reviews.py 0071` / `0073` validates full render metadata and exports the selected windows after rendering finishes. Large repairs/renders live under `target/experiments/generalization-v1/<clip>/`; compact reports remain here.

Positive user visual verdict recorded below. The accepted 0060 result is preserved; no new candidate settings have been chosen from these clips.


## Review artifacts complete

Both full production-baseline renders and both full candidate renders completed. Their dimensions, frame counts and cadence match their source-specific projects. All six review clips passed frame-count checks: 0071 throttle 11–21 s (1000 frames), late 252–260 s (800), confirmed clean 74–77 s (300); 0073 early 14–30 s (1600), late 132–146 s (1400), control 55.9–58.9 s (300). Decoded event frames from both clips were inspected for side-by-side layout. Controls show no repair-orientation change to round-off; this does not claim bit-identical encoded pixels.

The corrected first 0071 event has peak detector noise 455.6 deg/s but endpoint drift 23.77 degrees over the approximately 2.935-second severe interval. It does not meet the unchanged rebase criterion. It therefore exercises the shorter ramp and existing endpoint bridge, not the fixed-offset rebase correction. This is a useful distinct validation scenario; no rebase threshold was tuned to it.

All requested event comparisons are ready in `target/experiments/generalization-v1/0071/compare_{throttle,late,control}.mp4` and `0073/compare_{early,late,control}.mp4`. Left = production M2, right = frozen accepted candidate. User visual assessment is recorded below. Both new examples passed targeted Clippy. Production code/binaries and source clips were not changed.


## User visual verdict — overall improvement across new scenarios

User judges the right/fixed-edge-offset candidate better for most events, with subtler differences in some clips. Their overall verdict is: at worst no worse than the current default, at best a complete fix. This supports generalization beyond 0060, without establishing a perfect or universal fix.

Remaining symptoms:
- 0073 around 2:15 (135 s): mild judder / horizontal bouncing persists.
- Full-throttle forward-flight pins: occasionally a tiny amount of unwanted panning remains; the user did not assign exact timestamps to every instance.
- Earlier 0060 verdict still includes a tiny residual vibration at 249 s.

Retain the accepted fixed-edge-offset + 0.19 s ramp + original 0.20 s padding as the research baseline. No clip-specific settings were changed during this validation and no production promotion has occurred.

Next diagnostic priority: map the 0073 132–146 s residual against its multiple burst interiors and boundaries, then quantify endpoint-bridge contributions during non-rebased throttle bursts (including 0071 14–17 s and 0073 19.49–22.58 s). Compare the actual final orientation rates with the supplied patch; distinguish bridge-induced rates from optical bias, carry decay and intended motion. The 0071 throttle burst's implied bridge peak is about 12.15 deg/s, below the current 30 deg/s rebase gate, making it a concrete hypothesis for residual panning—not proof of causality. Do not lower rebase thresholds or remove endpoint constraints solely from this hypothesis. Exact handback findings on 0060 do not automatically apply to 0073; trace it locally if warranted.
