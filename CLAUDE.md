# DJI O4 Pro gyro noise fix — STATUS AFTER SESSION 3: MISSION ACCOMPLISHED

## Problem

DJI switched the gyro on the O4 Pro air unit in early 2026 (this unit: fw
01.00.07.00, camera model "O4P"). During high-throttle flight the motion
data carries strong noise, so Gyroflow-stabilized footage shudders /
wobbles / micro-pans. Goal was: renders as good as Gyroflow with a healthy
gyro. Test clip: `sample_vids/DJI_20260711124046_0021_D.MP4` (176.4 s,
1440x1080@100fps CFR, 17639 frames).

## SOLVED (session 3, 2026-07-13): in-place MP4 quat rewrite

`python python/o4fix.py VIDEO.MP4` → `VIDEO_fixed.MP4` (default mode; `--gcsv`
keeps the legacy rate-domain pipeline). The fixed file drops into Gyroflow
like a stock recording, using its native embedded-telemetry path.

Result on the test clip (residual RMS °/s, wobble 2-8 Hz / shake 8-30 Hz):

| render | clean | mild | severe | flicks |
|---|---|---|---|---|
| unstabilized original | 5.3/5.5 | 10.2/9.0 | 30.8/23.1 | 40.1/30.4 |
| A embedded (old target) | 3.1/5.1 | 6.9/7.1 | 18.6/26.3 | 21.0/24.9 |
| B best gcsv (ceiling-bound) | 5.6/5.9 | 9.0/7.8 | 36.8/29.0 | 20.4/20.0 |
| **M2 = o4fix default** | **3.3/4.8** | **6.7/7.5** | **12.5/17.0** | **17.8/20.3** |

Per-window: powerloop 7-12 s — A 10.9/18.2, B 5.5/5.1, **M2 3.9/4.0**;
aggressive 131-159 s — A 8.4/11.5, **M2 5.5/6.8**. Clean/mild diffs vs A
are within eval coupling noise (clean-zone telemetry is bit-identical).
Success criteria all met: matches A in clean/mild/flicks, crushes severe.

### How it works

1. **Encoding** (`mp4patch.py`, all verified): O4P metadata-track samples
   (handler `meta`, 1/frame) are protobufs that carry NO "oq101"/"WA530"
   marker (head: `dvtm_O4P.proto`), so telemetry-parser decodes them with
   its **wm169** proto. Path: `frame_meta(3).imu_frame_meta(3).
   IMU_attitude_after_fusion(2)` = `DeviceAttitude{timestamp=1, vsync=2,
   attitude[]=3, offset=4}`; attitude element = `Quaternion{w=1,x=2,y=3,
   z=4}` as **float32 LE fixed32** → in-place rewrite, no stsz/stco fixups.
   20 quats/sample = 2 kHz slots, each 1 kHz value duplicated. Parser
   transform: `q_out = (0,0,1,0) ⊗ q_file ⊗ (0.5,-0.5,-0.5,0.5)` + sign
   continuity (session-1 byte scan failed because it searched transformed
   values). Slot→1 kHz-sample mapping replicates extract_quats
   sort/dedupe on the parser's flat stream.
2. **Gates** (all pass; keep them passing): decode matches
   telemetry-parser exactly; `nullpatch` → byte-identical file; `inject`
   → parser returns exactly the injected values, timestamps unchanged.
   Unmodified samples keep their original bytes (clean zones bit-exact).
3. **Data** (`splice_orientation` in o4fix.py): base = raw embedded quats;
   inside severe bursts (30-180 Hz band-RMS > 8 °/s, pad 0.2 s, merge
   0.2 s — tighter than 0.3/0.5 which leaked LP content into mild zones
   and cost +0.9 mild shake) replace orientation with the integral of the
   B-style optical rates (LP8 optical + rate-aware handback, o4fix
   defaults); accumulated optical drift (up to ~55° on long bursts) is
   spread across the burst as a smoothstep rotation-vector correction so
   endpoints match raw exactly; 0.3 s slerp cross-fades at edges.

### Files

