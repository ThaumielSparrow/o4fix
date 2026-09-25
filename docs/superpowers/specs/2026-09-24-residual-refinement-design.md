# Residual refinement + edge-offset splice — production design

Date: 2026-09-24. Status: design approved in conversation; awaiting written-spec review.
Research basis: [feedback-v1 results](../../experiments/feedback-v1/results.md), [gyro-trace-v1 results](../../experiments/gyro-trace-v1/results.md).

## Goal

Ship the two research changes that visibly removed most of the remaining judder inside repaired gyro-noise bursts, in the existing MP4 quaternion re-injection path, so users keep loading `_fixed.MP4` into Gyroflow like a stock recording and keep every Gyroflow editing feature.

1. **Edge-offset splice** (gyro-trace-v1, user: "significant improvement"): in a rebased burst, the carried-offset change is applied only in the fully replaced interior, not across the entry/exit blends; ramp default 0.30 → 0.19 s.
2. **Residual refinement** (feedback-v1 `wp1c`, user: "looks better, even at 4:09 … a good strategy"): measure the leftover telemetry error directly from the source frames against the spliced orientation and correct it inside the bursts. No Gyroflow render in the loop.

## Decisions (user)

- Both default-on, with a toggle to disable refinement (CLI `--no-refine`, GUI checkbox).
- Rust only (o4core, CLI, GUI). Python `o4fix.py` stays the frozen historical reference; its docs note it lacks refinement.
- Single refinement pass (what was reviewed); no iteration.

## Evidence summary

- Refinement vs the edge-offset baseline, in-burst 1–8 / 8–50 Hz residual (render-measured): large wobble reductions on 0073 (6/7 bursts), 0060 (1:46, 3:45, 5:08), 0071 (all), usually with less fine shake; weaker wobble reduction at 0060 98 s and 4:09 but less fine shake, which the user preferred visually.
- Gyroflow reads injected corrections exactly (gain 1.00, r² 1.000, no lag) and its smoothed path does not respond to them.
- The probe works in true angles; the render-loop variant (fbn) over-corrected because its image→angle scale ignored adaptive zoom. Render-loop code is not part of this design.

## Architecture

Pipeline (`o4core::pipeline::process`):

```
extract → detect bursts → optical patch → splice (edge-offset) → refine → inject/verify → _fixed.MP4
```

### 1. Splice change (`patch::splice_orientation`)

Port the research `edge_offset_patch.rs` change: for a **rebased** burst whose duration exceeds 2×ramp, the carried-offset interpolation parameter becomes `smoothstep((t − t0 − ramp) / (dur − 2·ramp))` instead of `smoothstep((t − t0) / dur)`; overlapping ramps and non-rebased bursts keep the existing expression (non-rebased bursts stay bit-identical). `Config::ramp` default 0.30 → 0.19.

### 2. Refinement module (`o4core/src/refine.rs`, new)

Public entry:

```rust
pub fn refine(video: &Path, t: &[f64], q_spliced: &[[f64; 4]], intervals: &[(f64, f64)],
              meta: &Meta, cfg: &RefineConfig, log: &dyn Fn(&str), cancel: &AtomicBool)
    -> Result<RefineResult, O4Error>
// RefineResult { q: Vec<[f64;4]>, bursts: Vec<RefineBurst>, skipped_reason: Option<String> }
// RefineBurst { start, end, max_deg, applied: bool, note: Option<String> }
```

Steps (all constants in `RefineConfig`, defaults = reviewed wp1c values):

1. **Windows.** For each severe interval, window = [start − 1 s, end + 1 s], clipped to the clip; overlapping windows merge. Windows are processed in time order.
2. **Decode.** OpenCV `VideoCapture` (same backend as `optical::interval_rates`, seek via `CAP_PROP_POS_FRAMES`), full resolution, grayscale. No ffmpeg dependency.
3. **Track.** Per consecutive frame pair: GFTT (1200 pts, quality 0.005, min distance 18, block 7), pyramidal LK (21×21, 4 levels) forward and backward; keep points with round-trip error < 0.5 px.
4. **Bearings.** Fisheye-undistort both points with telemetry K/D (`optical::k_d` scaling reused). Keep only points whose normalized coordinates lie inside the rendered field `|x| < 1.30, |y| < 0.72` in both frames.
5. **World rotation.** Rotate each bearing by the mount `diag(1, −1, −1)` then by the **spliced** orientation interpolated (slerp) at that point's row time `frame_index/fps + row/height · readout` with `readout = 5.092569 ms`.
6. **Fit.** Linearized small-rotation least squares `b ≈ a + d × a` with 4 rounds of trimming (keep residual < max(3·median, 0.0008 rad)); require ≥ 30 inliers, else the pair is missing. Convert to the telemetry body frame at the pair midpoint (`q_mid⁻¹ · d · q_mid`) and to rad/s.
7. **Condition.** Fill missing pairs by linear interpolation; zero-phase 2nd-order Butterworth high-pass at 1 Hz (`dsp` filtfilt); confidence = clip((inliers − 60)/140, 0, 1) smoothed by a 9-tap box.
8. **Gate.** Per burst fully inside its window with ≥ 1 s margin: smoothstep gate over [start − 0.25, end + 0.25] with 0.15 s fades. Correction rate = −gate · confidence · residual.
9. **Integrate.** Cumulative angle per window (time-ordered, carried across windows), interpolated onto telemetry timestamps; applied as body-rate increments `q'ᵢ₊₁ = q'ᵢ · (qᵢ⁻¹ qᵢ₊₁) · exp(ΔAᵢ)`. Outside gates body rates are unchanged; any leftover is a constant world-frame offset (invisible to Gyroflow with horizon lock off).

