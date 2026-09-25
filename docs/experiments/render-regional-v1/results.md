# render-regional-v1 — 2026-09-15

3x3 bilinear residual field on top of frozen v1 translation. Regional triplet medians minus shared acceleration,40ms smoothing,4–30Hz band, persistent-support fade (support across25triplets then3Hz lowpass), max6half-px and3% spatial gradient. Common1.04zoom. Three fixed-point inverse-map iterations and source-perimeter margin>4px. Missing nodes contribute no local deviation before temporal support fade; the fade is not a strict per-frame missing-data mask. No lens/source-pose estimator or gyro changes. Three tests and Clippy pass.

LEFT prior visually supported v1 translation; RIGHT experimental residual correction. Full-resolution single resampling, identical crop per comparison. Actual encoded preview bands, common valid support,0.5s edges excluded. Proxies are not perceptual ground truth.

| Window | 2–8Hz translation | 2–8Hz roll | 8–30Hz translation | 8–30Hz roll |
|---|---:|---:|---:|---:|
| late | -4.01% | -0.53% | -6.81% | -38.09% |
| control | +46.25% | -0.26% | +21.92% | -55.20% |
| additional | +10.41% | +8.76% | +7.27% | -3.66% |

Decision: not a material general improvement. Preserve as a negative/mixed trial, no visual review or promotion. Rotation during the long141–144s burst remains the strongest localized candidate; follow-up render-selective-v1 tests conservative burst eligibility and explicit off behavior elsewhere. All experiment renders/band probes complete. Production/baseline unchanged. Full/previews, commands and data preserved under target/experiments/render-regional-v1.
