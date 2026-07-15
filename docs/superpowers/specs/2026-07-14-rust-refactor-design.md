# o4fix Rust refactor — design spec

Date: 2026-07-14
Status: approved pending final user review
Repo: https://github.com/ThaumielSparrow/o4fix

## 1. Context

`o4fix.py` repairs noisy DJI O4 Pro gyro telemetry by rewriting the MP4's
embedded orientation quaternions in place (see CLAUDE.md for the full
problem history; the pipeline beats the healthy-gyro reference on severe
sections and matches it elsewhere). It is currently a Python CLI requiring
`pip install telemetry-parser numpy scipy opencv-python`.

Goal: a "plug and play" desktop app for non-technical FPV pilots — download
a zip, drop MP4s in, get `_fixed.MP4` files out — with no Python or other
runtime to install. The core is ported to Rust; the existing Python
pipeline stays in the repo untouched as the golden reference for verifying
the port.

Key de-risking fact: telemetry-parser (the decoder the Python tool uses via
bindings) is natively a Rust crate — the same one Gyroflow uses. The Rust
port links it directly, for both extraction and the round-trip verify gate.

## 2. Scope decisions (agreed)

- **Port scope**: default MP4-repair pipeline only. The `--gcsv` legacy
  mode and the `analysis/` eval harness stay Python-only dev tools.
  gcsv-only flags (`--gcsv`, `--orientation`, `--lpf`, `--plot`,
  `--no-optical`) do not exist in the Rust app.
- **Platforms**: Windows first; `o4core` avoids Windows-only APIs so a
  macOS build is a future CI job. macOS is explicitly out of scope for v1.
- **GUI**: drop-zone + job queue + advanced settings panel exposing the
  full MP4-mode tuning surface, including the M2/M4 profile switch.
- **Distribution**: portable zip on GitHub Releases. No installer, no code
  signing, no auto-update in v1. Static-linked minimal OpenCV build is a
  backburner size optimization (approach 2 fallback).
- **Architecture**: Tauri 2 shell + `opencv` crate with bundled prebuilt
  DLLs (approach 1).

## 3. Architecture

New `rust/` directory beside the Python code:

```
rust/
  Cargo.toml            workspace
  o4core/               pure library; no GUI/CLI dependencies
    telemetry.rs        extract_quats: telemetry-parser crate + sort/dedupe/
                        hemisphere-continuity/normalize (o4fix.extract_quats)
    quat.rs             mul/conj/exp/log/slerp/smoothstep, quats_to_rates
    dsp.rs              butter + filtfilt, rolling median (hampel),
                        uniform_filter1d, interp, gradient
    detect.rs           noise envelope, alpha blend factor, find_intervals
    optical.rs          video_rates, pair_rotation, fit_video_alignment
                        (opencv: VideoCapture, goodFeaturesToTrack, pyrLK,
                        fisheye undistortPoints, findEssentialMat,
                        decomposeEssentialMat, Rodrigues; procrustes via SVD)
    patch.rs            optical_patch + splice_orientation
    mp4.rs              mp4patch.py port: box walker, meta-track sample
                        table (stsz/stco/co64/stsc), protobuf field scan,
                        aligned slots, patch_video, nullpatch/inject gates
    pipeline.rs         Config struct (all tuning params + defaults),
                        process() orchestration, progress callback,
                        cancellation flag
  o4fix-cli/            thin clap binary mirroring o4fix.py MP4-mode flags;
                        doubles as the golden-test driver
  o4fix-app/            Tauri 2 shell; plain HTML/CSS/JS frontend
                        (no Node build step)
```

Python stays at the repo root as the golden reference implementation.

## 4. Core pipeline

Data flow is a 1:1 mirror of Python `process_mp4`:

extract quats → quats_to_rates → adaptive_clean (hampel + 30–180 Hz
band-RMS noise envelope + light/strong LP blend) → optical_patch
(noisy/calibration interval selection, per-interval video rates, procrustes
video→gyro fit with time-shift refinement, R² ≥ 0.8 gate, rate-aware
handback incl. optional fast-wide/M4 branch with accel gate) → severe
interval detection (band-RMS > `severe`) → splice_orientation (integrate
optical rates, smoothstep drift spread, slerp edge ramps) → out_to_file
quat transform + per-row sign pinning → in-place f32 LE byte write →
round-trip verify gate.

### Numerics requirements

- All math in f64; f32 only at the file boundary (matches Python).
- `filtfilt` must reproduce scipy semantics: odd-extension padding,
  `padlen = 3*max(len(a),len(b))`, `lfilter_zi` initial conditions.
  First choice: `sci-rs` crate validated against golden fixtures;
  fallback: hand-port (~150 lines).
