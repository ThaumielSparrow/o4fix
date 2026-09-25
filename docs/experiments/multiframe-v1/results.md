# Six-frame rotation / translation / depth prototype

September 10, 2026. Implemented the first joint multi-frame experiment requested after source-pose-v1. The prototype does not pass its first clean control. No production change, repair or correction render was made.

## Model and scope

`o4core/examples/multiframe_pose_probe.rs` assembles six consecutive calibrated source frames (five pairs, 50 ms) from the preserved normalized tracks. Persistent IDs must survive across all five pairs and their shared-frame coordinates must agree; time gaps are not bridged. A deterministic hash subsample selects at most 90 tracks from each ID-parity group, at most 180 total. This avoids fixed-stride subsampling accidentally emptying one parity group.

Fix the first camera at identity. Independently optimize all five remaining camera rotations and translations plus one shared inverse depth per feature. The first observed bearing of each landmark is held fixed. Therefore this is an inverse-depth bundle prototype, **not** full bundle adjustment with freely optimized 3D landmark bearings. All observation errors at the first frame are inherited as exact anchors; that approximation remains a possible source of bias.

An endpoint essential matrix and positive-depth recovery initialize the five poses and feature depths. The endpoint pose is interpolated only for initialization. There is no temporal penalty or constant-speed constraint in the objective. Output the central adjacent-frame rotation at the original 100 Hz timestamp, not a wider-gap average. Huber loss uses the pre-existing 0.002 normalized-coordinate scale. Optimize with damped Gauss-Newton and Schur elimination of landmark depths; remove the translation/depth scale gauge by normalizing endpoint translation. This is a local solver and may reach a local minimum. A small residual or convergence flag does not establish observability or correct physical motion.

Separate parity-group solves estimate camera poses independently. Opposite-group features are evaluated with those poses held fixed and only their depth estimated. Missing fits and unsupported regions are retained. Diagnostic region bins are in normalized-coordinate bands, explicitly not the rendered-image grid or sensor-row timing. No correction gate is selected from these diagnostic residuals.

## Numerical issues addressed before interpretation

The initial direct inverse-depth implementation often stalled at the positivity boundary; one invalid initial projection could also abort an entire window. Its source snapshot and outputs are preserved in target/experiments/multiframe-v1, including linear_depth_source.rs and 0060-control.json. It produced 328 finite iterates and 175 converged fits from 399 requested pairs, with only 39 scorer samples available. Those incomplete numbers were not taken as evidence of improvement.

The final prototype optimizes log inverse depth to preserve positivity, excludes initially infeasible feature projections while reporting accepted-depth counts, and allows 60 iterations instead of 30. These are combined solver changes, not individually attributed ablations. Synthetic recovery and derivative tests pass after the changes. Final finite-iterate coverage rises to 390/399 and convergence to 339/399. A fit is marked converged only after sufficiently small accepted cost/step change or effectively zero cost; max-iteration and damping failures stay unconverged. No missing fit is substituted with zero motion.

## First real control

Use only the existing clean 0060 calibration interval 18.9438-22.9438 s, containing the verified 19-22 s visual control. Saved tracks supply context; 399 central pairs are requested. Freeze that interval's existing leave-one-interval-out calibration matrix and shift. Use the established cleaned-telemetry reference and contiguous-run scorer with the same observations for each candidate/reference comparison.

| Final solver result | Value |
|---|---:|
| Finite iterates | 390 / 399 |
| Converged fits | 339 / 399 |
| Longest consecutive converged run | 58 pairs |
| Both independent group fits finite | 355 / 399 |
| Both independent group fits converged | 288 / 399 |
| Median independent rotation disagreement | 1.524 deg/s |
| Median final/initial objective ratio | 0.350 |

The following scores include unconverged **finite iterates** and are exploratory diagnostics, not accepted output-quality results:

| Common-support comparison | Reference RMS | Prototype RMS | Scored samples |
|---|---:|---:|---:|
| Persistent pairwise essential | 3.741 deg/s | 4.322 deg/s | 318 |
| Cached production | 2.903 deg/s | 4.278 deg/s | 313 |

Thus lowering the objective and improving numerical coverage do not establish better telemetry consistency. The prototype's finite iterates are about 47% worse than production on the common scored support. Each row uses its own common support and should not be compared across rows as if timestamps were identical.

Requiring convergence leaves no contiguous run long enough for the established scorer (the longest is 58 pairs, below its 0.6-second run requirement). Both reference and candidate scores then remain null. Do not fill gaps, relax the scorer, or present the finite-iterate score as validation of a converged solution. The 339 converged pairs alone cannot establish a reliable improvement. These results reject this prototype for repair/render use; they do not rule out all multi-frame methods.

## Validation

Seven release-mode synthetic tests pass: analytic pose/depth derivatives versus finite differences; known nonconstant rotation/translation recovery; image-only initialization and recovery; fast-turn recovery with noise and outliers; translation-depth scale gauge invariance and exact-solution handling; pure-rotation depth non-observability; and refusal to join cached tracks across time gaps. Fast-turn testing concerns synthetic correspondences, not real LK tracking or a rendered fast-turn comparison.

Clippy with warnings denied passes for the solver and scorer. All final finite-iterate rates are checked finite by the summary script. No shipping code changed, so the full production regression suite and shipping rebuild were not run. Final example binaries were rebuilt after test additions and a type alias; runtime solver logic matches the final control run. Old source clips, binaries, experiments and working-tree changes are preserved.

## Decision and next bounded work

Do not broaden the run to complaint bursts or render a correction yet. The clean control has neither a better telemetry result nor sufficient converged coverage.

Next investigate the solver's noise model and conditioning before testing another stabilization mechanism: allow each landmark's anchor bearing to adjust with an observation residual rather than treating the first measurement as exact, and inspect unregularized pose observability after eliminating landmarks. Compare initialization sensitivity on the same control; damping convergence must not be mistaken for geometry being observable. These are hypotheses about the prototype, not established explanations for the original judder. Preserve the independent feature groups and frozen telemetry calibration. A full landmark-refining solver is not implemented yet.

## Reproduction and artifacts

Use the Cargo/OpenCV environment in SESSION_HANDOFF.md:

```powershell
cargo test --release --offline -p o4core --example multiframe_pose_probe
cargo build --release --offline -p o4core --example multiframe_pose_probe --example multiframe_score
target/release/examples/multiframe_pose_probe.exe target/experiments/source-pose-v1/0060.json 18.94 22.95 target/experiments/multiframe-v1/0060-control-logdepth.json
target/release/examples/multiframe_score.exe sample_vids/DJI_20260808151831_0060_D.MP4 docs/experiments/calibration-v1/0060.json target/experiments/multiframe-v1/0060-control-logdepth.json target/experiments/multiframe-v1/0060-control-logdepth-scores.json
python docs/experiments/multiframe-v1/summarize.py
```

The summary includes both initial and final prototypes and therefore requires their preserved raw outputs. Durable metrics, summary code and hashes are stored here; raw iterates and the initial source snapshot are under target/experiments/multiframe-v1. The source-pose-v1 input fingerprint is checked against its manifest. No commit or publication was requested or made.
