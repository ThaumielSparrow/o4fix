# Selective rotational residual repair — 2026-09-15

## Outcome

This is the strongest localized review candidate from the continued investigation, on top of the v1 translation correction the user found visibly helpful. During the independently gyro-detected141.253085–144.451232s burst, actual encoded-preview roll RMS decreases46.13% in2–8Hz and88.53% in8–30Hz. Translation changes+1.27% / -0.85% in those bands. These are apparent-motion measurements, not a quantified percentage of perceptual improvement. User confirms a visible reduction in both review views; see verdict below.

Whole14second window metrics remain mixed: low-band translation/roll -11.25/-7.48%, high-band translation/roll +8.62/-27.19%. Do not conceal the full-window translation increase or assume it is merely tracking noise. RANSAC feature selection, encoding, transition behavior and filtering may contribute; only the localized gain is established numerically. This is not a complete fix for all remaining judder or for the earlier135–140s events.

The zero-correction controls still show nonzero optical-proxy deltas between encoded panels (including additional high-band translation+9.87% and roll+13.45%). Both panels were generated with identical image transforms there, so encoding/measurement variability is material; do not treat small percentage changes as physical improvement or regression. The much larger selected-burst roll change must still be judged visually.

The additional and clean windows receive zero additional commanded correction. Their v1 translations are preserved. Production, source telemetry and shipping binaries remain unchanged; no promotion or commit.

## How the candidate was selected

The user asked to keep exploring until a material render improvement exists, then show the visual. Rotation-only, regional-field and2–8Hz translation trials were completed and retained. Unconditional rotation strongly improved late roll but worsened additional translation. Regional and low-frequency translation trials were mixed/negative. See their respective reports; these were not shown as successes.

A matched held-out-region diagnostic on the v1-corrected full-resolution footage separated ordinary least-squares translation from rotation. The long141–144s event shows repeatable predictive benefit from rotation alone. Regions share images/LK processing; the diagnostic does not prove physical camera/gyro rotation error or eliminate correlated tracking bias.

Eligibility is evaluated only inside existing severe gyro-corruption intervals from residual-stage-v1/0073/stages.json. A burst must be wholly observed, contain at least five half-second blocks, and have a positive2.5th-percentile paired block-resampling gain in held-out-region rotation-only squared acceleration error (4000resamples, seed271828). The accepted event has seven blocks/320triplets and gain interval[0.1126,0.2676]. Three shorter late events and all additional events fail the data-length or gain condition; clean has no candidate burst. Requiring four or six blocks instead of five yields the same selection on these windows.

This is an exploratory eligibility rule developed on known clips, not an independently validated confidence probability or a production detector. Half-second block resampling is a stability check and does not remove all temporal/spatial dependence. The arbitrary minimum-block safeguard needs unseen-footage validation, particularly on short bursts; abstention deliberately leaves these unresolved. Do not describe the known-window selection as generalization.

## Correction and safeguards

Retain frozen v1 translation corrections. Estimate residual angular acceleration from full-resolution three-frame LK features using the rotation field fit. Gaussian sigma40ms/radius120ms followed by4–30Hz bandpass produces the residual angle correction. Multiply it by a200ms smoothstep envelope entirely inside accepted burst boundaries. Compose rotation after the prior translation in one full-resolution Lanczos warp. Both panels use1.06zoom (5.66% linear field-of-view cost; more than v1's1.04zoom). No image blur or frame interpolation. Lens calibration, source gyro repair and source pose estimation are unchanged.

Refuse angles above0.5degree or inverse-warp source-corner margin below4pixels. Final late maximum is about0.4418degree. Additional/control angles are exactly zero; outside accepted intervals, both weights and residual angles are exactly zero. A smooth fade reduces the risk of transition steps but does not prove zero added perceptual artifact. Boundary behavior and whole-window metrics remain part of review.

The earlier broad80ms/2–30Hz rotation preflight was rejected at1.219degree before rendering. Final settings derive from the narrower rotation experiment; do not count this as independently preregistered tuning. Production invariants remain unchanged.

## Visuals

All in target/experiments/render-selective-v1:

- compare_detail.mp4: five-second, native-pixel crop of the upper-right scene, source140.5–145.5s. LEFT previous v1 translation; RIGHT selective residual correction. Same640x360 crop per panel,1280x360 overall. This is explicitly a magnified/detail view, not the full field of view.
- compare_focus.mp4: same five seconds, full image side by side,1440x406 preview.
- compare_late.mp4 / compare_late_full.mp4: entire132–146s review at preview/native full resolution.
- compare_control and compare_additional, each with _full companion: verify zero extra commanded correction and preserve the same reference context.

At the five-second clip's start the new rotation is off; it fades in around0.75s and fades out before3.96s. The copied v1 translation remains active according to its original saved trajectory. The detail frame at source143.5s was inspected for crop/layout. Static inspection is not a perceptual motion verdict.

## Validation and provenance

Targeted example tests and release Clippy with warnings denied pass for the new experiments. This includes a synthetic textured-image pan-versus-shake tracking check, known rotation-field recovery, correction sign/steady-motion checks and regional inverse-field consistency. No new production tests/rebuild required because production code was not changed. Existing nom future-compatibility warning remains.

verify.py counts decoded frames and verifies dimensions/cadence for26videos across all four new trials, including500frames in both five-second visuals. It checks exact-zero angular correction outside selected support and in both controls, and validates angle/source-margin limits. summary.json retains all video checks, selected events and full/localized metrics. manifest.json hashes experimental source/binaries, probe inputs, transformations, gate rules, stage intervals and outputs, and chains to the earlier full-resolution source manifest. Regional acquisition preceded an unused-import removal and cfg(test) addition; final rebuilt runtime logic is unchanged.

Reproduction: handoff OpenCV environment, build the named examples. prepare_gate.py writes new gate files exclusively; run_reviews.py late control additional renders only missing outputs and remeasures encoded previews. render_window_measure produces quarter-second bands; verify.py validates artifacts and writes summaries/manifests. Source FFV1 excerpts and prior translation JSONs remain in earlier experiment directories. Preserve all prior footage, reports and working-tree edits.

## User verdict and next step

User verdict (2026-09-15): “close-up looks noticably better. full frame looks better too. still a small bit of vibration/judder left but a visible reduction.” Record a confirmed visible localized improvement in both the detail and full-frame views, with small residual vibration/judder. This supersedes the pending visual-review status. No new control/additional or earlier-burst verdict was supplied.

Freeze this candidate and its artifacts as the strongest visually supported localized post-render correction. Preserve the broader accepted edge-offset baseline and v1 translation reference separately. Validate the unchanged eligibility rule and transition behavior on unseen bursts/clips before integration, including shorter events that currently abstain. Characterize the remaining small residual against this new reference; avoid blindly increasing correction strength. The earlier135–140s events remain unresolved. No production promotion, commit or shipping rebuild is authorized by this verdict alone.

User reports165Hz display/100fps recording and attributes burst failures to corrupt gyro; playback cadence remains set aside.
