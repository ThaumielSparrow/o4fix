# Raw gyro exposure at splice edges

2026-09-10. Research only; no production changes, repair output or render.

The user deferred lens correction and scaling research in favor of gyro-related causes. Backlog E0/E6/E7 still contain relevant unfinished checks: exact repair-stage attribution, post-burst fusion settling, and splice continuity. Prior zero-decay and approximate handback checks did not isolate the final orientation edge blend.

## Finding

Production `splice_orientation` blends the integrated replacement with a raw-orientation base over 0.3 s at both edges. Severe detection uses 0.2 s padding. Thus replacement rates can be fully optical while the final orientation still incorporates raw corruption. Rebased bursts also use a raw-orientation base with a varying world offset; rebase does not eliminate the edge blend.

The read-only `splice_exposure_probe` extracts original 0060 telemetry, calls production detection, snaps quaternion indices exactly as production does, and evaluates the same smoothstep formula. It does not reconstruct optical measurements or rerun a repair.

| Severe interval (s) | Noisy samples inside orientation ramps | Maximum raw slerp weight | Maximum band noise among exposed samples (deg/s) |
|---|---:|---:|---:|
| 105.830–106.610 | 200 | 26.37% | 209.30 |
| 223.439–223.957 | 118 | 26.37% | 13.26 |
| 225.027–226.179 | 201 | 26.36% | 321.76 |
| 248.345–249.316 | 201 | 26.38% | 435.95 |
| 308.011–309.279 | 202 | 26.38% | 181.95 |

Samples are approximately 1 ms apart. Weight and noise maxima need not occur together. Noise is the detector's band-RMS estimate, not ground-truth camera error. Slerp weight is an orientation interpolation parameter, not the fraction of raw angular rate or visible judder. Within all detected-noisy samples in these intervals, the separate light-rate partner weight is zero; fast handback was not remeasured here.

A synthetic stationary-camera test supplies perfect zero replacement rates and a raw 25 Hz, 0.5 degree orientation oscillation confined to 2.2–2.8 s within a padded 2–3 s interval. The production splice retains more than 1 deg/s spurious rate in the overlapping entry ramp, while the fully replaced interior is still to numerical tolerance. This establishes the leakage mechanism, not causation on real footage.

## Validation and reproduction

`cargo run --release --offline -p o4core --example splice_exposure_probe -- sample_vids/DJI_20260808151831_0060_D.MP4 target/experiments/splice-exposure-v1/exposure.json`

`cargo test --release --offline -p o4core --example splice_exposure_probe`: 1 passed. Use the OpenCV/LLVM environment from the handoff. Full sample rows remain in the target JSON; compact results are preserved in `metrics.json`.

## Next experiment

Prioritize boundary-aware replacement with blending entirely outside detected corruption, using unchanged optical estimation and clean/synthetic controls. First verify exact default repair parity. Track endpoint drift and rebase decisions because changing interval support can change both. Compare actual full-render excerpts before claiming a fix. Do not abruptly remove ramps: continuity is part of the experiment.

Next strongest alternatives are missed low-frequency corruption tails / post-burst DJI fusion settling, and exact fast-handback contamination at 249 s. Timing drift is conditional on evidence. Further lens/scaling work is deferred. No shipping binaries need rebuilding because production code is unchanged.


## Controlled candidate and render (2026-09-11)

`splice_ramp_repair` calls production optical_patch and splice_orientation twice: default 0.30 s and candidate 0.19 s ramps. The candidate fits the existing 0.20 s padding with a 10 ms margin. Intervals, rates, integration, drift, rebase decisions, and decay are identical. No enlarged intervals or new optical estimator. This is a diagnostic ablation, not a universal recommended ramp duration.

Default reconstruction agrees with the preserved MP4 to a maximum sign-folded quaternion component error of 4.6477e-8 (stored precision; not bit-exact floating-point parity). Candidate writeback verified unchanged timestamps and zero sign-folded value difference. The 19–22 s control orientation is unchanged to round-off. The synthetic test now verifies that the shortened ramp removes its injected corruption without introducing motion. Targeted Clippy checks passed (an existing unused imported smoother loop lint is scoped out).

