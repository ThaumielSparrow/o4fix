# partial-v1: partial refinement (2026-09-26)

Question: refinement is all-or-nothing per burst. Can a partial correction beat both "none" and
"full" on bursts the cap gate rejects or the agreement floor skips, without hurting bursts the
gate already handles well?

## Step 1: render-free screen (`screen.py`, output `target/experiments/partial-v1/screen.json`)

Data: the regions-v2 `--no-refine` runs (7 clips, 144 bursts; 3 clip-end bursts have no probe
window), plus the 13 capped bursts on 0076/0058/0005 that have forced full-refinement renders.
Each variant's correction is built only from the probe P and the essential-matrix estimate S.
The current render is only the referee: predicted ratio = rms(rr - c)/rms(rr), 1-8 Hz.
Plumbing: `full` reproduces pool.py's ratio (max diff 9e-16).

Variants: `axis` (correct only body axes whose own P~S correlation is >= 0.75 / 0.5); `local`
(sliding 0.3/0.4/0.6 s pooled P~S correlation mapped to a weight
clip((corr-0.3)/(0.75-0.3), 0, 1), times P); `proj` / `projloc` (projection of P onto S, whole
burst / sliding); `ess` (S itself); `mean` ((P+S)/2). The local constants reuse the production
floor and gate. They were not tuned, and the three window lengths score almost the same.

Median ratio, helps (< 0.9) / neutral / hurts (> 1.1):

| group | n | full | axis75 | local0.4 | proj | ess | mean |
|---|---|---|---|---|---|---|---|
| over cap, rejected (< 0.75) | 9 | 1.10 3/2/4 | 1.00 2/6/1 | 1.00 3/5/1 | 1.00 2/6/1 | 1.01 1/6/2 | 0.99 3/4/2 |
| floor-skipped (< 0.3) | 26 | 1.09 4/10/12 | 1.00 0/25/1 | 0.99 2/24/0 | 0.99 3/21/2 | 1.05 2/13/11 | 1.07 3/12/11 |
| refined, 0.3-0.75 | 70 | 0.85 41/20/9 | 1.00 14/55/1 | 0.89 38/31/1 | 0.94 22/46/2 | 1.02 10/36/24 | 0.91 30/31/9 |
| high (>= 0.75) | 39 | 0.69 30/8/1 | 0.77 30/7/2 | 0.69 31/7/1 | 0.77 32/6/1 | 0.81 28/10/1 | 0.72 32/6/1 |

Calibration: on the 13 capped bursts with forced renders, the full-refinement proxy gets the
direction right every time. It exaggerates harms (predicted 2.58 vs actual 1.98, 1.50 vs 1.14).

Read:
- **The time-local weight (local0.4) is the only variant that does not lose anywhere.** On
  high-agreement bursts it matches full refinement. On floor-skipped bursts it is neutral with a
  small upside (0073 178.8 0.80, 275.0 0.71). On the 0.3-0.75 refined bursts it removes 8 of 9
  predicted harms and keeps most of the gain (38 vs 41 helps). The other variants either cost
  performance on high-agreement bursts (axis75, proj, ess) or keep the harms (mean, ess).
- **Over-cap rejected bursts, the original target, barely move.** local helps 3 of 9 (0058 345.1
  0.79, 0076 275.1 0.65, which production already admits, and 0076 290.5 0.82). It hurts 0058
  255.1 (predicted 1.40; full is predicted 2.63 there, actual 1.98, on a calm 8 deg/s burst).
  The monster rejects (0058 41.3, 0076 258.8, 16.4, 43.6, 150.3) are 1.00 under every variant:
  neither estimate agrees on any axis or at any time, so no partial rule finds anything to apply.
- So partial refinement is mostly a **safer replacement for the gate + floor**, not a rescue for
  the rejected monsters. The weight covers both rules: a weight near 0 acts like the floor, and a
  weight near 1 acts like admission. The biggest predicted gain is on the 0.3-0.75 band, which
  production refines unconditionally today.

