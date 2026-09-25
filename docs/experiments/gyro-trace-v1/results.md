# Exact gyro contributions and rebase edge offsets

2026-09-11. Research only; production defaults and shipping binaries unchanged.

The user confirms the 0.19 s splice-ramp candidate looks slightly but clearly better, including at 308 s. It is now the comparison baseline. The user does not perceive the sharp-shake regression indicated by the image-motion proxy; that proxy does not override the visual verdict.

## Exact handback trace

`gyro_trace_probe` uses an instrumented copy of production optical_patch with unchanged formulas, preserving every optical/medium/final rate, optical weight, handback weight, segment peak/trust, and rate index. Reconstructing default output matches the preserved MP4 with maximum sign-folded component error 4.6477e-8, identical to the previous production reconstruction. The raw cache is `target/experiments/gyro-trace-v1/trace.json`; compact results and reproducer are `summary.json` and `summarize_trace.py` here.

Handback is exactly zero in the 106 s and both 225-window severe intervals. At 249 s it peaks at 84.4%, but its added rate relative to optical-only is just 0.01337 deg/s RMS, 0.09012 peak. At 308 s the corresponding values are 0.00117 RMS and 0.00929 peak. The optical and medium gyro estimates agree during this handback. Thus handback is not a substantial source of remaining error at these specific moments. The prepared `handback_ablation` example was not run or rendered because the trace does not justify it. This does not establish handback safety for every other burst.

## Two isolated boundary experiments

Both reuse exact cached production rates, pass cached-baseline parity against the preferred 0.19 s repair, retain coverage checks, and pass embedded timestamp/quaternion writeback verification.

1. `boundary_pad_repair`: increase severe padding from 0.20 to 0.40 s while keeping the 0.19 s ramp. This moves repair boundaries farther from detected corruption; it can change intervals, endpoint drift and rebase decisions. The clip still has five rebases. Results are in `pad040-metrics.json`.
2. `edge_offset_repair`: retain original 0.20 s padding, 0.19 s ramps, exact rates, intervals, endpoint drift and rebase decisions. Only change how the rebase offset affects the raw-orientation path used during the entry/exit blends. Results are in `edgeoffset-metrics.json`.

The second experiment has stronger mechanistic support. Production interpolates the full carried-offset change over the entire burst when constructing the raw-orientation blend base. Consequently the base already moves during the entry blend, even before corruption begins. The exit blend has the corresponding problem. The research variant holds the pre-offset fixed through entry, and the post-offset fixed through exit; their interpolation occurs only in the fully replaced interior, where this base has zero contribution. It preserves the old path for overlapping ramps and non-rebased intervals.

A stationary-camera synthetic test with perfect zero patch rates and a 60-degree raw drift confined to the middle of the burst produces 14.197 deg/s false peak rate under the preferred shorter-ramp baseline, and zero under the offset correction. A second test preserves known multi-axis motion and verifies exact equality for the non-rebased path. Both tests pass. This identifies a real artificial-motion mechanism; it does not establish the full cause of real footage judder.

On stored 0060 telemetry, the offset correction reduces local boundary acceleration RMS by 10.4%, 12.9%, 4.0% and 5.2% at the four rebased intervals. The non-rebased 223 s interval is unchanged. Peak acceleration changes are -0.9%, 0%, +1.9%, -1.1% respectively. These derivatives include real motion and stored precision effects; they are not ground-truth errors. The clean 19–22 s control remains unchanged to round-off.

## Reproduction and interpretation

Use the OpenCV/LLVM environment in the handoff. Examples print positional usage. `cargo test --release --offline -p o4core --example edge_offset_test -- --nocapture` runs the two synthetic checks. Targeted Clippy passed for the trace and repair examples. Trace writes are local research artifacts, not a production API.

Gyroflow projects use unchanged baseline settings and full-clip smoothing context. `export_reviews.py pad040` or `export_reviews.py edgeoffset` produces two-panel excerpts after its full render finishes: left = user-preferred 0.19 s ramp; right = named follow-up. Do not combine changes before isolating their visual benefit. No user verdict on either follow-up exists yet.


## Completed render checks (2026-09-12)

Both full renders and all ten two-panel excerpts completed. The exporter verified 1440x810 / 100 fps / 38269 frames for the full clips, 800 frames for each complaint excerpt and 300 for each clean control. A decoded edgeoffset comparison frame was inspected for correct side-by-side layout. All eight optical measurement JSONs are complete with common reliable active support. The old process-session handles expired across the resumed turn; completion is established by the saved, parsed outputs and validated videos.

Changes relative to the user-preferred ramp019 baseline (negative = lower apparent motion):

| Variant | Moment | 2–8 Hz translation | 2–8 Hz roll | 8–30 Hz translation | 8–30 Hz roll |
|---|---|---:|---:|---:|---:|
| edgeoffset | 106 | -10.6% | -81.3% | -12.9% | -33.9% |
| edgeoffset | 225 | -15.0% | -25.4% | +27.4% | +13.3% |
| edgeoffset | 249 | +2.8% | -6.0% | -0.3% | +1.4% |
| edgeoffset | 308 | -12.7% | -57.2% | +12.9% | +26.7% |
| pad040 | 106 | +25.2% | -55.0% | +11.9% | -12.3% |
| pad040 | 225 | -12.2% | -14.0% | +0.7% | -5.8% |
| pad040 | 249 | +15.1% | -12.0% | +15.4% | +38.0% |
| pad040 | 308 | -4.1% | -43.0% | +1.4% | +8.5% |

These proxies do not determine the visual verdict. The isolated edgeoffset correction is the first review priority: strong synthetic support and larger low-band roll reductions at 106/308, with mixed high-band measurements. Wider padding is a secondary comparison and is not combined with the offset change. At 249 s neither mechanism establishes a convincing numerical fix; remaining interior motion error needs further investigation after visual review.

Review artifacts: `target/experiments/gyro-trace-v1/edgeoffset_compare_{106,225,249,308,clean}.mp4` and corresponding `pad040_compare_*.mp4`. Left is the already preferred ramp019, right is the named follow-up. No production promotion. The edgeoffset visual verdict is recorded below; wider-padding verdict remains pending.


## User visual verdict — significant improvement

User reviewed the edgeoffset comparisons and reports very noticeably reduced judder at 106, 225 and 308 seconds, now almost unnoticeable. At 249 seconds a tiny vibration remains, but the result is still much better than before. They explicitly classify this as a significant improvement for these clips.

Adopt **edgeoffset with 0.19 s ramps and original 0.20 s padding** as the preferred research baseline. This supersedes ramp019 alone. Do not infer a verdict on the separate pad040 candidate. The measured high-band increases at 225/308 did not predict the perceived improvement and must not override it. This is strong evidence that splice/rebase handling contributed materially to the reviewed judder; it does not establish generalization to other clips or eliminate all remaining error.

Next: preserve this candidate, localize the tiny remaining vibration at 249 s, and validate the correction on additional development clips before production promotion. Production remains unchanged; promotion would require integrating the research change, appropriate regressions and rebuilding both shipping binaries.
