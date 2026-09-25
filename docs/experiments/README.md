# Stabilization experiment index

Start with [the current session handoff](../SESSION_HANDOFF.md) for user verdicts, environment setup, working-tree state, and the next step.

- [Calibration](calibration-v1/results.md): mixed/negligible real-data benefit.
- [Residual localization and zero decay](residual-v1/results.md): no meaningful visual improvement.
- [Tracking gates, seed control and two-frame measurement](tracking-v1/results.md): stronger numerical evidence, weak visual benefit.
- [Localized output smoothing](local-smooth-v1/results.md): confirmed visual regression; retired.
- [Spatial model diagnosis](spatial-v1/results.md): perspective fits better; regional disagreement remains.
- [Rolling-shutter ablation](rs-v1/results.md): possible slight benefit, no meaningful improvement.
- [Render-feedback telemetry refinement](feedback-v1/results.md): closed-loop in-burst correction, pending user review.

Compact reports and scores are here; large reproducible local artifacts are under target/experiments and are not source-controlled. No experimental candidate has been promoted.

- [Render-feedback telemetry refinement](feedback-v1/results.md): closed-loop in-burst telemetry correction from measured render residual; user: judder improved quite a bit, tiny residual.
