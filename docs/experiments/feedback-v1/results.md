# Render-feedback telemetry refinement (feedback-v1) — 2026-09-24

Research only. Production, shipping binaries and all prior candidates unchanged. Clip 0073. (Later promoted to a v0.1.3 candidate; see the production section at the end.)

## Idea

Every earlier optical estimator measured *absolute* camera motion in the source video (hundreds of deg/s inside bursts, heavy blur/RS), and its low-frequency error is what the repaired telemetry inherits. The post-render corrections (render-acceleration-v1, render-selective-v1) that the user *did* see improve instead measured the **residual** motion in Gyroflow's stabilized output, where motion is small. feedback-v1 closes that loop in the shipping architecture: measure the residual in the Gyroflow render, convert it to a body-frame telemetry error, subtract it from the repaired quaternions inside the severe bursts, and let Gyroflow re-render. No post-render warp, no extra crop, Gyroflow still does per-row RS correction.

## Pipeline

1. `render_residual_probe RENDER START DUR OUT` (new example): ffmpeg-decoded native 1440x810 frames, GFTT + forward/backward LK (0.3 px), RANSAC partial-affine per consecutive pair → center displacement, roll, log-scale rates.
2. **System identification (perturbation render).** `feedback_repair` injected known 0.4° body sinusoids (x 3 Hz, y 4.5 Hz, z 6 Hz, smooth envelopes) into the accepted edgeoffset repair at 56.2–58.6 s and 100.2–102.6 s; full Gyroflow render; perturbed-minus-baseline image rates regressed on the injected body rates. Result (`calibration.json`): lag −2 ms, R² 0.954 / 0.965 / 0.991, `M ≈ [[·,−545,·],[−488,·,·],[·,·,−1.00]]` (image dx,dy,roll per body x,y,z rad/s); per-window fits agree within ~1%. The render measurement therefore recovers known telemetry errors reliably.
3. **Intended motion removal.** Gyroflow `--export-metadata` stab_quat rates (with fov_scale) regressed onto measured image rates on non-burst samples (Huber IRLS; R² 0.98/0.95/0.96) and subtracted; then 1 Hz zero-phase high-pass (removes translational parallax / LF). Residual floor: clean control ~1 deg/s, non-burst active flight ~3–4, inside bursts 5–7.5.
4. **Correction.** body error = M⁻¹·residual; correction rate = −error × burst gate (severe intervals from residual-stage-v1 stages.json, ±0.25 s, 0.15 s smoothstep; only bursts fully inside a measured window) × tracking confidence (n inliers 60→200 ramp). Integrated as extra body rotation per sample: `q'ᵢ₊₁ = q'ᵢ · (qᵢ⁻¹qᵢ₊₁) · exp(ΔAᵢ)`, so outside the bursts body rates are untouched and any leftover is a constant **world-frame** offset. (First attempt `fb1` right-multiplied the accumulated angle, i.e. a body-frame remount; it changed the clean-control correction by up to 0.47°. Superseded by `fb1w`: ≤0.02° anywhere outside corrected windows, 0.01° on the control; 57.4 s frames PSNR 48.8 dB.)
5. Windows corrected: early 12–32 s (bursts 16.3–18.6, 19.5–22.6, 24.9–26.6) and late 130–148 s (134.7–135.4, 135.8–136.6, 137.8–139.3, 141.3–144.5). Max adjustment 1.7° late, 5.1° accumulated early.

## Measured result (re-measured on the new render; |3-axis| body deg/s, 1–8 Hz / 8–50 Hz)

