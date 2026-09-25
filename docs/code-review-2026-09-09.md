# Code review and stabilization assessment — 2026-09-09

The strongest immediate improvements are to failure handling and optical-data validity. The measured M2/M4 filtering and drift-rebase defaults should remain unchanged until an alternative wins a controlled comparison. This review implements those safety improvements; it does **not** claim a measured reduction in the already accepted residual judder on successfully tracked footage.

## Scope and historical evidence

Reviewed the shipped Rust core, CLI, GUI backend/frontend, Python reference pipeline and evaluation helpers, fixture tests, CI/release configuration, `CLAUDE.md`, the Superpowers design/implementation plans, progress history, and relevant task reports. The `.claude` directory contains a scheduling lock rather than additional substantive task history. The detailed historical reports are in the ignored `.superpowers/sdd` directory.

Three historical conclusions materially constrain further work:

- The July monster-burst fix needs **segment-peak** noise gating. Instantaneous gating and unconditional `min(gyro, optical)` handback already failed measured comparisons.
- The August drift-rebase default was accepted after correcting inherited Gyroflow `video_info` and examining body-frame corrections. The older `.superpowers/sdd/task-5-report.md` still argues against rebase using the superseded renders. Its conclusion is superseded by the later entries in `progress.md` and `CLAUDE.md`; do not use that report alone to select a new guard.
- The remaining 0060 tracker differences are comparable with changes in clean controls. Parameter tuning against those numbers alone cannot establish a visual improvement. The known bad ideas—global low-pass filtering, autosync, rate-weighted drift spreading, and the previously tested glitch filter—were not repeated.

## Findings fixed in this review

**1. [P1] An output failure could destroy an existing file, including the source.**

`pipeline::process` previously removed the requested destination on *any* injection error, including errors before the copy began. Passing `-o` equal to the source was not prohibited. `patch_video` also copied directly over the destination before verification. Both low-level patching and verified injection now stage a file beside the destination and publish it by rename only on success. Source aliases, including hard links, are rejected. Error cleanup removes only the staging file. Tests exercise failure preservation, alias rejection, and replacement only after successful verification. A force-killed process can leave a staging file, but no partially repaired destination is published.

Changed: `o4core/src/mp4.rs`, `pipeline.rs`, `error.rs`; added the already-locked `same-file` dependency without upgrading dependencies. Replacing an existing repair now requires free space for another full video until publication; this is the cost of preserving the previous file on failure.

**2. [P1] Rejected optical segments could still drive drift rebase.**

The design spec explicitly says skipped optical segments never rebase. The implementation did not carry segment acceptance to `splice_orientation`: short/poor-quality segments retained filtered gyro in the rate array, and all severe intervals were subsequently integrated and eligible for rebase. Thus a camera with failed tracking could accumulate a carried offset without an accepted optical measurement.

`optical_patch` now returns rates plus a support mask. Before integrating any severe interval, the pipeline requires support for every rate sample it will consume. Missing support produces an error naming the burst and leaves the clip unwritten. This deliberately refuses an incomplete repair rather than silently shipping one. A bounded edge allowance accounts for frame-pair timestamps, the fitted clock shift, and normal end-of-clip telemetry; it does not extrapolate an early decoder stop across the remaining burst. The ordinary O4 frame-edge case was checked against the actual 0021 fixture after an initial overly strict check rejected its final landing burst.

Changed: `o4core/src/patch.rs`, `pipeline.rs`, `error.rs`, golden test consumer. `OpticalPatch` is a small public API change: callers now use `.rates`, and shipping callers must check coverage.

**3. [P1] Invalid calibration could pass the R² gate.**

Stationary optical and gyro samples produce zero residual variance and zero total variance, hence `NaN` R². `NaN < 0.8` is false, so the old gate admitted it. Alignment now rejects degenerate/non-finite variance, excludes non-finite optical observations, and the caller explicitly requires finite R². A synthetic stationary-camera regression verifies refusal. Video decoding also rejects an unopened capture or invalid FPS/dimensions before computing rates.

Changed: `o4core/src/optical.rs`, `patch.rs`.

**4. [P2] Numeric settings could create invalid filters or misleading healthy outcomes.**

Neither interface validated the tuning arithmetic. Examples: `--severe NaN` makes every severe comparison false; `--ramp 0` divides by zero at burst edges; equal noise thresholds divide by zero; out-of-range cutoffs invalidate the filter design. Shared validation now checks finite values, signs, ordered threshold pairs, positive cutoffs/ramp, Hampel-size overflow, and the measured Nyquist limit. The CLI reports usage errors before opening clips; GUI/library processing performs the same checks. Processing also rejects non-finite orientations and non-increasing timestamps.

Changed: `o4core/src/config.rs`, `pipeline.rs`, `o4fix-cli/src/args.rs`; includes configuration and CLI regressions.

**5. [P2] Queue startup was still vulnerable during the IPC await.**

