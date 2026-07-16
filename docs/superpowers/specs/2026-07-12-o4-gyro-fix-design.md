# DJI O4 Pro Gyro Noise Fix — Design

Date: 2026-07-12

## Problem

DJI switched the gyro on the O4 Pro air unit in early 2026. The new gyro is
more sensitive to noise; during high-throttle maneuvers (e.g. powerloops) the
embedded gyro data carries broadband noise from ~10 Hz up to the ~180 Hz
internal filter cutoff (10–35 °/s RMS, spikes to ±370 °/s, worst on yaw).
Gyroflow-stabilized footage shudders. Gyroflow's built-in low-pass at
30–50 Hz reduces but does not remove the shudder (noise extends below 30 Hz);
0.5 Hz removes it but destroys stabilization.

## Analysis findings (from test_gyro.csv, Gyroflow per-frame export)

- Orientation quats at clean 1 kHz; org_gyro columns are all zeros in export.
- Calm cruise: >30 Hz residual ~1 °/s RMS. Noise is throttle-correlated.
- Real stick/camera motion lives below ~10–20 Hz → separable from noise.

## Solution

CLI tool `o4fix.py <video.mp4>`:

1. Extract raw gyro (+accel) from the original O4 Pro .mp4 using
   `telemetry-parser` (same library Gyroflow uses).
2. Clean the gyro:
   - **Hampel pre-pass**: rolling-median outlier rejection to kill isolated
     spikes without touching the rest.
   - **Noise-adaptive selective filter**: estimate local noise power from the
     30–180 Hz band energy in a rolling ~100 ms window (that band is known to
     be pure noise). Blend per-sample, with smooth crossfades, between a light
     zero-phase LPF (~50 Hz) where clean and an aggressive zero-phase LPF
     (~8–10 Hz) where noisy.
3. Write `<video>.gcsv` (GCSV v1.3, correct scales + IMU orientation in
   header) next to the video. User loads it as motion data in Gyroflow,
   replacing embedded telemetry.

Knobs: `--noise-thresh`, `--strong-cutoff`, `--light-cutoff`, `--lpf N`
(plain zero-phase LPF fallback mode for comparison).

## Validation

- Before/after plots of 7–12 s powerloop and a clean segment; confirm noise
  band removed, sub-10 Hz motion preserved.
- Cross-check extracted raw gyro aligns with quat-derived rates from the
  Gyroflow export of the same clip.
- Final acceptance: user renders the clip in Gyroflow with the .gcsv and
  confirms shudder is gone.

## Out of scope

Modifying the .mp4 embedded telemetry in place; GUI; batch processing
(trivial to add later).
