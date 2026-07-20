# o4fix — DJI O4 Pro gyro noise repair

Early-2026 DJI O4 Pro air units (fw 01.00.07.00, camera "O4P") record
noisy gyro data during high-throttle flight; Gyroflow output shudders,
wobbles and micro-pans. o4fix repairs the MP4's embedded orientation
telemetry in place — severe noise bursts are replaced with motion
measured optically from the video frames — and writes `VIDEO_fixed.MP4`,
which loads in Gyroflow like a stock recording. Clean sections keep
their original bytes.

## Download (Windows)

Grab the latest `o4fix-vX.Y.Z-windows-x64.zip` from
[Releases](https://github.com/ThaumielSparrow/o4fix/releases), unzip,
run `o4fix-app.exe`, drop your videos, click Start repair. No install
needed (Windows 10 may prompt once for Microsoft WebView2).

- "healthy — nothing to repair": the clip's telemetry is fine, use the
  original.
- "Couldn't calibrate motion from this clip": no calm flight sections to
  calibrate against; the file is left untouched.
- Advanced settings: Default (M2) suits most flying; Sharp-turn (M4)
  recovers flip/roll crispness at the cost of slight extra high-frequency
  shake — try both on your own footage.

## CLI

`o4fix.exe VIDEO.MP4 [VIDEO2.MP4 ...] [-o OUT.MP4] [--severe 8 ...]`
Same pipeline and defaults as the GUI; `o4fix.exe --help` lists the
tuning flags.

## Repository layout

- `rust/` — the shipped implementation (`o4core` library, `o4fix` CLI,
  `o4fix-app` Tauri GUI). See `rust/README.md` for building.
- `o4fix.py`, `mp4patch.py` — the original Python research pipeline,
  kept as the golden reference the Rust port is verified against.
- `analysis/`, `tools/` — evaluation harness and golden-fixture dev
  tools (Python; not needed to use o4fix).
- `docs/` — design specs, implementation plans, release checklist.

Details of the underlying problem and the verification methodology live
in `CLAUDE.md` and `docs/superpowers/specs/`.
