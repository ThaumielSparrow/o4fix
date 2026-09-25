# Can existing optical diagnostics identify remaining measurement error?

2026-09-13. Read-only study after the partner-gate late comparison showed little/no meaningful visual improvement. No new correction, optical acquisition, production change, render, or shipping rebuild.

## Outcome

Neither the current quality score nor 10/20 ms span disagreement supplies a reliable gate for the remaining two-frame optical error on these controls. Do not use either to reject/interpolate rates, change handback, or choose another correction render from this study.

The existing quality value is `clamp((essential_inliers - 60) / 150, 0, 1)`, so it saturates at 210 inliers. It is a support-count heuristic, not an error probability, fit-residual bound, spatial-support measure, or uncertainty estimate. All 2041 scored 0060 samples have quality exactly one in both variants. In 0073 132–146 s, all 1400 one-frame measurements have quality one; both variants have quality one throughout 143.00–143.75 s. In contrast, the telemetry-clean 0073 55.9–58.9 s control has median quality only 0.517/0.507. This does not mean its optical rates are more accurate; it shows why confidence labels cannot order visual reliability without validation.

## Frozen reference and parity

Reused tracking-v1 gap observations and calibration-v1 leave-section-out transforms across 0021/0027/0060. Each section is scored with a transform fitted on the other sections; no calibration is refitted. Original one-frame values were already resampled to the two-frame midpoints in those caches. The same common quality>0.5 support, contiguous-run 5 Hz filters, real cadence, and 0.15 s boundary trims are retained.

`optical_reliability_score` exposes individual filtered predictions/reference rates from an isolated copy of the existing experimental calibration scorer. For every fold and variant, it checks sample counts and RMS against the original `CalibrationData` scorer to 1e-12. Common timestamps, run labels and gyro values match exactly between variants. The summarizer independently reaggregates per-sample errors and checks them against the fold scores to 1e-12.

All 16 sections completed, with 5622 scored pairs. Pooled baseline/gap2 RMS reproduces prior results exactly to reported precision: 2.349/1.764, 3.185/1.906 and 3.287/2.119 deg/s. This parity validates the diagnostic extraction; it is not a new improvement claim or proof of physical camera motion. Clean gyro is the reference, and the sample selection restricts the confidence range.

## Reliability association and negative controls

Spearman rank correlation of span disagreement with reference error:

| Clip | One-frame error | Two-frame error | Two-frame, ranks centered/scaled within each run |
|---|---:|---:|---:|
| 0021 | 0.382 | 0.083 | 0.055 |
| 0027 | 0.632 | 0.225 | 0.118 |
| 0060 | 0.515 | 0.170 | 0.020 |

Disagreement tracks the one-frame error more strongly than the remaining two-frame error. This is compatible with the already-known gap2 clean-score benefit, but disagreement shares estimator noise with error and does not independently establish accuracy. The weak within-run relationships show that some pooled association reflects differences between sections/runs. Section-level two-frame correlations include negative values on every clip; 0060 ranges from -0.380 to +0.368.

Circularly shifting the two-frame error within runs by 25/37.5/50/62.5/75% provides a temporal negative control, keeping at least 50 samples from either wrap direction. On matched 0060 support, unshifted correlation is 0.166; shifted values include 0.168, 0.198 and 0.257. Hence the apparent association does not robustly localize instantaneous remaining error there. These are sensitivity controls, not permutation p-values: filtering creates dependence, and circular wrapping is artificial. Complete matched-support results are saved in `summary.json`.

A leave-one-clip-out diagnostic learns the top-quintile span threshold and top-quintile gap2-error threshold from the other two clips. On the held-out clip:

| Clip | Flagged fraction | Large-error recall | Precision | Large-error prevalence |
|---|---:|---:|---:|---:|
| 0021 | 6.9% | 6.5% | 13.0% | 13.7% |
| 0027 | 27.6% | 51.4% | 28.0% | 15.0% |
| 0060 | 24.9% | 42.1% | 50.2% | 29.6% |

Transfer is inconsistent, particularly on 0021, and many large errors remain unflagged. Quality-based thresholds detect almost none because of saturation. Optical speed alone also associates with errors on 0027/0060, reinforcing the need to separate genuine fast motion from reliability. None of these thresholds is proposed for a repair: discarding samples would create unsupported gaps, and error/threshold samples are temporally dependent. The three clips and 16 sections are the meaningful replication units.

The prior 0073 span scores use 8 Hz filtering; do not compare them directly to this study's 5 Hz thresholds or infer severe-window accuracy using corrupted gyro.

## Next bounded investigation

The existing caches contain rates and capped quality, but not the uncapped feature/inlier counts, fit residuals, LK errors or feature coverage needed to diagnose this saturation. The next useful acquisition is a read-only instrumented copy of the **unchanged** tracker that retains these diagnostics and verifies rate/quality parity. Start with the same clean sections and 0073 late/control windows, using the original lens handling and resolution. Validate candidate reliability signals against frozen held-out clean references and time/motion controls before any gate or repaired video.

This is measurement instrumentation, not permission to expand the geometric estimator or revisit lens/scaling. Do not repeat the already-negative forward/backward gate, RANSAC seed change, partner suppression, or a wider temporal span without new evidence. Preserve the accepted edge-offset baseline and the separate gap2 candidate.

## Reproduction

With the OpenCV environment from SESSION_HANDOFF.md:

```powershell
cargo clippy --release --offline -p o4core --example optical_reliability_score -- -D warnings
cargo build --release --offline -p o4core --example optical_reliability_score
target/release/examples/optical_reliability_score.exe SOURCE CALIBRATION_JSON GAP_OBSERVATIONS_JSON NEW_OUTPUT
python docs/experiments/optical-reliability-v1/summarize.py
```

Inputs: `target/experiments/calibration-v1/{0021,0027,0060}/calibration.json`; tracking-v1 `0021-gap.json`, `0027-gap.json`, `gap-observations.json`; corresponding original source MP4s. Per-sample output lives under `target/experiments/optical-reliability-v1/{0021,0027,0060}.json`. Existing 0073 quality observations come from bounce-localize-v1/spans.json. Source and old artifact fingerprints remain in their prior manifests; new input/code/output hashes are in this directory's manifest.

Release Clippy with warnings denied and example build passed. All 32 fold/variant score-parity checks and per-sample reaggregation checks passed. Production tests were not rerun for this read-only example/docs change; existing nom future-compatibility warning remains. No candidate was generated for visual review.
