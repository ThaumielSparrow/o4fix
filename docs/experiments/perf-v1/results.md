# perf-v1: repair speed and resource usage (2026-09-25)

User: running o4fix (especially several processes at once) chokes a powerful PC; a midrange
laptop would freeze. Goal: cut wall time, CPU contention and memory without changing output.

## Baseline profile (`profile.py`, psutil 0.5 s samples + timestamped log)

0083 (215 s, 1440x1080 HEVC @100 fps, keyframe every 0.3 s), release build of main
(v0.2.0 core), 24 logical / 16 physical cores, 32 GB. Wall 209 s, 1320 CPU-s.

| stage | wall | avg cores | peak RSS | peak threads |
|---|---|---|---|---|
| calibration optics | 15 s | 8 | 1.1 GB | 150 |
| burst optics | 98 s | 6 | 3.5 GB | 374 |
| refinement | 95 s | 8 | 2.5 GB | 266 |

Findings:
1. **Oversubscription.** Single-threaded, the optical pair pipeline costs 17.5 ms/frame
   (decode 4.3, gray+resize 0.8, goodFeaturesToTrack 4.4, pyrLK 5.0, undistort 0.1,
   findEssentialMat RANSAC 2.9). The real run burns ~74 CPU-ms/frame: every rayon task opens
   its own `VideoCapture` (FFmpeg frame-threading, default thread count = all cores) and
   OpenCV's own parallel_for pool is active too, so ~20 tasks x many threads contend.
   Memory scales with the number of concurrent multi-threaded decoders.
2. **Load imbalance.** `par_iter` is per burst/window; the longest one runs alone at the end
   (burst optics: 84-113 s at ~1 core; refinement tail similar).
3. Seeking is cheap (0.1-0.17 s, keyframes every 0.3 s) — not a cause.
4. GUI runs up to 3 files concurrently (`concurrent_files`), multiplying 1-2.
5. No process/thread priority lowering: the desktop competes with the repair.

## Changes (branch `perf`)

- `o4core/src/threads.rs`: one bounded rayon pool per size, shared by the process
  (`Config.threads`, `None`/0 = auto = max(2, logical/2)). The optical and refine stages
  run inside it (`pool.install` in `pipeline::process`), never on the global pool.
- OpenCV threading off: `cv::setNumThreads(1)` once per process (it is process-global in
  4.12; the `opencv_serial_is_process_wide` test checks a worker thread sees 1). Every
  chunk `VideoCapture` opens with `CAP_PROP_N_THREADS = 1`.
- Windows workers: `THREAD_PRIORITY_BELOW_NORMAL` plus an EcoQoS opt-out (see below).
- Load balancing: every burst/window is cut into 100-pair chunks (consecutive chunks share
  their boundary frame, so the pair set is unchanged), `par_iter().with_max_len(1)` so
  each chunk is its own rayon task, outputs concatenated in order. Refine concatenates a
  window's chunks into its `PairSeries` before `signal::condition`. Pairs are independent
  in both stages (only the previous frame is carried; optical RNG is seeded per absolute
  frame). A chunk whose decode ends early ends its interval, as the old `break` did.
- CLI `--threads N`. GUI "CPU usage" Low / Auto / High = cores/4, auto, cores-1
  (`cpu_usage` in settings.json; files without the key read as auto, no version bump).
  Concurrent GUI jobs all get the same size, so they share one pool: the budget is a
  machine-wide cap and work stealing balances the jobs.

## Results

Same machine (i9-12900K: 8 P-cores with HT + 8 E-cores = 24 logical), one run at a time,
idle desktop. Stage boundaries are the log lines named in the table header; "refine" runs
up to "samples unchanged" and includes ~6.5 s of serial post-refine work (after-trace,
MP4 copy).

**Parity:** bit-identical. On 0083 (auto, `--threads 23`, `--threads 4`) and 0021 (auto),
extracted quats have max abs diff 0 against the baseline binary, the stdout logs (bursts,
drift, refinement corrections) match line for line, and the whole output MP4s are
byte-identical (`cmp`).

### 0083 (215 s clip, 105 s of optical sections)

