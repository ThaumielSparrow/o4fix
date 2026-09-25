# Rotation-only residual experiment — 2026-09-15

User instruction: continue exploring until a material render improvement is found, then show the visual. Do not stop at another negative diagnostic. This trial is complete but withheld as a mixed result; regional work follows. No production/source/telemetry changes or promotion.

Matched diagnostics on the prior v1-corrected full-resolution output use identical LK tracks and entire-region holdouts. Compare ordinary least-squares translation, ordinary least-squares translation+rotation, and rotation alone (the rotation coefficient from the joint fit). Late held-out energy reductions: -5.82%, +8.41%, +10.08%. Thus a rotation-specific component predicts other regions. Control rotation-only transfer is -6.13%; additional +1.25%. The fit is an image acceleration pattern, not proof of physical gyro or camera-rotation error.

Trial: retain saved v1 translations. Integrate the fitted angular acceleration, Gaussian sigma40ms/radius120ms, band-limit the resulting correction to4–30Hz, then compose rotation after the prior translation and common1.06 zoom in one full-resolution Lanczos warp. Both panels have identical extra crop. Enforce <=0.5degree correction and full inverse-corner source margin >4px. Final maxima: late0.4418deg, clean0.0191deg, additional0.0828deg. An earlier80ms/2–30Hz preflight exceeded the bound (1.219deg), refused before encoding, and was narrowed; do not portray final parameters as preregistered independently of this preflight.

LEFT prior v1 translation; RIGHT translation+rotation. Remeasure encoded previews with existing independent pairwise motion-band estimator; common valid support and0.5s edges excluded. Late has1248common pairs due one low-quality baseline fit; other details in summary.json. These are motion proxies, not a visual verdict.

| Window | 2–8Hz translation | 2–8Hz roll | 8–30Hz translation | 8–30Hz roll |
|---|---:|---:|---:|---:|
| late | +5.56% | -11.21% | +0.98% | -41.86% |
| control | -18.49% | -24.44% | -11.52% | -76.03% |
| additional | -9.21% | -15.38% | +17.13% | -14.67% |

Decision: late high-band roll decreases about42%, but additional high-band translation rises17%. Do not promote or present as a general solution. No user visual verdict. Keep existing v1 translation reference. This motivates a bounded spatially varying rendered-image field rather than assuming all regions share one rigid motion. Lens and source gyro/pose estimation remain unchanged.

Five probe tests and two correction tests pass; Clippy passes with warnings denied; release examples built. Source-margin checks pass before encoding. Full/preview clips and correction arrays are preserved under target/experiments/render-rotation-v1. run_reviews.py reproduces previews and band scoring from rendered outputs. Final frame-count verification is included with the regional experiment verification. Existing nom future-compatibility warning remains.
