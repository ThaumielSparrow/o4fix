# regions-v2: out-of-sample check of the regions-v1 cap gate (2026-09-26)

Question: does the regions-v1 gate (admit a cap-skipped burst to refinement when the pooled
1-8 Hz correlation between the rotation-only probe and the essential-matrix estimate is
>= 0.75) hold on clips it was not fitted on?

Clips: every clip not used for the fit, repaired with the current main build (veto on,
refinement on, 4 deg cap): 0076, 0057, 0058, 0059 (`repair.sh`). 0027/0071/0073 have no
cap-skipped bursts under this build; 0057/0059 have none either. That leaves **10 unseen
capped bursts** (0076: 7, 0058: 3). Small sample.

Method (`gatecheck.py`, driver `pipe.sh`; outputs `target/experiments/regions-v2/`): probe with
`O4_REGIONS=1 O4_CROP=1.30,0.72` on each capped burst +-2 s (reproduces the regions-v1
`src-*.json` bit-for-bit). The label is the render-measured probe~render correlation on the
current-default render. Outcome: forced refinement on all capped bursts (`all`) and on
admitted bursts only (`gate`), `feedback_repair` + Gyroflow render, render-measured
1-8 Hz / 8-50 Hz wobble (deg/s). Render measurement noise: on bursts the `gate` variant never
touched, `gate` vs `cur` still moves by up to 4.3 deg/s, so |delta| < ~5 is inconclusive.

| burst | agreement | label | cur | forced | verdict for forcing |
|---|---|---|---|---|---|
| 0076 153.74-154.69 (13.8 deg) | **0.88 ADMIT** | 0.87 | 87.0 / 23.9 | 40.1 / 22.8 (gate) | big win: admitted correctly |
| 0076 275.09-275.59 | 0.66 | 0.86 | 18.3 / 9.1 | 11.4 / 9.0 | better (miss) |
| 0058 345.07-346.36 | 0.47 | 0.68 | 38.1 / 11.1 | 27.9 / 8.6 | better (miss) |
| 0076 290.49-291.31 | 0.39 | 0.51 | 54.0 / 33.3 | 51.0 / 23.8 | inconclusive (8-50 better) |
| 0058 41.25-42.12 | 0.06 | 0.21 | 60.3 / 51.2 | 54.8 / 55.3 | mixed |
| 0076 16.35-16.78 | -0.17 | -0.52 | 24.0 / 36.8 | 27.1 / 33.0 | inconclusive |
| 0076 43.58-44.30 | 0.16 | -0.26 | 44.7 / 35.7 | 50.7 / 37.4 | worse: rejected correctly |
| 0076 150.33-151.45 | 0.18 | -0.36 | 30.8 / 49.1 | 36.0 / 52.8 | worse: rejected correctly |
| 0076 258.83-259.61 | 0.04 | 0.57 | 39.9 / 18.4 | 45.5 / 25.1 | worse: rejected correctly |
| 0058 255.06-255.92 | 0.44 | -0.22 | 7.9 / 9.2 | 15.6 / 10.1 | worse: rejected correctly |

Controls (non-burst, n >= 300) move <= 2 deg/s in every variant.

Result: **the gate held out of sample.** Its one admission is a correct, large win. No
false admits. 4 of the 9 rejections would clearly have hurt. 2 were misses where forcing
would have helped (0.66 and 0.47 agreement), 3 are inconclusive. corr(label, agreement) on
the unseen bursts = 0.69 (in-sample 0.63; pooled n = 51, 0.64). Every burst ever admitted at
>= 0.75 has label >= 0.62 (8 bursts). The gate is conservative. Lowering the threshold to
catch the two misses would also admit 0058 255.06 (0.44, forcing clearly worse), so keep 0.75.

