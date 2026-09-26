# regions-v1: a render-free gate for refinement past the 4 deg cap (2026-09-26)

Goal: decide, without Gyroflow, when the source-frame probe's correction can be trusted in a
burst whose correction exceeds the 4 deg cap (judder83-v1: forcing it fixed 0083 1:53, but the
probe is wrong in 0021 powerloop/109.6 s and 0060 48.7 s).

`warp_residual_probe` gained an opt-in `O4_REGIONS=1` output: the same rotation-only fit on
image halves (top/bottom/left/right) and a translation-aware estimate of the same residual
(`ess`: essential matrix + smaller-angle decomposition on the telemetry-derotated bearings,
in the pair's mid camera frame, rotated to body). Scripts: `agree.py`, `label.py`.

## 1. Regional agreement: negative
In-burst 1-8 Hz, rms(top - bottom)/rms(full): good 0083 burst 1.09, bad 0.75-3.39. Half-image
fits are ill-conditioned (rotation vs translation ambiguity over a narrow field), so halves
disagree for any sizeable residual. Not a gate.

## 2. Probe vs translation-aware estimate: works at the top end
Labels from the render measurement (fbclip pipeline; 0021 all 31 bursts on the no-refine
render, 0083 capped bursts on the veto render, 0060 two capped bursts): label = pooled
1-8 Hz correlation probe~render. Predictor = pooled correlation probe~ess (render-free).
n = 41, corr(label, predictor) = 0.63 (correction magnitude: -0.26).
- predictor >= 0.75: 7 bursts, 6 with label 0.87-0.98 (probe right) and 0021 143.0 s at 0.62
  (under the cap, already refined in production). (Corrected 2026-09-26; first write-up said 6/6.)
- known-bad: 0021 powerloop 0.29, 0021 109.6 s 0.34, 0060 48.7 s -0.13.
- 0083 1:53 burst (user-validated B): 0.41 — not admitted by this rule.
- 0021 and 0060 have no capped burst >= 0.75, so a gate there changes nothing on them.

0083 capped bursts that pass: 48.82-51.70 (14.3 deg), 143.60-145.79 (15.5), 148.84-149.35
(11.3), 211.69-214.22 (crash, 61.6). Forced refinement on exactly those (veto build base,
`feedback_repair`), render-measured 1-8 Hz / 8-45 Hz wobble:

| burst | current | gated refinement |
|---|---|---|
| 0:48.8-0:51.7 | 41.6 / 8.7 | 12.0 / 6.9 |
| 2:23.6-2:25.8 | 86.4 / 22.2 | 35.7 / 24.1 |
| 2:28.8-2:29.3 | 43.5 / 23.0 | 16.7 / 20.8 |
| 3:31.7-3:34.2 crash | 90.1 / 59.9 | 42.2 / 20.6 |
| control 2:26.2-2:27.0 | 17.3 / 12.4 | 18.9 / 12.7 |

Review page `target/experiments/judder83/review-gate.html` (LEFT current default, RIGHT
gated). User verdict (2026-09-26): "gated refinement side looks noticably better for all
clips." (all three 0083 review sections: 0:48.8, 2:23.6+2:28.8, 3:31.7 crash). Caveat: the threshold was read off the same 41 bursts it is
judged on. Out-of-sample check: [regions-v2](../regions-v2/results.md) (held: 1/1 correct admit on 0076,
no false admits across 10 unseen capped bursts). The ess estimate is noisy on its own
(calm-section ess noise > probe noise) — use it only as an agreement check, not a correction.