| Burst | edgeoffset baseline | fb1w | 1–8 Hz change |
|---|---|---|---|
| 134.7–135.4 | 3.92 / 8.82 | 2.34 / 6.68 | −40% |
| 135.8–136.6 | 7.46 / 12.30 | 4.34 / 10.04 | −42% |
| 137.8–139.3 | 3.25 / 10.24 | 1.90 / 7.62 | −42% |
| 141.3–144.5 | 4.05 / 8.45 | 2.52 / 8.11 | −38% |
| 16.3–18.6 | 4.01 / 4.56 | 2.24 / 4.28 | −44% |
| 19.5–22.6 | 6.14 / 6.25 | 3.28 / 6.22 | −47% |
| 24.9–26.6 | 5.29 / 6.75 | 3.24 / 6.58 | −39% |
| late non-burst | 2.14 / 5.05 | 2.17 / 5.20 | control |
| early non-burst | 2.34 / 5.01 | 2.36 / 5.25 | control |
| clean 55–60 s | 0.57 / 1.53 | 0.49 / 1.46 | control |

8–50 Hz falls 18–26% in three late bursts and is roughly flat elsewhere; that band is near the tracker/active-flight floor (~5 deg/s) and contains intra-frame content a 100 Hz correction cannot reach. A second iteration (`fb2`, gain 1 on top of body-frame fb1) was mixed (−0.9 to +0.7 deg/s per burst): the loop converges in one pass to the measurement floor. Do not iterate further without a better residual measurement.

Limits: the scorer uses the same tracker as the correction (though on a *new* render, so injected tracker noise would show up as new residual). The correction is per-clip closed-loop by design — it is derived from this clip, so this is not a held-out generalization result; it needs Gyroflow in the repair loop (two renders per clip) if productized. Measurement confidence drops in very fast turns (n < 150 inliers), which are down-weighted. Only 0073 tested.

## Review

`target/experiments/feedback-v1/compare_{early,late,control}{,_full,_detail}.mp4` — LEFT accepted edgeoffset render (current best validated implementation), RIGHT fb1w. Preview = 720-wide panels; `_full` = native 1440x810 panels; `_detail` = native top-right quadrant crop. Windows: early 14–30 s, late 132–146 s, control 55.9–58.9 s. Same zoom/crop on both sides (no extra crop, unlike render-selective-v1). Frame counts verified (`review-validation.json`).

## Reproduction

Examples `render_residual_probe`, `feedback_repair` (handoff OpenCV env). Python (numpy): `fb.py perturb|calibrate|project NAME`, `fb3.py` (intended-motion fit → intended_B.npy), `fb4.py TAG GAIN MEAS_PREFIX [CAMERA_JSON]` (correction angles), `score.py PREFIX:CAMERA ...`, `export_reviews.py`. Gyroflow portable render + `--export-metadata 3:ABS_PATH` per variant.

## User verdict

User verdict (2026-09-24): “judder is improved quite a bit. i can still see a tiny amount on the right but its far less noticable than the left.” Clear visible improvement over the accepted edgeoffset baseline on 0073, with a tiny residual. Strongest visual result so far in the product architecture (telemetry-only, no extra crop). Next: validate the unchanged procedure on 0060/0071. No promotion yet.


## Update 2026-09-24 (afternoon): fixes, generalization, second feedback step

**Window-order bug.** fb4.py chained the late window's offset into the early window, leaving a one-sample 1.4° step at 12.0 s and a slow 0.05°/s ramp over 32–130 s in `fb1w` (outside the reviewed excerpts; in-burst corrections unaffected). `fbclip.py` (general per-clip driver) orders windows by time; 0073 was regenerated as `fb1x` (in-burst scores equal to fb1w within noise).

**Generalization, unchanged procedure (0073 calibration reused; per-clip intended-motion fit, R² 0.76/0.88/0.96 on 0060, 0.99/0.94/0.95 on 0071).** 1–8 Hz in-burst, edgeoffset → fb1: 0060 98 s 10.0→4.7, 100 s 3.9→2.7, 106 s 2.2→1.2, 223 s 2.7→1.8, 225 s 3.0→2.1, 249 s 3.9→2.3, 308 s 3.0→2.9 (already at the local measurement floor); 0071 14–17 s 7.9→7.2, 18.7 s 4.4→1.7, 256 s 2.5→1.6. Controls/non-burst within noise.