Proxy only. No partial variant has been rendered yet.

## Step 2: forced renders (`drive.py`, `run.sh`, `compare.py`; 0058/0076/0073, `--no-refine` base)

Variants: `pv1prod` refines exactly the bursts current main refines (taken from the remap
`prod_run.log` files; research-path correction). `pv1local` refines every burst with the
local0.4 weight. Outputs are in `target/experiments/regions-v2/<clip>nr/pv1-score.txt` and
`score-cur-pv1prod-pv1local.json`.

Noise: 9 bursts get the same correction in both variants. There, the 1-8 Hz difference is a
median of 0.9 deg/s and at most 2.8. On 22 bursts with zero correction in both, pv1prod vs cur
differs by a median of 0.6 and at most 4.5 (monsters). 8-50 Hz is noisier: up to 13 on a
zero-correction burst. So treat |delta| < 2-3 as noise on normal bursts and < 5 on monsters.

Result, sum of 1-8 Hz over all 82 scored bursts: cur 1199.6, pv1prod 1015.9, pv1local 1015.1.
**The two are even overall. Local does not dominate.** By group (local minus prod):

| group | n | better / same / worse (±2) | sum |
|---|---|---|---|
| refined today | 53 | 3 / 46 / 4 | +16.6 (local gives up gain) |
| capped, rejected | 7 | 4 / 2 / 1 | -25.3 |
| floor-skipped | 12 | 1 / 11 / 0 | -1.8 |

- Capped, rejected: two real wins, both predicted by step 1. 0076 290.49 goes from 57.3 to
  46.2 (predicted 0.82, actual 0.82), and 0058 345.07 from 37.3 to 29.6 (predicted 0.79,
  actual 0.80). The other two "wins" are monsters with weight ~0 (0058 41.25, 0076 43.58),
  i.e. noise. The one loss, 0076 258.83 at +3.2 with weight 0.08, is also noise. 0058 255.06,
  step 1's feared harm, is 8.3 -> 9.2 (noise).
- Refined today: the weight is too timid where full correction helps, for example 0058 210.04
  (prod/cur 0.74, local 1.05), 236.13, 342.05 and 214.50. 214.50 has whole-burst agreement 0.97
  but a mean weight of only 0.56, and it is local's worst loss (+5.6). Local does avoid two prod
  harms: 0073 329.91 (prod 1.48) and 0058 268.48 (1.23).
- Floor-skipped: neutral, as predicted.
- Proxy vs actual: corr 0.81 (local) and 0.74 (prod); it is on the correct side of 1 about
  two-thirds of the time.

Conclusion: keep production's gate + floor for every burst it refines or skips today. Apply the
local weight only to the over-cap bursts the gate rejects. That captures the two real wins and
changes nothing else. Because corrections are time-local (constant carried offsets are
invisible), this hybrid's per-burst outcome can be read off these renders: capped rows from
pv1local, everything else from pv1prod.

Review page: `target/experiments/regions-v2/review-partial.html` (LEFT = not refined as today,
RIGHT = partial). Items: 0076 290.5 s and 0058 345.1 s.
User verdict (2026-09-27, review-partial): "honestly, i cant really tell the difference. its not
any worse, but if you didnt tell me which side was which, i wouldnt be able to tell."

**Decision: partial refinement is shelved, not ported.** A 20% cut in render-measured 1-8 Hz
wobble on bursts that are still at 30-60 deg/s is not visible. This matches 0076 153.7 s, where
87 -> 40 was also barely visible. Visible wins so far came from calm-ending outcomes (0083,
0005, 0073 floor: bursts brought down to a few deg/s), not from partial cuts on monster bursts.
The hybrid's safety check on 0021/0060/0083 was not run because there is nothing to ship.

Still unchecked for the hybrid: the known probe-wrong bursts on 0021 (powerloop, 26.4, 109.6,
148.2 s) and 0060 (48.7, 191.7 s), and 0083 1:53. These are the bursts the gate exists to
reject, and the partial weight must stay ~0 there.
