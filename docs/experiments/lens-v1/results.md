# Source residual patterns and fisheye inverse audit

September 10, 2026. The requested source residual study exposed an upstream numerical problem: OpenCV's default inverse is not reliable over the whole image for the supplied O4 distortion polynomial. A principal-branch experimental inverse fixes the tested round trips but does **not** improve all clean-clip motion scores. Production remains unchanged.

## Finding before spatial interpretation

The source lens profile is the existing telemetry profile, with focal length about 546.40271 pixels and coefficients [0.1551311, 0.1371409, -0.0938614, 0.0041704]. Its radial polynomial is not monotonic all the way to pi/2. The first turning point is theta = 1.4160276 rad (81.1324 degrees), with distorted normalized radius 1.6612378, or about 907.705 source pixels. This extends beyond the approximately 900-pixel image diagonal radius, so a principal-branch solution exists throughout the tested image rectangle.

A round-trip regression at source point (1310,40) demonstrates a default inverse failure: projecting its returned value gives approximately (135.973,-44.027), over 1177 pixels away. This is not a floating-point tolerance issue. On a uniform 73x55 grid within the 1440x1080 source image:

| Audit result | Grid points |
|---|---:|
| Total | 4015 |
| Default inverse returns sentinel-scale coordinates | 37 |
| Default inverse/projection round trip exceeds 0.1 pixel | 195 |
| Round trip succeeds, but solution is beyond the first monotonic branch | 51 |
| Experimental principal inverse has no solution | 0 |

The 37 sentinel cases are included in the 195 round-trip failures. Grid fractions are not fractions of video tracks. A successful round trip alone cannot distinguish multiple branches of a non-injective polynomial. Neither this audit nor principal-branch consistency independently verifies the physical camera calibration.

`support/lens_inverse.rs` brackets the first monotonic branch and uses bisection to invert it. It rejects points outside that branch's radius domain. A 475-point source-grid test round-trips to within 0.001 pixel. This is a bounded research implementation for the tested O4 profile, not a production-ready generic fisheye inverse: branch scanning and profile/domain validation need broader work before general reuse.

## Isolated motion comparison

`lens_tracking_probe.rs` changes only inversion (and excludes points with no valid principal-branch inverse) in an isolated copy of the established tracker. Feature acquisition, forward-only LK, one-frame gap, seeds, essential-matrix fitting and quality ramp otherwise retain the production path. It does not introduce persistent tracking, reverse gating or smoothing. Compare against cached production observations on exactly matching timestamps, preserving the saved leave-one-interval-out calibration matrix and shift.

| Clip | Baseline held-out RMS | Principal inverse RMS | Change | Common scored samples |
|---|---:|---:|---:|---:|
| 0021 | 2.354 deg/s | 2.249 deg/s | -4.45% | 1463 |
| 0027 | 3.197 deg/s | 3.196 deg/s | -0.05% | 2128 |
| 0060 | 3.293 deg/s | 3.656 deg/s | +11.01% | 2052 |

All 16 clean intervals were evaluated. Within-clip results are mixed; 0060 worsens in five of six folds. No corrected-inverse repair or render is warranted from these numbers alone. Calibration was deliberately frozen for this single-mechanism comparison; this is not a test of a refitted complete pipeline. A mathematically consistent inverse does not guarantee better motion estimation under the current noise model, feature weighting and physical-camera assumptions.

## Rebuilt spatial observations

Earlier source-pose caches retained normalized points but not original distorted pixels. Default inverse failures and alternate branches mean those caches cannot safely reconstruct actual source locations or establish clean geometric input. Their historical measurements remain measurements of those algorithms, but do not justify blanket conclusions about reliable source geometry. Preserve them; do not silently rewrite them or treat their failure as proof that all multi-frame methods fail.

`source_pose_principal_probe.rs` rebuilds all 6195 pairs across the same 16 clean sections, using principal inversion and retaining both original source pixel coordinates and normalized rays with persistent IDs. It otherwise follows the prior persistent diagnostic acquisition, which differs from production tracking. The new caches identify their inverse model explicitly.

`source_residual_probe.rs` requires original source_b pixels and verifies each retained point's inverse/projection round trip within 0.001 pixel before analysis. It refuses normalized-only caches. Fit an essential matrix on one persistent-ID parity group, evaluate the other, and swap. Correct the second normalized point toward its epipolar line, then distort it back to pixel coordinates. The residual vector is the measured pixel location minus that correction, reported in half-resolution source pixels. This is an **epipolar-normal displacement**, not full correspondence/reprojection error or a camera rotation accuracy measure; error along the epipolar line is not measured, and the closest point is defined in normalized coordinates rather than isotropic pixel distance.

Measure per-frame signed vertical residual versus source row, signed radial residual versus radius, and residual magnitude versus location. Cells with fewer than eight points and correlations with fewer than twenty remain missing. Scramble row/radius labels within each frame using 32 deterministic permutations. These are descriptive negative controls, not independent statistical replicates or calibrated p-values. Angular speed comes from the existing cleaned telemetry at the frozen per-fold shift and is used only for correlation, not to correct observations. Circularly shift speed by one-quarter, one-half and three-quarters of each interval as temporal controls.

