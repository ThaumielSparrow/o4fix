# Two-frame independent-subset averaging trial

2026-09-13. Research only. Reference is the previously reviewed gap2-plus-fixed-edge candidate, which had a small perceived improvement but unresolved jello/vibration. The strongest cross-clip accepted fixed-edge baseline remains separately preserved.

## Candidate

Use the exact estimator tested in [subset-stability-v1](../subset-stability-v1/results.md): two disjoint, spatially balanced correspondence sets, same essential estimator and thresholds, equal SO(3) midpoint of the resulting rotations. Retain the original full-feature fit and its quality; if either half lacks existing minimum support, use the original full-fit rate. This is not temporal output smoothing or a new confidence gate.

The cached clean-reference test improved pooled error by 4.02/10.94/5.05% on 0021/0027/0060, with 14/16 sections improving. Those are small numerical gains and do not establish visible improvement or generalization.

Two sections worsen: 0021 66.170–70.170 s by 6.05%, and 0060 18.944–22.944 s by 2.98%. The trial changes noisy-section tracking only; these clean-reference scores measure estimator accuracy, not rendered clean-zone changes. Preserve the regressions alongside the pooled gains.

`subset_ensemble_repair` reuses `gap_patch` unchanged, with an ensemble-tracker wrapper substituted only for noisy measurements. Calibration still calls the original one-frame production tracker. The wrapper verifies 100 fps, as used by this research. Severe padding, alpha partner, handback, accepted fixed-edge splice, 0.19 s ramps, original rebase gate and offset decay are unchanged. No partner-gate suppression or forced rebasing is combined with this trial.

The diagnostic full-fit rates/quality have already passed cache parity across 9981 acquired pairs. Four synthetic tests cover the partition, known rotation/translation and fast turns, insufficient support and shared-bias limitations. Subset-ensemble cache and repair examples pass release Clippy with warnings denied and build successfully. Optical coverage and transactional MP4 verification remain required before rendering.

## Status

The new 0073 repair completed with accepted optical coverage and zero timestamp/sign-folded quaternion writeback discrepancy. Original one-frame calibration reports R2=0.995, shift 0 ms. The subset average was used for 10384/10406 noisy pairs; 22 retained full-fit rates. All burst intervals and six rebase decisions match the previous gap2-edge candidate. Drift values and carried offsets can change.

Stored body-rate differences from gap2-edge are 1.491 deg/s RMS in the early window, 1.245 late, 0.00338 in the clean control, and 0.755 at 264–272 s. These are mechanism/change measurements, not physical errors or visual scores. The clean control is nearly unchanged in rate but is not exact quaternion parity because offsets/decay can differ.

The full render completed in 183.090 s. Both full comparison inputs passed 1440x810 / 100 fps / 37594-frame checks. Four excerpts passed frame counts (early 1600, late 1400, control 300, additional 800) and a decoded late-event layout was visually inspected. No repair, renderer or export process remains running for this trial.

Review baseline is `target/experiments/bounce-localize-v1/gap2edge-render.mp4`; new files live under `target/experiments/subset-ensemble-v1`. Full-render project settings match that baseline, with only input/output paths differing. Horizon lock off, no autosync, empty offsets, unchanged clip metadata/lens/output settings; no stale gyro metadata.

Completed comparisons: LEFT previous gap2-edge, RIGHT subset average; `compare_early.mp4` (14–30 s), `compare_late.mp4` (132–146 s), `compare_control.mp4` (55.9–58.9 s), `compare_additional.mp4` (264–272 s). Production and shipping binaries remain unchanged.

**User verdict, 2026-09-14:** “maybe a tiny bit better, but judder is still essentially identical in the late clip.” Record a possible tiny improvement with essentially unchanged late-clip judder, not a meaningful demonstrated fix. No separate early/control/additional verdict was supplied. Preserve the candidate and numerical gains, but do not promote it or increase ensemble size on this evidence alone.

The clean-reference score improvement has not translated into the desired perceptual benefit. Together with the negative partner-gate result and weak confidence diagnostics, this calls for reassessing shared systematic errors and the observable residual before another estimator/gate sweep. It does not prove optical estimation is accurate, identify a specific physical cause, or authorize resuming deferred lens/scaling or geometric-model expansion. Retain the accepted fixed-edge baseline and separate gap2 candidate.

## Reproduction

With the usual OpenCV environment:

```powershell
cargo build --release --offline -p o4core --example subset_ensemble_repair
target/release/examples/subset_ensemble_repair.exe sample_vids/DJI_20260829141435_0073_D.MP4 NEW_DESTINATION
```

Repair refuses an existing destination and non-100-fps source. `prepare.py` creates the fresh project/window list with exclusive writes. Launch local portable Gyroflow hidden with project path and `--stdout-progress`; use separate logs. `export_reviews.py` validates the full output before extracting and checking frame counts. Preserve prior source/reference/candidate files.