**Response analysis.** Regressing the measured residual change (fb1 − base) on the applied correction rate, per burst, over a 1 ms timing scan: the render responds with gain 1.03–1.37 (1.67 in the 0071 throttle burst) and best timing 2–4 ms earlier than the clean-zone calibration (r² 0.50–0.85). The clean-zone perturbation calibration (gain 1.00) therefore under-predicts the in-burst response, so gain-1 fb1 overshoots, badly in the 0071 throttle burst. Cause of the higher in-burst gain not established.

**Second step `fbn` (`fbclip.py correct2`).** Residual of the fb1 render corrected per burst by 1/(measured gain), at the per-burst best timing, on top of fb1. 1–8 Hz / 8–50 Hz in-burst, base → fb1 → fbn:

| Burst | base | fb1(x) | fbn |
|---|---|---|---|
| 0073 16.3 | 4.01/4.56 | 2.09/4.08 | 2.01/4.18 |
| 0073 19.5 | 6.14/6.25 | 3.25/5.71 | 2.51/6.49 |
| 0073 24.9 | 5.29/6.75 | 3.51/7.40 | 2.39/6.40 |
| 0073 134.7 | 3.92/8.82 | 2.12/6.32 | 2.17/6.17 |
| 0073 135.8 | 7.46/12.30 | 5.02/10.78 | 2.96/11.25 |
| 0073 137.8 | 3.25/10.24 | 2.10/7.80 | 1.45/6.89 |
| 0073 141.3 | 4.05/8.45 | 2.68/7.44 | 2.51/6.98 |
| 0060 98.0 | 10.00/12.54 | 4.66/13.21 | 3.78/9.15 |
| 0060 100.4 | 3.88/7.15 | 2.66/7.24 | 2.78/6.79 |
| 0060 105.8 (1:46) | 2.18/3.83 | 1.18/3.25 | 1.09/3.16 |
| 0060 223.4 | 2.73/4.36 | 1.82/5.48 | 1.94/4.22 |
| 0060 225.0 (3:45) | 2.96/6.42 | 2.12/6.10 | 2.23/5.50 |
| 0060 248.3 (4:09) | 3.94/5.33 | 2.25/5.10 | 1.90/6.25 |
| 0060 308.0 (5:08) | 2.96/6.84 | 2.87/6.75 | 2.19/6.52 |

fbn: 1–8 Hz in-burst −25% to −62% vs edgeoffset on all 14 bursts; 8–50 Hz mostly lower (−36% to +17%; 4:09 is the one clear high-band increase). Non-burst spans move within tracker noise (Gyroflow smoothing is non-local, so frames near bursts are not bit-identical). 0071 `fbn` was interrupted (Claude Code stopped the background job for low system memory) before its repair completed; its `fbn-adjust.json` exists, the MP4/render do not.

**Reviews** (LEFT edgeoffset, RIGHT variant; preview / `_full` native / `_detail` native top-right crop):
- 0073: `compare_fbn_{early,late,control}*` (fbn) and `compare_{early,late,control}*` (fb1w, user-reviewed).
- 0060: `0060/compare_fbn_{106,225,249,308,control}*` (fbn) and `0060/compare_{...}*` (fb1).
- 0071: `0071/compare_{throttle,late,control}*` (fb1 only).


## User verdict — fbn

User verdict (2026-09-24, fbn): “fbn looks better in most of these clips. its near perfect for all except 0060_308, which i think there is a slight regression, but its very subtle. overall still a win.”

## 0071 fbn (rerun after memory stop)

1–8 Hz / 8–50 Hz in-burst, base → fb1 → fbn: throttle 14.2–17.1 7.87/11.19 → 7.18/11.86 → **4.29/9.87**; 18.7–19.2 4.39/10.08 → 1.71/6.06 → **1.16/5.16**; 255.6–257.1 2.54/6.18 → 1.55/6.49 → **1.06/7.22**. Control 0.98/2.44 → 0.88/2.32. The gain-corrected second step fixes the throttle burst that gain-1 fb1 overshot (response gain 1.67 there). Reviews: `0071/compare_fbn_{throttle,late,control}*`.

