# Persistent perspective validation — September 10, 2026

Continued the handoff's bounded perspective investigation. No correction was rendered or promoted. Production core, shipping binaries, source footage and reference repairs are unchanged.

## Method

New isolated example `o4core/examples/perspective_track_probe.rs` retains surviving LK tracks when adding features, instead of replacing the entire set every ten frames as spatial-v1 did. Additions are at least ten half-resolution pixels from existing tracks, capped at 600. The prior one-pixel forward/backward gate is retained for diagnostic comparability; this does not promote that gate into the production tracker.

Compare parity-held-out similarity/homography on all tracks and homography on tracks aged at least ten consecutive pairs (100 ms). Separately withhold each entire 3x3 region from the homography fit and evaluate its observations. Require 60 training and eight evaluation points; unsupported regions remain null. This spatial extrapolation test can expose both geometric conditioning and inconsistent scene motion; it cannot distinguish them alone. Persistent feature IDs are disjoint between fits, but their image measurement errors need not be statistically independent.

Evaluate the preserved default full render at 102–110 s and 18–23 s, and lossless contexts at 221–229, 245–253, 304–312 s with source offsets 220, 244, 303. Report severe pairs for complaint windows and 19–22 s for the clean control. Units below are pixels at 720x405. Metrics are medians across available per-pair values, not pooled ground truth.

## Results

| Window | Pairs | Median track age, frames | All-track held-out homography | Mature-track held-out homography |
|---|---:|---:|---:|---:|
| 106 | 78 | 52 | 0.242 | 0.245 |
| 225 | 167 | 40 | 0.258 | 0.262 |
| 249 | 98 | 43.5 | 0.374 | 0.275 |
| 308 | 127 | 44 | 0.319 | 0.321 |
| Clean 19–22 | 300 | 159.5 | 0.315 | 0.294 |

At 249 s the all-track leave-region-out upper-right error is 2.835 (64/98 supported pairs), and bottom-left is 3.178 (98/98). Mature tracks reduce upper-right error to 0.570 while coverage falls to 43/98; this is not a common-support improvement and does not establish resolution of the sky discrepancy. Bottom-left mature error is 4.415 (70/98). At 308 s mature bottom-right error is 3.436 (91/127), versus 0.991 with all tracks (95/127). Track age alone is therefore not a reliable global acceptance criterion.

106 s is more coherent: every region's all-track median is 0.243–0.336, with 71–78 supported pairs. However, the clean control itself has all-track bottom-right error 1.191 (299/300). A universal low pixel threshold could reject healthy motion. Do not choose a correction gate from the complaint-window aggregates alone.

## Decision and next step

The aggregate perspective fit still conceals regional uncertainty. Stop before correction/crop/render work because regional consistency has not passed. No visual benefit is claimed. The candidate estimator and fast-turn/crop validation remain unfinished.

Next, add normalized geometric conditioning and spatial-support diagnostics for each excluded-region fit, including corner/horizon denominator behavior and local prediction sensitivity. Compare region errors on identical supported frame pairs. Then test synthetic pure camera rotation, fast turns, clustered features, outliers and two independently moving depth/plane groups to separate extrapolation instability from incompatible image motion. Validate on source-image geometry with the actual lens model before interpreting a rendered-image homography as camera rotation. If a rotation estimate becomes supportable, preserve the production fusion and compare that single optical mechanism; do not smooth a full projective warp sequence.

## Verification and reproduction

Three release-mode synthetic tests pass: held-out projective versus similarity discrimination, translation recovery, and withheld-region independent-motion detection with missing coverage preserved. All five video runs completed (799 pairs per complaint window, 499 control-context pairs). No production regression suite was rerun because changes are isolated to a new example and documentation.

Use the Cargo/OpenCV environment in SESSION_HANDOFF.md, then:

```powershell
cargo test --release --offline -p o4core --example perspective_track_probe
cargo build --release --offline -p o4core --example perspective_track_probe
target/release/examples/perspective_track_probe.exe target/experiments/local-smooth-v1/context_249.avi 244 245 253 target/experiments/local-smooth-v1/bursts.json target/experiments/perspective-v1/249.json
```

The other input windows/offsets are listed above. Full per-pair observations live in `target/experiments/perspective-v1`. Durable `metrics.json` includes each region's valid-pair count; `manifest.json` fingerprints input media, burst mask, code, executable and raw results. The old spatial-v1 example and results are preserved.
