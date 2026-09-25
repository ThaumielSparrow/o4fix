# Residual Refinement + Edge-Offset Splice Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the edge-offset splice fix (0.19 s ramp) and the Gyroflow-free residual refinement (research `wp1c`) default-on in the Rust repair pipeline, so `_fixed.MP4` files loaded in Gyroflow have visibly less in-burst judder.

**Architecture:** `patch::splice_orientation` gets the fixed-edge-offset interpolation. A new `o4core::refine` module (pure `geometry` + `signal` submodules, OpenCV `measure` submodule, orchestration in `mod.rs`) runs after the splice: it re-reads each severe burst ±1 s from the source video, measures the leftover frame-to-frame rotation against the spliced orientation, and applies a burst-gated body-frame correction. Config/CLI/GUI get an on/off switch; the pipeline gains a `Refine` stage.

**Tech Stack:** Rust 1.88, opencv 0.99 (crate), rayon, existing `o4core::{dsp, quat, telemetry, patch, pipeline}`; Tauri GUI (plain JS); portable Gyroflow 1.6.3 + Python/numpy only for validation.

**Spec:** `docs/superpowers/specs/2026-09-24-residual-refinement-design.md`. Research reference: `docs/experiments/feedback-v1/results.md`, `o4core/examples/warp_residual_probe.rs`, `docs/experiments/feedback-v1/warpcmp.py`, `o4core/examples/feedback_repair.rs`, `o4core/examples/support/edge_offset_patch.rs`.

## Global Constraints

- Both changes default-on: `Config::ramp = 0.19`, `Config::refine = true`. Refinement can be disabled with CLI `--no-refine` and a GUI checkbox.
- Rust only (o4core, o4fix-cli, o4fix-app). Do not modify `python/`.
- Single refinement pass. No render loop, no iteration.
- Refinement is best effort: only `O4Error::Cancelled` may propagate out of it; every other failure skips the affected burst(s) or the whole clip with a logged reason, and the repair still writes.
- Samples before the first refinement gate stay bit-identical to the splice output; after gates only a constant world-frame offset remains.
- Defaults (RefineConfig): window margin 1.0 s; min edge margin 0.5 s; readout 5.092569 ms; render crop |x|<1.30, |y|<0.72 (normalized); high-pass 1.0 Hz 2nd-order zero-phase; gate pad 0.25 s, fade 0.15 s; confidence clip((inliers−60)/140,0,1) with 9-tap box; min 30 inliers; features 1200; per-burst cap 4.0°; geometry check floor RMS < 8.0 deg/s and motion ratio < 0.25 (confirm in Task 5).
- Mount (camera → telemetry body): `diag(1, −1, −1)`.
- Existing goldens (0021 M2/M4 e2e, golden_splice) must stay byte-identical when run with legacy settings `ramp: 0.3, refine: false`.
- Build env (every cargo command, PowerShell): `$env:OPENCV_INCLUDE_PATHS='C:\opencv\build\include'; $env:OPENCV_LINK_PATHS='C:\opencv\build\x64\vc16\lib'; $env:OPENCV_LINK_LIBS='opencv_world4120'; $env:LIBCLANG_PATH='C:\Program Files\LLVM\bin'; $env:PATH += ';C:\opencv\build\x64\vc16\bin;C:\Program Files\LLVM\bin'`. Use `--offline`.
- Do not commit unless the user asks (repo practice this session); each task's "Commit" step is a local checkpoint only if the user has authorized commits — otherwise skip it and report the changed files.
- After any production core change is complete (end of Task 7), rebuild both shipping binaries: `cargo build --release --offline --workspace`.

## Review Focus

1. **Burst near clip start/end** (window clipped to <1 s margin): expect the burst to be refined if ≥0.5 s margin remains, otherwise skipped with a note; never an index panic. → Task 4 test `burst_too_close_to_clip_edge_is_skipped`, Task 5 `windows_clip_to_video`.
2. **Adjacent/overlapping bursts whose windows merge, or intervals passed out of order**: the corrected orientation must be continuous with no step at the first sample (the research `fb1w` bug). → Task 4 test `merged_windows_are_time_ordered_and_continuous`.
3. **Refinement disabled or no bursts refinable**: output must be bit-identical to the splice output. → Task 7 test `refine_off_is_bit_identical_to_splice`, Task 4 `zero_correction_returns_input_bits`.
4. **Non-100 fps or different resolution video**: frame timing and K/D scaling must come from the actual video, not constants. → Task 5 test `window_frames_uses_video_fps`.
5. **Cancellation during refinement**: must return `Cancelled` promptly and write nothing. → Task 5 ignored test `refine_honours_cancel`.

---

### Task 1: Edge-offset splice + 0.19 s ramp default

**Files:**
- Modify: `o4core/src/patch.rs` (inside `splice_orientation`, the `for k in 0..=n` base-path loop, currently `let s = smoothstep((tt - t[i0]) / dur);`)
- Modify: `o4core/src/config.rs` (`ramp: 0.3` → `0.19`; `defaults_match_spec` test)
- Modify: `o4fix-cli/src/args.rs` (`#[arg(long, default_value_t = 0.3)] pub ramp` → `0.19`)
- Modify: `o4fix-app/ui/help.js` (DEFAULTS `ramp: 0.3` → `0.19`; HELP ramp text default 0.19)
- Modify: `o4core/tests/golden_splice.rs`, `o4core/tests/e2e.rs` (pin legacy `ramp: 0.3`)
- Modify: `o4core/tests/rebased_clip.rs` (reference → research edge-offset repair)

**Interfaces:**
- Consumes: none.
- Produces: `patch::splice_orientation` signature unchanged; `Config::default().ramp == 0.19`.

- [ ] **Step 1: Write failing unit tests** in `o4core/src/patch.rs` inside `mod splice_rebase_tests` (append):

```rust
    #[test]
    fn stationary_camera_rebased_burst_has_no_edge_motion() {
        // camera still; raw drift of 60 deg confined to the burst middle
        let t: Vec<f64> = (0..5001).map(|i| i as f64 / 1000.).collect();
        let q: Vec<[f64; 4]> = t
            .iter()
            .map(|&s| qexp([0., 0., 60_f64.to_radians() * ((s - 2.3) / 0.4).clamp(0., 1.)]))
            .collect();
        let rates = vec![[0.; 3]; t.len() - 1];
        let (out, st) = splice_orientation(&t, &q, &rates, &[(2., 3.)], 0.19, 30., 0.);
        assert!(st[0].rebased);
        let (_, r) = quats_to_rates(&t, &out);
        let peak = r.iter().map(|v| v[2].abs().to_degrees()).fold(0., f64::max);
        assert!(peak < 1e-8, "false rate {peak} deg/s");
    }

    #[test]
    fn non_rebased_burst_path_unchanged_by_edge_fix() {
        let t: Vec<f64> = (0..5001).map(|i| i as f64 / 1000.).collect();
        let q: Vec<[f64; 4]> = t
            .iter()
            .map(|&s| qexp([3_f64.to_radians() * ((s - 2.3) / 0.4).clamp(0., 1.), 0., 0.]))
            .collect();
        let rates = vec![[0.; 3]; t.len() - 1];
        let (out, st) = splice_orientation(&t, &q, &rates, &[(2., 3.)], 0.19, 30., 0.);
        assert!(!st[0].rebased);
        // pre-burst samples keep original bits
        assert!(out[..1900].iter().zip(&q[..1900]).all(|(a, b)| a == b));
    }
```

- [ ] **Step 2: Run to verify the first fails**

Run: `cargo test --offline -p o4core --lib splice_rebase_tests -- --nocapture`
Expected: `stationary_camera_rebased_burst_has_no_edge_motion` FAILS (peak ≈ 14 deg/s); the second passes.

- [ ] **Step 3: Implement** — in `splice_orientation`, replace the line in the base-path loop

```rust
            let s = smoothstep((tt - t[i0]) / dur);
```

with

```rust
            // Fixed edge offsets (gyro-trace-v1): in a rebased burst the carried
            // offset changes only in the fully replaced interior, so the raw
            // path blended in at the entry/exit ramps does not move. Overlapping
            // ramps fall back to the whole-burst interpolation.
            let s = if rebased && dur > 2.0 * ramp_s {
                smoothstep((tt - t[i0] - ramp_s) / (dur - 2.0 * ramp_s))
            } else {
                smoothstep((tt - t[i0]) / dur)
            };
```

In `config.rs` set `ramp: 0.19` and add to `defaults_match_spec`: `assert_eq!(c.ramp, 0.19);`. In `args.rs` change the ramp default to `0.19`. In `help.js` DEFAULTS `ramp: 0.19` and HELP ramp text `"s, slerp cross-fade to the raw path at burst edges (default 0.19)"`.

- [ ] **Step 4: Pin legacy goldens.** In `o4core/tests/golden_splice.rs` replace `let cfg = o4core::config::Config::default();` with

```rust
    // goldens were produced with the pre-0.1.3 0.3 s ramp
    let cfg = o4core::config::Config { ramp: 0.3, ..o4core::config::Config::default() };
```

In `o4core/tests/e2e.rs`, add at top-level:

```rust
/// Settings the committed Python goldens were produced with.
fn legacy(c: Config) -> Config {
    Config { ramp: 0.3, ..c }
}
```

and change `run(&Config::default(), &out)` → `run(&legacy(Config::default()), &out)` and `run(&Config::m4(), &out)` → `run(&legacy(Config::m4()), &out)`.

- [ ] **Step 5: Retarget the 0060 regression.** In `o4core/tests/rebased_clip.rs` change the reference path to `target/experiments/gyro-trace-v1/edgeoffset.MP4` (the user-approved research repair), rename the test `monster_bursts_match_edge_offset_research_repair`, replace the exact `assert_eq!(max_error, 0.0, ...)` with

