# Uncapped tracking diagnostics with unchanged-estimator parity

2026-09-13. Read-only follow-up to optical-reliability-v1. The accepted edge-offset repair and separate gap2 candidate remain preserved. No production change or new repair/render.

## Acquisition and validation

`tracking_diagnostic_probe` uses an isolated copy of the existing tracker. It retains uncapped detected/tracked/inlier counts, inlier fraction, median LK photometric error, median unsquared Sampson distance on all tracked points and on inliers, inlier bounding-box fraction, and a 4x3 inlier occupancy grid. These observations do not feed back into feature selection, the essential fit, pose-branch selection or quality.

Feature detection, half-resolution images, original lens inversion, LK settings, essential-matrix parameters, seed, pose branch and pair spans are unchanged. This is instrumentation, not geometric-estimator expansion or lens/scaling work. The fit residual uses the already-estimated essential matrix and its own points; it is an optimistic in-sample diagnostic, not independent prediction error or pixel-space uncertainty.

Completed 9981 frame pairs:

| Clip | Span | Acquired pairs | Maximum rate difference from saved cache, rad/s |
|---|---:|---:|---:|
| 0021, four clean sections | 20 ms | 1579 | 2.22e-16 |
| 0027, six clean sections | 20 ms | 2312 | 2.22e-16 |
| 0060, six clean sections | 20 ms | 2288 | 4.44e-16 |
| 0073, late plus clean control and context | 10 ms | 1902 | 4.44e-16 |
| 0073, same windows | 20 ms | 1900 | 4.44e-16 |

Maximum quality discrepancy is 1.11e-16. Every acquired timestamp matched its cache. The summarizer independently checked complete expected 100 fps timestamp sequences and lengths for each requested interval, inlier-grid sums, count ordering, and reconstruction of the original capped quality. Release build and Clippy with warnings denied passed; existing nom future-compatibility warning remains.

The clean diagnostics join all 5622 previously scored samples on exact rounded timestamps. Their reference errors retain the frozen leave-section-out calibration and common-support 5 Hz scoring from optical-reliability-v1. No transform or error target is refitted here.

## What the extra data shows

There is no obvious shortage of tracked support in the late target. For 0073 132–146 s with 20 ms pairs, median support is 558 inliers, inlier fraction 0.943, bounding-box fraction 0.865 and 11 of 12 occupied image cells. Around 143.00–143.25 s, medians are 576 inliers, fraction 0.962, bounding-box fraction 0.892 and 11 occupied cells. These are count/coverage diagnostics, not proof of correct rotation.

The telemetry-clean 55.9–58.9 s control has only 136 median inliers and seven occupied cells, but a higher inlier fraction of 0.969. Different scene content strongly affects support. Therefore neither capped confidence nor uncapped count is a direct ordering of motion-estimation accuracy.

Within-run rank correlations with the remaining two-frame clean-reference error:

| Hypothesized risk signal | 0021 | 0027 | 0060 |
|---|---:|---:|---:|
| Fewer inliers | -0.167 | 0.145 | -0.076 |
| Lower inlier fraction | -0.015 | 0.039 | 0.069 |
| Larger LK error | 0.018 | 0.006 | 0.026 |
| Larger median Sampson distance | 0.045 | 0.078 | 0.121 |
| Smaller inlier bounding box | 0.022 | 0.032 | -0.108 |
| Fewer occupied cells | 0.155 | 0.097 | 0.051 |

Taking a predeclared +/-0.10 s median of each diagnostic within the same scored run does not make these relationships strong or consistent. For example, locally aggregated Sampson-distance correlations are 0.058/0.141/0.118. Counts and coverage sometimes have the opposite sign from the proposed risk interpretation. Optical speed retains a stronger association on 0027/0060 (0.256/0.208), so selection based on these metrics could conflate real motion with error.

Leave-one-clip-out thresholds also fail to transfer reliably. A local Sampson-distance threshold trained on the other clips catches only 3.8% of the large errors on 0027, with 9.2% precision versus 15.0% prevalence. A local LK-error threshold flags no 0027 samples at all, but flags 50.5% of 0060. All signal results, matched-support circular time-shift controls and 0073 quarter-second distributions are retained in `summary.json`; no predictor was chosen or tuned into a gate.

## Decision and next step

Do not introduce count-, LK-error-, fit-residual- or coverage-based rejection/interpolation from these results. The extra diagnostics confirm that the confidence ceiling was hiding variation, but that variation does not reliably identify remaining rotation error across these controls. The read-only study does not establish an optical root cause for the visible residuals.

A more discriminating next diagnostic is **rotation stability across independent spatially balanced feature subsets**, using the same essential estimator and unchanged image/lens handling. Compare subset disagreement against the same held-out clean references, with exact unchanged full-fit parity and synthetic rotation/translation controls. This tests whether a dense, apparently well-fitting correspondence set nonetheless produces an unstable rotation estimate; it is different from changing a RANSAC seed or repeating the negative forward/backward gate. Do not change repair behavior or expand to a new geometric model until that diagnostic establishes something useful. If this stability signal also fails, reassess what is identifiable from the current footage rather than stacking weak confidence gates.

## Reproduction and artifacts

Code: `o4core/examples/tracking_diagnostic_probe.rs`, `support/diagnostic_tracker.rs`. Set the OpenCV environment from SESSION_HANDOFF.md and build the example. Positional usage:

```powershell
target/release/examples/tracking_diagnostic_probe.exe SOURCE CACHE_JSON NEW_OUTPUT
python docs/experiments/tracking-diagnostics-v1/summarize.py
```

The three clean inputs are preserved as `target/experiments/tracking-diagnostics-v1/{0021,0027,0060}-input.json`: copies of the previous gap caches with `probe_intervals` restricted to exact matching calibration sections. In particular, 0060's four severe windows were excluded from this clean acquisition. 0073 uses the existing bounce-localize-v1/spans.json intervals (131.5–146.5 and 55.4–59.4 s), in both spans. Run logs and output JSONs remain in the same target directory. `manifest.json` fingerprints new code, diagnostic acquisition and reference-error inputs.

No acquisition processes remain running. Production tests and shipping binaries were not rebuilt because only read-only research examples/docs changed. Preserve original footage, all candidates and earlier diagnostic outputs.
