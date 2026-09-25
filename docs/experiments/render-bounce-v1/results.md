# render-bounce-v1 — 2026-09-15

Separate2–8Hz shared residual correction on top of frozen v1 translation.80ms smoothing followed by2–8Hz bandpass; max6half-px. Common1.06zoom after1.04preflight lacked Lanczos source margin (1.839px), refused before rendering. Final source margins>4px. Two correction tests and initial Clippy pass; final combined verification includes the crop-only update. No gyro changes. A cached regional band diagnostic finds+20.74% transfer in late2–4Hz but negative transfer in4–8 and8–30Hz; additional all negative, control insufficient region support. This does not establish an automatic correction gate.

LEFT prior visually supported v1 translation; RIGHT experimental residual correction. Full-resolution single resampling, identical crop per comparison. Actual encoded preview bands, common valid support,0.5s edges excluded. Proxies are not perceptual ground truth.

| Window | 2–8Hz translation | 2–8Hz roll | 8–30Hz translation | 8–30Hz roll |
|---|---:|---:|---:|---:|
| late | +11.41% | -3.94% | -2.51% | -0.97% |
| control | +5.94% | +11.07% | -16.27% | +2.40% |
| additional | +21.45% | -7.50% | +1.43% | -3.33% |

Decision: not a material general improvement. Preserve as a negative/mixed trial, no visual review or promotion. Rotation during the long141–144s burst remains the strongest localized candidate; follow-up render-selective-v1 tests conservative burst eligibility and explicit off behavior elsewhere. All experiment renders/band probes complete. Production/baseline unchanged. Full/previews, commands and data preserved under target/experiments/render-bounce-v1.