```rust
    assert!(
        max_error <= 1e-6,
        "default splice must reproduce the reviewed edge-offset repair (max {max_error})"
    );
```

and update the module doc comment to say the reference is the reviewed research repair (float32-level tolerance because that file was written from cached rates).

- [ ] **Step 6: Run tests**

Run: `cargo test --offline --workspace` → all pass.
Run: `cargo test --release --offline -p o4core --test rebased_clip -- --ignored --nocapture` → PASS, prints max error (expect ≤ 1e-6).
Run: `cargo test --release --offline -p o4core --test golden_splice -- --ignored` → PASS.

- [ ] **Step 7: Commit (only if authorized)** — `git add o4core/src/patch.rs o4core/src/config.rs o4fix-cli/src/args.rs o4fix-app/ui/help.js o4core/tests/golden_splice.rs o4core/tests/e2e.rs o4core/tests/rebased_clip.rs && git commit -m "feat: fixed edge offsets in rebased bursts, 0.19 s ramp default"`

---

### Task 2: `dsp::butter_high`

**Files:**
- Modify: `o4core/src/dsp.rs` (add fn after `butter_low`; add tests to its test module or a new `#[cfg(test)] mod high_tests`)

**Interfaces:**
- Produces: `pub fn butter_high(order: usize, wn: f64) -> Ba` (wn normalized to Nyquist like `butter_low`).

- [ ] **Step 1: Failing test**

```rust
#[cfg(test)]
mod high_tests {
    use super::*;
    fn sine(f: f64, n: usize) -> Vec<f64> {
        (0..n).map(|i| (2.0 * std::f64::consts::PI * f * i as f64 / 100.0).sin()).collect()
    }
    fn rms(x: &[f64]) -> f64 {
        (x.iter().map(|v| v * v).sum::<f64>() / x.len() as f64).sqrt()
    }
    #[test]
    fn butter_high_passes_high_blocks_low_and_dc() {
        let ba = butter_high(2, 1.0 / 50.0); // 1 Hz at fs=100
        assert_eq!(ba.b.len(), 3);
        let dc = filtfilt_padlen(&ba, &vec![5.0; 2000], 150);
        assert!(dc[300..1700].iter().all(|v| v.abs() < 1e-6));
        let hi = filtfilt_padlen(&ba, &sine(10.0, 2000), 150);
        assert!((rms(&hi[300..1700]) / rms(&sine(10.0, 2000)[300..1700]) - 1.0).abs() < 0.01);
        let lo = filtfilt_padlen(&ba, &sine(0.1, 4000), 150);
        assert!(rms(&lo[500..3500]) < 0.01);
        // coefficient sanity: high-pass b sums to 0
        assert!(ba.b.iter().sum::<f64>().abs() < 1e-12);
    }
}
```

- [ ] **Step 2: Run** `cargo test --offline -p o4core --lib high_tests` → FAIL (`butter_high` not found).

- [ ] **Step 3: Implement** (scipy `butter(N, Wn, 'high')`: lowpass prototype → `lp2hp_zpk` → bilinear):

```rust
/// scipy.signal.butter(order, wn, 'high') (zpk -> lp2hp -> bilinear).
pub fn butter_high(order: usize, wn: f64) -> Ba {
    let warped = 4.0 * (std::f64::consts::PI * wn / 2.0).tan();
    // lp2hp_zpk: z = zeros at 0 (order), p = warped / p_proto, k = 1 * real(prod(-z)/prod(-p)) with no proto zeros
    let proto = buttap(order);
    let p: Vec<C> = proto.iter().map(|&x| C::new(warped, 0.0) / x).collect();
    let z = vec![C::new(0.0, 0.0); order];
    let k = (C::new(1.0, 0.0) / proto.iter().fold(C::new(1.0, 0.0), |acc, &x| acc * (-x))).re;
    let (zd, pd, kd) = bilinear_zpk(&z, &p, k);
    Ba {
        b: poly(&zd).iter().map(|c| c * kd).collect(),
        a: poly(&pd),
    }
}
```

- [ ] **Step 4: Run** `cargo test --offline -p o4core --lib high_tests` → PASS.
- [ ] **Step 5: Commit (only if authorized)** — `git commit -am "feat(dsp): scipy-compatible butter_high"`

---

### Task 3: `refine::geometry` (pure math)

**Files:**
- Create: `o4core/src/refine/geometry.rs`
- Create: `o4core/src/refine/mod.rs` (only `pub mod geometry;` for now)
- Modify: `o4core/src/lib.rs` (add `pub mod refine;`)

**Interfaces:**
- Produces:
  - `pub type V3 = [f64; 3]; pub type M3 = [[f64; 3]; 3];`
  - `pub const O4P_MOUNT: M3`
  - `pub fn mat_vec(m: &M3, v: V3) -> V3`
  - `pub fn rotate(q: [f64; 4], v: V3) -> V3` (active rotation by unit quaternion, wxyz)
  - `pub fn fit_small_rotation(a: &[V3], b: &[V3], min_inliers: usize, floor: f64) -> Option<(V3, usize)>` — `d` (rad) with `b ≈ a + d × a`
  - `pub struct Orientation<'a> { pub t: &'a [f64], pub q: &'a [[f64; 4]] }` with `pub fn at(&self, t: f64) -> [f64; 4]` (slerp, clamped to ends)

- [ ] **Step 1: Write `geometry.rs` tests first** (bottom of the new file):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::quat::qexp;
    fn lcg(seed: &mut u64) -> f64 {
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((*seed >> 11) as f64) / ((1u64 << 53) as f64)
    }
    fn bearings(n: usize, seed: &mut u64) -> Vec<V3> {
        (0..n)
            .map(|_| {
                let (x, y) = (lcg(seed) * 2.6 - 1.3, lcg(seed) * 1.44 - 0.72);
                let m = (x * x + y * y + 1.0).sqrt();
                [x / m, y / m, 1.0 / m]
            })
            .collect()
    }
    #[test]
    fn fit_recovers_small_rotation_with_outliers() {
        let mut s = 7;
        let a = bearings(400, &mut s);
        let d = [0.0012, -0.0021, 0.0006];
        let q = qexp(d);
        let mut b: Vec<V3> = a.iter().map(|&v| rotate(q, v)).collect();
        for v in b.iter_mut().step_by(5) {
            v[0] += 0.02; // 20% gross outliers
        }
        let (e, n) = fit_small_rotation(&a, &b, 30, 0.0008).unwrap();
        for k in 0..3 {
            assert!((e[k] - d[k]).abs() < 2e-6, "{e:?} vs {d:?}");
        }
        assert!(n >= 300);
    }
    #[test]
    fn fit_refuses_too_few_points() {
        let mut s = 3;
        let a = bearings(20, &mut s);
        assert!(fit_small_rotation(&a, &a, 30, 0.0008).is_none());
    }
    #[test]
    fn mount_is_proper_rotation_and_rotate_identity() {
        let m = O4P_MOUNT;
        let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
        assert_eq!(det, 1.0);
        assert_eq!(rotate([1., 0., 0., 0.], [0.3, 0.4, 0.5]), [0.3, 0.4, 0.5]);
        assert_eq!(mat_vec(&m, [1., 2., 3.]), [1., -2., -3.]);
    }
    #[test]
    fn orientation_interpolates_and_clamps() {
        let t = [0.0, 1.0];
        let q = [[1., 0., 0., 0.], qexp([0., 0., 0.2])];
        let o = Orientation { t: &t, q: &q };
        let mid = o.at(0.5);
        let want = qexp([0., 0., 0.1]);
        assert!((0..4).all(|k| (mid[k] - want[k]).abs() < 1e-12));
        assert_eq!(o.at(-5.0), q[0]);
        assert!((0..4).all(|k| (o.at(9.0)[k] - q[1][k]).abs() < 1e-12));
    }
}
```

- [ ] **Step 2: Run** `cargo test --offline -p o4core --lib refine::geometry` → FAIL (items missing).

- [ ] **Step 3: Implement** (top of `geometry.rs`):

```rust
//! Pure geometry for residual refinement (feedback-v1 `warp_residual_probe`).
use crate::quat;

pub type V3 = [f64; 3];
pub type M3 = [[f64; 3]; 3];

/// Camera axes (x right, y down, z forward) -> telemetry body axes of DJI O4P
/// quaternions as decoded by `telemetry::extract_quats` (exhaustive
/// signed-permutation search, feedback-v1).
pub const O4P_MOUNT: M3 = [[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, -1.0]];

pub fn mat_vec(m: &M3, v: V3) -> V3 {
    std::array::from_fn(|r| m[r][0] * v[0] + m[r][1] * v[1] + m[r][2] * v[2])
}

pub fn rotate(q: [f64; 4], v: V3) -> V3 {
    let p = quat::qmul(quat::qmul(q, [0.0, v[0], v[1], v[2]]), quat::qconj(q));
    [p[1], p[2], p[3]]
}

fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn det3(m: &M3) -> f64 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

fn solve3(a: M3, b: V3) -> Option<V3> {
    let det = det3(&a);
    if det.abs() < 1e-18 {
        return None;
    }
    Some(std::array::from_fn(|k| {
        let mut m = a;
        for r in 0..3 {
            m[r][k] = b[r];
        }
        det3(&m) / det
    }))
}