The documented early-event buffer addressed backend events arriving before row registration, but `busy()` remained false while `start_queue` was pending. Another click could launch a duplicate batch; adding files in that window could strand rows when `pending` was cleared. Startup now locks controls synchronously, preserves the row snapshot, and recovers from invocation rejection while keeping inputs retryable. Completed backend jobs are also removed from the cancellation map.

Changed: `o4fix-app/ui/app.js`, `src/queue.rs`; `tools/test_queue_ui.cjs` checks duplicate startup, file-add prevention, early completion replay, and failure recovery without a browser.

**6. [P2] Evaluation labels contradicted the established clean controls.**

`eval_render.py` still labeled 60–65 and 100–105 seconds “clean,” despite `CLAUDE.md` explicitly recording that they are not. Those labels now use the verified 67–72.5, 94–98, and 120–128 second windows. The hard-coded “64% patched” aggregate label is replaced with “optical-triggered,” which accurately describes its mask rather than implying the exact severe-splice coverage. `eval_windows.py` locates its cache relative to the script.

Existing cached metrics were not rewritten. These label changes do not modify the tracker or filtering numerics.

## Remaining actionable findings

**7. [P1] The custom MP4/protobuf parser still panics on malformed input.**

`mp4.rs:30` accepts an extended-size box without first checking for all 16 header bytes. Sample-table counts are indexed without checking each table's declared payload length; `first_chunk == 0` underflows; `read_varint` at line 193 has neither a buffer bound nor a ten-byte limit; unsupported wire types panic. Declared box sizes can also drive excessive allocation. The GUI catches ordinary Rust panics, but the CLI does not; allocation aborts are not catchable. Replace these parser primitives with bounded `Result` readers, check arithmetic and file/sample extents, and add synthetic malformed-box/protobuf tests plus fuzzing. The new staged writer limits destination damage but does not make this parser robust. This remains unfixed.

**8. [P2] Calibration filters concatenate discontinuous time ranges as a 100 Hz signal.**

`fit_video_alignment` selects good observations, concatenates selected intervals (which are ordered by calibration score), then applies one fixed-coefficient low-pass filter to the resulting array. Large time gaps and removed observations are treated as adjacent frames. At other video frame rates, the nominal 5 Hz filter also has a different physical cutoff. This can bias the alignment and clock shift while still giving a high aggregate R². Filter/resample within each chronological contiguous run at the actual rate, then combine residuals. Refit the Procrustes matrix at the final selected shift—the current final matrix precedes the last shift update. Validate this change with synthetic gaps, 50/60/100/120 fps cases, and the existing clips before changing defaults.

**9. [P2] Tracking confidence does not limit continuous dropouts.**

`patch.rs` rejects a segment only when more than 30% of samples have quality below 0.3. A two-second contiguous failure inside a ten-second segment therefore passes, and interpolation invents motion over those two seconds. The support mask added here represents *segment acceptance*, not independent confidence in every interpolated observation. A follow-up should bound continuous missing-observation duration, avoid unrestricted endpoint extrapolation, and propagate confidence to the repair decision. Choosing that bound needs measured tracking behavior; this review does not introduce an arbitrary new tuning threshold.

**10. [P2] Feature-count confidence is not geometric accuracy.**

`pair_rotation` ignores the LK error output, has no forward/backward consistency check, and maps essential-matrix inlier count directly to quality. Concentrated features, moving objects, or rolling-shutter deformation can yield many mutually consistent but biased tracks. “Optical never invents motion” in the handback comment is too strong. Add diagnostics for reprojection residuals, image coverage, reverse-tracking error, and estimated rotational uncertainty. Preserve the current defaults until controlled comparisons show that rejecting more tracks improves the final repair.

**11. [P2] Batch output-name collisions are unresolved.**

With an output-folder override, two inputs in different directories with the same basename both target the same `_fixed.MP4`. Atomic staging prevents interleaved corrupt writes, but the last successful rename still replaces the other result. Reserve canonical destination paths before dispatch and reject or disambiguate collisions; do not rely on string equality of input paths. The GUI startup fix does not resolve this independent naming issue.

**12. [P2] Cancellation remains incomplete during extraction and writing.**

`telemetry::open_input` creates a fresh false cancellation flag, and the write/copy/verification loops do not receive the job's flag. Cancellation during these phases may end in a completed output instead of a cancelled job. Thread the job flag through parsing, bounded copy/write batches, and verification; check it before publishing the staged file. Keep the current per-frame optical cancellation.

**13. [P2] The Python research path retains the old failure semantics.**

The reference `python/o4fix.py` and `mp4patch.py` still allow filtered-gyro fallbacks through splicing, use the old non-finite calibration gate, and write directly to the destination. They were retained as the historical numerical reference, not silently rewritten while validating the Rust implementation. Use the Rust binaries for repairs. Before resuming Python-based algorithm development, port the coverage/transaction contracts there and test the failure cases. `prep_inject.py` also needs to enforce coverage before producing an injection archive.

**14. [P2] Evaluation portability and validity need broader work.**

