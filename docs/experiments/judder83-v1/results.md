# judder83-v1: 0083 judder at 1:53-1:54 (handback veto + capped refinement), 2026-09-25

User report (v0.2.0 output `sample_vids/DJI_20260919133302_0083_D_fixed.MP4`): judder at ~1:53
after a turn (possibly propwash) and at ~1:54 on the throttle-up right before a loop. Also a
judder at the clip end (crash/bounce, not a target).

Artifacts: `target/experiments/judder83/` (base = current output, byte-identical to the user's
`_fixed`; `veto`, `veto_refine`, renders, camera exports, probes, review page `review.html`).
Tools: `docs/experiments/fullprobe-v1/{analyze,bursts,rbins,xburst}.py`.

## Diagnosis

- Both spots are one merged severe burst **113.09-114.68 s**: noise peaks 112 deg/s at
  113.3-113.5 (after a 150 deg/s turn) and 270 deg/s at 114.30-114.40 (throttle punch; the
  loop that follows runs 390-470 deg/s).
- Refinement **skipped** it: correction 11.97 deg > 4 deg cap. 7 of 21 bursts on this clip are
  cap-skipped (far more than earlier clips).
- Render-measured 1-8 Hz residual (render_residual_probe + fbclip intended-path removal):
  113.0-113.75 s 21-40 deg/s, 114.25 s **135 deg/s**, vs ~2 deg/s in calm flight. Source-frame
  probe agrees (HP residual 28-37, 122 at 114.25).
- 1:54 cause: **fast handback** (rate-aware blend back to LP8 gyro) engaged because the motion
  is genuinely fast (frames ~330 deg/s; min(gyro, optical) also says fast), while the gyro
  carried its worst phantom at that instant (segment peak noise 270 sits inside the 200-300
  gyro-trust ramp, which only changes the rate *estimate*). Fixed telemetry had a
  274-490 deg/s spike on one axis where frames show 70-155.
- 1:53.7 cause: inside the merged burst the noise dips to 3 deg/s and the patch blends the gyro
  back in (partner path), but the fused DJI attitude is still wrong there (~35 deg/s on one
  axis for ~0.2 s, both instruments agree). Post-vibration fusion transient.

## Candidates

A. **Handback noise veto** (`--handback-noise-veto 100 200`, branch `handback-veto`, commit
   e3edd7d; default off): multiply the fast-handback weight by
   1 - ramp(running-max noise over +-0.15 s, 100, 200). Render 1-8 Hz: 114.0 s 33 -> 12,
   114.25 s 135 -> 41, 114.5 s 52 -> 45; 113.x unchanged. Burst correction 12.0 -> 6.0 deg
   (still over cap).
B. **A + refinement forced on this burst** (research, `feedback_repair` with probe-derived
   adjust, max 6.02 deg): 113.75 38 -> 18, 114.25 41 -> 23, 114.5 45 -> 27, 114.75 24 -> 13.
   113.0-113.25 barely moves (render measurement has few inliers there).

Veto regression scan (orientation diff vs current output, >0.05 deg): 0060 none; 0071 none;
0073 only the 11.3 s burst (<=0.4 deg, then invisible carried offset); 0027 monster bursts
86-88 s and 99.8-102.3 s (refinement residual there 2.8 -> 1.0 deg, suggests a more accurate
patch); **0021 powerloop 21.9-26.1 s changes up to 11 deg** (needs eyeball).

## Why the cap cannot simply be raised (measured)

Render-free discriminators for "probe correction is trustworthy" all failed on the known
cases (0021 powerloop/109.6 s and 0060 48.7 s = probe wrong per render; 0083 = probe right):
- telemetry-rate gating of the refinement weight (100-400 deg/s ramps): capped angles barely
  change; probe failures occur at moderate rates;
- probe per-pair fit RMS and inlier counts: 0060 48.7 (wrong) 288 vs 0083 (right) 303;
- (fullprobe-v1) probe residual outside bursts is uncorrelated with the render (|r| <= 0.26),
  so whole-clip refinement is not a target either.

## Status

Review page `target/experiments/judder83/review.html` (A and B on 0083; veto regression
checks on 0021 powerloop and 0027 86/100 s). User verdict pending. No production default
changed. If A is accepted: make it default-on (config default + GUI + goldens/docs), release
build. If B's extra gain is visible, the next research problem is a trustworthy per-burst
(or per-segment) gate for refinement beyond 4 deg.

## User verdict (2026-09-25, review.html)

"A does fix the second burst, but the first one is essentially unchanged. B looks much better
than A on the first burst, marginally better on the second (its already much better so there
isnt much left to improve, but still its better). as you said though, we dont have a good way
to determine when its safe to apply. I can't tell if C is significantly regressed, might be a
touch worse, if you hadnt said anything i probably wouldnt have noticed. D,E,F, are all
essentially the same, might be a touch smoother, again not super noticable."

Reading: A (veto) = clear fix at 1:54 with no noticeable regressions (0021 powerloop at most
a touch worse, 0027 monster bursts a touch smoother). B (forced refinement) is the bigger
visible win and fixes 1:53 — the missing piece is a render-free safety gate for refinement
corrections above the 4 deg cap.
