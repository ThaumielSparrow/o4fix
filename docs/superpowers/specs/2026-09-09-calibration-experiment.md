# First experiment: calibration timing and residual diagnostics

Status: authorized research slice of the September follow-up backlog. No default-algorithm promotion is authorized by a numerical win alone.

## Scope

Implement an experimental calibration fitter alongside the existing fitter. Preserve the shipped M2/M4 defaults, calibration-window selection, detector, tracker, handback, and rebase logic. Acquire the same clean calibration observations for both fits on 0021, 0027, and 0060. Keep 0057–0059 out of tuning.

The candidate sorts observations by timestamp, filters each contiguous high-quality run at its measured cadence, discards short runs and edge transients, and fits the orthogonal mapping afresh for every clock-shift candidate. Reflections remain allowed. A coarse-to-fine bounded search minimizes vector residual energy; it does not average undefined correlations on stationary axes. Calibration failures remain explicit.

Provisional experiment constants: 5 Hz optical/gyro comparison low-pass; split at gaps exceeding 1.5 frame periods or materially irregular cadence; require at least 0.6 seconds in each run; exclude 0.15 seconds at each filter boundary; search +/-60 ms at 2 ms steps then refine locally at 0.2 ms. These are experiment settings to record, not new user defaults.

## Gates

- Synthetic known reflected transforms and offsets at 50/60/100/120 fps, with separated/reordered intervals and quality gaps: recover shift within 2 ms and mapping within 1% Frobenius error. Reject stationary/insufficient data. Results must be invariant to interval ordering.
- Real data: leave one original calibration interval out, fit the others, and score both models on the same untouched interval using the same contiguous-run evaluator. Record failures and retained duration. A training R² improvement alone is not a pass.
- Only produce repaired candidate footage if held-out results justify it; retain the current patch coverage and transactional output safeguards. Do not use Python's unsafeguarded repair path.
- Original fixtures remain unchanged. Keep original regression checks, default-path parity, and release-build provenance.

## Reproducible artifacts

Use `target/experiments/calibration-v1/` for copied baseline binaries, hashes, source/configuration fingerprints, acquired optical observations, numerical reports, diagnostic data, and any render outputs. A manifest names each clip and its complaint/control windows. Frame dimensions, count, FPS, and duration come from the actual input, never an inherited shorter template.

Candidate and baseline renders must use identical Gyroflow settings, empty autosync offsets, and correct zoom metadata. Compare body-frame corrections and keep tracker/optical confidence separate from visual quality. Short-window low-frequency RMS is not the decision criterion.

## Human review

If a candidate clears the numerical gates, prepare matched before/after clips with context around the prior 0060 complaint moments and a clean/flick control. Ask the user which has less judder and unwanted panning, whether turns remain crisp, and whether either adds zoom or edge artifacts. “Indistinguishable” is a valid result. Until then, no manual viewing is needed.

R1 parser hardening and other product backlog items remain separate from this first quality experiment. E0 localization starts with the existing complaint windows and will be refined with user observations and recorded diagnostics.