## Gyroflow-free residual probe (warp-v1 inside feedback-v1)

Goal: replace the render+measure loop with an in-process measurement (user asked whether rebuilding Gyroflow's warp/smoothing is feasible). Gyroflow's geometry here is a plain OpenCV fisheye (f 546.4 px, D from telemetry) with 5.09 ms top-to-bottom readout, and smoothing is irrelevant after the 1 Hz high-pass, so no image warp is needed: `warp_residual_probe` tracks features between consecutive **source** frames (full-res LK + fwd/back check), fisheye-undistorts them, rotates each bearing into the world frame with the file's telemetry at that feature's row time (camera→telemetry mount found by exhaustive signed-permutation search = diag(1,−1,−1)), and fits the leftover small rotation (trimmed least squares). Optional `O4_CROP=1.30,0.72` keeps only features inside the rectilinear field Gyroflow renders.

Findings:
- Gyroflow reads injected corrections exactly (exported org_quat change vs injected change: gain 1.00, r² 1.000, zero lag) and its smoothed path does not respond (stab change ≈ 0). So the "in-burst response gain 1.2–1.7" of the render loop is a **render-measurement scale error**, not Gyroflow behaviour: part of it is adaptive zoom (image→angle factor calibrated at fov ≈1.06–1.09, bursts render at 0.94–1.04); fb1's overshoot and fbn's measured gains follow from it.
- Base 0073 in-burst: probe vs render residual correlation 0.56–0.74 on 6 of 7 bursts (crop improves 16.3 s 0.39→0.61), amplitude 0.6–0.8× render (consistent with the render over-scale). 135.8 s disagrees (r ≈ 0.25; burst ends in a fast turn).
- One-pass correction from the probe alone (`wp1`, gain 1, no render in the loop), scored by the render measurement: 1–8 Hz in-burst 16.3 4.01→3.23, 19.5 6.14→4.73, 24.9 5.29→**1.84**, 134.7 3.92→2.84, 135.8 7.46→7.34, 137.8 3.25→2.39, 141.3 4.05→**1.91** (fbn: 2.01/2.51/2.39/2.17/2.96/1.45/2.51). Caveat: the render measurement also built fb1/fbn, so it favours them by construction; the user's eye is the independent judge.
- The probe's own residual on wp1 falls to ~1 °/s in bursts (below the non-burst floor), so iterating the probe alone would not change much; its remaining misses are model/visibility limits, not convergence.

## User verdict — fbn vs wp1c (0073)

"looking closer, the late fbn clip on 0073 has a slightly stronger judder ~8 seconds into the clip (where the fast turn happens)" — i.e. source ~140–141 s, favouring the Gyroflow-free wp1c there. Exported corrections: in 140.75–141.75 s (fast turn into the 141.25 s burst entry) fbn deviates from base by up to 1.2°, wp1c by ≤0.3°. Render-measurement corrections are least reliable in fast motion (few inliers, blur), matching the 0060 5:08 edge observation. No issue reported at 135.8 s, the burst where the probe and render measurement disagreed most.

## Production promotion: v0.1.3 candidate (branch `refine-v013`, 2026-09-24)

This is a candidate, not a release. `o4core/src/refine/` ports the wp1c probe and correction into the repair pipeline. It is on by default; `--no-refine` or the GUI toggle turns it off. The edge-offset splice fix and the 0.19 s ramp are now the defaults. A burst is skipped when its correction exceeds 4°, when it lies within 0.5 s of the clip start, or when its window cannot be measured. The geometry check `max_motion_ratio` is 0.94.

**Parity (Task 1 and Task 5).** `rebased_clip`: with refine off, the release splice reproduces `gyro-trace-v1/edgeoffset.MP4` with max error 0.0 (tolerance 1e-6). `refine_research_parity` compares the worst in-burst applied angle against wp1c-adjust (tolerance 0.15°): 0073 0.024°, 0060 0.046°, 0071 0.010°. The geometry motion ratio (median |residual| / telemetry rate on non-burst pairs above 30°/s) is 0.139, 0.474 and 0.144 with the correct mount, and 1.864 with the wrong mount on 0073. The threshold is the geometric midpoint, 0.94.

**Release CLI runs (Task 8; `target/experiments/release-v013/`).** All four clips exit 0 with an exact round-trip and no clip-level "refinement skipped":

| clip | wall time (refine on) | bursts refined | not refined |
|---|---|---|---|
| 0073 | 253 s | 30/31 | 375.6 s: window not measurable (burst runs past video end) |
| 0060 | 231 s (`--no-refine` 112 s) | 24/27 | 0.21 s: too close to clip edge; 48.7 s: 5.0° over cap; 191.7 s: 4.8° over cap |
| 0071 | 138 s | 7/7 | none |
| 0021 | 353 s (`--no-refine` 125 s) | 26/31 | 21.7–26.1 s: 27.2° over cap; 26.4–34.0 s: 5.3°; 109.6–110.5 s: 8.6°; 148.2–150.0 s: 4.8°; 175.9 s: unmeasurable |

Refinement roughly doubles repair time, and the extra time scales with total burst duration.

**Applied correction vs the reviewed research renders.** Gyroflow `--export-metadata` body-frame `org⁻¹·stab`, release vs wp1c. 0073 132–146 s: median 0.10°, max 0.66°. 0060 245–253 s: median 0.05°, max 0.16°. The clean controls are at most 0.03°. Whole-clip maxima are 3.2° (0073) and 0.9° (0060). These come from bursts that the research runs did not refine.

**0021 regression (Gyroflow portable 1.6.3, `eval_M2_tight` project via `make_project.py`, `eval_render.py` + `rank_renders.py`; wobble 2–8 / shake 8–30 °/s).** v0.1.2 is the shipped `_fixed.MP4`, rendered with the identical project:

| render | clean | mild | severe | flicks |
|---|---|---|---|---|
| eval_M2_tight (historical) | 3.31/4.84 | 6.68/7.45 | 12.49/16.97 | 17.84/20.29 |
| v0.1.2 shipped | 3.28/4.83 | 6.69/7.47 | 12.91/16.97 | 17.84/20.29 |
| release `--no-refine` | 2.79/5.16 | 6.91/7.77 | 13.51/19.85 | 17.57/22.32 |
| release (refine on) | 3.01/4.84 | 6.29/7.41 | 13.22/20.56 | 16.76/22.69 |
| severe, t < 175.5 s: v0.1.2 / release | | | 10.11/12.93 → 9.75/13.13 | |

- Clean and mild are within ±0.4°/s of v0.1.2. Refined bursts (26 of them, time-weighted in-burst) go from 7.03/7.95 (v0.1.2) to 6.57/7.65 (release); `--no-refine` gives 7.21/7.38.
- The severe-shake increase comes entirely from the clip-end landing (175.9–176.6 s, ground impact, not refinable). Without it, severe shake is flat and severe wobble improves.
- The flicks-shake increase sits in the cap-skipped bursts: flick 22.1 lies in 21.7–26.1 s and flick 109.8 in 109.6–110.5 s. There, the Gyroflow-applied correction of release and `--no-refine` is identical (max 0.000°), yet the tracker scores flick 22.1 shake at 13.3 and 21.0. So that difference is tracker/encoder noise (PSNR 34 dB between the two renders there). It is not refinement.
- The only real change in the skipped bursts is the edge-offset splice fix: up to 3.9° applied-correction change at 21.7–26.1 s relative to v0.1.2.

**Review page:** `target/experiments/release-v013/review-release.html` (spec `docs/experiments/feedback-v1/review-release-vs-wp1c.json`). It shows 0073 late and 0060 4:09, with LEFT wp1c and RIGHT release; these should look identical. It also shows 0021 at 4–12, 20–35 (powerloop, not refined) and 150–163 s, with LEFT v0.1.2 and RIGHT release. User verdict pending.