Several analysis tools still assume 0021 masks; `eval_render.py --cache` changes the aggregate mask but not the named-window definitions. `eval_windows.py` filters short windows independently; low-frequency estimates near 0.3 Hz on roughly two-second windows are dominated by limited support and filter boundaries. Cache files do not include robust input/configuration/build fingerprints. Follow-up: explicit clip/window manifests, source and settings hashes, full-series filtering before window selection, quality/dropout reporting, and rejection of insufficient-duration metrics. Do not interpret changes below the demonstrated clean-control noise as improvements.

**15. [P3] Performance, reproducibility, and maintenance gaps.**

Telemetry is parsed multiple times per clip; box/slot preparation can fail only after expensive optical processing; `uniform_filter1d` is O(samples × window) rather than a running sum; four-byte quaternion payloads are individually sought and written; file workers multiply the demand from interval-level Rayon/OpenCV processing. Profile before optimizing, and retain byte/numerical gates where accumulation order changes. CI does not run private-clip tests and the release job does not itself run the fast suite or include `o4core` in the version comparison. Settings saves are direct writes and frontend save errors are ignored. Small public DSP helpers still assume nonempty, well-shaped inputs. These are separate follow-ups, not evidence that the tuned quaternion convention or default cutoff is wrong.

## Stabilization improvements worth testing next

1. **Repair calibration timing and confidence first.** This is a relatively bounded experiment and addresses concrete assumptions in the current code. Require synthetic recovery of a known transform/shift under gaps and different FPS; use a held-out calibration segment rather than reporting only fit-on-training R².
2. **Improve optical correspondences before replacing the fusion model.** An opt-in forward/backward LK check plus spatial-coverage diagnostics is a practical first experiment. OpenCV's own [4.12 LK tracking example](https://raw.githubusercontent.com/opencv/opencv/4.12.0/samples/python/lk_track.py) verifies tracks by tracking them back to their starting frame. That supports the technique, but is not evidence of an O4 quality win.
3. **Explore rolling-shutter-aware motion inside monster bursts.** Current measurement assigns one rotation to a whole frame pair. Row-time-aware estimation could reduce the low-frequency optical error that rebase carries. This is a research hypothesis for this camera, not a drop-in replacement: readout timing and blur must be established. [Rolling-Shutter-Aware Differential SfM and Image Rectification](https://arxiv.org/abs/1903.03943) provides a relevant motion-model starting point; its assumptions must be tested on aggressive FPV motion.
4. **Only then consider uncertainty-weighted fusion.** A new EKF or complementary estimator cannot disambiguate phantom gyro motion by itself. It needs credible optical confidence and deliberate treatment of gyro trust. Retain segment-peak monster-burst gating and the measured M2/M4 trade-off until a candidate demonstrably improves them.

For each candidate, freeze the input, baseline build, calibration, Gyroflow lens/zoom settings, and offsets; change one mechanism; evaluate 0021 plus monster-burst clips; include verified clean controls; compare body-frame corrections; and perform perceptual A/B when the metrics are within the known noise floor. Do not regenerate reference fixtures simply to make a changed algorithm pass.

## Validation

- Baseline before changes: 24 fast Rust tests passed; existing private-clip tests were ignored by the default command.
- After changes: **31 fast Rust tests passed** with `cargo test --workspace --locked --offline`. Clippy with `--workspace --all-targets -- -D warnings`, rustfmt, and `git diff --check` passed. The pre-existing transitive `nom 6.1.2` future-incompatibility notice remains.
- **All 13 original private-clip tests passed** in release mode: M2/M4 end-to-end, healthy/no-calibration paths, telemetry/detection/alignment/optical/patch/splice goldens, byte-identical null patch, and exact injection round trip. On 0021, both profile runs retain 124551/176372 original samples; the existing clean-zone exactness and repaired-zone tolerances pass. No golden fixture was regenerated.
- The dependency-free Node queue regression passed and is added to CI. All 18 Python files passed AST syntax parsing. Python numerical tests were not run in this session because the available runtimes lacked SciPy/OpenCV; production numerical verification used the Rust tests against the existing Python fixtures.
- Both release binaries were rebuilt in `target/release`, as required by the stale-build incident notes. No release/tag/commit was created.
- **0060 preserved-release comparison passed:** 27 severe bursts, the same five rebases, 104329/382658 samples unchanged from raw, and all **765316 parser slots exactly equal to the preserved release output up to quaternion sign** (maximum error 0; timestamps identical). Its injection round trip was exact. This makes **14 passing private-clip checks** in total. The temporary comparison video was deleted after success; original/reference videos were not overwritten.
- Logs and release-binary hashes are retained under `target/review/`. The rebuilt CLI also returns usage exit 2 for a zero ramp before opening the supplied video.

Limits: no new Gyroflow render or perceptual A/B was performed, and no clean-PATH packaged GUI smoke test was performed. The new guards improve validity and failure safety; they are not evidence that the accepted residual judder has decreased. The remaining parser, batch-name collision, and Python-reference issues above are still open.
