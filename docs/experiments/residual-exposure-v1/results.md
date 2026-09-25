# Residual gyro exposure after the accepted edge fix

2026-09-13. Read-only audit of the accepted 0073 fixed-edge-offset candidate, using the existing parity-validated trace, source telemetry and rendered localization. Production and preserved videos unchanged.

## Findings

The 0.19 s orientation ramps contain **zero detector-flagged noisy rate samples** in both 14–30 s and 132–146 s, checking the raw SLERP weight at **both quaternion endpoints** of each rate interval. There are 4509 and 3165 flagged samples respectively in the cached trace coverage. Thus another shorter-ramp experiment is not supported by this detector evidence. This does not prove the gyro outside the detector mask is accurate; low-frequency contamination and endpoint effects remain possible.

There is a separate gyro path inside fully replaced orientation intervals. The supplied rate is

`(1-w) * light_gyro + w * ((1-handback) * optical + handback * medium_gyro)`.

Segment-peak trust controls handback, but does not prevent the outer light-gyro partner from returning when instantaneous alpha falls. In the optical segment 140.857086–144.827243 s, trust is zero. In 141.25–144.46 s, handback is identically zero but 1155/3210 cached samples have some light-gyro partner contribution. “Rebased interior” therefore means output rates match supplied patch rates; it does **not** mean those supplied rates are optical-only. This distinction corrects a possible overreading of prior stage reports.

Quarter-second bins, all in the same rebased interior:

| Start | Maximum light-gyro weight | Patch minus optical RMS, deg/s | Apparent horizontal 2–8 Hz, half-res px/s | Intended rotation 2–8 Hz, deg/s |
|---|---:|---:|---:|---:|
| 142.50 | 1.000 | 8.121 | 7.649 | 0.193 |
| 143.00 | 0.000 | 0.000 | 11.934 | 0.296 |
| 143.25 | 1.000 | 6.834 | 7.373 | 0.155 |
| 143.50 | 0.273 | 0.493 | 16.717 | 0.112 |

All four bins have zero direct raw-orientation SLERP weight and zero medium handback. The 142.50 bin also has 38.709 px/s apparent 8–30 Hz horizontal RMS. These are source-contribution measurements and image-motion proxies, not estimated physical error. The strongest low-band bin does not coincide with the largest gyro contribution; no simple causal correlation is established.

The 143.00–143.25 s bin is entirely optical-only at the rate-input stage, at least 1.011 s from either orientation ramp. Across reliable image samples in rebased interiors with zero patch gyro weight and a 0.5 s ramp guard, 130 samples retain 10.272 px/s low-band and 20.924 px/s high-band horizontal RMS, with 0.174 deg/s intended low-band rotation. With a 1 s guard, 41 samples remain. Guarded samples are fragmented and filtering/render smoothing may carry effects over longer distances; they do not establish independent optical error or exclude neighboring gyro leakage. Nevertheless, local gyro mixture cannot explain every instantaneous apparent-motion peak.

## Decision and next discriminating experiment

Do not repeat ramp shortening, wider padding, forced rebasing, or gap2 alone. A more specific remaining gyro hypothesis is **light-gyro partner re-entry during alpha dips within an already zero-trust severe interval**, distinct from the previously measured fast handback and endpoint bridge.

Before a new visual comparison, isolate that partner in a research-only ablation: suppress only the light-gyro partner inside zero-trust severe interiors with supported optical data, retain fast handback and accepted splice/rebase/decay settings, and use a continuous transition outside the tested interior. Verify accepted-cache parity, synthetic alpha-dip leakage removal, genuine fast-motion preservation, original coverage refusal, and unchanged clean controls. Inspect whether endpoint drift/rebase decisions change and report any such changes instead of calling it a purely local orientation change. Use 143.00 optical-only motion as a negative target alongside the affected 142.50/143.25 bins. Do not assume the optical estimate is correct or promote this rule from rate differences alone. If the isolated experiment does not visibly help, prioritize optical measurement reliability with clean controls. Lens/scaling remains deferred.

## Reproduction and verification

`residual_exposure_probe` loads source quaternion timestamps and snaps the exact saved burst intervals using the accepted splice indexing. It verifies every cached trace timestamp against the source midpoint to 1e-9 s, reports both endpoint SLERP weights and separates light-partner from medium-handback weights. No new optical acquisition or repair was run. Release build and Clippy with warnings denied passed; existing nom future-compatibility warning remains. Full production tests and shipping rebuilds were not run because only research code/docs changed.

After the OpenCV environment from SESSION_HANDOFF.md:

```powershell
cargo build --release --offline -p o4core --example residual_exposure_probe
target/release/examples/residual_exposure_probe.exe sample_vids/DJI_20260829141435_0073_D.MP4 target/experiments/residual-stage-v1/0073/trace.json target/experiments/residual-stage-v1/0073/stages.json target/experiments/residual-exposure-v1/0073-sources.json
python docs/experiments/residual-exposure-v1/summarize.py
```

The probe refuses existing output paths; use a new destination for reruns and adjust the summarizer input. `summary.json` preserves all quarter-second bins and guard checks; `manifest.json` fingerprints inputs and diagnostic code. The first `0073.json` output is retained but superseded by `0073-sources.json`, which separates the two rate contributions. The 55.9–58.9 s clean control has no cached optical trace rows: its metrics are unavailable, not evidence of zero gyro exposure. No new control visual verdict or 0060 result is claimed.