- `uniform_filter1d` must match scipy's origin convention for **even**
  window sizes (used: `int(0.2*fs)` = 200 at 1 kHz).
- Stable sorts, `searchsorted` left/right semantics, `np.interp` edge
  clamping, `np.gradient` central/one-sided edges — all pinned by tests.
- Parser transform (from mp4patch.py, verified session 3):
  `q_out = (0,0,1,0) ⊗ q_file ⊗ (0.5,-0.5,-0.5,0.5)` + norm-jump sign
  continuity; slot→1 kHz-sample mapping replicates extract_quats
  sort/dedupe over the parser's flat stream.
- telemetry-parser crate pinned by git rev; the pin is validated by golden
  test (extraction must equal the Python bindings' output on the test
  clip), not trusted.
- OpenCV pinned to the same 4.x minor on Python (golden) and Rust sides so
  decoded frames are bit-comparable (H.265 decode is spec-exact).

### Unchanged-zone guarantee

Samples outside severe bursts keep their original file bytes (same
mechanism as Python: rows equal to the deduped reference keep original
bytes; per-row sign pinned to the previous written value).

## 5. Parallelism

- **Within a job (v1)**: the optical stage rayon-parallelizes over
  intervals (noisy + calibration), one `VideoCapture` per interval —
  intervals are pure functions by design. Rayon pool bounded (~cores/2,
  tuned once on the test clip) because OpenCV LK and ffmpeg decode thread
  internally. RANSAC RNG seeded per frame pair (OpenCV RNG is
  thread-local) so results are deterministic under any scheduling.
- **Across jobs**: queue is a worker pool; `concurrent_files` setting in
  the advanced panel, default 1, cap 3. Marginal benefit (I/O overlap)
  once interval parallelism saturates cores; the knob exists, the default
  is safe for spinning disks.
- DSP stages are milliseconds and stay single-threaded. Copy/verify are
  disk-bound.

## 6. GUI (o4fix-app)

Main window:

- Large drop zone ("Drop DJI O4 Pro videos here, or click to browse");
  native file picker as alternative.
- Job queue, one row per file: status chip (queued → analyzing → measuring
  motion → patching → verifying → done / healthy-nothing-to-repair /
  error) + progress bar.
- Collapsible log pane mirroring CLI detail (burst intervals, per-burst
  optical drift, alignment R²).
- Output: `VIDEO_fixed.MP4` next to source by default; optional output
  folder override in settings.

Advanced settings drawer:

- Profile selector: **Default (M2)** / **Sharp-turn (M4)** / **Custom**.
  M4 = `fast_wide_cutoff 16` (accel gate on). Any manual field edit flips
  the selector to Custom.
- Full MP4-mode tuning surface, grouped like the CLI argument groups
  (repair thresholds / filter tuning / optical), each field with the
  argparse help text as tooltip. Reset-to-defaults button.
- `concurrent_files` (1–3, default 1).
- Settings persist to JSON in the app config dir.

Tauri plumbing:

- Commands: `pick_files`, `start_queue(files, config)`, `cancel`.
- Events: `job_progress {file, stage, pct, detail}`, `job_done {status}`,
  `log_line`.
- Progress originates from `o4core`'s progress callback; the shell
  re-emits as events. CLI consumes the same callback for stdout printing.
- Cancellation: per-job `AtomicBool` checked between stages and between
  optical intervals; cancel mid-file deletes any partial output.

## 7. Error handling

Per file; each job runs under `catch_unwind` so one bad file never kills
the queue.

| condition | behavior |
|---|---|
| no quat telemetry / wrong camera | error: "No DJI O4 telemetry found — is this an O4 Pro recording?" |
| no severe bursts | green "healthy, nothing to repair"; no output written |
| optical calibration fails or R² < 0.8 | error: "Couldn't calibrate motion from this clip (needs some clean flight sections) — file left unrepaired"; no output |
| round-trip verify gate fails | output deleted; hard error (the app can never ship a corrupt file) |
| I/O failure (disk full, locked file) | surfaced verbatim with failing path |
| cancel | partial output deleted; row marked "cancelled" (re-queueable by re-adding the file) |

## 8. Configuration

`Config` struct defaults = the tuned Python defaults:

| param | default | notes |
|---|---|---|
| severe | 8.0 °/s | 30–180 Hz band-RMS splice threshold |
| severe_pad / severe_merge | 0.2 / 0.2 s | tighter than 0.3/0.5 (session 3) |
| ramp | 0.3 s | slerp edge cross-fade |
| light_cutoff / strong_cutoff | 25 / 2.5 Hz | |
| noise_low / noise_high | 1.5 / 5.0 °/s | |
| noise_band | 30–180 Hz | |
| noise_window | 100 ms | |
| hampel_window / hampel_sigma | 7 / 6.0 | |
| optical_cutoff | 8.0 Hz | |
| handback_cutoff | None (= optical_cutoff) | |
| fast_handback | 100–250 °/s | |
| patch_pad / patch_merge | 0.5 / 1.0 s | |
| optical_noise | None (= noise_low/high) | |
| fast_wide_cutoff | 0 (M2); 16 = M4 profile | |
| fast_wide_ramp | 150–300 °/s | |
| fast_wide_accel | 1500 °/s² | 0 disables gate |
| anchor_mode / anchor_cutoff | off / 1.5 Hz | |
| concurrent_files | 1 | app-level, not in Config |

