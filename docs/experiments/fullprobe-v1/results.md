# fullprobe-v1: where measurable residual remains in v0.1.3+ output (2026-09-25)

Question: after the v0.1.3 refinement (wp1c in production), where does measurable
telemetry error remain, and is any of it a fixable target on the way to "completely smooth"?

Method: `warp_residual_probe` (O4_CROP=1.30,0.72, readout 5.092569 ms, mount 3) over the
**whole** production output (0073_fixed_v2 full clip, 0021_fixed_v2 full clip, 0060_fixed
around its two cap-skipped bursts), joined with the source clip's 30–180 Hz gyro-noise
envelope and the repair log's burst list (`analyze.py`, `bursts.py`). Independent check:
`render_residual_probe` on the matching Gyroflow render + fbclip intended-path removal,
correlated against the probe (`xcheck.py`, `xburst.py`). Data under
`target/experiments/fullprobe-v1/`.

## Findings

1. **Refined bursts are now the quietest part of the clip.** 0073 HP residual (|vec| RMS,
   1–8 / 8–45 Hz): refined bursts 1.18/0.89, near-burst margins 1.92/1.36, far-from-burst
   clean flight 1.54/1.59, rising to 3.5/4.4 at mild source noise 6–8 °/s.
2. **The non-burst residual is not a fixable target.** In four far-from-burst 0073 windows
   (0.5–9.5, 55–82, 288–315, 338–354 s) the probe and render-measured 1–8 Hz residuals are
   uncorrelated (|r| ≤ 0.26 per axis; inside bursts the same comparison gave 0.56–0.74).
   Extending refinement to the whole clip would inject probe noise. Not pursued.
3. **Cap-skipped bursts: the probe fails in fast motion, per axis.** 0021 powerloop
   21.7–26.1 s shows 67 °/s 1–8 Hz probe residual and 109.6 s 41 °/s, where earlier render
   evaluations were smooth — probe breakdown, and the 4° cap correctly blocks it. 0060
   48.7 s: probe 4–7 °/s per axis vs render 1–2 °/s (probe wrong, skip correct). 0060
   191.7 s: axis 0 is real (probe↔render r 0.90, slope 0.89, 3.7 °/s) while axes 1/2 are
   probe over-estimates (render 1.1 °/s vs probe 10 °/s). A render-free per-axis validity
   test was not found (regressions on telemetry rate are confounded because the phantom
   lives in the telemetry rate).

## Conclusion

By every instrument available, refined output now sits at the measurement floor except
in (a) the few cap-skipped fast-motion bursts, where the source-frame probe itself is
unreliable, and (b) clip-edge landings. Further progress toward "completely smooth" needs
the user to name the timestamps where judder is still visible in current output; the
instruments can no longer locate it. No production change.