/// Small rotation `d` (rad) with `b ≈ a + d × a`, by linear least squares with
/// four trimming rounds (keep residual < max(3·median, floor)).
/// Returns None when fewer than `min_inliers` points survive.
pub fn fit_small_rotation(a: &[V3], b: &[V3], min_inliers: usize, floor: f64) -> Option<(V3, usize)> {
    let mut keep = vec![true; a.len()];
    let mut d = [0.0; 3];
    let mut n = 0;
    for _ in 0..4 {
        let mut ata = [[0.0; 3]; 3];
        let mut atb = [0.0; 3];
        n = 0;
        for i in (0..a.len()).filter(|&i| keep[i]) {
            n += 1;
            let ax = [[0.0, -a[i][2], a[i][1]], [a[i][2], 0.0, -a[i][0]], [-a[i][1], a[i][0], 0.0]];
            let r0: V3 = std::array::from_fn(|k| b[i][k] - a[i][k]);
            for p in 0..3 {
                for q in 0..3 {
                    ata[p][q] += (0..3).map(|k| ax[k][p] * ax[k][q]).sum::<f64>();
                }
                atb[p] -= (0..3).map(|k| ax[k][p] * r0[k]).sum::<f64>();
            }
        }
        if n < min_inliers {
            return None;
        }
        d = solve3(ata, atb)?;
        let res: Vec<f64> = (0..a.len())
            .map(|i| {
                let c = cross(d, a[i]);
                (0..3).map(|k| (b[i][k] - a[i][k] - c[k]).powi(2)).sum::<f64>().sqrt()
            })
            .collect();
        let mut s: Vec<f64> = (0..a.len()).filter(|&i| keep[i]).map(|i| res[i]).collect();
        s.sort_by(f64::total_cmp);
        let thr = (3.0 * s[s.len() / 2]).max(floor);
        for i in 0..a.len() {
            keep[i] = res[i] < thr;
        }
    }
    (n >= min_inliers).then_some((d, n))
}

/// Orientation track with slerp lookup, clamped to its ends.
pub struct Orientation<'a> {
    pub t: &'a [f64],
    pub q: &'a [[f64; 4]],
}

impl Orientation<'_> {
    pub fn at(&self, t: f64) -> [f64; 4] {
        let i = self.t.partition_point(|&x| x <= t).clamp(1, self.t.len() - 1);
        let f = ((t - self.t[i - 1]) / (self.t[i] - self.t[i - 1])).clamp(0.0, 1.0);
        quat::slerp(self.q[i - 1], self.q[i], f)
    }
}
```

`mod.rs`: `//! Residual refinement (feedback-v1 wp1c), see spec 2026-09-24.` + `pub mod geometry;`. `lib.rs`: `pub mod refine;`.

Note `slerp(qa, qb, 0.0)` returns `qnorm(qa)`; the clamp test compares `o.at(-5.0) == q[0]` exactly, which holds for a unit identity. If it fails on round-off, compare with `< 1e-12`.

- [ ] **Step 4: Run** `cargo test --offline -p o4core --lib refine::geometry` → PASS.
- [ ] **Step 5: Commit (only if authorized)** — `git add o4core/src/refine o4core/src/lib.rs && git commit -m "feat(refine): geometry primitives"`

---

### Task 4: `refine::signal` (conditioning, gating, integration, checks)

**Files:**
- Create: `o4core/src/refine/signal.rs`
- Modify: `o4core/src/refine/mod.rs` (add `pub mod signal;`)

**Interfaces:**
- Consumes: `geometry::V3`, `dsp::{butter_high, filtfilt_padlen}`, `quat::{qmul, qconj, qexp, qnorm, smoothstep}`.
- Produces:

```rust
pub struct PairSeries { pub t: Vec<f64>, pub resid: Vec<Option<V3>>, pub inliers: Vec<usize>, pub tel_rate: Vec<f64> } // resid rad/s body frame; tel_rate rad/s; t = telemetry time of pair midpoint
pub struct Conditioned { pub t: Vec<f64>, pub hp: Vec<V3>, pub raw: Vec<V3>, pub conf: Vec<f64>, pub inliers: Vec<usize>, pub tel_rate: Vec<f64>, pub valid: Vec<bool> }
pub fn condition(s: &PairSeries, fps: f64, hp_hz: f64) -> Option<Conditioned>
pub fn gate(t: &[f64], bursts: &[(f64, f64)], pad: f64, fade: f64) -> Vec<f64>
pub fn burst_correction(c: &Conditioned, burst: (f64, f64), pad: f64, fade: f64, dt: f64) -> Vec<V3> // cumulative angle (rad) from this burst alone
pub fn max_angle_deg(ang: &[V3]) -> f64
pub struct GeometryStats { pub hp_rms_deg: f64, pub motion_ratio: Option<f64>, pub pairs: usize }
pub fn geometry_stats(ws: &[Conditioned], bursts: &[(f64, f64)], pad: f64, fade: f64) -> GeometryStats
pub fn apply_increments(t_tel: &[f64], q: &[[f64; 4]], t_ang: &[f64], ang: &[V3]) -> Vec<[f64; 4]>
```

- [ ] **Step 1: Failing tests** (bottom of `signal.rs`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::quat::{qexp, quats_to_rates};

    fn series(t0: f64, n: usize, f: impl Fn(f64) -> V3) -> PairSeries {
        let t: Vec<f64> = (0..n).map(|i| t0 + i as f64 * 0.01).collect();
        PairSeries {
            resid: t.iter().map(|&x| Some(f(x))).collect(),
            inliers: vec![500; n],
            tel_rate: vec![0.5; n],
            t,
        }
    }

    #[test]
    fn condition_fills_gaps_and_highpasses() {
        let mut s = series(10.0, 1000, |x| [0.02 * (2.0 * std::f64::consts::PI * 5.0 * x).sin() + 0.3, 0., 0.]);
        s.resid[400] = None;
        s.inliers[400] = 0;
        let c = condition(&s, 100.0, 1.0).unwrap();
        // DC 0.3 removed, 5 Hz kept
        let mid = &c.hp[200..800];
        let mean = mid.iter().map(|v| v[0]).sum::<f64>() / mid.len() as f64;
        assert!(mean.abs() < 1e-3);
        assert!(c.hp[400][0].is_finite() && !c.valid[400]);
        assert!(c.conf[400] < 1.0 && c.conf[100] == 1.0);
    }

    #[test]
    fn condition_rejects_mostly_missing() {
        let mut s = series(0.0, 300, |_| [0.; 3]);
        for r in s.resid.iter_mut().skip(100) {
            *r = None;
        }
        assert!(condition(&s, 100.0, 1.0).is_none());
    }

    #[test]
    fn gate_zero_outside_and_one_inside() {
        let t: Vec<f64> = (0..600).map(|i| i as f64 * 0.01).collect();
        let g = gate(&t, &[(2.0, 3.0)], 0.25, 0.15);
        assert_eq!(g[100], 0.0);
        assert_eq!(g[250], 1.0);
        assert_eq!(g[500], 0.0);
    }

    #[test]
    fn burst_correction_opposes_residual_and_is_flat_outside() {
        let s = series(0.0, 600, |x| [0.05 * (2.0 * std::f64::consts::PI * 4.0 * x).sin(), 0., 0.]);
        let c = condition(&s, 100.0, 1.0).unwrap();
        let ang = burst_correction(&c, (2.0, 4.0), 0.25, 0.15, 0.01);
        assert_eq!(ang[..150].iter().map(|v| v[0]).fold(0., |a: f64, b| a.max(b.abs())), 0.0);
        // derivative of the angle inside the burst is -residual
        let i = 300;
        let d = (ang[i + 1][0] - ang[i][0]) / 0.01;
        assert!((d + c.hp[i + 1][0]).abs() < 1e-9);
        // constant after the gate
        assert_eq!(ang[500], ang[599]);
    }

    #[test]
    fn zero_correction_returns_input_bits() {
        let t: Vec<f64> = (0..2000).map(|i| i as f64 * 0.001).collect();
        let q: Vec<[f64; 4]> = t.iter().map(|&x| qexp([0.1 * x, 0.02, -0.3 * x])).collect();
        let out = apply_increments(&t, &q, &[0.5, 1.5], &[[0.; 3], [0.; 3]]);
        assert_eq!(out, q);
    }

    #[test]
    fn increments_keep_rates_outside_and_leave_world_offset() {
        let t: Vec<f64> = (0..3001).map(|i| i as f64 * 0.001).collect();
        let q: Vec<[f64; 4]> = t.iter().map(|&x| qexp([0.4 * x, 0.1 * (3.0 * x).sin(), 0.2])).collect();
        let t_ang: Vec<f64> = (0..301).map(|i| i as f64 * 0.01).collect();
        let ang: Vec<V3> = t_ang
            .iter()
            .map(|&x| { let s = ((x - 1.0) / 1.0).clamp(0.0, 1.0); [0.01 * s, -0.02 * s, 0.005 * s] })
            .collect();
        let out = apply_increments(&t, &q, &t_ang, &ang);
        assert!(out[..1000].iter().zip(&q[..1000]).all(|(a, b)| a == b), "pre-gate bits");
        let (_, r0) = quats_to_rates(&t[2100..], &q[2100..]);
        let (_, r1) = quats_to_rates(&t[2100..], &out[2100..]);
        for (a, b) in r0.iter().zip(&r1) {
            for k in 0..3 { assert!((a[k] - b[k]).abs() < 1e-9); }
        }
        // after the gate, out = W * q with a constant world rotation W
        let w = |i: usize| qmul(out[i], qconj(q[i]));
        let (wa, wb) = (w(2100), w(3000));
        let dot: f64 = (0..4).map(|k| wa[k] * wb[k]).sum();
        assert!(dot.abs() > 1.0 - 1e-12);
    }

    #[test]
    fn merged_windows_are_time_ordered_and_continuous() {
        // two windows supplied out of order must still yield zero angle before the first
        let a = (vec![5.0, 5.01, 5.02], vec![[0.01, 0., 0.]; 3]);
        let b = (vec![1.0, 1.01, 1.02], vec![[0.02, 0., 0.]; 3]);
        let (t, ang) = chain_windows(vec![a, b]);
        assert!(t.windows(2).all(|w| w[1] > w[0]));
        assert_eq!(ang[0], [0.02, 0., 0.]); // first window's own angle, no carried offset from the later one
        assert_eq!(ang[3], [0.03, 0., 0.]); // later window carries the earlier total
    }

    #[test]
    fn geometry_stats_floor_rms() {
        let s = series(0.0, 800, |x| [0.02 * (2.0 * std::f64::consts::PI * 6.0 * x).sin(), 0., 0.]);
        let c = condition(&s, 100.0, 1.0).unwrap();
        let g = geometry_stats(&[c], &[(3.0, 4.0)], 0.25, 0.15);
        assert!(g.pairs > 300);
        let expect = 0.02_f64.to_degrees() / 2f64.sqrt();
        assert!((g.hp_rms_deg - expect).abs() < 0.1 * expect);
    }
}
```

`chain_windows` is also produced (add to Interfaces): `pub fn chain_windows(ws: Vec<(Vec<f64>, Vec<V3>)>) -> (Vec<f64>, Vec<V3>)` — sorts windows by first time, offsets each window's angles by the previous window's final total.

- [ ] **Step 2: Run** `cargo test --offline -p o4core --lib refine::signal` → FAIL.

- [ ] **Step 3: Implement** (top of `signal.rs`):

```rust
//! Residual conditioning, burst gating and orientation update (feedback-v1).
use super::geometry::V3;
use crate::dsp;
use crate::quat::{qconj, qexp, qmul, qnorm, smoothstep};

