# Independent feature-subset rotation stability

2026-09-13. Same essential estimator and original image/lens handling; no production changes. This follows the failure of capped confidence and uncapped tracking diagnostics to provide a useful reliability gate.

## Method and safeguards

After the unchanged full-feature fit, split all successfully tracked correspondences into disjoint groups. Alternate assignments within a 4x3 source-image grid, offsetting the starting side by cell index to balance counts and feature-strength ordering. Do not select subsets using the full fit's inlier mask. Each subset uses the original essential-matrix parameters, minimum 60 inliers and smaller-angle rotation-branch rule. Both subset fits use the same fixed seed; the next full fit resets its original per-frame seed.

Store subset rotations, track/inlier counts and exact relative-rotation angle, divided by the pair duration for a deg/s diagnostic. Missing subset support remains unavailable, not zero disagreement. Disjoint features do not imply independent image noise, lens/model error, or scene evidence.

Four tests pass: disjoint complete partition, known rotation/translation including a fast turn, missing-support handling, and a shared-bias counterexample. In the latter, both fits agree closely despite a common observation bias; agreement cannot certify accuracy. Release Clippy with warnings denied and example build pass. Initial syntax errors in the new research files were corrected before these tests or any acquisition; no parameter tuning was involved.

## Acquisition and reference checks

All 9981 requested pairs completed. Full-fit rates/quality match prior caches within 4.44e-16 rad/s and 1.11e-16 respectively. Requested timestamps and frame-pair counts were checked independently. Both subsets are valid for 1579/1579 pairs on 0021, 2312/2312 on 0027, 2264/2288 on 0060, and 1862/1902 one-frame / 1854/1900 two-frame pairs on 0073. All 5622 previously scored clean-reference samples have valid subset estimates; missing 0060 pairs fall outside that scored support.

Existing frozen leave-section-out transforms, common accepted support, contiguous-run 5 Hz reference filtering and boundary trims remain unchanged. No calibration refit or severe-gyro ground truth is introduced.

## Stability signal does not justify a gate

Within-run rank correlations with remaining two-frame clean-reference error:

| Clip | Raw subset disagreement | +/-0.10 s median disagreement |
|---|---:|---:|
| 0021 | 0.021 | 0.035 |
| 0027 | 0.073 | 0.203 |
| 0060 | 0.002 | -0.110 |

The positive result on 0027 does not transfer consistently. On 0060 the local signal's pooled correlation is 0.027, while circular time-shift controls can be larger. With thresholds trained on the other two clips, local disagreement recalls 15.0%, 51.4% and 33.7% of large errors on 0021/0027/0060, respectively. These are exploratory associations on temporally dependent data, not calibrated error probabilities.

0073 two-frame median disagreement is 4.405 deg/s in the late window versus 4.899 in the clean control. All 1400 late samples have both halves valid, compared with 254/300 in the control; differing scene support prevents a simple accuracy ranking. At 143.50–143.75 s, where apparent output motion was prominent, median disagreement is only 3.329 deg/s. No subset-disagreement gate is proposed.

## Separate cached test: equal averaging

Because the subset rotations were already saved, a separate frozen test averages the two rotations at their SO(3) midpoint, with original full-fit fallback if either subset lacks support. This is not a confidence gate or temporal smoothing. It uses the same original common quality masks and leave-section-out calibration. Baseline two-frame scores reproduce the prior values to 1e-12.

| Clip | Full gap2 RMS, deg/s | Subset average RMS | Change | Improved sections |
|---|---:|---:|---:|---:|
| 0021 | 1.76409 | 1.69314 | -4.02% | 3/4 |
| 0027 | 1.90555 | 1.69717 | -10.94% | 6/6 |
| 0060 | 2.11937 | 2.01242 | -5.05% | 5/6 |

This modest benefit in 14/16 clean sections justifies a bounded visual trial, not promotion. It does not contradict the weak reliability result: averaging estimates and using disagreement to identify errors are different interventions. The trial retains two-frame noisy tracking, original one-frame calibration, accepted fixed-edge splice, rebase/decay rules and all coverage safeguards. See [subset-ensemble-v1](../subset-ensemble-v1/results.md) for its repair/render and user-verdict status.

The two worse sections are 0021 66.170–70.170 s (1.805 to 1.914 deg/s, +6.05%) and 0060 18.944–22.944 s (2.334 to 2.403 deg/s, +2.98%). The latter includes the known clean 0060 control. These are optical-estimation regressions on clean reference data, not a claim of a rendered regression where original clean gyro would remain in use. They limit any general improvement claim.

## Reproduction

With the usual OpenCV environment, build/test `subset_stability_probe`. Run it with `SOURCE CACHE NEW_OUTPUT`, using the tracking-diagnostics-v1 clean input caches and bounce-localize-v1/spans.json for 0073. The new probe uses `support/subset_tracker.rs` and `support/subset_rotation.rs`; previous diagnostic code and outputs are preserved.

`subset_ensemble_cache SUBSETS ORIGINAL_GAP_CACHE NEW_OUTPUT` creates the averaging observations for the existing `tracking_score` example. `summarize.py` preserves cadence/parity, association, time controls, cross-clip thresholds, clean averaging scores and input/code/output hashes. Raw data and logs live in `target/experiments/subset-stability-v1`.

No acquisition process remains running for this diagnostic. Production tests/release applications were not rebuilt; only research examples changed. Existing nom future-compatibility warning remains.
