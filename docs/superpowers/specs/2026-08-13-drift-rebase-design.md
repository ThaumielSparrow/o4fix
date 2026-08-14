# Drift rebase with capped decay — design spec

Date: 2026-08-13. Approved by user in-session ("looks good, continue").

## 1. Context

Post-v0.1.1 residual on monster-burst clips (0027, 0057-0060) is the
**drift bridge**: each optical-patched severe burst must end on the raw
quat sample (clean zones bit-exact by design), and the integrated optical
path ends 26-175 deg away from that endpoint. `splice_orientation`
spreads that error inside the burst as a smoothstep rotation-vector
correction. Implied peak fake rotation rate = `1.5 * drift / duration`
(smoothstep max slope): 0060's 4:09 burst = 103 deg / 0.98 s ~= 150 deg/s
of sub-2 Hz fake rotation. That is the user-visible "small, slower
judder". Established (2026-07-21, measured): drift is mostly DJI fusion
divergence under monster vibration, not optical error; rate-weighted
spreading is WORSE; uniform smoothstep is the measured-best *bridge* —
this spec removes the bridge instead of reshaping it.

Key insight: Gyroflow's per-frame correction is `q_smooth^-1 (x) q_actual`.
Left-multiplying the whole orientation trajectory by a constant rotation R
leaves it unchanged — with horizon lock OFF (user's setup) stabilization
sees only the rate of change of orientation error, never absolute
attitude. Relative sample-to-sample rotations (what sub-frame/RS
correction uses) are exactly preserved. So a burst does not need to land
back on the raw endpoint at all.

## 2. Mechanism

`splice_orientation` maintains a running offset `O(t)` (unit quat,
identity at clip start), applied to every sample outside bursts:
`q_out = O(t) (x) q_raw`.

Per qualifying burst:
- Integrate the optical path from the offset entry quat `O(t_i0) (x)
  q_raw[i0]`; do NOT apply the smoothstep endpoint correction — the path
  ends where optical says it ends.
- New offset at burst exit: `O <- q_optical_end (x) q_raw[i1]^-1`.
- The existing `--ramp` 0.3 s edge slerps stay, now blending between
  offset-raw and the optical path (rate-content blending only).

After a burst, `O(t)` slerps toward identity at a capped angular rate
(`--drift-decay-rate`, default 1.5 deg/s; 0 = carry forever). A 100 deg
offset bleeds off over ~67 s — far below Default-smoothing bandwidth.
Offsets compose across bursts; a burst starting mid-decay integrates from
the current offset state. Decay pauses during subsequent bursts (offset
is folded into their entry state).

## 3. Per-burst gating

Rebase engages per burst only when the implied bridge rate
`1.5 * drift_deg / duration_s` exceeds `--drift-rebase-above` (deg/s;
0 = feature off = exactly current behavior). Below the threshold the
burst keeps today's in-burst smoothstep spread and contributes no offset.

Default threshold: **chosen by harness sweep**, not a priori. Constraint:
0021 (drift <= 55 deg over long bursts, currently excellent, implied
rates ~5-17 deg/s) must stay on current behavior unless eval shows rebase
helps it too. Monster bursts (100+ deg over ~1 s => ~150 deg/s implied)
qualify unambiguously. Candidate default ~20-30 deg/s pending sweep.

## 4. On-disk effect

- Samples under a non-identity offset are rewritten: constant-rotation
  change only (rates, HF vibration, relative motion preserved exactly in
  the rotated frame).
- Samples where `O` = identity keep original bytes. Clips where no burst
  crosses the gate (e.g. 0021 at default) remain bit-exact end to end.
- All mp4patch gates keep passing: decode matches telemetry-parser,
  nullpatch byte-identical, inject round-trip exact.

## 5. Surface

Two knobs, ported like `--gyro-trust-noise` was (Python `o4fix.py` +
Rust `o4core::config` + `o4fix-cli` args + GUI field + settings.json +
help text):
- `--drift-rebase-above DEG_S` — implied-bridge-rate gate (0 = off).
- `--drift-decay-rate DEG_S` — offset decay cap (0 = carry forever).

Default-on only if validation is clean (M2 precedent). Goldens
regenerated (`python/tools/dump_goldens.py`) after Python is the proven
reference.

## 6. Testing & verification

Order matters — cheapest kill-test first:
1. **Synthetic offset render (kill-test):** inject a constant 30 deg
   rotation over a whole clean span of 0021, render, diff per-frame
   corrections (`--export-metadata`) vs the offset-free render. Any
   difference beyond float noise kills the design (Gyroflow has an
   absolute-attitude dependency); stop and report.
2. Python implementation + unit checks (offset composition, decay rate
   cap, gate arithmetic, bit-exactness where O=identity).
3. Harness loop: render + eval 0060 (both complaint bursts) and 0027
   (drift-floor clip); regression eval 0021. Threshold sweep for the
   gate default.
4. Rust port with quat-level parity vs seeded Python, goldens, e2e.
5. Rebuild `target\release` binaries (stale-build rule, 2026-08-13
   incident).

Success criteria: monster-burst sub-2 Hz residual drops materially vs
v0.1.1 output on 0060/0027; all other eval cells within coupling noise
(<= 1.5 deg/s); 0021 unchanged at default gate; all mp4patch gates pass.

## 7. Risks & open questions

- **Absolute-attitude dependency in Gyroflow** — covered by kill-test 1.
- **Horizon lock ON would fight non-identity offsets** — decay bounds
  exposure; document in README + GUI help ("if you use horizon lock,
  set --drift-rebase-above 0").
- **Optical endpoint quality** — existing low-quality-flow skip logic
  already keeps filtered gyro for garbage segments; gated bursts never
  rebase on skipped segments (no optical path integrated there).
- **Adaptive zoom invariance** — theoretically invariant (corrections
  unchanged); confirmed empirically by kill-test 1's correction diff.
- **Post-burst DJI EKF re-convergence** (fake slow rotation in raw clean
  data after monster bursts) is a separate, unmeasured artifact —
  explicitly OUT of scope; note for a future session (measure raw-vs-
  optical LF rotation 5-10 s after monster bursts before designing
  anything).