| run | wall | CPU-s | peak RSS | peak threads | avg cores |
|---|---|---|---|---|---|
| baseline (EcoQoS-throttled run) | 208 s | 1317 | 3.5 GB | 374 | 6.3 |
| baseline (unthrottled run) | 112 s | 1438 | 3.5 GB | 375 | 12.8 |
| new, auto (12 threads) | **95 s** | **960** | **0.73 GB** | **16** | 10.1 |
| new, `--threads 23` | 80 s | 1336 | 1.26 GB | 27 | 16.6 |
| new, `--threads 4` (laptop proxy) | 197 s | 734 | 0.32 GB | 8 | 3.7 |

Per stage (wall / CPU-s / avg cores / peak RSS / peak threads):

| run | calibration optics (analyzing -> alignment) | burst optics (alignment -> refining) | refinement (refining -> samples unchanged) |
|---|---|---|---|
| baseline throttled | 14.7 s / 139 / 9.5 / 1.1 GB / 150 | 94.2 s / 518 / 5.5 / 3.5 GB / 374 | 97.5 s / 660 / 6.8 / 2.5 GB / 262 |
| baseline unthrottled | 10.5 s / 146 / 13.9 / 1.1 GB / 150 | 39.5 s / 503 / 12.7 / 3.5 GB / 375 | 60.6 s / 788 / 13.0 / 2.5 GB / 263 |
| new auto | 9.3 s / 103 / 11.1 / 0.51 GB / 16 | 31.9 s / 356 / 11.2 / 0.51 GB / 16 | 52.7 s / 501 / 9.5 / 0.73 GB / 16 |
| new 23 | 9.1 s / 137 / 15.1 / 0.91 GB / 27 | 24.0 s / 476 / 19.8 / 0.93 GB / 27 | 45.6 s / 722 / 15.8 / 1.26 GB / 27 |
| new 4 | 21.1 s / 81 / 3.8 / 0.22 GB / 8 | 72.3 s / 281 / 3.9 / 0.22 GB / 8 | 101.5 s / 371 / 3.7 / 0.32 GB / 6 |

### 0021 (176 s clip, 20% noisy)

| run | wall | CPU-s | peak RSS | peak threads | avg cores |
|---|---|---|---|---|---|
| baseline | 352 s | 2172 | 4.0 GB | 422 | 6.2 |
| new, auto | **142 s** | **1540** | **0.72 GB** | **16** | 10.8 |

### Findings from the measurement

1. **EcoQoS was the hidden bottleneck.** The baseline 0083 wall time swings between 112 s
   and 208 s on the same idle machine. When Windows 11 treats the (non-foreground) process
   as background work, it runs it only on the 8 E-cores: the process then plateaus at
   ~7.7 cores whatever the thread count (seen with 12 and 23 workers, normal or below-normal
   priority), and threads are ready but not running (thread CPU/wall 63%). Opting the pool
   workers out of execution-speed throttling (`SetThreadInformation(ThreadPowerThrottling)`,
   state 0) makes runs repeatable at the full budget (optical-only 0083: 88-92 s -> 47-49 s).
   Below-normal priority still yields to interactive work. The GUI UI thread and the
   pipeline's caller thread are untouched.
2. **Tail imbalance** remained after chunking until `with_max_len(1)`: rayon's default
   splitting left runs of ~8 consecutive chunks as one sequential leaf, so one worker
   finished the last ~17 s alone.
3. **Decode threads (1 vs 2), optical only, 12 workers:** 48.6 s / 467 CPU-s / 16 threads vs
   49.9 s / 539 CPU-s / 40 threads. Picked 1.
4. **Chunk length (100 vs 200 pairs):** 48.6 s / 467 CPU-s vs 49.7 s / 442 CPU-s. Each chunk
   pays one seek (FFmpeg decodes from an earlier keyframe), ~13% of the optical CPU at 100.
   Kept 100 (better tail for small budgets).
5. Per frame with 1-thread decode (12 workers, P-cores): decode+BGR ~22 ms CPU, pair
   rotation ~16 ms, gray+resize ~1 ms. Decode is now the largest single cost.
6. `--threads 23` is only 16% faster than auto for 39% more CPU (HT siblings contend),
   which supports half the logical cores as the default.