## Spatial results

| Clip | Median of section median residual magnitudes | Range of section median vertical row slopes | Speed/error correlation range |
|---|---:|---:|---:|
| 0021 | 0.0965 half pixels | -0.0051 to -0.0001 | -0.049 to +0.132 |
| 0027 | 0.0809 half pixels | -0.0081 to +0.0783 | -0.041 to +0.182 |
| 0060 | 0.0921 half pixels | +0.0029 to +0.0192 | -0.070 to +0.403 |

Row slope is half pixels per full image-height change. Signed radial slopes also vary in sign across sections. There is no consistent signed row/radial pattern here that identifies a unique readout-time or calibration correction.

Residual magnitude correlates positively with radius in all 16 sections (section medians approximately 0.068-0.209); all exceed their location-scrambled median ranges. This supports a repeatable peripheral measurement effect, but not its cause. Lens-transform error scaling, epipolar-line orientation, scene depth/composition, tracking noise and physical-model error can all contribute. The current normalized-coordinate residual construction itself matters to interpretation.

Speed correlations are generally small; two 0060 sections reach about 0.403 and 0.298, while the clean 19-second section is negative. Time-shifted correlations are sometimes comparable or larger. Speed magnitude alone omits direction, acceleration and row-specific timing, so these controls neither prove nor rule out rolling shutter. The prior near-inconclusive rolling-shutter-off visual comparison remains unchanged. No gyro-derived correction is evaluated against the same gyro as if it were independent evidence.

## Decision and next bounded step

Keep all production defaults and shipping binaries. Do not promote principal inversion simply because its round trips are correct, or tune readout time from these correlations. No new repair or video comparison was produced.

Next, test **source-pixel-aware geometric error scaling** on the verified principal-branch observations. A fixed normalized-coordinate threshold represents different pixel errors across this lens. Derive the local lens Jacobians and use both image measurements when assessing epipolar error/uncertainty; verify against synthetic pixel noise before comparing held-out clean motion. Keep the corrected inverse fixed for that experiment and compare on identical observations, with missing coverage explicit. Do not introduce another flexible warp or temporal smoother. Refit calibration only as a separately labeled comparison if needed; retain the frozen-calibration result above.

## Verification and artifacts

13 test executions passed across lens_tracking_probe (2), source_pose_principal_probe (7) and source_residual_probe (4); the two inverse-helper tests are shared. They cover full-grid principal round trips, domain refusal, the known default edge failure, central round trips, epipolar-sign invariance, shuffled spatial controls, and the prior synthetic motion/degeneracy cases. Clippy with warnings denied passes for all four new examples. All 16 tracking intervals, 16 rebuilt source intervals and three residual-study runs completed. Every analyzed source point passed its round-trip check. No production suite or shipping rebuild was needed; only experimental example binaries were built.

New code: source_residual_probe.rs, lens_inverse_audit.rs, lens_tracking_probe.rs, source_pose_principal_probe.rs, support/lens_inverse.rs and support/lens_tracker.rs. Raw data live under target/experiments/lens-v1. Durable metrics.json contains all section statistics and negative controls; summarize.py regenerates it. The new source-cache metadata was annotated explicitly after acquisition; numeric observations are unchanged. Earlier caches, clips and working-tree changes are preserved.

Use the Cargo/OpenCV environment in SESSION_HANDOFF.md, then build the four examples. Each prints positional argument usage. Representative commands:

```powershell
target/release/examples/lens_inverse_audit.exe sample_vids/DJI_20260808151831_0060_D.MP4 target/experiments/lens-v1/grid-audit.json
target/release/examples/lens_tracking_probe.exe sample_vids/DJI_20260808151831_0060_D.MP4 docs/experiments/calibration-v1/0060.json target/experiments/calibration-v1/0060/observations.json target/experiments/lens-v1/0060.json
target/release/examples/tracking_score.exe sample_vids/DJI_20260808151831_0060_D.MP4 docs/experiments/calibration-v1/0060.json target/experiments/lens-v1/0060.json target/experiments/lens-v1/0060-scores.json
target/release/examples/source_pose_principal_probe.exe sample_vids/DJI_20260808151831_0060_D.MP4 docs/experiments/calibration-v1/0060.json target/experiments/calibration-v1/0060/observations.json target/experiments/lens-v1/0060-source.json
target/release/examples/source_residual_probe.exe sample_vids/DJI_20260808151831_0060_D.MP4 target/experiments/lens-v1/0060-source.json docs/experiments/calibration-v1/0060.json target/experiments/lens-v1/0060-residual.json
```

Repeat the last four commands for 0021/0027 before running `python docs/experiments/lens-v1/summarize.py`. Manifest hashes identify inputs, outputs and final experimental builds. No commit or publication was requested.
