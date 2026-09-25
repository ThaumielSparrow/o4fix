# Light-gyro partner suppression inside zero-trust severe intervals

2026-09-13. Research-only ablation on 0073. Reference is the accepted fixed-edge-offset candidate with 0.19 s ramps, one-frame optical tracking, original 0.20 s padding, 30 deg/s rebase gate and 1.5 deg/s decay. The separately reviewed gap2 candidate is preserved and is not combined with this change.

## Hypothesis and isolated change

The [exact exposure audit](../residual-exposure-v1/results.md) found that fully replaced orientation interiors can still receive light-filtered gyro when instantaneous alpha drops, despite zero segment-peak trust. This path is separate from medium-gyro fast handback. The 142.50 and 143.25 s bins have up to 100% light-gyro partner weight, with substantial apparent motion and little intended path motion. Correlation and optical disagreement do not establish which rate is accurate.

`partner_gate_repair` reuses the previously validated cached one-frame rates. It selects severe intervals wholly inside an existing zero-trust optical segment, throughout the clip rather than only at complaint timestamps. Within each selected interval it raises the outer optical/burst weight toward one, using the existing 0.19 s smoothstep entry/exit shape and exact snapped source endpoints. Fully interior samples lose the light-gyro partner; exterior rates are untouched. The internal optical/medium handback coefficient remains unchanged. Raising the outer weight also increases effective medium contribution wherever handback is nonzero; handback is zero in the key 141.253–144.451 s target burst. No new optical estimator, rebase rule, decay, or output smoothing is introduced.

The change is continuous at the severe endpoints. It does not assume that optical rates are ground truth, and cannot promise removal of all residual jello. The 143.00–143.25 s bin is an optical-only negative target; it should not receive a direct rate change from suppressing the light-gyro partner.

## Verification and measured scope

- Three synthetic/regression tests pass: alpha-dip contamination removal with known fast multi-axis rates, unchanged handback/unselected behavior and smooth gate limit, and missing-coverage refusal.
- Release Clippy with warnings denied and release example build pass. The existing nom future-compatibility warning remains.
- Cached accepted reconstruction has maximum sign-folded quaternion component difference 3.9984e-8, consistent with MP4 storage precision.
- Recomputed light/optical/medium mixture matches cached final rates within 1.137e-13 deg/s across trace rows; source midpoint timestamps match within 1e-9 s. This verifies the source-mixture formula used by the ablation.
- Both original and modified patches pass severe-interval optical coverage. The new MP4 passes transactional timestamp/quaternion writeback with zero discrepancy.
- 2630 rate samples change across nine selected intervals; 485 are outside the early/late review windows, around 267–269 s. All original burst intervals and six rebase decisions remain unchanged. Endpoint drift values change slightly, so carried offsets and later slow decay can differ.

Stored body-rate differences versus accepted reference:

| Window | RMS difference, deg/s |
|---|---:|
| Early 14–30 s | 0.760 |
| Late 132–146 s | 2.124 |
| Clean control 55.9–58.9 s | 0.0123 |
| 142.50–142.75 s | 8.121 |
| 143.00–143.25 s, negative target | 0.0020 |
| 143.25–143.50 s | 6.834 |
| 143.50–143.75 s | 0.493 |

The small nonzero negative-target difference is measured after independent float quaternion storage and rate differentiation; the supplied rate is unchanged there. Filtering has full-clip context and can spread neighboring differences. Clean control band magnitudes are effectively unchanged, but the 0.0123 deg/s slow difference is not exact control parity. World-angle difference is not a direct stabilization error with horizon lock off.

These measurements verify that the candidate changes the intended mechanism. They are not perceptual improvement scores. No production files or shipping binaries were changed, and no promotion or commit was made.

## Review and reproduction

Full-clip rendering completed in 184.390 s; the 1440x810 / 100 fps / 37594-frame output passed metadata validation. Four comparisons passed frame-count checks: early 14–30 s (1600 frames), late 132–146 s (1400), clean control 55.9–58.9 s (300), and additional changed bursts 264–272 s (800). A decoded late-event frame was inspected for correct side-by-side layout. No renderer or export process remains running for this experiment.

Left is accepted edgeoffset, right is partner suppression. The late target is around source 142.5–143.75 s (10.5–11.75 s into the excerpt), including the negative target around 143.00 s.

**User verdict:** “i dont see much improvement in the late clip”. Record little/no meaningful demonstrated visual improvement in the late comparison only. No verdict was supplied for early, clean-control or additional-burst clips; do not infer regression or equivalence there. This weakens light-gyro partner re-entry as a practically useful explanation for the reviewed residual, despite verified numerical removal. It does not prove the gyro contribution is harmless everywhere or that optical estimation is the sole cause.

Retain the accepted fixed-edge baseline and the separate gap2 incremental candidate. Do not promote partner suppression, expand its gate, or combine it with gap2 without new discriminating evidence. Next investigate optical measurement reliability using clean controls and existing observations first, keeping lens/scaling and geometric-estimator expansion deferred. Preserve this candidate as a negative late-window experiment.

Use the OpenCV environment in SESSION_HANDOFF.md:

```powershell
cargo test --release --offline -p o4core --example partner_gate_repair
cargo clippy --release --offline -p o4core --example partner_gate_repair -- -D warnings
cargo build --release --offline -p o4core --example partner_gate_repair
target/release/examples/partner_gate_repair.exe sample_vids/DJI_20260829141435_0073_D.MP4 target/experiments/generalization-v1/0073/edgeoffset.MP4 target/experiments/residual-stage-v1/0073/trace.json NEW_MP4 NEW_METRICS
```

`prepare.py` creates a fresh full-clip project from the accepted project: same clip metadata, horizon off, no autosync, empty offsets, fresh embedded gyro source, unchanged lens/output settings. It refuses existing project/window files. Launch portable Gyroflow with the `.gyroflow` file and `--stdout-progress`, hidden, with separate logs; the renderer needs sandbox escalation. `export_reviews.py` validates full frame counts before extraction and verifies excerpt counts. `summarize.py` preserves compact metrics and input/code/artifact hashes after rendering completes. Large outputs remain under `target/experiments/partner-gate-v1`; the accepted references and source remain untouched.