Review page (LEFT current default, RIGHT gated): `target/experiments/regions-v2/review-gate-0076.html`.
User verdict (2026-09-26): "honestly i dont see much difference between the two, maybe the right side looks a little smoother, but the left doesnt have much judder/vibration to begin with."
So the 87 -> 40 deg/s render-measured drop was barely visible on this burst: safe, but not a
visible win here (unlike 0083, where all three sections looked noticeably better).

## Clip 0005 (added 2026-09-26, user-flagged residual judder at 0:18-0:30)

Current build: 101 severe bursts (120 s). Three are capped. The user's 0:18-0:30 complaint is
exactly one 13 s burst, 17.31-30.26 s, skipped at 5.06 deg (just over the cap).

| burst | agreement | label | cur | forced | |
|---|---|---|---|---|---|
| 17.31-30.26 | **0.96 ADMIT** | 0.98 | 10.8 / 5.8 | 3.4 / 3.3 | correct admit, both bands improve |
| 2.89-3.40 | 0.21 | 0.15 | 3.3 / 18.7 | 13.0 / 9.9 | correct reject (1-8 Hz 4x worse if forced) |
| 327.96-328.60 | 0.19 | -0.18 | 14.7 / 11.1 | 40.5 / 10.6 | correct reject |

Review: `target/experiments/regions-v2/review-gate-0005.html` (LEFT current, RIGHT gated).
User verdict (2026-09-26, 0005): "much better than before in terms of judder, this clip doesnt have much choppiness, ive just noticed it in general in certain clips post-fix."
The choppiness question is open: 0005 is not a choppy clip. Next, get the clips/timestamps where the user sees it.

## Choppiness ("looks like the frame rate dropped" in repaired areas)

- The source cadence is exact: 0005 15-32 s has 1700 packets at 10 ms, no gaps, and no
  near-duplicate frames.
- Consecutive source frames at 0005 24 s jump 100-150 px back and forth, and every frame is
  sharp. The camera physically shakes at close to half the frame rate, so the stabilizer must
  cancel each frame on its own. Consecutive frames of the current (unrefined) render show a
  stall, a 33 px jump and a step back, which is the dropped-frame look. In the gated render
  the same frames move more evenly, and the render-measured 8-50 Hz band falls 5.8 -> 3.3.
- Caveat: inside blurry bursts the render tracker's high band is mostly tracking noise.
  `render_residual_probe` with `O4_HALVES=1` (new, opt-in) fits the left and right halves on
  0076. At 8-45 Hz the halves are uncorrelated in bursts (the common part is about the same
  size as the left-right difference), while clean flight is dominated by the common part.
  Do not read small 8-50 Hz differences in bursts as real.

## All-burst check (7 unseen clips, --no-refine, 2026-09-26)

`nr.sh`: 0076/0058/0057/0059/0027/0071/0073 repaired with `--no-refine`, so every burst is
unrefined. Probe and render were measured and labelled for every burst (144 bursts; 0058 re-run
after the clip-end fix). `pool.py` pools them. Besides the correlation label, a refinement-outcome
proxy is computed: the predicted rms(render - probe)/rms(render). On the 13 capped bursts with
real forced renders it tracks the actual forced/cur ratio closely and always gets the direction
right (0.49 vs 0.46, 0.25 vs 0.31, 2.68 vs 2.75, 4.93 vs 3.91, ...).

| group | n | median proxy | helps (<0.9) | neutral | hurts (>1.1) |
|---|---|---|---|---|---|
| admitted by gate (>= 0.75) | 39 | 0.69 | 30 | 8 | 1 |
| refined today (under cap), agreement < 0.3 | 26 | 1.09 | 4 | 10 | 12 |
| refined today, 0.3-0.5 | 35 | 0.91 | 17 | 11 | 7 |
| refined today, 0.5-0.75 | 35 | 0.81 | 24 | 9 | 2 |

Gate verdict: holds on 144 unseen bursts. 30/39 admissions are predicted to help. The one
predicted harm (0073 252.2 s, +20%) is under the cap and already refined today. On capped
bursts with real renders (0076/0058/0005): 2 admits, both large wins; 0 false admits.

