# o4fix — DJI O4 Pro gyro noise repair

Early-2026 DJI O4 Pro air units record noisy or corrupted gyro data during high-throttle flight which causes shaking in Gyroflow-stabilized footage.
o4fix repairs the MP4's embedded orientation telemetry in place. Severe noise bursts are replaced with motion measured optically from the video frames and writes `VIDEO_fixed.MP4`, which loads in Gyroflow like a stock recording.
Clean sections keep their original motion, offset by a constant that
stabilization ignores (see the horizon-lock note below); with
`--drift-rebase-above 0` they keep their original bytes exactly.

## Download (Windows)

Grab the latest `o4fix-vX.Y.Z-windows-x64.zip` from
[Releases](https://github.com/ThaumielSparrow/o4fix/releases), unzip,
run `o4fix-app.exe`, drop your videos, click Start repair. No install
needed (Windows 10 may prompt once for Microsoft WebView2).

- "healthy — nothing to repair": the clip's telemetry is fine, use the original.
- "Couldn't calibrate motion from this clip": no calm flight sections to calibrate against; the file is left untouched.
- "Insufficient optical motion coverage": a severe burst could not be measured reliably; no output is written. Existing input and output files are preserved on repair failure.
- Advanced settings: Default (M2) suits most flying; Sharp-turn (M4) recovers flip/roll crispness at the cost of slight extra high-frequency shake.
- **Horizon lock:** if you stabilize with Gyroflow's horizon lock ON, set
  "Drift rebase above" to 0 first. Above the default gate o4fix carries a
  burst's leftover orientation drift forward as a constant offset — invisible
  to normal stabilization, but it would fight the horizon reference.
  Refinement (below) also carries a constant offset forward from each
  refined burst. Each burst normally adds at most 4 degrees (more for a
  burst refined past the cap, see below), and the offsets add up: 7 degrees
  by the end of 0021. For horizon lock, turn refinement off as well.
- **Refinement (default on; CLI `--no-refine`, GUI "Refine residual
  judder").** After the repair, o4fix re-measures the leftover judder
  inside each repaired burst from the source video frames (feature
  tracking with fisheye and rolling-shutter geometry, no Gyroflow in the
  loop) and corrects it. The correction is limited to the burst
  (1 Hz high-pass, 0.25 s pad, 0.15 s fade). A burst is skipped, and its
  repair is kept as is, when the correction would exceed 4 degrees, the
  window cannot be measured, or it is within 0.5 s of the clip start or end.
  **Cap gate (default on; CLI `--refine-cap-gate 0.75` or
  `--no-refine-cap-gate`):** a burst over 4 degrees is still refined when
  the rotation-only measurement and a translation-aware (essential-matrix)
  estimate of the same residual agree (1-8 Hz correlation >= 0.75), because
  then the measurement is trustworthy. Validated on 8 clips: it fixed
  visible judder on 0083 and 0005 (0:17-0:30) and admitted no burst that
  forced refinement made worse. **Agreement floor (default on; CLI
  `--refine-agreement-floor 0.3` or `--no-refine-agreement-floor`):** the
  same check in reverse under the cap. When the two estimates disagree
  (below 0.3), the measured leftover is not a gyro error, and correcting it
  added judder (0073 2:58.8 render wobble 2.9 -> 16.8), so the burst keeps
  its repair unrefined. It
  is the slowest stage: on 0083 (215 s clip) at the Auto CPU setting on a
  24-thread desktop, refinement takes 53 s of a 95 s repair.
- **Handback noise veto (default on; CLI `--handback-noise-veto LO HI`,
  default 100 200, or `--no-handback-veto`).** During fast motion the repair
  normally hands orientation back to the gyro. On a throttle punch the gyro
  can carry its worst noise at that exact moment, so handback is now
  blocked wherever the local gyro noise (30-180 Hz, max over +/-0.15 s)
  passes 100-200 deg/s. Fixed the judder at 1:54 in 0083 (render wobble
  135 -> 41 deg/s); 0060 and 0071 are unchanged.
- Splice ramps default to 0.19 s (was 0.3 s). The edges of each
  repaired burst now start from the correct orientation offset.
- **CPU usage (GUI setting; CLI `--threads N`).** Video analysis runs on
  a bounded pool of below-normal-priority worker threads: Auto (default)
  uses half the logical cores, Low a quarter, High all but one. Clips
  processed at once share the same budget. The output is identical for
  every setting.

## CLI

`o4fix.exe VIDEO.MP4 [VIDEO2.MP4 ...] [-o OUT.MP4] [--severe 8 ...]`
Same pipeline and defaults as the GUI; `o4fix.exe --help` lists the
tuning flags.

## Repository layout

This is a Rust-first project: the Cargo workspace is the repository root.

- `o4core/`, `o4fix-cli/`, `o4fix-app/` — the shipped implementation
  (`o4core` library, `o4fix` CLI, `o4fix-app` Tauri GUI). See
  `docs/development.md` for building.
- `python/o4fix.py`, `python/mp4patch.py` — the original Python research
  pipeline, kept as the golden reference the Rust port is verified against.
- `python/analysis/`, `python/tools/` — evaluation harness and
  golden-fixture dev tools (Python; not needed to use o4fix).
- `docs/` — design specs, implementation plans, release checklist,
  and `development.md` (Rust build/dev notes).

Details of the underlying problem and the verification methodology live
in `CLAUDE.md` and `docs/superpowers/specs/`.

## Development handoff

Current stabilization research and session continuity: [docs/SESSION_HANDOFF.md](docs/SESSION_HANDOFF.md).