pub struct PairSeries {
    pub t: Vec<f64>,
    pub resid: Vec<Option<V3>>,
    pub inliers: Vec<usize>,
    pub tel_rate: Vec<f64>,
}

pub struct Conditioned {
    pub t: Vec<f64>,
    pub hp: Vec<V3>,
    pub raw: Vec<V3>,
    pub conf: Vec<f64>,
    pub inliers: Vec<usize>,
    pub tel_rate: Vec<f64>,
    pub valid: Vec<bool>,
}

/// Fill missing pairs linearly, zero-phase high-pass, confidence weights.
/// None when fewer than 60% of pairs are valid or the window is too short.
pub fn condition(s: &PairSeries, fps: f64, hp_hz: f64) -> Option<Conditioned> {
    let n = s.t.len();
    let valid: Vec<bool> = s.resid.iter().map(|r| r.is_some()).collect();
    let nv = valid.iter().filter(|&&v| v).count();
    if n < 50 || (nv as f64) < 0.6 * n as f64 {
        return None;
    }
    let known: Vec<usize> = (0..n).filter(|&i| valid[i]).collect();
    let tk: Vec<f64> = known.iter().map(|&i| s.t[i]).collect();
    let mut raw = vec![[0.0; 3]; n];
    for k in 0..3 {
        let fk: Vec<f64> = known.iter().map(|&i| s.resid[i].unwrap()[k]).collect();
        let filled = dsp::interp(&s.t, &tk, &fk);
        for i in 0..n {
            raw[i][k] = filled[i];
        }
    }
    let ba = dsp::butter_high(2, hp_hz / (fps / 2.0));
    let pad = 150.min(n - 1);
    let cols: Vec<Vec<f64>> = (0..3)
        .map(|k| dsp::filtfilt_padlen(&ba, &raw.iter().map(|r| r[k]).collect::<Vec<_>>(), pad))
        .collect();
    let hp: Vec<V3> = (0..n).map(|i| [cols[0][i], cols[1][i], cols[2][i]]).collect();
    let c0: Vec<f64> = (0..n)
        .map(|i| if valid[i] { ((s.inliers[i] as f64 - 60.0) / 140.0).clamp(0.0, 1.0) } else { 0.0 })
        .collect();
    // numpy convolve(c0, ones(9)/9, 'same')
    let conf: Vec<f64> = (0..n)
        .map(|i| (i.saturating_sub(4)..(i + 5).min(n)).map(|j| c0[j]).sum::<f64>() / 9.0)
        .collect();
    Some(Conditioned { t: s.t.clone(), hp, raw, conf, inliers: s.inliers.clone(), tel_rate: s.tel_rate.clone(), valid })
}

fn env(t: f64, a: f64, b: f64, fade: f64) -> f64 {
    smoothstep(((t - a).min(b - t) / fade).clamp(0.0, 1.0))
}

pub fn gate(t: &[f64], bursts: &[(f64, f64)], pad: f64, fade: f64) -> Vec<f64> {
    t.iter()
        .map(|&x| bursts.iter().map(|&(a, b)| env(x, a - pad, b + pad, fade)).fold(0.0, f64::max))
        .collect()
}

/// Cumulative correction angle (rad) for one burst: integral of −gate·conf·hp.
pub fn burst_correction(c: &Conditioned, burst: (f64, f64), pad: f64, fade: f64, dt: f64) -> Vec<V3> {
    let g = gate(&c.t, &[burst], pad, fade);
    let mut acc = [0.0; 3];
    (0..c.t.len())
        .map(|i| {
            let w = g[i] * c.conf[i];
            for k in 0..3 {
                acc[k] -= w * c.hp[i][k] * dt;
            }
            acc
        })
        .collect()
}

pub fn max_angle_deg(ang: &[V3]) -> f64 {
    ang.iter().map(|v| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()).fold(0.0, f64::max).to_degrees()
}

pub struct GeometryStats {
    pub hp_rms_deg: f64,
    pub motion_ratio: Option<f64>,
    pub pairs: usize,
}

/// Non-burst (gate < 0.01, ≥ 300 inliers) residual statistics across windows.
pub fn geometry_stats(ws: &[Conditioned], bursts: &[(f64, f64)], pad: f64, fade: f64) -> GeometryStats {
    let (mut ss, mut n) = (0.0, 0usize);
    let (mut res_mag, mut tel_mag) = (vec![], vec![]);
    for c in ws {
        let g = gate(&c.t, bursts, pad, fade);
        for i in 0..c.t.len() {
            if g[i] >= 0.01 || c.inliers[i] < 300 || !c.valid[i] {
                continue;
            }
            n += 1;
            ss += c.hp[i].iter().map(|v| v * v).sum::<f64>();
            if c.tel_rate[i] > 30f64.to_radians() {
                res_mag.push(c.raw[i].iter().map(|v| v * v).sum::<f64>().sqrt());
                tel_mag.push(c.tel_rate[i]);
            }
        }
    }
    let med = |mut v: Vec<f64>| { v.sort_by(f64::total_cmp); v[v.len() / 2] };
    GeometryStats {
        hp_rms_deg: if n > 0 { (ss / n as f64).sqrt().to_degrees() } else { f64::NAN },
        motion_ratio: (res_mag.len() >= 50).then(|| med(res_mag) / med(tel_mag)),
        pairs: n,
    }
}

/// Sort windows by start time and carry each window's final angle into the next.
pub fn chain_windows(mut ws: Vec<(Vec<f64>, Vec<V3>)>) -> (Vec<f64>, Vec<V3>) {
    ws.sort_by(|a, b| a.0[0].total_cmp(&b.0[0]));
    let (mut t, mut ang, mut total) = (vec![], vec![], [0.0; 3]);
    for (wt, wa) in ws {
        let base = total;
        for (x, a) in wt.into_iter().zip(wa) {
            let v = [base[0] + a[0], base[1] + a[1], base[2] + a[2]];
            t.push(x);
            ang.push(v);
            total = v;
        }
    }
    (t, ang)
}

/// Apply angle increments as extra body rotation per telemetry sample:
/// q'[i+1] = q'[i] · (q[i]⁻¹ q[i+1]) · exp(ΔA). Angle is 0 before t_ang[0],
/// held after the end; untouched samples keep their bits.
pub fn apply_increments(t_tel: &[f64], q: &[[f64; 4]], t_ang: &[f64], ang: &[V3]) -> Vec<[f64; 4]> {
    let cols: Vec<Vec<f64>> = (0..3)
        .map(|k| {
            let a: Vec<f64> = ang.iter().map(|v| v[k]).collect();
            t_tel
                .iter()
                .map(|&x| if x < t_ang[0] { 0.0 } else { dsp::interp(&[x], t_ang, &a)[0] })
                .collect()
        })
        .collect();
    let mut out = q.to_vec();
    for i in 0..q.len() - 1 {
        let inc = [cols[0][i + 1] - cols[0][i], cols[1][i + 1] - cols[1][i], cols[2][i + 1] - cols[2][i]];
        if inc == [0.0; 3] && out[i] == q[i] {
            continue;
        }
        let dq = qmul(qconj(q[i]), q[i + 1]);
        out[i + 1] = qnorm(qmul(qmul(out[i], dq), qexp(inc)));
    }
    out
}
```

Check `dsp::interp` semantics first (`o4core/src/dsp.rs:242`, numpy-style `interp(xq, xp, fp)` holding end values). For performance on 380k samples, the per-sample `dsp::interp(&[x], …)` call is O(n log n) acceptable only if `interp` binary-searches; if it scans linearly, replace with a single `dsp::interp(t_tel, t_ang, &a)` call and then zero the samples with `x < t_ang[0]`.

- [ ] **Step 4: Run** `cargo test --offline -p o4core --lib refine::signal` → PASS. Fix any test tolerance only if the failure is round-off (< 1e-9); otherwise fix code.
- [ ] **Step 5: Commit (only if authorized)** — `git commit -am "feat(refine): conditioning, gating and orientation update"`

---

### Task 5: `refine::measure` + `refine()` orchestration

**Files:**
- Create: `o4core/src/refine/measure.rs`
- Modify: `o4core/src/refine/mod.rs` (RefineConfig, RefineBurst, RefineResult, `refine()`)
- Modify: `o4core/src/optical.rs` (`fn k_d` → `pub(crate) fn k_d`)
- Create: `o4core/tests/refine_research_parity.rs` (ignored real-clip tests)

**Interfaces:**
- Consumes: Tasks 3–4 items; `optical::k_d(meta, w, h) -> (Mat, Mat)`; `telemetry::Meta`.
- Produces:

```rust
#[derive(Clone, Debug, PartialEq)]
pub struct RefineConfig {
    pub margin_s: f64,          // 1.0
    pub min_edge_margin_s: f64, // 0.5
    pub readout_ms: f64,        // 5.092569
    pub crop: (f64, f64),       // (1.30, 0.72)
    pub hp_hz: f64,             // 1.0
    pub gate_pad_s: f64,        // 0.25
    pub gate_fade_s: f64,       // 0.15
    pub max_correction_deg: f64,// 4.0
    pub max_floor_rms_deg: f64, // 8.0
    pub max_motion_ratio: f64,  // 0.25 (confirmed in Step 6)
    pub max_features: i32,      // 1200
    pub min_inliers: usize,     // 30
}
impl Default for RefineConfig
impl RefineConfig { pub fn validate(&self) -> Result<(), O4Error> }
pub struct RefineBurst { pub start: f64, pub end: f64, pub max_deg: f64, pub applied: bool, pub note: Option<String> }
pub struct RefineResult { pub q: Vec<[f64; 4]>, pub bursts: Vec<RefineBurst>, pub skipped_reason: Option<String>, pub stats: Option<signal::GeometryStats>,
                          pub angle_t: Vec<f64>, pub angle: Vec<geometry::V3> } // applied correction-angle track (chain_windows output); empty when nothing applied