Repo is rust-first: the Cargo workspace (`o4core/`, `o4fix-cli/`,
`o4fix-app/`, `Cargo.toml`) is at the root; all Python lives under
`python/`. Rust dev/build notes are in `docs/development.md`.

- `python/o4fix.py` — the user tool. Default = MP4 repair; `--gcsv` = legacy.
  Owns splice_orientation/quat_exp/quat_log/slerp. MP4-repair flags:
  `--severe 8 --severe-pad 0.2 --severe-merge 0.2 --ramp 0.3`.
- `python/mp4patch.py` — encoding toolkit: scan/verify/nullpatch/inject CLI +
  `inject_and_check()` used by o4fix.
- `python/prep_inject.py` — dev tool: builds an injection .npz without writing
  the MP4 (for harness iterations). Same defaults as o4fix.
- `python/analysis/` — harness (eval_render/rank_renders/diff_renders/...,
  caches incl. eval series for all renders in the table).
- `sample_vids/` — test clip, projects, renders kept:
  `eval_A_embedded.mp4` (reference), `eval_M2_tight.mp4` (best,
  = MPATCH2 source `DJI..._MPATCH2.MP4`), `DJI..._D_fixed.MP4` (o4fix
  end-to-end output). Superseded renders deleted; their eval caches kept.

## Harness (unchanged from session 2 — use it, don't render blind)

- Gyroflow CLI: store 1.6.3 exe at `C:\Program Files\WindowsApps\
  29160AdrianRoss.Gyroflow_1.63.2453.0_x64__q81n4e8pq4bra\Gyroflow.exe`;
  `& $gf project.gyroflow -f --stdout-progress` (~90 s/clip on RTX 5090).
  Dev build gh2622 in scratchpad (glitch-filter test) renders identically
  to release on identical data.
- Project variants: JSON-edit a copy of `eval_M2_tight.gyroflow` (or
  `eval_A_embedded.gyroflow`): change `videofile` + `gyro_source.filepath`
  (and DROP `gyro_source.file_metadata` when the gyro file changes),
  `offsets={}`, set `output.output_filename`, bitrate 30.
- `python/analysis/eval_render.py RENDER.mp4` (~7 min) → cache;
  `python/analysis/rank_renders.py [stems...]` → banded table.
- Masks trap: "clean cruise 60-65/100-105 s" are NOT clean. Verified-clean
  windows: 67-72.5, 94-98, 120-128, 145.5-146.5 s. Tracker floor ~2-3 °/s.

## Established facts (verified; do not re-derive)

- O4 Pro embeds ONLY orientation quats (no raw IMU); `normalized_imu()`
  empty; use `.telemetry()` → Quaternion/Data; 1 kHz duplicated at 2 kHz;
  sort+dedupe (o4fix.extract_quats).
- Rates from quats: vector part of dq (`2*asin(|vec|)`), never arccos(w).
- Telemetry timestamps on container clock (0 offset; optical↔gyro −4 ms).
  Never run Gyroflow autosync on patched data (latches onto garbage in
  band-limited sections) — keep `offsets={}`.
- gcsv delivery has a hard quality ceiling in Gyroflow 1.6.3 (~5.5 °/s
  2-8 Hz wobble floor in clean zones vs 3.1 embedded, +2-3 °/s in mild) —
  bit-identical data renders worse via gcsv than embedded. Mechanism
  unproven (suspected per-scanline orientation sampling; org-path A-vs-B
  sub-frame scan has symmetric minima at ±4 ms, local MAX at 0).
- Noise: throttle-correlated bursts; detector = 30-180 Hz band-RMS
  (1 kHz fs). Phantom is broadband inside severe bursts; mild-zone 2-8 Hz
  gyro is trustworthy. Real HF (100-500 Hz vibration) matters for
  sub-frame correction — never low-pass the clean zones.
- Optical pipeline (o4fix.video_rates): fisheye undistort (K/D from
  telemetry), LK half-res, findEssentialMat(RANSAC 0.002) on normalized
  pts, decomposeEssentialMat smaller-angle R (recoverPose degenerates).
  Procrustes video→gyro allowing det=−1. This clip: R²=0.995, −4 ms.
  Optical LF error grows with rate; handback above 100-250 °/s.