Samples before the first gate are returned unchanged (bit-identical).

### 3. Configuration and UI

- `Config` gains `refine: bool` (default `true`) and a nested `RefineConfig` (defaults above; not exposed in UI except the on/off switch). `validate()` covers the new numeric fields.
- CLI: `--no-refine`. Transcript prints a `Refine` stage: per-burst max correction (deg) and any skipped bursts/reasons.
- GUI: "Refine residual judder (recommended)" checkbox with help text; progress bar gains a Refine stage.
- Settings migration (pattern of the drift-rebase migration in `o4fix-app/src/settings.rs`): missing `refine` → true; stored `ramp == 0.3` (old default) → 0.19.

## Failure handling and safety

- **Best effort.** Decode/tracking failure in a window → those bursts get zero refinement, logged; the repair still completes. Only `Cancelled` propagates (checked per frame).
- **Size cap.** A burst whose correction angle exceeds 4° is not refined (logged). Largest reviewed correction ≈ 2.1°.
- **Geometry check.** Before applying any correction, using the non-burst pairs inside the windows (gate < 0.01, ≥ 300 inliers), two conditions must hold, otherwise refinement is skipped for the whole clip with a clear message (covers wrong mount/readout/lens conventions, e.g. future firmware):
  1. high-passed residual RMS < 8 deg/s (observed on 0060/0071/0073: 1.1–4.7);
  2. motion ratio: median |residual rotation| / median |telemetry rotation| over pairs with telemetry rate > 30 deg/s < 0.25. A wrong axis mapping leaves residual comparable to the motion itself (ratio ≳ 1); the correct one leaves mostly translation parallax. The exact threshold is confirmed from measured values on the three clips during implementation and recorded in the test.
  Missing lens metadata → skip whole clip.
- **Invariants.** Samples before the first gate unchanged; after gates only a constant world-frame offset; inject/verify round trip unchanged. Horizon-lock caveat documented alongside drift rebase.

## Testing and validation

Unit (synthetic, no video):
- rotation fit recovers known small rotations with outliers;
- increment integration: rates unchanged outside gates, leftover exactly a constant world-frame offset, pre-gate samples bit-identical;
- gate zero outside bursts; windows merge/order correctly;
- size cap and geometry check trigger;
- splice: non-rebased bursts unchanged; rebased burst matches research edge-offset expression.

Parity:
- `rebased_clip` (0060 exact parity) updated: production with `refine=false`, ramp 0.19 must match `target/experiments/gyro-trace-v1/edgeoffset.MP4` quaternions exactly (sign-folded).
- New ignored test: production (`refine=true`) vs research `wp1c` on 0073/0060/0071 — per-burst correction-angle difference small (tolerance set from the first run's OpenCV-vs-ffmpeg decode difference; target < 0.1° typical).
- Existing goldens (0021 M2/M4 e2e) run with legacy settings (`ramp=0.3`, `refine=false`) and must stay byte-identical; new-default e2e test added.

Regression and release checks:
- 0021: render new-default output; render-measurement comparison vs shipped M2 render — clean/mild zones must not worsen beyond measurement noise.
- Release CLI on 0060/0071/0073: exit 0, round-trip verified, runtime reported (refinement adds full-res tracking over burst windows only).
- User review page: release-build output vs research wp1c (expected visually identical).
- Rebuild both shipping binaries; update README, GUI help, CLAUDE.md, SESSION_HANDOFF. Release (v0.1.3) only on user request.

## Out of scope

Render-loop (fbn) code path; iteration; Python port; refinement outside severe bursts; horizon-lock interlock; tuning the probe for fast-turn edges (future research).