pub fn refine(video: &Path, t: &[f64], q: &[[f64; 4]], intervals: &[(f64, f64)], meta: &Meta,
              cfg: &RefineConfig, log: &(dyn Fn(&str) + Sync), cancel: &AtomicBool) -> Result<RefineResult, O4Error>
// measure.rs
pub struct VideoInfo { pub fps: f64, pub width: i32, pub height: i32, pub frames: i64 }
pub fn video_info(video: &Path) -> Result<VideoInfo, O4Error>
pub fn window_frames(a: f64, b: f64, info: &VideoInfo) -> (i64, i64)  // inclusive first/last frame index
pub fn measure_window(video: &Path, a: f64, b: f64, info: &VideoInfo, meta: &Meta,
                      cfg: &RefineConfig, orient: &Orientation, cancel: &AtomicBool) -> Result<PairSeries, O4Error>
```

- [ ] **Step 1: Failing unit test** in `measure.rs` (no video needed):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn window_frames_uses_video_fps() {
        let i100 = VideoInfo { fps: 100.0, width: 1440, height: 1080, frames: 1000 };
        assert_eq!(window_frames(2.0, 3.5, &i100), (200, 350));
        let i60 = VideoInfo { fps: 59.94, width: 3840, height: 2160, frames: 600 };
        assert_eq!(window_frames(2.0, 3.5, &i60), (120, 210));
        // clipped to the video
        assert_eq!(window_frames(-1.0, 50.0, &i100), (0, 999));
    }
}
```

and in `mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_clip_to_video_and_merge() {
        let w = plan_windows(&[(0.3, 0.8), (1.2, 1.5), (5.0, 6.0)], 1.0, 10.0);
        assert_eq!(w, vec![(0.0, 2.5), (4.0, 7.0)]);
    }
    #[test]
    fn default_refine_config_is_valid() {
        assert!(RefineConfig::default().validate().is_ok());
        assert!(RefineConfig { max_correction_deg: f64::NAN, ..Default::default() }.validate().is_err());
    }
}
```

`plan_windows(intervals, margin, duration) -> Vec<(f64, f64)>` is private in `mod.rs`: pad each interval by `margin`, clip to [0, duration], sort, merge overlaps.

- [ ] **Step 2: Run** `cargo test --offline -p o4core --lib refine` → FAIL.

- [ ] **Step 3: Implement `measure.rs`** (port of `o4core/examples/warp_residual_probe.rs` with OpenCV decode):

```rust
//! OpenCV measurement of per-pair residual rotation against an orientation track.
use super::geometry::{fit_small_rotation, mat_vec, rotate, Orientation, O4P_MOUNT, V3};
use super::signal::PairSeries;
use super::RefineConfig;
use crate::error::O4Error;
use crate::quat::{qconj, qlog, qmul};
use crate::telemetry::Meta;
use opencv::core::{self, Mat, Point2f, Size, TermCriteria, Vector};
use opencv::prelude::*;
use opencv::{calib3d, imgproc, video, videoio};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct VideoInfo { pub fps: f64, pub width: i32, pub height: i32, pub frames: i64 }

pub fn video_info(video: &Path) -> Result<VideoInfo, O4Error> {
    let cap = videoio::VideoCapture::from_file(video.to_str().unwrap_or_default(), videoio::CAP_ANY)?;
    let info = VideoInfo {
        fps: cap.get(videoio::CAP_PROP_FPS)?,
        width: cap.get(videoio::CAP_PROP_FRAME_WIDTH)? as i32,
        height: cap.get(videoio::CAP_PROP_FRAME_HEIGHT)? as i32,
        frames: cap.get(videoio::CAP_PROP_FRAME_COUNT)? as i64,
    };
    if !cap.is_opened()? || !info.fps.is_finite() || info.fps <= 0.0 || info.width < 2 || info.height < 2 {
        return Err(O4Error::Cv("cannot decode video for refinement".into()));
    }
    Ok(info)
}

pub fn window_frames(a: f64, b: f64, info: &VideoInfo) -> (i64, i64) {
    let last = (info.frames - 1).max(0);
    (((a * info.fps).round() as i64).clamp(0, last), ((b * info.fps).round() as i64).clamp(0, last))
}

fn track(p: &Mat, g: &Mat, max_features: i32) -> opencv::Result<Vec<(Point2f, Point2f)>> {
    let mut a = Vector::<Point2f>::new();
    imgproc::good_features_to_track(p, &mut a, max_features, 0.005, 18.0, &core::no_array(), 7, false, 0.04)?;
    if a.len() < 40 {
        return Ok(vec![]);
    }
    let crit = TermCriteria::new(core::TermCriteria_COUNT + core::TermCriteria_EPS, 30, 0.01)?;
    let (mut b, mut st, mut err) = (Vector::<Point2f>::new(), Vector::<u8>::new(), Vector::<f32>::new());
    video::calc_optical_flow_pyr_lk(p, g, &a, &mut b, &mut st, &mut err, Size::new(21, 21), 4, crit, 0, 1e-4)?;
    let (mut r, mut sr, mut er) = (Vector::<Point2f>::new(), Vector::<u8>::new(), Vector::<f32>::new());
    video::calc_optical_flow_pyr_lk(g, p, &b, &mut r, &mut sr, &mut er, Size::new(21, 21), 4, crit, 0, 1e-4)?;
    let mut out = vec![];
    for i in 0..a.len() {
        let (x, y, z) = (a.get(i)?, b.get(i)?, r.get(i)?);
        if st.get(i)? == 1 && sr.get(i)? == 1 && (z.x - x.x).hypot(z.y - x.y) < 0.5 {
            out.push((x, y));
        }
    }
    Ok(out)
}

fn bearings(pts: &[Point2f], k: &Mat, d: &Mat) -> opencv::Result<Vec<V3>> {
    let src: Vector<Point2f> = pts.iter().copied().collect();
    let mut u = Vector::<Point2f>::new();
    calib3d::fisheye_undistort_points_def(&src, &mut u, k, d)?;
    Ok(u.iter()
        .map(|p| {
            let m = ((p.x as f64).powi(2) + (p.y as f64).powi(2) + 1.0).sqrt();
            [p.x as f64 / m, p.y as f64 / m, 1.0 / m]
        })
        .collect())
}

#[allow(clippy::too_many_arguments)]
pub fn measure_window(video: &Path, a: f64, b: f64, info: &VideoInfo, meta: &Meta, cfg: &RefineConfig,
                      orient: &Orientation, cancel: &AtomicBool) -> Result<PairSeries, O4Error> {
    let (k, d) = crate::optical::k_d(meta, info.width, info.height);
    let readout = cfg.readout_ms / 1000.0;
    let (f0, f1) = window_frames(a, b, info);
    let mut cap = videoio::VideoCapture::from_file(video.to_str().unwrap_or_default(), videoio::CAP_ANY)?;
    cap.set(videoio::CAP_PROP_POS_FRAMES, f0 as f64)?;
    let mut s = PairSeries { t: vec![], resid: vec![], inliers: vec![], tel_rate: vec![] };
    let (mut prev, mut frame) = (None::<Mat>, Mat::default());
    let row_t = |fi: i64, y: f32| fi as f64 / info.fps + y as f64 / info.height as f64 * readout;
    for fi in f0..=f1 {
        if cancel.load(Ordering::Relaxed) {
            return Err(O4Error::Cancelled);
        }
        if !cap.read(&mut frame)? {
            break;
        }
        let mut g = Mat::default();
        imgproc::cvt_color_def(&frame, &mut g, imgproc::COLOR_BGR2GRAY)?;
        if let Some(p) = &prev {
            let fa = fi - 1;
            let tmid = fi as f64 / info.fps + readout * 0.5;
            let qa_mid = orient.at(fa as f64 / info.fps + readout * 0.5);
            let qb_mid = orient.at(tmid);
            let dv = qlog(qmul(qconj(qa_mid), qb_mid));
            let tel_rate = (dv[0] * dv[0] + dv[1] * dv[1] + dv[2] * dv[2]).sqrt() * info.fps;
            let tr = track(p, &g, cfg.max_features)?;
            let mut resid = None;
            let mut n = 0;
            if tr.len() >= 40 {
                let ba = bearings(&tr.iter().map(|x| x.0).collect::<Vec<_>>(), &k, &d)?;
                let bb = bearings(&tr.iter().map(|x| x.1).collect::<Vec<_>>(), &k, &d)?;
                let inside = |v: &V3| v[2] > 0.0 && (v[0] / v[2]).abs() < cfg.crop.0 && (v[1] / v[2]).abs() < cfg.crop.1;
                let (mut wa, mut wb) = (vec![], vec![]);
                for i in 0..tr.len() {
                    if !(inside(&ba[i]) && inside(&bb[i])) {
                        continue;
                    }
                    wa.push(rotate(orient.at(row_t(fa, tr[i].0.y)), mat_vec(&O4P_MOUNT, ba[i])));
                    wb.push(rotate(orient.at(row_t(fi, tr[i].1.y)), mat_vec(&O4P_MOUNT, bb[i])));
                }
                if let Some((dw, ni)) = fit_small_rotation(&wa, &wb, cfg.min_inliers, 0.0008) {
                    let q_mid = orient.at(tmid);
                    let db = rotate(qconj(q_mid), dw);
                    resid = Some([db[0] * info.fps, db[1] * info.fps, db[2] * info.fps]);
                    n = ni;
                }
            }
            s.t.push(tmid);
            s.resid.push(resid);
            s.inliers.push(n);
            s.tel_rate.push(tel_rate);
        }
        prev = Some(g);
    }
    Ok(s)
}
```

