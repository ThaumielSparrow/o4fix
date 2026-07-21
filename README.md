# o4fix — DJI O4 Pro gyro noise repair

Early-2026 DJI O4 Pro air units record noisy or corrupted gyro data during high-throttle flight which causes shaking in Gyroflow-stabilized footage.
o4fix repairs the MP4's embedded orientation telemetry in place. Severe noise bursts are replaced with motion measured optically from the video frames and writes `VIDEO_fixed.MP4`, which loads in Gyroflow like a stock recording.
Clean sections keep their original bytes.

## Download (Windows)

Grab the latest `o4fix-vX.Y.Z-windows-x64.zip` from
[Releases](https://github.com/ThaumielSparrow/o4fix/releases), unzip,
run `o4fix-app.exe`, drop your videos, click Start repair. No install
needed (Windows 10 may prompt once for Microsoft WebView2).

- "healthy — nothing to repair": the clip's telemetry is fine, use the original.
- "Couldn't calibrate motion from this clip": no calm flight sections to calibrate against; the file is left untouched.
- Advanced settings: Default (M2) suits most flying; Sharp-turn (M4) recovers flip/roll crispness at the cost of slight extra high-frequency shake.

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