- Gyroflow project facts: user runs 1.6.3, Default smoothing ~0.5,
  horizon lock OFF, adaptive zoom ON. Embedded loads use integration
  "None"; gcsv loads integrate.

## Dead ends (all measured — do not revisit)

1. Temporal filtering at any cutoff (phantom is broadband).
2. gcsv content variants B/C/D/E/G — all ceiling-bound.
3. `frame_readout_time` in gcsv header — ignored by Gyroflow.
4. Integration method — only shifts LF wander.
5. Autosync offsets — root-caused; keep offsets={}.
6. Project-file blob injection — blobs ignored, always recomputed.
7. Betaflight blackbox — user-rejected (32 MB flash, workflow).
8. **Gyroflow's new glitch filtering (commit 7ac9d11, 2026-07-12, in
   dev builds; toggle + `glitch_filter`/`glitch_strength` in project
   gyro_source)** — tested at strength 50/100 on this clip: detects only
   ~7 s (p99-relative threshold saturates with our 17% noisy samples,
   misses 131-159 s entirely, identical regions at both strengths) and
   its constant-rate slerp bridges erase real motion → severe 32.8/32.8
   (s50), 32.4/45.8 (s100) vs A 18.6/26.3; clean/mild unchanged. Useless
   for sustained noisy aggressive flight; fine upstream idea validation.

## Fast-motion shake investigation (post-solution, 2026-07-14)