- [ ] **Step 4: Implement `refine()` in `mod.rs`**:

```rust
//! Residual refinement (feedback-v1 wp1c), spec 2026-09-24.
pub mod geometry;
pub mod measure;
pub mod signal;

use crate::error::O4Error;
use crate::telemetry::Meta;
use rayon::prelude::*;
use std::path::Path;
use std::sync::atomic::AtomicBool;

// RefineConfig / RefineBurst / RefineResult as in Interfaces, with Default giving the listed values.

impl RefineConfig {
    pub fn validate(&self) -> Result<(), O4Error> {
        let pos = [self.margin_s, self.readout_ms, self.crop.0, self.crop.1, self.hp_hz, self.gate_fade_s,
                   self.max_correction_deg, self.max_floor_rms_deg, self.max_motion_ratio];
        let nonneg = [self.min_edge_margin_s, self.gate_pad_s];
        if pos.iter().any(|v| !v.is_finite() || *v <= 0.0) || nonneg.iter().any(|v| !v.is_finite() || *v < 0.0)
            || self.max_features < 40 || self.min_inliers < 3
        {
            return Err(O4Error::InvalidConfig("refine settings must be finite and positive".into()));
        }
        Ok(())
    }
}

fn plan_windows(intervals: &[(f64, f64)], margin: f64, duration: f64) -> Vec<(f64, f64)> {
    let mut w: Vec<(f64, f64)> = intervals.iter().map(|&(a, b)| ((a - margin).max(0.0), (b + margin).min(duration))).collect();
    w.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut out: Vec<(f64, f64)> = vec![];
    for (a, b) in w {
        match out.last_mut() {
            Some(l) if a <= l.1 => l.1 = l.1.max(b),
            _ => out.push((a, b)),
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
pub fn refine(video: &Path, t: &[f64], q: &[[f64; 4]], intervals: &[(f64, f64)], meta: &Meta,
              cfg: &RefineConfig, log: &(dyn Fn(&str) + Sync), cancel: &AtomicBool) -> Result<RefineResult, O4Error> {
    let unchanged = |reason: String| RefineResult { q: q.to_vec(), bursts: vec![], skipped_reason: Some(reason), stats: None, angle_t: vec![], angle: vec![] };
    if meta.camera_matrix.is_none() || meta.distortion.is_none() {
        return Ok(unchanged("no lens metadata in telemetry".into()));
    }
    let info = match measure::video_info(video) {
        Ok(i) => i,
        Err(e) => return Ok(unchanged(format!("cannot decode video ({e})"))),
    };
    let duration = info.frames as f64 / info.fps;
    let windows = plan_windows(intervals, cfg.margin_s, duration);
    let orient = geometry::Orientation { t, q };
    let measured: Vec<Result<Option<signal::Conditioned>, O4Error>> = windows
        .par_iter()
        .map(|&(a, b)| match measure::measure_window(video, a, b, &info, meta, cfg, &orient, cancel) {
            Ok(s) => Ok(signal::condition(&s, info.fps, cfg.hp_hz)),
            Err(O4Error::Cancelled) => Err(O4Error::Cancelled),
            Err(e) => {
                log(&format!("     refine: window {a:.2}-{b:.2}s unreadable ({e})"));
                Ok(None)
            }
        })
        .collect();
    let mut conds: Vec<((f64, f64), signal::Conditioned)> = vec![];
    for (w, r) in windows.iter().zip(measured) {
        if let Some(c) = r? {
            conds.push((*w, c));
        }
    }
    let stats = signal::geometry_stats(&conds.iter().map(|x| &x.1).cloned().collect::<Vec<_>>(), intervals, cfg.gate_pad_s, cfg.gate_fade_s);
    // (Conditioned needs #[derive(Clone)] for the line above; add it in signal.rs.)
    if stats.pairs < 100 || !(stats.hp_rms_deg < cfg.max_floor_rms_deg)
        || stats.motion_ratio.is_some_and(|r| r >= cfg.max_motion_ratio)
    {
        let mut r = unchanged(format!(
            "geometry check failed (floor {:.1} deg/s, motion ratio {:?}, {} pairs) - camera/lens conventions not recognised",
            stats.hp_rms_deg, stats.motion_ratio, stats.pairs));
        r.stats = Some(stats);
        return Ok(r);
    }
    let dt = 1.0 / info.fps;
    let mut bursts = vec![];
    let mut per_window: Vec<(Vec<f64>, Vec<geometry::V3>)> = vec![];
    for &(a, b) in intervals {
        if !conds.iter().any(|(w, _)| a >= w.0 && b <= w.1) {
            bursts.push(RefineBurst { start: a, end: b, max_deg: 0.0, applied: false, note: Some("window not measurable".into()) });
        }
    }
    for ((wa, wb), c) in &conds {
        let mut total = vec![[0.0; 3]; c.t.len()];
        for &(a, b) in intervals.iter().filter(|(a, b)| *a >= *wa && *b <= *wb) {
            if a - wa < cfg.min_edge_margin_s || wb - b < cfg.min_edge_margin_s {
                bursts.push(RefineBurst { start: a, end: b, max_deg: 0.0, applied: false, note: Some("too close to clip edge".into()) });
                continue;
            }
            let ang = signal::burst_correction(c, (a, b), cfg.gate_pad_s, cfg.gate_fade_s, dt);
            let m = signal::max_angle_deg(&ang);
            if m > cfg.max_correction_deg {
                bursts.push(RefineBurst { start: a, end: b, max_deg: m, applied: false, note: Some(format!("correction {m:.2} deg over cap")) });
                continue;
            }
            for (tot, v) in total.iter_mut().zip(&ang) {
                for k in 0..3 { tot[k] += v[k]; }
            }
            bursts.push(RefineBurst { start: a, end: b, max_deg: m, applied: true, note: None });
        }
        per_window.push((c.t.clone(), total));
    }
    bursts.sort_by(|x, y| x.start.total_cmp(&y.start));
    if !bursts.iter().any(|b| b.applied) {
        return Ok(RefineResult { q: q.to_vec(), bursts, skipped_reason: None, stats: Some(stats), angle_t: vec![], angle: vec![] });
    }
    let (ta, aa) = signal::chain_windows(per_window);
    let q2 = signal::apply_increments(t, q, &ta, &aa);
    Ok(RefineResult { q: q2, bursts, skipped_reason: None, stats: Some(stats), angle_t: ta, angle: aa })
}
```

Add `#[derive(Clone)]` to `Conditioned` and make `geometry_stats` take `&[Conditioned]` (as written). `Orientation` must be `Sync` for `par_iter` — it holds shared slices, so it is.

- [ ] **Step 5: Run unit tests** `cargo test --offline -p o4core --lib refine` → PASS. `cargo clippy --release --offline -p o4core -- -D warnings` → clean.

- [ ] **Step 6: Real-clip parity + threshold confirmation** — create `o4core/tests/refine_research_parity.rs`:

```rust
//! Production refine() vs research wp1c (feedback-v1). Needs local artifacts under target/experiments.
mod common;
use o4core::refine::{refine, RefineConfig};
use o4core::telemetry::extract_quats;
use std::sync::atomic::AtomicBool;

fn bursts(stages: &str) -> Vec<(f64, f64)> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(common::repo(stages)).unwrap()).unwrap();
    v["bursts"].as_array().unwrap().iter()
        .map(|b| (b["interval"][0].as_f64().unwrap(), b["interval"][1].as_f64().unwrap())).collect()
}

fn research_angle(adjust: &str, t: &[f64]) -> Vec<[f64; 3]> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(common::repo(adjust)).unwrap()).unwrap();
    let ta: Vec<f64> = serde_json::from_value(v["t"].clone()).unwrap();
    let aa: Vec<[f64; 3]> = serde_json::from_value(v["angle"].clone()).unwrap();
    t.iter().map(|&x| {
        if x < ta[0] { return [0.0; 3]; }
        let i = ta.partition_point(|&y| y <= x).clamp(1, ta.len() - 1);
        let f = ((x - ta[i - 1]) / (ta[i] - ta[i - 1])).clamp(0.0, 1.0);
        std::array::from_fn(|k| aa[i - 1][k] + f * (aa[i][k] - aa[i - 1][k]))
    }).collect()
}

fn case(base_mp4: &str, stages: &str, adjust: &str) {
    let tel = extract_quats(&common::repo(base_mp4)).unwrap();
    let iv = bursts(stages);
    let r = refine(&common::repo(base_mp4), &tel.t, &tel.q, &iv, &tel.meta, &RefineConfig::default(),
                   &|s| println!("{s}"), &AtomicBool::new(false)).unwrap();
    let st = r.stats.as_ref().unwrap();
    println!("{base_mp4}: floor {:.2} deg/s, motion ratio {:?}, pairs {}, skipped {:?}", st.hp_rms_deg, st.motion_ratio, st.pairs, r.skipped_reason);
    assert!(r.skipped_reason.is_none());
    // compare applied correction-angle tracks (sum of body increments), per burst,
    // relative to each burst's start; carried offsets from other bursts cancel
    let research = research_angle(adjust, &r.angle_t);
    let mut worst = 0.0_f64;
    for b in r.bursts.iter().filter(|b| b.applied) {
        let (i0, i1) = (r.angle_t.partition_point(|&x| x < b.start - 0.3), r.angle_t.partition_point(|&x| x <= b.end + 0.3));
        if i1 <= i0 + 1 { continue; }
        let (p0, q0) = (r.angle[i0], research[i0]);
        // research did not refine this burst (outside its windows): skip
        if (0..3).all(|k| (research[i1 - 1][k] - q0[k]).abs() < 1e-12) { continue; }
        for i in i0..i1 {
            let diff = (0..3).map(|k| ((r.angle[i][k] - p0[k]) - (research[i][k] - q0[k])).powi(2)).sum::<f64>().sqrt().to_degrees();
            worst = worst.max(diff);
        }
        println!("  burst {:.2}-{:.2} max {:.3} deg", b.start, b.end, b.max_deg);
    }
    println!("  worst in-burst difference vs research {worst:.4} deg");
    assert!(worst < 0.15, "production diverges from reviewed wp1c by {worst} deg");
}

#[test] #[ignore]
fn parity_0073() { case("target/experiments/generalization-v1/0073/edgeoffset.MP4", "target/experiments/residual-stage-v1/0073/stages.json", "target/experiments/feedback-v1/wp1c-adjust.json"); }
#[test] #[ignore]
fn parity_0071() { case("target/experiments/generalization-v1/0071/edgeoffset.MP4", "target/experiments/residual-stage-v1/0071/stages.json", "target/experiments/feedback-v1/0071/wp1c-adjust.json"); }
```

For 0060 the research bursts come from a different file shape; add:

```rust
#[test] #[ignore]
fn parity_0060() { case("target/experiments/gyro-trace-v1/edgeoffset.MP4", "target/experiments/gyro-trace-v1/edgeoffset-metrics.json", "target/experiments/feedback-v1/0060/wp1c-adjust.json"); }
```

(`edgeoffset-metrics.json` also has `bursts[].interval`.) Research wp1c refined only bursts inside its measurement windows; the loop above skips bursts where the research angle is constant (not refined there).

Also add a wrong-mount check to confirm the motion-ratio threshold — a temporary `#[ignore]` test is not possible without a mount parameter, so instead print `motion_ratio` for all three clips and record it. Then run the research probe with a wrong mount on the 0073 late window:

`O4_CROP=1.30,0.72 target/release/examples/warp_residual_probe.exe target/experiments/generalization-v1/0073/edgeoffset.MP4 130 18 5.092569 0 0 target/experiments/feedback-v1/warp/wrongmount-late.json`

and compute its raw-residual/telemetry-rate ratio with a short numpy snippet (pairs with telemetry rate > 30 deg/s: telemetry rate from `quats_to_rates` of the file at pair times). Set `max_motion_ratio` default halfway (geometrically) between the largest correct-mount ratio and the wrong-mount ratio if the correct values exceed 0.2; otherwise keep 0.25. Record the three measured ratios and the wrong-mount ratio in a comment on the `max_motion_ratio` default.

Run: `cargo test --release --offline -p o4core --test refine_research_parity -- --ignored --nocapture --test-threads=1`
Expected: three PASS; printed floors 1–5 deg/s; worst differences < 0.15°. If a clip fails parity, diff `measure_window` pair series against `target/experiments/feedback-v1/warp/<clip>/basecrop-<win>.json` (`rate` per pair) to locate the stage before changing anything; do not loosen the tolerance without recording why.

- [ ] **Step 7: Cancellation test** — append to the same file:

```rust
#[test] #[ignore]
fn refine_honours_cancel() {
    let base = "target/experiments/generalization-v1/0073/edgeoffset.MP4";
    let tel = extract_quats(&common::repo(base)).unwrap();
    let cancel = AtomicBool::new(true);
    let r = refine(&common::repo(base), &tel.t, &tel.q, &[(141.25, 144.45)], &tel.meta,
                   &RefineConfig::default(), &|_| {}, &cancel);
    assert!(matches!(r, Err(o4core::error::O4Error::Cancelled)));
}
```

Run it; expected PASS.

- [ ] **Step 8: Commit (only if authorized)** — `git add o4core/src/refine o4core/src/optical.rs o4core/tests/refine_research_parity.rs && git commit -m "feat(refine): source-frame residual measurement and refine()"`

---

### Task 6: Config, CLI and GUI plumbing

**Files:**
- Modify: `o4core/src/config.rs` (fields `refine: bool`, `refine_cfg: RefineConfig`; defaults; `validate()` calls `self.refine_cfg.validate()?`; `defaults_match_spec` asserts `c.refine`)
- Modify: `o4fix-cli/src/args.rs` (`--no-refine`), `o4fix-cli/tests/cli_args.rs`
- Modify: `o4fix-app/src/settings.rs` (DTO field `refine`, migration v2), `o4fix-app/ui/help.js` (DEFAULTS, HELP, FIELDS)

**Interfaces:**
- Consumes: `refine::RefineConfig` (Task 5).
- Produces: `Config { refine: bool, refine_cfg: RefineConfig, .. }`; CLI flag `--no-refine`; `ConfigDto.refine: bool`; `CURRENT_SETTINGS_VERSION = 2`.

- [ ] **Step 1: Failing tests.**

`o4fix-cli/tests/cli_args.rs`:

```rust
#[test]
fn refine_on_by_default_and_flag_disables() {
    assert!(Cli::parse_from(["o4fix", "a.MP4"]).to_config().refine);
    assert!(!Cli::parse_from(["o4fix", "a.MP4", "--no-refine"]).to_config().refine);
    assert_eq!(Cli::parse_from(["o4fix", "a.MP4"]).to_config().ramp, 0.19);
}
```

`o4fix-app/src/settings.rs` tests:

```rust
    #[test]
    fn v1_settings_migrate_ramp_and_refine() {
        let mut s: GuiSettings = serde_json::from_value(serde_json::json!({
            "settings_version": 1, "profile": "m2",
            "config": { "ramp": 0.3, "drift_rebase_above": 0.0 },
            "output_dir": null, "concurrent_files": 1
        })).unwrap();
        assert!(s.config.refine, "missing key adopts default-on");
        assert!(s.migrate());
        assert_eq!(s.config.ramp, 0.19);
        assert_eq!(s.config.drift_rebase_above, 0.0, "v1 deliberate 0 untouched");
        assert_eq!(s.settings_version, CURRENT_SETTINGS_VERSION);
        assert!(!s.migrate());
    }

    #[test]
    fn v1_custom_ramp_kept() {
        let mut s: GuiSettings = serde_json::from_value(serde_json::json!({
            "settings_version": 1, "profile": "m2", "config": { "ramp": 0.25 },
            "output_dir": null, "concurrent_files": 1
        })).unwrap();
        assert!(s.migrate());
        assert_eq!(s.config.ramp, 0.25);
    }

    #[test]
    fn v2_deliberate_legacy_values_kept() {
        let mut s = GuiSettings { settings_version: CURRENT_SETTINGS_VERSION, ..GuiSettings::default() };
        s.config.ramp = 0.3;
        s.config.refine = false;
        assert!(!s.migrate());
        assert_eq!(s.config.ramp, 0.3);
        assert!(!s.config.refine);
    }
```

The existing `v0_settings_migrate_drift_rebase_on` test must still pass (v0 → v2 performs both steps; that file has no ramp key, so ramp comes from the new default 0.19).

- [ ] **Step 2: Run** `cargo test --offline --workspace` → compile errors / FAIL.

- [ ] **Step 3: Implement.**

config.rs: add fields after `drift_decay_rate`:

```rust
    /// Source-frame residual refinement inside severe bursts (spec 2026-09-24).
    pub refine: bool,
    pub refine_cfg: crate::refine::RefineConfig,
```

defaults `refine: true, refine_cfg: crate::refine::RefineConfig::default()`; at the end of `validate()` before `Ok(())`: `self.refine_cfg.validate()?;`; in `defaults_match_spec` add `assert!(c.refine);`.

args.rs: add

```rust
    /// skip the source-frame residual refinement inside repaired bursts
    /// (default: on). It removes most remaining judder; turn it off only to
    /// save processing time or to reproduce pre-0.1.3 output
    #[arg(long)]
    pub no_refine: bool,
```

and in `to_config`: `refine: !self.no_refine, refine_cfg: o4core::refine::RefineConfig::default(),`.

settings.rs: DTO `pub refine: bool,`; `from_config`: `refine: c.refine,`; `to_config`: `refine: self.refine, refine_cfg: o4core::refine::RefineConfig::default(),`. Set `CURRENT_SETTINGS_VERSION: u32 = 2` and rewrite `migrate`:

```rust
    /// v0 -> v1 (0.1.2): drift rebase default-on (see below).
    /// v1 -> v2 (0.1.3): edge ramp default 0.3 -> 0.19. A stored 0.3 recorded
    /// the old default, so adopt the new one; other values are deliberate.
    /// `refine` needs no step: files without the key read as default-on.
    fn migrate(&mut self) -> bool {
        if self.settings_version >= CURRENT_SETTINGS_VERSION {
            return false;
        }
        if self.settings_version < 1 && self.config.drift_rebase_above == 0.0 {
            self.config.drift_rebase_above = Config::default().drift_rebase_above;
        }
        if self.settings_version < 2 && self.config.ramp == 0.3 {
            self.config.ramp = Config::default().ramp;
        }
        self.settings_version = CURRENT_SETTINGS_VERSION;
        true
    }
```

Keep the existing v0→v1 doc text above the new lines.

help.js: DEFAULTS add `refine: true`; HELP add `refine: "re-measure the leftover judder inside each repaired burst from the video frames and correct it (recommended; adds processing time proportional to burst length). The corrected file still loads in Gyroflow like a stock recording"`; FIELDS insert first in the list: `["Refinement", "refine", "Refine residual judder (recommended)", "bool"],`.

- [ ] **Step 4: Run** `cargo test --offline --workspace` → PASS; `node tools/test_queue_ui.cjs` → PASS.
- [ ] **Step 5: Commit (only if authorized)** — `git commit -am "feat: refine config, --no-refine flag, GUI toggle, settings v2 migration"`

---

### Task 7: Pipeline integration + `Refine` stage

**Files:**
- Modify: `o4core/src/pipeline.rs` (Stage enum, call refine after splice)
- Modify: `o4fix-app/src/queue.rs` (stage label), `o4fix-app/ui/style.css` (chip class)
- Modify: `o4core/tests/e2e.rs` (legacy settings add `refine: false`; new default-on e2e test), `o4core/tests/rebased_clip.rs` (`refine: false`)
- Create: test in `o4core/tests/e2e.rs` `refine_off_is_bit_identical_to_splice`

**Interfaces:**
- Consumes: `refine::refine`, `Config::refine`, `Config::refine_cfg`.
- Produces: `Stage::Refine`; `Outcome::Repaired` unchanged.

- [ ] **Step 1: Failing tests.** In `o4core/tests/e2e.rs`, change `legacy` to also set `refine: false`:

```rust
fn legacy(c: Config) -> Config {
    Config { ramp: 0.3, refine: false, ..c }
}
```

and add:

```rust
#[test]
#[ignore] // ~12 min
fn refine_off_is_bit_identical_to_splice() {
    // with refinement off, output equals the splice-only pipeline exactly;
    // with it on, only in-burst windows change and the file still verifies
    let off = std::env::temp_dir().join("o4fix_refine_off.MP4");
    let on = std::env::temp_dir().join("o4fix_refine_on.MP4");
    for p in [&off, &on] { let _ = std::fs::remove_file(p); }
    run(&Config { refine: false, ..Config::default() }, &off).unwrap();
    let r = run(&Config::default(), &on).unwrap();
    assert!(matches!(r, Outcome::Repaired { .. }));
    let (t0, q0) = stream(&off);
    let (t1, q1) = stream(&on);
    assert_eq!(t0, t1);
    let mut zi = gt::npz("intervals.npz");
    let sev: ndarray::Array2<f64> = zi.by_name("severe").unwrap();
    let first = sev[[0, 0]] - 1.5; // before the first refinement window
    let n_before = t0.iter().filter(|&&x| x / 1000.0 < first).count();
    assert_eq!(&q0[..n_before], &q1[..n_before], "samples before the first gate must be identical");
    assert!(q0 != q1, "refinement changed nothing on 0021");
}
```

In `o4core/tests/rebased_clip.rs` pass `&o4core::config::Config { refine: false, ..o4core::config::Config::default() }`.

- [ ] **Step 2: Run** `cargo build --offline --workspace` → fails until the enum/pipeline change compiles (queue.rs match is exhaustive).

- [ ] **Step 3: Implement.** `Stage` gains `Refine` between `Splice` and `Write`. In `process`, after the splice-report loop and before `check()?; let out_path…`:

```rust
    let q_out = if cfg.refine {
        check()?;
        say(Stage::Refine, 0.88, format!("   refining residual judder in {} bursts", intervals.len()));
        let rlog = |s: &str| say(Stage::Refine, 0.89, s.to_string());
        let r = crate::refine::refine(video, &tel.t, &q_out, &intervals, &tel.meta, &cfg.refine_cfg, &rlog, cancel)?;
        if let Some(reason) = &r.skipped_reason {
            say(Stage::Refine, 0.91, format!("   refinement skipped: {reason}"));
        }
        for b in &r.bursts {
            say(Stage::Refine, 0.91, match &b.note {
                None => format!("     [{:7.2}, {:7.2}] residual correction {:4.2} deg", b.start, b.end, b.max_deg),
                Some(n) => format!("     [{:7.2}, {:7.2}] not refined: {n}", b.start, b.end),
            });
        }
        r.q
    } else {
        q_out
    };
```

(`refine()` only returns `Err(Cancelled)`; `?` propagates it.) Add `use crate::refine;` only if you prefer the short path. `o4fix-app/src/queue.rs`: add arm `Stage::Refine => "refining",`. `o4fix-app/ui/style.css`: extend the selector `.chip.analyzing, .chip.measuring-motion, .chip.patching, .chip.verifying` with `, .chip.refining`.

Update the `process` doc comment: list the refine stage and that cancellation is polled per frame inside it.

- [ ] **Step 4: Run.**
`cargo test --offline --workspace` → PASS.
`cargo clippy --release --offline --workspace -- -D warnings` → clean.
`cargo test --release --offline -p o4core --test e2e -- --ignored --nocapture --test-threads=1` → all PASS (legacy goldens byte-identical; new test PASS).
`cargo test --release --offline -p o4core --test rebased_clip -- --ignored --nocapture` → PASS.
`node tools/test_queue_ui.cjs` → PASS.

- [ ] **Step 5: Rebuild shipping binaries** — `cargo build --release --offline --workspace` (stale-GUI rule).
- [ ] **Step 6: Commit (only if authorized)** — `git commit -am "feat: refine stage in repair pipeline (default on)"`

---

### Task 8: Validation, review page, docs (controller-run; long jobs)

**Files:**
- Create: `docs/experiments/feedback-v1/review-release-vs-wp1c.json`
- Modify: `README.md`, `CLAUDE.md` (top status line), `docs/SESSION_HANDOFF.md`, `docs/experiments/feedback-v1/results.md` (production section), `docs/development.md` (refine module note)

**Interfaces:**
- Consumes: release binaries from Task 7.

- [ ] **Step 1: Real-binary runs.** For each clip run the release CLI to a NEW path under `target/experiments/release-v013/` and record wall time:

```powershell
Measure-Command { target\release\o4fix.exe sample_vids\DJI_20260829141435_0073_D.MP4 -o target\experiments\release-v013\0073_fixed.MP4 } | Select TotalSeconds
```

Repeat for 0060 (`DJI_20260808151831_0060_D.MP4`), 0071 (`DJI_20260829140442_0071_D.MP4`), 0021 (`DJI_20260711124046_0021_D.MP4`). Expected: exit 0, "round-trip exact" line, per-burst refine lines, no "refinement skipped". Also time one run with `--no-refine` on 0060 to report the added cost.

- [ ] **Step 2: Parity with reviewed research outputs (no new test).** Parity is established by composition: Task 1's `rebased_clip` proves the release splice reproduces `edgeoffset.MP4` (≤ 1e-6), and Task 5's `refine_research_parity` proves `refine()` on that input matches wp1c in every burst the research run refined (< 0.15°). A whole-file comparison is not meaningful: production also refines bursts outside the research windows, and each leaves its own constant world-frame offset afterwards. Record both test outputs in the results doc; Step 4's review page is the end-to-end visual check.

- [ ] **Step 3: 0021 regression render.** Create a project with `python docs/experiments/feedback-v1/fbclip.py`-style retargeting (copy `sample_vids/eval_M2_tight.gyroflow` if present, else `target/experiments/...` 0021 project; set videofile/gyro filepath to the release 0021 output, drop `gyro_source.file_metadata`, offsets `{}`), render with `tools/gyroflow-portable/Gyroflow.exe`, then score with `python/analysis/eval_render.py` against the shipped M2 render (`sample_vids/eval_M2_tight.mp4`) using `rank_renders.py`. Expected: clean/mild rows within ±1 deg/s of M2 (eval coupling noise); severe/flicks equal or better. Record the table in the results doc.

- [ ] **Step 4: Review page.** Render the release outputs of 0073 and 0060 (Gyroflow portable, same project retargeting) and export side-by-sides LEFT research wp1c render vs RIGHT release render with `fbclip.reviews_pair`-style ffmpeg commands (windows: 0073 late 132–146, 0060 249 245–253). Write `docs/experiments/feedback-v1/review-release-vs-wp1c.json` and run `python docs/experiments/feedback-v1/review_page.py docs/experiments/feedback-v1/review-release-vs-wp1c.json`. Give the user the page path; expected verdict "identical".

- [ ] **Step 5: Docs.** README: describe refinement (default on, `--no-refine`, runtime cost, horizon-lock note alongside drift rebase, 0.19 s ramp). CLAUDE.md top status: v0.1.3-candidate with refinement; SESSION_HANDOFF: production promotion recorded; results.md: production section with Task 5/8 numbers; development.md: `o4core/src/refine/` layout and the parity test command.

- [ ] **Step 6: Final checks** — `cargo test --offline --workspace`, `cargo clippy --release --offline --workspace -- -D warnings`, `cargo fmt --all --check`, `node tools/test_queue_ui.cjs`; confirm `target/release/o4fix-app.exe` and `o4fix.exe` timestamps are newer than the last core change. Release/tag only on user request.