Side finding (proxy only, not yet render-verified): under the cap, production refines
unconditionally, and low-agreement bursts (< 0.3) are predicted to get worse more often than
better (12 vs 4). These are small residuals (median 5.7 deg/s render). Candidate: skip refinement
below an agreement floor. It needs forced-off renders and a user review before any change,
because refinement itself had positive visual verdicts (feedback-v1 wp1c, release-v013).

## Production port (branch `refine-cap-gate`, 2026-09-26)

`RefineConfig.cap_gate: Option<f64>` (default `Some(0.75)`). When the gate is on, `measure_chunk`
fits an essential matrix per pair on the telemetry-derotated bearings (`essential_residual`,
per-pair seed `1_000_000 + frame`). `signal::agreement` computes the pooled 1-8 Hz correlation
with the rotation-only residual inside the burst. `plan()` admits an over-cap burst when
agreement >= threshold. CLI flags: `--refine-cap-gate AGREEMENT` and `--no-refine-cap-gate`.
The GUI follows the core default (like the handback veto). Log: "residual correction X deg
(over cap, admitted: agreement 0.95)" / "not refined: correction X deg over cap (agreement 0.21)".

Checks:
- Decisions match research. 0083 refines the same four bursts (0.95/0.85/0.94/0.84) and
  rejects 1:53 (0.37). 0005 admits 0:17-0:30 (0.95) and rejects both others. 0076 admits
  153.7 s (0.85) and also 275.1 s (0.83; research 0.66, and its forced render helped
  18.3 -> 11.5). All known-bad bursts are rejected: 0021 powerloop 0.25, 26.4 s 0.20,
  109.6 s 0.21, 148.2 s 0.28; 0060 48.7 s -0.06, 191.7 s 0.38. The production window margin
  is 1 s (research 2 s), which explains the small shifts in agreement.
- Production 0076 render: 153.7 s 87.0 -> 39.6, 275.1 s 18.3 -> 11.5, the rest within noise.
- `gatediff.py`, 0083 gate on vs `--no-refine-cap-gate`: body rates outside the admitted
  windows differ by at most 0.015 deg/s (float32 rounding of the carried offset).
- Runtime (0083, idle machine): gate 97.2 / 97.6 s, no gate 98.7 / 95.9 s. No measurable cost.
- Unit tests (3 new plan tests plus config validation), clippy and fmt pass. The research
  parity test pins the gate off and is unchanged (0.017 / 0.046 deg).

## Agreement floor under the cap (step 2, 2026-09-26)

Production refines every under-cap burst unconditionally. Test: from the `--no-refine`
repairs of 0058/0057/0073, research-path refinement of every burst production refines today
(`refined`) vs the same minus bursts with agreement < 0.3 (`floor`). Both were rendered
(`floor.sh`). 14 bursts differ; render 1-8 / 8-50 Hz deg/s:

| burst | agreement | unrefined | refined today | skipped |
|---|---|---|---|---|
| 0073 178.84 | -0.00 | 2.9 / 3.9 | 16.8 / 4.4 | **2.8 / 3.8** |
| 0058 206.87 | -0.03 | 3.5 / 6.9 | 19.7 / 8.6 | **4.5 / 7.4** |
| 0058 318.45 | -0.01 | 5.5 / 6.9 | 10.3 / 7.7 | **4.1 / 8.7** |
| 0073 9.73 | 0.29 | 2.9 / 4.7 | 5.3 / 3.5 | **3.0 / 4.5** |
| 0057 45.34 | 0.21 | 22.1 / 18.5 | 23.7 / 20.7 | 17.5 / 23.3 (mixed) |
| 0058 201.71 | 0.08 | 5.4 / 5.2 | **4.0 / 4.2** | 5.2 / 4.1 |
| 0073 220.20 | 0.11 | 4.9 / 5.1 | **3.3 / 4.2** | 4.9 / 5.1 |
| 7 others | 0.22-0.29 | | | within +-2 |