User feedback on M2: panning gone, but slight shake remains during sharp
turns/180s/flips. Localized by cell analysis (rate regime × patched/raw ×
band; script pattern lives in the session log, rebuild from eval caches):
ALL of it is in **>150 °/s motion inside patched bursts** (6.5 s of clip).
Cause: LP8 handback discards the gyro's 8-16 Hz content, which raw data
shows is ~26 °/s there; Gyroflow can't correct motion it can't see.
Key asymmetry (from comparing A's per-cell corrections vs unstabilized):
gyro 8-15 Hz is REAL-dominated above ~150 °/s sustained, but
PHANTOM-dominated at 50-150 °/s and corrupted during snap transitions
(throttle punches). Fixes tried, full loop each:

- M3 (`--fast-wide-cutoff 16`, ramp 150-300 °/s): sustained-fast 8-15
  improved 20.2→17.2, but flicks regressed (snap transitions corrupt the
  mid-band) — rejected.
- M4 (+ `--fast-wide-accel 1500` °/s² gate): flick wobble 17.8→16.1,
  flick 8-15 10.4→9.6, sustained-fast better than M2, clean/mild/severe
  wobble all best-in-table — but flick 15-30 Hz shake +1.4 vs M2
  (19.0 vs 17.6). Genuine trade-off, neither dominates.

DECISION: default stays M2 behavior (`--fast-wide-cutoff 0`); M4 profile
available via `--fast-wide-cutoff 16` (accel gate defaults on). Renders
`eval_M2_tight.mp4` vs `eval_M4_accelgate.mp4` kept for the user to
eyeball. Residual-floor honesty: even A only reaches 13.5/23.3 °/s in the
fast-patched cell (vs M2 20.2/24.1, M4 18.1/24.6) — the 15-30 Hz part is
phantom-mixed in ALL sources and partially blur/RS artifact; do not chase
it further with the current signal sources (a better source, e.g. 
blackbox gyro, would be needed).

## Monster-burst handback fix (2026-07-21, second clip)

New clip `sample_vids/DJI_20260721141550_0027_D.MP4` (248 s, same format):
noise bursts hit **300-580 °/s** 30-180 Hz band-RMS (0021 never exceeds
161) but cover only 5.9% of the clip. o4fix's output still shook violently
there. Root cause: `w_fast` (rate-aware handback) measured "rate" from the
LP8 gyro itself; monster phantom leaks below 8 Hz, inflates that estimate
past the 100-250 °/s ramp, and hands orientation back to the very gyro
being repaired — the fixed file kept 150-330 °/s of 2-8 Hz wobble inside
those bursts. Optical ground truth: real motion in most monster bursts is
<150 °/s, and LK flow stays quality-1.0 even at 553 °/s real.

Fix (default-on, Python `o4fix.py` + Rust `o4core` + GUI field):
`--gyro-trust-noise 200 300` — per optical-patch segment, when the
segment's **peak** noise exceeds the ramp, the handback rate estimate
becomes `min(gyro, optical)`, so handback engages only when both sources
agree the motion is genuinely fast. Two designs that FAILED and why:
- pure min() ungated regressed 0021 (severe wobble +1.2, flick shake
  +1.8): real fast/violent moments (landing 175.9 s, flick 22.1 s) need
  full gyro handback when noise is sane;
- gating on *instantaneous* noise recovered only 115/165 °/s in-burst
  wobble (vs 24 for segment-peak): the RMS estimate dips mid-burst while
  the phantom LF persists.

Render results 0027 (wobble/shake °/s; eval windows in
`scratchpad cmp_eval27.py` pattern, caches `eval_eval27_*.npz`):
monster bursts old-fix 128-198 / 26-64 → **6-23 / 4-9**; unstabilized
141-188 / 111-387. Mild + clean windows unchanged (deltas ≤1.5 = eval
coupling noise; the eval tracker floor on this clip is ~3-5 °/s,
median track quality 0.62). 0021 regression: new pipeline output is
byte-identical to the shipped `_fixed.MP4` except ONE quat sample × one
float32 ulp (segment-local smoothing boundary); rank_renders row
`eval21_MINRATE` (pure-min variant, worst case) already sat on the M2 row
in clean/mild. Renders kept for eyeballing: `eval27_ORIG.mp4` (stabilized
on noisy telemetry), `eval27_OLDFIX.mp4` (pre-fix pipeline),
`eval27_V3.mp4` (fixed, = render of regenerated
`DJI_20260721141550_0027_D_fixed.MP4`).

Residual on 0027 after the fix (user: "smaller, slower judders,
noticeable but not footage-ruining"): the drift bridge is the floor. Each
monster burst must end on the raw quat frame (clean zones are bit-exact
by design), and the drift to bridge is 33-84 deg — larger than the
burst's real rotation, so it is mostly DJI fusion divergence under
monster vibration, not optical error; SOME bridge motion is unavoidable
with current sources. Dead end (measured, do not revisit): rate-weighted
drift spreading (concentrate the correction where |optical rate| is high,
10 deg/s floor) — WORSE on both clips (0027 monster 5.5s 2-8 Hz
11.0→14.2, flick 219.5s 14.5→29.9; 0021 flicks 17.8/20.3→20.1/23.6 —
fake rate stacked on fast motion is exactly what Gyroflow can't follow).
Uniform smoothstep-over-time is the measured-best bridge shape. Ideas NOT
tried: absorb drift into following clean zone (breaks bit-exactness),
blackbox gyro as LF reference (user-rejected hardware-wise).

Eval-harness notes for non-0021 clips: `make_cache.py CLIP.MP4` then
`eval_render.py RENDER.mp4 --cache .../CLIP_rates.npz`; the named WINDOWS
in eval_render.py are 0021-specific (ignore them, use the series npz).
Store Gyroflow.exe now refuses direct CreateProcess from this harness —
use the dev build `447e384b...\scratchpad\gyroflow-dev\Gyroflow.exe`
(renders identically) or `cmd /c start "" /wait`.

## Possible follow-ups (nothing blocking)

- DONE (Plan 2, 2026-07-19): Rust port shipped as o4fix-app GUI + CLI, portable zip on GitHub Releases (v0.1.0), CI on GitHub Actions. Multi-clip validation still open (deferred post-release).
- Validate on more clips from the same unit (only one test clip so far);
  o4fix prints per-burst optical drift — watch for calibration R² < 0.8
  fallback on clips without clean sections.
- Visual A/B: user should eyeball `eval_M2_tight.mp4` vs
  `eval_A_embedded.mp4` and `eval_M4_accelgate.mp4` (numbers are
  tracker-based; the M2-vs-M4 choice is perceptual).
- If DJI firmware changes the dvtm layout, `mp4patch.py verify` is the
  canary (it hard-fails on any mismatch before writing).
