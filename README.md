# o4fix

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

- "healthy": clip telemetry did not report any unhealthy sections.
- "Couldn't calibrate motion from this clip": no calm flight sections to calibrate against.
- "Insufficient optical motion coverage": a severe burst could not be measured reliably.
- Advanced settings: Default (M2) suits most flying; Sharp-turn (M4) recovers flip/roll crispness at the cost of slight extra high-frequency shake.
- **Horizon lock:** if you stabilize with Gyroflow's horizon lock ON, set
  "Drift rebase above" to 0 first.

## CLI

`o4fix.exe VIDEO.MP4 [VIDEO2.MP4 ...] [-o OUT.MP4] [--severe 8 ...]`
Same pipeline and defaults as the GUI; `o4fix.exe --help` lists the
tuning flags.