Compared with the supplied patch, noisy-sample RMS rate discrepancy drops from 18.81/10.97/19.08/15.46 deg/s to approximately 0/1.03/0/0 in the four windows. The remaining 225-window contribution includes endpoint bridging. This is repair-path attribution, not an optical accuracy score. Maximum orientation changes are 2.34–3.28 degrees. Local edge acceleration RMS falls 19–48% at all five nearby intervals; peak acceleration does not increase. Whole-window peak acceleration is unchanged, showing that the biggest peaks also exist outside this changed mechanism.

Full candidate render completed at 1440x810, 100 fps, 38269 frames using baseline Gyroflow settings. Four 8 s / 800-frame comparisons and a 3 s / 300-frame clean control are validated. Left = default; right = shorter ramp. A decoded comparison frame was inspected for layout. No additional fixed zoom. Adaptive zoom can respond to the changed trajectory.

Measured changes in exported image-motion proxies on shared reliable support around severe bursts (negative means lower magnitude):

| Moment | 2–8 Hz translation | 2–8 Hz roll | 8–30 Hz translation | 8–30 Hz roll |
|---|---:|---:|---:|---:|
| 106 | -20.8% | -39.2% | -8.7% | -1.3% |
| 225 | +21.2% | -38.8% | -16.6% | -19.9% |
| 249 | -2.6% | -14.2% | +3.4% | -14.8% |
| 308 | +4.2% | -44.3% | +9.2% | +15.8% |

All four show reduced 2–8 Hz roll, but 225 s increases low-band translation and 308 s increases both high-band measures. These are similarity-based apparent-motion proxies, not independent camera truth or a perceptual verdict. The result is promising but mixed. **Human visual verdict pending; no production promotion.**

Files: `target/experiments/splice-exposure-v1/ramp019.MP4` is the verified repair; `ramp019-render.mp4` is the full render; `compare_106.mp4`, `compare_225.mp4`, `compare_249.mp4`, `compare_308.mp4`, `compare_clean.mp4` are reviews. `export_reviews.py` reproduces exports after rendering. `splice_boundary_probe` measures stored-quaternion derivatives; `splice_render_measure` measures two exported panels using the existing pair tracker. Numerical reports are preserved beside this document.


## Next-ranked lead: post-burst consistency

`settling_probe SOURCE CALIBRATION OUTPUT` tracked ten seconds after the four primary burst ends and the 19–22 s clean control. Raw gyro and calibrated optical rates were both filtered at 8 Hz; tracking included 0.5 s context on each side. Padded severe intervals and optical quality failures with 0.25 s neighborhoods were excluded. The saved production alignment was used unchanged. Targeted Clippy passed. All windows completed; summaries are in `settling.json`.

No common smoothly decaying 5–10 s signature emerged. One-second RMS gyro/optical residuals immediately after 106.61/226.18/249.32/309.28 s were 2.73/3.40/14.95/3.24 deg/s. The next second after 249.32 s falls to 2.78 deg/s. Delayed excursions occur at 227–229 s and 310–313 s, with additional later spikes. The three clean-control bins are 2.33/3.89/3.04 deg/s (the last has only 35 accepted pairs due to quality exclusion). These windows are not motion-matched; the control is also part of calibration, so this is not a held-out performance claim.

The localized 249 s endpoint discrepancy is more actionable than assuming universal fusion settling. Next priority: exact production handback and source contribution tracing across 248–251 s, separating the final ramp, fast handback and return to raw orientation. Compare multiple temporal optical estimates on common support before attributing the discrepancy to gyro rather than fast-turn optical error. Do not extend every repair by several seconds based on these results. A 30–180 Hz detector cannot exclude low-frequency gyro errors, but the current optical evidence does not prove them.


## User visual verdict

User confirms the right/0.19 s candidate is slightly but visibly less wobbly, explicitly not just placebo. Wobble and judder remain. At 308 s they do not perceive the measured sharp-shake increase and judge the candidate better. Treat this as confirmed visual improvement and the preferred research baseline; the proxy is not a perceptual veto. Continue targeted gyro-path investigation. No production promotion yet.