## 9. Testing & verification

Four layers, strictest where the risk is:

1. **Unit tests** (everywhere incl. CI; no clip): pure math vs small
   numpy/scipy-generated fixtures checked into the repo — filtfilt
   padding/zi, even-window uniform_filter1d, rolling median, quat
   exp/log/slerp edge cases, interp/gradient/searchsorted semantics.
2. **Golden stage tests** (local; need the test clip): new small Python
   dump script runs the existing pipeline once on
   `sample_vids/DJI_20260711124046_0021_D.MP4` and saves every
   intermediate to npz: extracted t/q, omega, noise envelope, alpha,
   interval sets (noisy/calib/severe), seeded optical tv/ov/qv, fit
   (shift, N, R²), patched rates, final q_out. Rust matches stage by
   stage: extraction and intervals **exactly**; filtered rates and final
   quats within 1e-6 max-abs component difference; optical near-exact
   under matched per-pair seeds (exact per-stage tolerances pinned in the
   implementation plan as each stage lands).
   Failures localize to a stage instead of debugging end-to-end.
3. **Byte-level gates** (integration tests, local): decode matches the
   telemetry-parser crate exactly; nullpatch → byte-identical file;
   inject → parser returns exactly the injected values.
4. **End-to-end acceptance**: Rust CLI output vs Python's existing
   `DJI..._D_fixed.MP4`: clean zones byte-identical, repaired-zone quats
   within tolerance. Once before first release: Gyroflow render of Rust
   output through the existing `analysis/` harness must sit on the M2 row
   within eval noise.

## 10. Repo, packaging, CI

- **Git**: init at project root; `.gitignore`: `sample_vids/`, analysis
  caches, `rust/target/`, golden npz dumps, `*_fixed.MP4`. Commit Python
  reference + `rust/` + docs. Remote:
  `github.com/ThaumielSparrow/o4fix` (check for pre-created README and
  reconcile before first push; `gh` CLI is authenticated).
- **CI** (GitHub Actions, windows-latest): every push — build, clippy,
  rustfmt, unit tests (layer 1 only). OpenCV from the prebuilt Windows
  release, cargo cached. On version tag — release build, assemble
  portable folder, zip, publish GitHub Release.
- **Portable folder**: `o4fix-app.exe`, `o4fix-cli.exe`,
  `opencv_world4xx.dll`, `opencv_videoio_ffmpeg4xx_64.dll`, `README.txt`.
  Expected zip ~50–80 MB.
- **WebView2**: Tauri embedded download-bootstrapper (no-op on Win11,
  one-time small prompt on bare Win10).

## 11. Risks & open questions

- **Spike (first implementation task)**: confirm the telemetry-parser
  Rust crate exposes the per-sample quaternion stream in emission order
  (needed for the 1:1 slot alignment in `mp4.rs`). The data is proven
  present (Python bindings expose it); only the Rust API shape needs
  confirming.
- **filtfilt parity**: highest-risk numeric port; mitigated by layer-1
  fixtures + layer-2 golden stages + hand-port fallback.
- **Frame-decode parity**: H.265 decoding is spec-exact, but resize/
  cvtColor must use the same OpenCV version both sides; pinned.
- **RANSAC determinism**: per-pair seeding required for golden
  comparisons; unseeded production runs may differ from goldens by
  RANSAC noise (acceptable; goldens use seeded mode on both sides).
- **opencv crate build friction on CI**: known-fiddly env vars; mitigated
  by pinning the prebuilt OpenCV release and caching.

## 12. Success criteria

1. All four test layers pass; gates (nullpatch/inject/round-trip) green.
2. Rust CLI on the test clip reproduces Python output: clean zones
   byte-identical, repaired zones ≤1e-6 quat difference (seeded).
3. Pre-release render metrics match the M2 row of the CLAUDE.md table
   within eval-coupling noise.
4. A non-technical user on stock Win11 can: download zip → unzip → run
   exe → drop MP4 → get `_fixed.MP4`, with no installs beyond the
   possible WebView2 bootstrap prompt.
5. GUI exposes M2/M4 profiles + full advanced tuning; settings persist;
   queue handles multi-file with per-file progress, cancel, and per-file
   error isolation.