Controls moved <= 1.3. The big harms all sit at agreement ~0: there, refinement adds
6-14 deg/s of fake 1-8 Hz motion to bursts that were already calm (2-5 deg/s). Skipping
loses ~1.5 deg/s on two bursts. Caveat: the measurement noise on untouched monster bursts can
reach ~12 deg/s (0058 41.3 s), but the harms above are on calm bursts, where it is small.

Review page (both steps): `target/experiments/regions-v2/review-capgate-floor.html`.
User verdict (2026-09-26, review-capgate-floor): "clips 1-3: looks slightly better / clip 4 and 5: difference is minimal" (1 = production gate 0076 275.1 s; 2-3 = floor 0073 178.8, 0058 206.9; 4-5 = floor 0058 318.5, 0057 45.3).
The floor is not implemented in production yet.

## Next idea (backlog): partial refinement

See the backlog entry in `docs/SESSION_HANDOFF.md` (per-axis / time-local / consensus correction for
over-cap bursts the gate rejects).

## Agreement floor in production (2026-09-26)

After the review ("clips 1-3: looks slightly better, clip 4 and 5: difference is minimal"),
the floor was added: `RefineConfig.agreement_floor` (default 0.3; CLI
`--refine-agreement-floor` / `--no-refine-agreement-floor`). Under the cap, a burst whose
agreement is measurable and below the floor is left unrefined ("not refined: measurements
disagree (agreement X)"). The essential-matrix estimate is measured whenever either rule is on.
Production skip sets (1 s windows, so agreement values differ from the 2 s research ones):
- 0058: skips 9.04, 80.00, 196.54, 206.87, 213.18, 226.90, 247.37, 318.45. Versus the tested set it
  adds 9.04 (refining hurt 2.0 -> 6.7), 80.00 and 196.54 (neutral). It no longer skips 120.86
  (neutral) or 201.71 (refinement helped).
- 0073: skips 9.73, 178.84, 188.73. It keeps 220.20 (refinement helped) and 275.02 (neutral).

## Remap: where judder remains after the cap gate + floor (2026-09-26)

`remap.py` runs the final build on 9 clips, renders each with Gyroflow, measures the whole render
(`render_residual_probe`, 0.5 s bins, 1-8 Hz after intended-path removal) and classifies every bin
by production state. `outside.py` explains the worst bins outside bursts. Only reliable bins
(>= 150 tracker inliers, 5733 of 5940) count. "Excess" is the sum of max(0, wobble - 8 deg/s):

| state | bins | median | p90 | share of excess |
|---|---|---|---|---|
| outside bursts, > 1.5 s from any burst | 4100 | 1.3 | 5.8 | **43%** |
| outside bursts, < 1.5 s from a burst | 720 | 2.1 | 8.8 | 18% |
| refined | 702 | 2.9 | 9.8 | 16% |
| over cap, rejected by the gate | 33 | 11.4 | 35.4 | 10% (worst per bin) |
| floor-skipped | 127 | 4.4 | 13.2 | 6.5% |
| refined past cap | 46 | 3.0 | 21.3 | 5% |

About 60% of the remaining excess is outside the repaired bursts, concentrated in 0076 (and 0083).
Of the 109 worst outside bins (> 15 deg/s): source gyro noise (30-180 Hz band RMS) is 1.5-5 in 72
(the light-blend zone, where the gyro is trusted), 5-8 in 12, >= 8 in 9 (brief spikes that
never became a burst), and clean in 16. 70 of 109 are at moderate rates (< 150 deg/s), so the
bins are mostly not tracker failure on fast motion. Hypothesis: on the noisier Sep-19 clips, phantom
motion in the light-noise zone leaks into 1-8 Hz. The 0021-era fact "mild-zone 2-8 Hz gyro is
trustworthy" may not hold there. Next test: probe these bins with the source-frame probe and
check probe~render and probe~essential agreement. Where both are high, the agreement-gated
refinement could be extended to noisy non-burst stretches.
