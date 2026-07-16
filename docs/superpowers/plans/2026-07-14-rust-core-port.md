# o4fix Rust Core Port + CLI Parity — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port the o4fix MP4-repair pipeline (o4fix.py default mode + mp4patch.py) to a Rust library + CLI that reproduces the Python output on the test clip, verified stage-by-stage against golden dumps.

**Architecture:** Cargo workspace `rust/` with `o4core` (pure library: telemetry, quat math, DSP, detection, optical flow, splice, MP4 patching, pipeline orchestration) and `o4fix-cli` (thin clap wrapper). Python code at repo root is the **read-only golden reference**; a Python dump script produces per-stage npz goldens the Rust tests compare against. Spec: `docs/superpowers/specs/2026-07-14-rust-refactor-design.md`.

**Tech Stack:** Rust 2021; crates: `telemetry-parser` (git-pinned), `opencv` (0.9x, prebuilt Windows OpenCV), `nalgebra` (3×3 SVD only), `num-complex` (filter design), `thiserror`, `rayon`, `clap` (CLI), `sha2` (gate test); dev: `serde_json`, `ndarray` + `ndarray-npy` (reading golden .npz).

## Global Constraints

- All math in f64; f32 only at the MP4 byte boundary (`mp4.rs` read/write).
- Python reference files (`o4fix.py`, `mp4patch.py`, `prep_inject.py`, `analysis/`) are **read-only**. Never edit them.
- Config defaults exactly per spec §8: severe 8.0, severe_pad 0.2, severe_merge 0.2, ramp 0.3, light_cutoff 25.0, strong_cutoff 2.5, noise_low 1.5, noise_high 5.0, noise_band (30,180), noise_window 100 ms, hampel_window 7, hampel_sigma 6.0, optical_cutoff 8.0, handback_cutoff None, fast_handback (100,250), patch_pad 0.5, patch_merge 1.0, optical_noise None, fast_wide_cutoff 0.0 (M4 profile = 16.0), fast_wide_ramp (150,300), fast_wide_accel 1500.0, anchor_mode false, anchor_cutoff 1.5.
- Golden tolerances in tasks are contracts. **Never loosen a tolerance without stopping and getting user signoff** (Task 11 documents the one sanctioned fallback path).
- Test clip: `sample_vids/DJI_20260711124046_0021_D.MP4` (176.4 s, 1440x1080@100fps). Python reference output: `sample_vids/DJI_20260711124046_0021_D_fixed.MP4`. Goldens land in `goldens/` (gitignored). Committed fixtures land in `rust/o4core/tests/fixtures/`.
- Golden/clip-dependent Rust tests are `#[ignore]`; run with `cargo test -p o4core -- --ignored --nocapture` from `rust/`. Fixture unit tests run un-ignored.
- Shell is Windows PowerShell unless a step says otherwise. Run cargo commands from `rust/`.
- RNG seed scheme (both languages): `setRNGSeed(1_000_000 + global_frame_index)` immediately before each essential-matrix estimation. Rust seeds always (deterministic production output); Python only in the golden dump script.
- Rust code blocks in this plan are reference implementations: the semantics and signatures are the contract; mechanical compile fixes against pinned crate APIs (especially `opencv` and `telemetry-parser`) are expected and fine. The tests are the arbiter.
- Commits: `<type>: <subject>` plus trailer line `Claude-Session: https://claude.ai/code/session_01Y4QB81pia8MGdXyUZKkdQU`.

## File Map

| file | responsibility |
|---|---|
| `tools/gen_fixtures.py` | generate committed JSON unit fixtures from numpy/scipy/o4fix |
| `tools/dump_goldens.py` | run Python pipeline once (seeded), dump per-stage npz goldens |
| `rust/Cargo.toml` | workspace |
| `rust/o4core/src/lib.rs` | module declarations |
| `rust/o4core/src/error.rs` | `O4Error` |
| `rust/o4core/src/config.rs` | `Config` + defaults + M4 profile |
| `rust/o4core/src/quat.rs` | quaternion ops, `quats_to_rates` |
| `rust/o4core/src/dsp.rs` | butter/lfilter/lfilter_zi/filtfilt, median/uniform/hampel/interp/gradient/searchsorted |
| `rust/o4core/src/telemetry.rs` | `extract_quats`, `flat_quat_stream`, `Meta` |
| `rust/o4core/src/detect.rs` | `adaptive_clean`, `find_intervals` |
| `rust/o4core/src/optical.rs` | `video_rates`, `pair_rotation`, `fit_video_alignment` |
| `rust/o4core/src/patch.rs` | `optical_patch`, `splice_orientation` |
| `rust/o4core/src/mp4.rs` | box walk, protobuf scan, aligned slots, `patch_video`, gates |
| `rust/o4core/src/pipeline.rs` | `process()`, `Progress`, `Outcome`, cancellation |
| `rust/o4fix-cli/src/main.rs` + `args.rs` | clap CLI mirroring o4fix.py MP4 mode |

Shared list types used throughout `o4core`: timestamps `Vec<f64>` (seconds unless named `_ms`), rate series `Vec<[f64;3]>` (rad/s unless named `_deg`), quats `Vec<[f64;4]>` (wxyz, Hamilton), intervals `Vec<(f64,f64)>` (seconds).

---

### Task 1: Repo bootstrap — commit the Python reference, wire the remote

**Files:** none created; commits existing `CLAUDE.md`, `o4fix.py`, `mp4patch.py`, `prep_inject.py`, `analysis/*.py`, spec + this plan.

**Interfaces:** Produces: git remote `origin` = `https://github.com/ThaumielSparrow/o4fix`, branch `main` pushed.

- [ ] **Step 1: Commit the Python reference implementation**

```powershell
git add CLAUDE.md o4fix.py mp4patch.py prep_inject.py analysis docs
git commit -m @'
chore: add Python reference implementation and analysis harness

Claude-Session: https://claude.ai/code/session_01Y4QB81pia8MGdXyUZKkdQU
'@
```

- [ ] **Step 2: Check remote state and reconcile**

```powershell
gh repo view ThaumielSparrow/o4fix --json defaultBranchRef,isEmpty
git remote add origin https://github.com/ThaumielSparrow/o4fix.git
git fetch origin
```

If `isEmpty` is true (no fetch results): nothing to reconcile. If the repo has an initial commit (README/license): `git merge --allow-unrelated-histories origin/main -m "chore: merge GitHub-initialized history"` (keep both; resolve any README conflict by keeping ours plus theirs appended).

- [ ] **Step 3: Push**

```powershell
git push -u origin main
```

Expected: branch visible on GitHub with CLAUDE.md, Python files, docs.

---

### Task 2: Cargo workspace + error/config skeleton

**Files:**
- Create: `rust/Cargo.toml`, `rust/o4core/Cargo.toml`, `rust/o4core/src/lib.rs`, `rust/o4core/src/error.rs`, `rust/o4core/src/config.rs`, `rust/o4fix-cli/Cargo.toml`, `rust/o4fix-cli/src/main.rs`, `rust/README.md`
- Modify: `.gitignore` (add `rust/target/` if not already present — it is; verify)

**Interfaces:**
- Produces: `o4core::error::O4Error`, `o4core::config::Config` (+ `Config::m4()`), used by every later task.

- [ ] **Step 1: Write workspace + crate manifests**

`rust/Cargo.toml`:
```toml
[workspace]
members = ["o4core", "o4fix-cli"]
resolver = "2"

[profile.release]
lto = "thin"
```

`rust/o4core/Cargo.toml`:
```toml
[package]
name = "o4core"
version = "0.1.0"
edition = "2021"

[dependencies]
thiserror = "1"
num-complex = "0.4"
nalgebra = "0.33"
rayon = "1"

[dev-dependencies]
serde_json = "1"
ndarray = "0.16"
ndarray-npy = { version = "0.9", features = ["npz"] }
sha2 = "0.10"
```

`rust/o4fix-cli/Cargo.toml`:
```toml
[package]
name = "o4fix-cli"
version = "0.1.0"
edition = "2021"

[dependencies]
o4core = { path = "../o4core" }
clap = { version = "4", features = ["derive"] }
```

- [ ] **Step 2: Write error.rs, config.rs, lib.rs, CLI stub**

`rust/o4core/src/lib.rs`:
```rust
pub mod config;
pub mod detect;
pub mod dsp;
pub mod error;
pub mod mp4;
pub mod patch;
pub mod pipeline;
pub mod quat;
pub mod telemetry;
// mod optical is added in Task 11 (requires local OpenCV install)
```
(Comment out `detect`/`mp4`/`patch`/`pipeline`/`quat`/`telemetry` declarations until their tasks create the files — each task uncomments its own line. Only `config` and `error` exist after this task.)

`rust/o4core/src/error.rs`:
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum O4Error {
    #[error("No DJI O4 telemetry found ({0}) — is this an O4 Pro recording?")]
    NoTelemetry(String),
    #[error("Couldn't calibrate motion from this clip (needs some clean flight sections){}",
            .r2.map(|r| format!(" — alignment R2={r:.3} < 0.8")).unwrap_or_default())]
    CalibrationFailed { r2: Option<f64> },
    #[error("round-trip verification failed; output deleted")]
    VerifyFailed,
    #[error("cancelled")]
    Cancelled,
    #[error("MP4 structure error: {0}")]
    Mp4(String),
    #[error("telemetry parse error: {0}")]
    Telemetry(String),
    #[error("OpenCV error: {0}")]
    Cv(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
```

`rust/o4core/src/config.rs`:
```rust
/// Tuning parameters. Defaults are the tuned M2 profile (spec §8).
#[derive(Clone, Debug)]
pub struct Config {
    pub severe: f64,
    pub severe_pad: f64,
    pub severe_merge: f64,
    pub ramp: f64,
    pub light_cutoff: f64,
    pub strong_cutoff: f64,
    pub noise_low: f64,
    pub noise_high: f64,
    pub noise_band: (f64, f64),
    pub noise_window_ms: f64,
    pub hampel_window: usize,
    pub hampel_sigma: f64,
    pub optical_cutoff: f64,
    pub handback_cutoff: Option<f64>,
    pub fast_handback: (f64, f64),
    pub patch_pad: f64,
    pub patch_merge: f64,
    pub optical_noise: Option<(f64, f64)>,
    pub fast_wide_cutoff: f64,
    pub fast_wide_ramp: (f64, f64),
    pub fast_wide_accel: f64,
    pub anchor_mode: bool,
    pub anchor_cutoff: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            severe: 8.0, severe_pad: 0.2, severe_merge: 0.2, ramp: 0.3,
            light_cutoff: 25.0, strong_cutoff: 2.5,
            noise_low: 1.5, noise_high: 5.0, noise_band: (30.0, 180.0),
            noise_window_ms: 100.0, hampel_window: 7, hampel_sigma: 6.0,
            optical_cutoff: 8.0, handback_cutoff: None,
            fast_handback: (100.0, 250.0), patch_pad: 0.5, patch_merge: 1.0,
            optical_noise: None, fast_wide_cutoff: 0.0,
            fast_wide_ramp: (150.0, 300.0), fast_wide_accel: 1500.0,
            anchor_mode: false, anchor_cutoff: 1.5,
        }
    }
}

impl Config {
    /// M4 "sharp-turn" profile: wider fast-motion handback, accel gate on.
    pub fn m4() -> Self {
        Self { fast_wide_cutoff: 16.0, ..Self::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_match_spec() {
        let c = Config::default();
        assert_eq!(c.severe, 8.0);
        assert_eq!(c.noise_band, (30.0, 180.0));
        assert_eq!(c.fast_wide_cutoff, 0.0);
        assert_eq!(Config::m4().fast_wide_cutoff, 16.0);
        assert!(c.handback_cutoff.is_none() && c.optical_noise.is_none());
    }
}
```

`rust/o4fix-cli/src/main.rs` (stub, replaced in Task 16):
```rust
fn main() { println!("o4fix-cli scaffold"); }
```

`rust/README.md`: sections "Layout" (crate map from File Map above), "Dev commands" (`cargo build`, `cargo test -p o4core`, `cargo test -p o4core -- --ignored` for clip tests), "OpenCV setup" (placeholder line: "filled in by Task 11"), "telemetry-parser pin" (filled by Task 3).

- [ ] **Step 3: Build + test**

Run: `cargo build && cargo test -p o4core` (from `rust/`)
Expected: builds; `defaults_match_spec` passes.

- [ ] **Step 4: Commit**

```powershell
git add rust; git commit -m @'
feat: cargo workspace with o4core error/config skeleton

Claude-Session: https://claude.ai/code/session_01Y4QB81pia8MGdXyUZKkdQU
'@
```

---

### Task 3: Spike — telemetry-parser crate flat quat stream matches Python

**Files:**
- Create: `rust/o4core/examples/dump_quats.rs`, `tools/dump_quats_py.py`
- Modify: `rust/o4core/Cargo.toml` (add dep), `rust/README.md` (record pin + API notes)

**Interfaces:**
- Produces: pinned `telemetry-parser` dependency; documented crate API for the quaternion stream (consumed by Task 7's `telemetry.rs`). CSV format: `t_ms,w,x,y,z` one row per flat-stream slot, full `{:.17e}` precision.

- [ ] **Step 1: Add the dependency**

```powershell
cd rust; cargo add telemetry-parser --package o4core --git https://github.com/AdrianEddy/telemetry-parser
```
Then copy the resolved `rev` from `Cargo.lock` into `o4core/Cargo.toml` as an explicit `rev = "..."` pin. Also run `pip show telemetry-parser` and record both versions in `rust/README.md`.

- [ ] **Step 2: Write the Rust dump example**

`rust/o4core/examples/dump_quats.rs` — expected API shape (adjust to the pinned rev; the DJI parser emits quats under GroupId::Quaternion / TagId::Data as `Vec<TimeQuaternion<f64>>` with `t` in ms):
```rust
use std::fs::File;
use telemetry_parser::{Input, tags_impl::*};

fn main() {
    let path = std::env::args().nth(1).expect("usage: dump_quats VIDEO.MP4");
    let mut f = File::open(&path).unwrap();
    let size = f.metadata().unwrap().len() as usize;
    let input = Input::from_stream(&mut f, size, &path, |_| (), std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false))).unwrap();
    for sample in input.samples.as_ref().unwrap() {
        let Some(map) = sample.tag_map.as_ref() else { continue };
        let Some(grp) = map.get(&GroupId::Quaternion) else { continue };
        if let Some(TagValue::Vec_TimeQuaternion_f64(arr)) = grp.get_t(TagId::Data) {
            for q in arr.get() {
                println!("{:.17e},{:.17e},{:.17e},{:.17e},{:.17e}",
                         q.t, q.v.w, q.v.x, q.v.y, q.v.z);
            }
        }
    }
}
```

- [ ] **Step 3: Write the Python twin**

`tools/dump_quats_py.py`:
```python
#!/usr/bin/env python3
"""Print telemetry-parser's flat quat stream as CSV: t_ms,w,x,y,z."""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from mp4patch import _flat_reference

ts, qs = _flat_reference(sys.argv[1])
for t, (w, x, y, z) in zip(ts, qs):
    print(f"{t:.17e},{w:.17e},{x:.17e},{y:.17e},{z:.17e}")
```

- [ ] **Step 4: Compare**

```powershell
cargo run --example dump_quats -- ..\sample_vids\DJI_20260711124046_0021_D.MP4 > ..\goldens\quats_rs.csv
python ..\tools\dump_quats_py.py ..\sample_vids\DJI_20260711124046_0021_D.MP4 > ..\goldens\quats_py.csv
python -c "import sys; a=open('../goldens/quats_rs.csv').read(); b=open('../goldens/quats_py.csv').read(); print('IDENTICAL' if a==b else 'DIFFER'); sys.exit(a!=b)"
```
Expected: `IDENTICAL`. If the crate API differs from the sketch, adapt the example until output matches, then document the real API in `rust/README.md` §telemetry-parser (Task 7 codes against those notes). If values differ (crate rev drift vs Python package), pin an older rev matching the Python package version and re-run.

- [ ] **Step 5: Commit**

```powershell
git add rust tools; git commit -m @'
feat: pin telemetry-parser crate, verified flat quat stream matches Python

Claude-Session: https://claude.ai/code/session_01Y4QB81pia8MGdXyUZKkdQU
'@
```

---

### Task 4: Fixture generator + quat.rs

**Files:**
- Create: `tools/gen_fixtures.py`, `rust/o4core/src/quat.rs`, `rust/o4core/tests/helpers.rs`, `rust/o4core/tests/quat_fixtures.rs`, `rust/o4core/tests/fixtures/*.json` (generated, committed)
- Modify: `rust/o4core/src/lib.rs` (uncomment `pub mod quat;`)

**Interfaces:**
- Produces: `quat::{qmul, qconj, qnorm, qexp, qlog, slerp, smoothstep, quats_to_rates}` (signatures in Step 5); fixtures `quat_ops.json`, `filters.json`, `kernels.json` (Tasks 5–6 consume the latter two); shared test helpers in `tests/helpers.rs`.

- [ ] **Step 1: Write `tools/gen_fixtures.py` (complete; generates ALL fixtures for Tasks 4–6)**

```python
#!/usr/bin/env python3
"""Generate JSON unit fixtures for the Rust port from numpy/scipy/o4fix.

Writes rust/o4core/tests/fixtures/*.json. Values come from the exact
reference code the port must match. Commit the outputs.
"""
import json, sys
from pathlib import Path
import numpy as np
from scipy.ndimage import median_filter, uniform_filter1d
from scipy.signal import butter, filtfilt, lfilter, lfilter_zi

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
import o4fix

OUT = ROOT / "rust/o4core/tests/fixtures"

def j(x):
    return np.asarray(x, dtype=np.float64).tolist()

def signal(n, seed):
    rng = np.random.default_rng(seed)
    t = np.arange(n) / 1000.0
    return (np.sin(2 * np.pi * 3 * t) + 0.5 * np.sin(2 * np.pi * 40 * t + 1.0)
            + 0.2 * rng.standard_normal(n))

def main():
    OUT.mkdir(parents=True, exist_ok=True)

    # ---------------- quat_ops.json (via o4fix itself)
    rng = np.random.default_rng(7)
    q = rng.standard_normal((32, 4))
    q /= np.linalg.norm(q, axis=1, keepdims=True)
    qa, qb = q[:16].copy(), q[16:].copy()
    v = 0.5 * rng.standard_normal((16, 3))
    v[0] = 0.0                              # exact small-angle limit
    v[1] = [1e-13, 0.0, 0.0]                # near-limit branch
    w = np.linspace(0.0, 1.0, 16)
    t = np.sort(rng.uniform(0.0, 1.0, 33))
    qs = o4fix.quat_exp(np.cumsum(0.01 * rng.standard_normal((33, 3)), axis=0))
    tm, om = o4fix.quats_to_rates(t, qs)
    ss_x = np.linspace(-0.5, 1.5, 21)
    json.dump({
        "qa": j(qa), "qb": j(qb),
        "mul": j(o4fix.quat_mul(qa, qb)),
        "conj": j(o4fix.quat_conj(qa)),
        "v": j(v), "exp": j(o4fix.quat_exp(v)),
        "log": j(o4fix.quat_log(qa)),
        "w": j(w), "slerp": j(o4fix.slerp(qa, qb, w)),
        "smoothstep_x": j(ss_x), "smoothstep": j(o4fix.smoothstep(ss_x)),
        "rates_t": j(t), "rates_q": j(qs), "rates_tm": j(tm), "rates_om": j(om),
    }, open(OUT / "quat_ops.json", "w"))

    # ---------------- filters.json (butter/lfilter_zi/lfilter/filtfilt)
    x = signal(256, 1)
    cases = []
    for kind, arg in [("low", [0.05]), ("low", [0.005]), ("low", [0.016]),
                      ("low", [0.032]), ("low", [0.1]),
                      ("band", [0.06, 0.36])]:
        b, a = butter(2, arg[0] if kind == "low" else arg,
                      "low" if kind == "low" else "band")
        zi = lfilter_zi(b, a)
        y, zf = lfilter(b, a, x, zi=zi * x[0])
        cases.append({"kind": kind, "arg": arg, "b": j(b), "a": j(a),
                      "zi": j(zi), "lfilter_y": j(y), "lfilter_zf": j(zf),
                      "filtfilt": j(filtfilt(b, a, x))})
    json.dump({"x": j(x), "cases": cases}, open(OUT / "filters.json", "w"))

    # ---------------- kernels.json
    x64 = signal(64, 2)
    x300 = signal(300, 3)
    uni = {str(s): j(uniform_filter1d(x300, size=s, mode="nearest"))
           for s in (3, 4, 10, 15, 100, 200)}
    med = {"15": j(median_filter(x300, size=15, mode="nearest"))}
    h_in = np.stack([signal(300, 5), signal(300, 6), signal(300, 8)], axis=1)
    h_in[50, 0] += 30.0
    h_in[200, 2] -= 25.0                    # injected spikes
    h_out, h_bad = o4fix.hampel(h_in, 7, 6.0)
    xq = np.array([-0.5, 0.0, 0.01, 0.5, 0.999, 1.0, 1.5])
    xp = np.linspace(0.0, 1.0, 32)
    fp = signal(32, 9)
    srt = np.sort(signal(32, 10))
    queries = [srt[0] - 1, srt[0], srt[10], (srt[10] + srt[11]) / 2,
               srt[-1], srt[-1] + 1]
    json.dump({
        "x64": j(x64), "x300": j(x300),
        "uniform": uni, "median": med,
        "hampel_in": j(h_in), "hampel_out": j(h_out),
        "hampel_frac": float(h_bad.mean()),
        "interp_xq": j(xq), "interp_xp": j(xp), "interp_fp": j(fp),
        "interp_y": j(np.interp(xq, xp, fp)),
        "gradient_in": j(x64), "gradient_out": j(np.gradient(x64)),
        "sorted": j(srt), "queries": j(queries),
        "ss_left": [int(np.searchsorted(srt, v, "left")) for v in queries],
        "ss_right": [int(np.searchsorted(srt, v, "right")) for v in queries],
    }, open(OUT / "kernels.json", "w"))
    print("fixtures written to", OUT)

if __name__ == "__main__":
    main()
```

- [ ] **Step 2: Generate fixtures**

Run: `python tools/gen_fixtures.py` (repo root). Expected: three JSON files in `rust/o4core/tests/fixtures/`.

- [ ] **Step 3: Write shared test helpers + failing quat test**

`rust/o4core/tests/helpers.rs`:
```rust
#![allow(dead_code)]
pub fn fx(name: &str) -> serde_json::Value {
    let p = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}
pub fn col(v: &serde_json::Value) -> Vec<f64> {
    v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect()
}
pub fn rows3(v: &serde_json::Value) -> Vec<[f64; 3]> {
    v.as_array().unwrap().iter()
        .map(|r| core::array::from_fn(|i| r[i].as_f64().unwrap())).collect()
}
pub fn rows4(v: &serde_json::Value) -> Vec<[f64; 4]> {
    v.as_array().unwrap().iter()
        .map(|r| core::array::from_fn(|i| r[i].as_f64().unwrap())).collect()
}
pub fn close(a: &[f64], b: &[f64], tol: f64, what: &str) {
    assert_eq!(a.len(), b.len(), "{what} len");
    for i in 0..a.len() {
        assert!((a[i] - b[i]).abs() <= tol * b[i].abs().max(1.0),
                "{what}[{i}]: {} vs {}", a[i], b[i]);
    }
}
pub fn assert_close<const N: usize>(a: &[[f64; N]], b: &[[f64; N]], tol: f64, what: &str) {
    assert_eq!(a.len(), b.len(), "{what} len");
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        for k in 0..N {
            assert!((x[k] - y[k]).abs() <= tol,
                    "{what}[{i}][{k}]: {} vs {}", x[k], y[k]);
        }
    }
}
```

`rust/o4core/tests/quat_fixtures.rs`:
```rust
#[path = "helpers.rs"] mod helpers;
use helpers::*;
use o4core::quat::*;

#[test]
fn quat_ops_match_python() {
    let f = fx("quat_ops.json");
    let (qa, qb) = (rows4(&f["qa"]), rows4(&f["qb"]));
    let mul: Vec<[f64; 4]> = qa.iter().zip(&qb).map(|(a, b)| qmul(*a, *b)).collect();
    assert_close(&mul, &rows4(&f["mul"]), 1e-14, "mul");
    let conj: Vec<[f64; 4]> = qa.iter().map(|a| qconj(*a)).collect();
    assert_close(&conj, &rows4(&f["conj"]), 0.0, "conj");
    let ex: Vec<[f64; 4]> = rows3(&f["v"]).iter().map(|v| qexp(*v)).collect();
    assert_close(&ex, &rows4(&f["exp"]), 1e-14, "exp");
    let lg: Vec<[f64; 3]> = qa.iter().map(|a| qlog(*a)).collect();
    assert_close(&lg, &rows3(&f["log"]), 1e-13, "log");
    let w = col(&f["w"]);
    let sl: Vec<[f64; 4]> = (0..qa.len()).map(|i| slerp(qa[i], qb[i], w[i])).collect();
    assert_close(&sl, &rows4(&f["slerp"]), 1e-13, "slerp");
    let sx = col(&f["smoothstep_x"]);
    let ss: Vec<f64> = sx.iter().map(|x| smoothstep(*x)).collect();
    close(&ss, &col(&f["smoothstep"]), 1e-15, "smoothstep");
    let (tm, om) = quats_to_rates(&col(&f["rates_t"]), &rows4(&f["rates_q"]));
    close(&tm, &col(&f["rates_tm"]), 1e-15, "rates_tm");
    assert_close(&om, &rows3(&f["rates_om"]), 1e-11, "rates");
}
```

- [ ] **Step 4: Run to verify failure**

Run: `cargo test -p o4core --test quat_fixtures`
Expected: FAIL — module `quat` unresolved.

- [ ] **Step 5: Implement `rust/o4core/src/quat.rs`**

```rust
//! Quaternion ops (wxyz, Hamilton). Ports o4fix.py:39-188.

pub fn qmul(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    let [w1, x1, y1, z1] = a;
    let [w2, x2, y2, z2] = b;
    [w1*w2 - x1*x2 - y1*y2 - z1*z2,
     w1*x2 + x1*w2 + y1*z2 - z1*y2,
     w1*y2 - x1*z2 + y1*w2 + z1*x2,
     w1*z2 + x1*y2 - y1*x2 + z1*w2]
}

pub fn qconj(a: [f64; 4]) -> [f64; 4] { [a[0], -a[1], -a[2], -a[3]] }

pub fn qnorm(q: [f64; 4]) -> [f64; 4] {
    let n = (q[0]*q[0] + q[1]*q[1] + q[2]*q[2] + q[3]*q[3]).sqrt();
    [q[0]/n, q[1]/n, q[2]/n, q[3]/n]
}

/// Rotation vector -> unit quat; sin(t/2)/t -> 0.5 small-angle (o4fix.py:97-103).
pub fn qexp(v: [f64; 3]) -> [f64; 4] {
    let theta = (v[0]*v[0] + v[1]*v[1] + v[2]*v[2]).sqrt();
    let k = if theta > 1e-12 { (theta / 2.0).sin() / theta } else { 0.5 };
    qnorm([(theta / 2.0).cos(), v[0]*k, v[1]*k, v[2]*k])
}

/// Unit quat -> rotation vector via 2*asin(|vec|) (o4fix.py:106-112).
pub fn qlog(q: [f64; 4]) -> [f64; 3] {
    let q = if q[0] < 0.0 { [-q[0], -q[1], -q[2], -q[3]] } else { q };
    let vecn = (q[1]*q[1] + q[2]*q[2] + q[3]*q[3]).sqrt();
    let theta = 2.0 * vecn.clamp(0.0, 1.0).asin();
    let k = if vecn > 1e-12 { theta / vecn } else { 2.0 };
    [q[1]*k, q[2]*k, q[3]*k]
}

/// Element-wise slerp (o4fix.py:115-129).
pub fn slerp(qa: [f64; 4], qb: [f64; 4], w: f64) -> [f64; 4] {
    let dot: f64 = (0..4).map(|i| qa[i] * qb[i]).sum();
    let (qb, dot) = if dot < 0.0 {
        ([-qb[0], -qb[1], -qb[2], -qb[3]], -dot)
    } else { (qb, dot) };
    let theta = dot.clamp(-1.0, 1.0).acos();
    let sin_t = theta.sin();
    let (wa, wb) = if sin_t < 1e-6 {
        (1.0 - w, w)
    } else {
        (((1.0 - w) * theta).sin() / sin_t.max(1e-12),
         (w * theta).sin() / sin_t.max(1e-12))
    };
    qnorm(core::array::from_fn(|i| wa * qa[i] + wb * qb[i]))
}

pub fn smoothstep(x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Body angular rate between consecutive quats (o4fix.py:176-188).
/// Returns (t_mid, omega rad/s), len N-1.
pub fn quats_to_rates(t: &[f64], q: &[[f64; 4]]) -> (Vec<f64>, Vec<[f64; 3]>) {
    let n = q.len() - 1;
    let mut tm = Vec::with_capacity(n);
    let mut om = Vec::with_capacity(n);
    for i in 0..n {
        let mut dq = qmul(qconj(q[i]), q[i + 1]);
        if dq[0] < 0.0 { for c in dq.iter_mut() { *c = -*c; } }
        let vecn = (dq[1]*dq[1] + dq[2]*dq[2] + dq[3]*dq[3]).sqrt();
        let theta = 2.0 * vecn.clamp(0.0, 1.0).asin();
        let scale = if vecn > 1e-12 { theta / vecn } else { 2.0 };
        let dt = t[i + 1] - t[i];
        let s = scale / dt.max(1e-9);
        tm.push(t[i] + dt / 2.0);
        om.push([dq[1]*s, dq[2]*s, dq[3]*s]);
    }
    (tm, om)
}
```
Uncomment `pub mod quat;` in lib.rs.

- [ ] **Step 6: Run tests, expect pass** — `cargo test -p o4core --test quat_fixtures`: PASS.

- [ ] **Step 7: Commit**

```powershell
git add tools/gen_fixtures.py rust
git commit -m @'
feat: quat module with fixture parity tests; fixture generator

Claude-Session: https://claude.ai/code/session_01Y4QB81pia8MGdXyUZKkdQU
'@
```

---

### Task 5: dsp.rs — Butterworth design, lfilter, lfilter_zi, filtfilt

**Files:**
- Create: `rust/o4core/src/dsp.rs`, `rust/o4core/tests/dsp_filters.rs`
- Modify: `rust/o4core/src/lib.rs` (uncomment `pub mod dsp;`)

**Interfaces:**
- Produces: `dsp::Ba { b: Vec<f64>, a: Vec<f64> }`, `dsp::butter_low(order, wn) -> Ba`, `dsp::butter_band(order, wn_lo, wn_hi) -> Ba` (wn normalized to Nyquist, scipy convention), `dsp::lfilter_zi(&Ba) -> Vec<f64>`, `dsp::lfilter(&Ba, &[f64], zi: &[f64]) -> (Vec<f64>, Vec<f64>)`, `dsp::filtfilt(&Ba, &[f64]) -> Vec<f64>`, `dsp::filtfilt_padlen(&Ba, &[f64], padlen) -> Vec<f64>`, `dsp::filtfilt3(&Ba, &[[f64;3]]) -> Vec<[f64;3]>` (per-column, axis 0).

- [ ] **Step 1: Write failing fixture test**

`rust/o4core/tests/dsp_filters.rs`:
```rust
#[path = "helpers.rs"] mod helpers;
use helpers::*;
use o4core::dsp::*;

#[test]
fn filters_match_scipy() {
    let f = fx("filters.json");
    let x = col(&f["x"]);
    for case in f["cases"].as_array().unwrap() {
        let arg = col(&case["arg"]);
        let ba = if case["kind"] == "low" { butter_low(2, arg[0]) }
                 else { butter_band(2, arg[0], arg[1]) };
        let what = format!("{}{:?}", case["kind"], arg);
        close(&ba.b, &col(&case["b"]), 1e-12, &format!("{what} b"));
        close(&ba.a, &col(&case["a"]), 1e-12, &format!("{what} a"));
        let zi = lfilter_zi(&ba);
        close(&zi, &col(&case["zi"]), 1e-10, &format!("{what} zi"));
        let zi0: Vec<f64> = zi.iter().map(|z| z * x[0]).collect();
        let (y, zf) = lfilter(&ba, &x, &zi0);
        close(&y, &col(&case["lfilter_y"]), 1e-10, &format!("{what} lfilter"));
        close(&zf, &col(&case["lfilter_zf"]), 1e-9, &format!("{what} zf"));
        close(&filtfilt(&ba, &x), &col(&case["filtfilt"]), 1e-9,
              &format!("{what} filtfilt"));
    }
}
```

- [ ] **Step 2: Run to verify failure** — `cargo test -p o4core --test dsp_filters`: FAIL (no `dsp`).

- [ ] **Step 3: Evaluate sci-rs (spec's first choice)**

`cargo add sci-rs --package o4core`; implement the Interfaces API as thin wrappers over sci-rs's butter/filtfilt at the pinned version; run the test. **Decision gate:** all fixture cases pass at stated tolerances → keep sci-rs, record outcome in `rust/README.md`, skip Step 4. Any failure → `cargo remove sci-rs`, do Step 4 (hand-port), record that instead. Either way the public `dsp::` API is identical for downstream tasks.

- [ ] **Step 4 (fallback): Hand-port the filter core in `rust/o4core/src/dsp.rs`**

```rust
//! scipy.signal-compatible filtering. butter via scipy's zpk+bilinear path;
//! filtfilt = odd-ext padding, default padlen = 3*max(len(a),len(b)).
use num_complex::Complex64 as C;

#[derive(Clone, Debug)]
pub struct Ba { pub b: Vec<f64>, pub a: Vec<f64> }

/// scipy buttap: p = -exp(1j*pi*arange(-N+1, N, 2)/(2N)), k=1.
fn buttap(n: usize) -> Vec<C> {
    let mut p = Vec::with_capacity(n);
    let mut m = -(n as i64) + 1;
    while m < n as i64 {
        let th = std::f64::consts::PI * m as f64 / (2.0 * n as f64);
        p.push(-C::new(th.cos(), th.sin()));
        m += 2;
    }
    p
}

/// Real coefficients of prod(x - r_i); roots come in conjugate pairs.
fn poly(roots: &[C]) -> Vec<f64> {
    let mut c = vec![C::new(1.0, 0.0)];
    for r in roots {
        c.push(C::new(0.0, 0.0));
        for i in (1..c.len()).rev() {
            let prev = c[i - 1];
            c[i] = c[i] - r * prev;
        }
    }
    c.iter().map(|z| z.re).collect()
}

/// scipy bilinear_zpk with fs=2 (its internal digital-design value).
fn bilinear_zpk(z: &[C], p: &[C], k: f64) -> (Vec<C>, Vec<C>, f64) {
    let fs2 = 4.0; // 2*fs
    let mut zd: Vec<C> = z.iter().map(|&x| (fs2 + x) / (fs2 - x)).collect();
    let pd: Vec<C> = p.iter().map(|&x| (fs2 + x) / (fs2 - x)).collect();
    let num = z.iter().fold(C::new(1.0, 0.0), |acc, &x| acc * (fs2 - x));
    let den = p.iter().fold(C::new(1.0, 0.0), |acc, &x| acc * (fs2 - x));
    let kd = k * (num / den).re;
    while zd.len() < pd.len() { zd.push(C::new(-1.0, 0.0)); }
    (zd, pd, kd)
}

pub fn butter_low(order: usize, wn: f64) -> Ba {
    let warped = 4.0 * (std::f64::consts::PI * wn / 2.0).tan();
    let p: Vec<C> = buttap(order).iter().map(|&x| x * warped).collect();
    let (zd, pd, kd) = bilinear_zpk(&[], &p, warped.powi(order as i32));
    Ba { b: poly(&zd).iter().map(|c| c * kd).collect(), a: poly(&pd) }
}

pub fn butter_band(order: usize, wn_lo: f64, wn_hi: f64) -> Ba {
    let w1 = 4.0 * (std::f64::consts::PI * wn_lo / 2.0).tan();
    let w2 = 4.0 * (std::f64::consts::PI * wn_hi / 2.0).tan();
    let (bw, wo) = (w2 - w1, (w1 * w2).sqrt());
    let mut p_bp = Vec::with_capacity(2 * order);
    for &pp in &buttap(order) {
        let pl = pp * (bw / 2.0);
        let disc = (pl * pl - C::new(wo * wo, 0.0)).sqrt();
        p_bp.push(pl + disc);
        p_bp.push(pl - disc);
    }
    let z_bp = vec![C::new(0.0, 0.0); order];
    let (zd, pd, kd) = bilinear_zpk(&z_bp, &p_bp, bw.powi(order as i32));
    Ba { b: poly(&zd).iter().map(|c| c * kd).collect(), a: poly(&pd) }
}

/// Direct-form II transposed; scipy lfilter with initial conditions.
pub fn lfilter(ba: &Ba, x: &[f64], zi: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let n = ba.b.len().max(ba.a.len());
    let g = |v: &[f64], i: usize| v.get(i).copied().unwrap_or(0.0);
    let mut z: Vec<f64> = (0..n - 1).map(|i| g(zi, i)).collect();
    let mut y = Vec::with_capacity(x.len());
    for &xi in x {
        let yi = g(&ba.b, 0) * xi + z[0];
        for i in 0..n - 2 {
            z[i] = g(&ba.b, i + 1) * xi + z[i + 1] - g(&ba.a, i + 1) * yi;
        }
        z[n - 2] = g(&ba.b, n - 1) * xi - g(&ba.a, n - 1) * yi;
        y.push(yi);
    }
    (y, z)
}

/// scipy lfilter_zi: solve (I - companion(a)^T) zi = b[1:] - a[1:]*b[0].
pub fn lfilter_zi(ba: &Ba) -> Vec<f64> {
    let n = ba.b.len().max(ba.a.len());
    let g = |v: &[f64], i: usize| v.get(i).copied().unwrap_or(0.0);
    let m = n - 1;
    // companion(a)[0][j] = -a[j+1]/a[0]; subdiagonal ones. Build I - C^T.
    let mut mat = vec![vec![0.0; m]; m];
    for i in 0..m {
        for jj in 0..m {
            let ct = if i == 0 || jj == i - 1 {
                if i == 0 { -g(&ba.a, jj + 1) / g(&ba.a, 0) }
                else { 1.0 }
            } else { 0.0 };
            // note: when i==0 && jj==i-1 is impossible; branches exclusive
            mat[i][jj] = if i == jj { 1.0 - ct } else { -ct };
        }
    }
    let mut rhs: Vec<f64> = (0..m)
        .map(|i| g(&ba.b, i + 1) - g(&ba.a, i + 1) * g(&ba.b, 0)).collect();
    // gaussian elimination with partial pivoting (m <= 4)
    for c in 0..m {
        let piv = (c..m).max_by(|&i, &jj| mat[i][c].abs().total_cmp(&mat[jj][c].abs())).unwrap();
        mat.swap(c, piv);
        rhs.swap(c, piv);
        for r in c + 1..m {
            let f = mat[r][c] / mat[c][c];
            for k in c..m {
                let v = mat[c][k];
                mat[r][k] -= f * v;
            }
            rhs[r] -= f * rhs[c];
        }
    }
    let mut zi = vec![0.0; m];
    for r in (0..m).rev() {
        let s: f64 = (r + 1..m).map(|k| mat[r][k] * zi[k]).sum();
        zi[r] = (rhs[r] - s) / mat[r][r];
    }
    zi
}

/// CAREFUL: C^T index fix — companion(a)^T[i][jj] = companion(a)[jj][i]:
/// = -a[i+1]/a[0] when jj == 0; = 1 when i == jj - 1; else 0.
/// The loop above must use THIS definition (test will catch it if not).

pub fn filtfilt(ba: &Ba, x: &[f64]) -> Vec<f64> {
    filtfilt_padlen(ba, x, 3 * ba.b.len().max(ba.a.len()))
}

/// scipy filtfilt, method="pad", padtype="odd".
pub fn filtfilt_padlen(ba: &Ba, x: &[f64], padlen: usize) -> Vec<f64> {
    let n = x.len();
    assert!(padlen < n, "padlen {padlen} >= signal len {n}");
    let mut ext = Vec::with_capacity(n + 2 * padlen);
    for i in (1..=padlen).rev() { ext.push(2.0 * x[0] - x[i]); }
    ext.extend_from_slice(x);
    for i in 1..=padlen { ext.push(2.0 * x[n - 1] - x[n - 1 - i]); }
    let zi = lfilter_zi(ba);
    let zi0: Vec<f64> = zi.iter().map(|z| z * ext[0]).collect();
    let (fwd, _) = lfilter(ba, &ext, &zi0);
    let rev: Vec<f64> = fwd.into_iter().rev().collect();
    let zi1: Vec<f64> = zi.iter().map(|z| z * rev[0]).collect();
    let (bwd, _) = lfilter(ba, &rev, &zi1);
    let out: Vec<f64> = bwd.into_iter().rev().collect();
    out[padlen..padlen + n].to_vec()
}

pub fn filtfilt3(ba: &Ba, x: &[[f64; 3]]) -> Vec<[f64; 3]> {
    let cols: Vec<Vec<f64>> = (0..3).map(|k| {
        filtfilt(ba, &x.iter().map(|r| r[k]).collect::<Vec<_>>())
    }).collect();
    (0..x.len()).map(|i| [cols[0][i], cols[1][i], cols[2][i]]).collect()
}
```
Note the comment block after `lfilter_zi`: the transpose indexing is the classic off-by-one trap here; the fixture test disambiguates. Fix the loop to the documented definition if the first run fails.

- [ ] **Step 5: Run tests, expect pass** — `cargo test -p o4core --test dsp_filters`: PASS all 6 cases. (`filtfilt_padlen` is public because `optical_patch` needs scipy's `padlen=min(nseg-1, int(2*fs))` override, o4fix.py:522-524.)

- [ ] **Step 6: Commit**

```powershell
git add rust; git commit -m @'
feat: scipy-parity Butterworth + filtfilt in dsp module

Claude-Session: https://claude.ai/code/session_01Y4QB81pia8MGdXyUZKkdQU
'@
```

---

### Task 6: dsp.rs — kernels (median/uniform/hampel/interp/gradient/searchsorted)

**Files:**
- Create: `rust/o4core/tests/dsp_kernels.rs`
- Modify: `rust/o4core/src/dsp.rs` (append)

**Interfaces:**
- Produces: `dsp::median_filter(&[f64], size) -> Vec<f64>` (odd size, mode nearest), `dsp::uniform_filter1d(&[f64], size) -> Vec<f64>` (mode nearest, scipy even-size origin), `dsp::uniform_filter3(&[[f64;3]], size) -> Vec<[f64;3]>`, `dsp::hampel(&[[f64;3]], k, nsig) -> (Vec<[f64;3]>, f64)` (cleaned, replaced-scalar fraction), `dsp::interp(xq, xp, fp) -> Vec<f64>` (np.interp clamping), `dsp::gradient(&[f64]) -> Vec<f64>` (unit spacing), `dsp::searchsorted_left/right(&[f64], f64) -> usize`.

- [ ] **Step 1: Write failing test**

`rust/o4core/tests/dsp_kernels.rs`:
```rust
#[path = "helpers.rs"] mod helpers;
use helpers::*;
use o4core::dsp::*;

#[test]
fn kernels_match_scipy() {
    let f = fx("kernels.json");
    let x300 = col(&f["x300"]);
    for (s, key) in [(3usize, "3"), (4, "4"), (10, "10"), (15, "15"),
                     (100, "100"), (200, "200")] {
        close(&uniform_filter1d(&x300, s), &col(&f["uniform"][key]), 1e-12, key);
    }
    close(&median_filter(&x300, 15), &col(&f["median"]["15"]), 0.0, "median15");
    let hin = rows3(&f["hampel_in"]);
    let (hout, frac) = hampel(&hin, 7, 6.0);
    assert_close(&hout, &rows3(&f["hampel_out"]), 1e-12, "hampel");
    assert!((frac - f["hampel_frac"].as_f64().unwrap()).abs() < 1e-15);
    close(&interp(&col(&f["interp_xq"]), &col(&f["interp_xp"]), &col(&f["interp_fp"])),
          &col(&f["interp_y"]), 1e-14, "interp");
    close(&gradient(&col(&f["gradient_in"])), &col(&f["gradient_out"]), 1e-14, "gradient");
    let srt = col(&f["sorted"]);
    for (i, q) in col(&f["queries"]).iter().enumerate() {
        assert_eq!(searchsorted_left(&srt, *q), f["ss_left"][i].as_u64().unwrap() as usize);
        assert_eq!(searchsorted_right(&srt, *q), f["ss_right"][i].as_u64().unwrap() as usize);
    }
}
```

- [ ] **Step 2: Run to verify failure** — `cargo test -p o4core --test dsp_kernels`: FAIL (missing fns).

- [ ] **Step 3: Append implementations to `rust/o4core/src/dsp.rs`**

```rust
/// scipy.ndimage.median_filter, 1-D, odd size, mode="nearest".
pub fn median_filter(x: &[f64], size: usize) -> Vec<f64> {
    assert!(size % 2 == 1);
    let n = x.len() as isize;
    let h = (size / 2) as isize;
    let mut win = vec![0.0; size];
    (0..n).map(|i| {
        for (w, j) in win.iter_mut().zip(i - h..=i + h) {
            *w = x[j.clamp(0, n - 1) as usize];
        }
        win.sort_by(f64::total_cmp);
        win[size / 2]
    }).collect()
}

/// scipy.ndimage.uniform_filter1d, mode="nearest", origin=0.
/// Even size: window [i - size/2, i + size/2 - 1] (left-heavy), per scipy.
pub fn uniform_filter1d(x: &[f64], size: usize) -> Vec<f64> {
    let n = x.len() as isize;
    let s = size as isize;
    let lo = -(s / 2);
    (0..n).map(|i| {
        let mut acc = 0.0;
        for k in 0..s { acc += x[(i + lo + k).clamp(0, n - 1) as usize]; }
        acc / size as f64
    }).collect()
}

pub fn uniform_filter3(x: &[[f64; 3]], size: usize) -> Vec<[f64; 3]> {
    let cols: Vec<Vec<f64>> = (0..3).map(|k| {
        uniform_filter1d(&x.iter().map(|r| r[k]).collect::<Vec<_>>(), size)
    }).collect();
    (0..x.len()).map(|i| [cols[0][i], cols[1][i], cols[2][i]]).collect()
}

/// o4fix.hampel (o4fix.py:193-201), per axis. Returns (cleaned, spike frac).
pub fn hampel(x: &[[f64; 3]], k: usize, nsig: f64) -> (Vec<[f64; 3]>, f64) {
    let size = 2 * k + 1;
    let mut out = x.to_vec();
    let mut bad = 0usize;
    for ax in 0..3 {
        let col: Vec<f64> = x.iter().map(|r| r[ax]).collect();
        let med = median_filter(&col, size);
        let dev: Vec<f64> = col.iter().zip(&med).map(|(a, m)| (a - m).abs()).collect();
        let sig = median_filter(&dev, size);
        for i in 0..col.len() {
            if dev[i] > nsig * (1.4826 * sig[i] + 1e-9) {
                out[i][ax] = med[i];
                bad += 1;
            }
        }
    }
    (out, bad as f64 / (x.len() * 3) as f64)
}

/// np.interp with edge clamping.
pub fn interp(xq: &[f64], xp: &[f64], fp: &[f64]) -> Vec<f64> {
    xq.iter().map(|&q| {
        if q <= xp[0] { return fp[0]; }
        if q >= xp[xp.len() - 1] { return fp[fp.len() - 1]; }
        let j = searchsorted_right(xp, q) - 1;
        let t = (q - xp[j]) / (xp[j + 1] - xp[j]);
        fp[j] + t * (fp[j + 1] - fp[j])
    }).collect()
}

/// np.gradient, unit spacing: central diffs, one-sided edges.
pub fn gradient(x: &[f64]) -> Vec<f64> {
    let n = x.len();
    (0..n).map(|i| {
        if i == 0 { x[1] - x[0] }
        else if i == n - 1 { x[n - 1] - x[n - 2] }
        else { (x[i + 1] - x[i - 1]) / 2.0 }
    }).collect()
}

pub fn searchsorted_left(a: &[f64], v: f64) -> usize { a.partition_point(|&e| e < v) }
pub fn searchsorted_right(a: &[f64], v: f64) -> usize { a.partition_point(|&e| e <= v) }
```
Parity note: scipy's `median_filter(..., axes=0)` on (N,3) equals the per-column 1-D filter; the hampel fixture proves it end-to-end.

- [ ] **Step 4: Run tests, expect pass** — `cargo test -p o4core`: all suites green.

- [ ] **Step 5: Commit**

```powershell
git add rust; git commit -m @'
feat: dsp kernels (median/uniform/hampel/interp/gradient/searchsorted)

Claude-Session: https://claude.ai/code/session_01Y4QB81pia8MGdXyUZKkdQU
'@
```

---

### Task 7: Golden dump script + telemetry.rs

**Files:**
- Create: `tools/dump_goldens.py`, `rust/o4core/src/telemetry.rs`, `rust/o4core/tests/golden_telemetry.rs`
- Modify: `rust/o4core/src/lib.rs` (uncomment `pub mod telemetry;`)

**Interfaces:**
- Consumes: telemetry-parser crate API as documented in `rust/README.md` by Task 3.
- Produces: `telemetry::Meta { camera: String, model: String, camera_matrix: Option<[[f64;3];3]>, distortion: Option<[f64;4]>, calib_w: Option<f64>, calib_h: Option<f64> }`, `telemetry::Telemetry { t: Vec<f64> /*s*/, q: Vec<[f64;4]>, meta: Meta }`, `telemetry::extract_quats(&Path) -> Result<Telemetry, O4Error>`, `telemetry::flat_quat_stream(&Path) -> Result<(Vec<f64> /*ms*/, Vec<[f64;4]>), O4Error>`; golden files `goldens/*.npz` + `goldens/meta.json` consumed by Tasks 8–15.

- [ ] **Step 1: Write `tools/dump_goldens.py` (complete)**

```python
#!/usr/bin/env python3
"""Dump per-stage goldens of the Python pipeline (M2 defaults, seeded optical)
for the Rust port's stage tests. Writes goldens/*.npz + goldens/meta.json.

The ONLY deviation from o4fix defaults: cv2.setRNGSeed(1_000_000 + frame_idx)
before each frame pair's essential-matrix estimation, so RANSAC is
reproducible cross-language. o4fix.py itself is never modified; video_rates
is monkeypatched with a seeded copy of its loop.

Usage: python tools/dump_goldens.py   (~10-20 min: optical runs twice)
"""
import argparse, json, sys
from pathlib import Path
import cv2
import numpy as np

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
import mp4patch
import o4fix

SEED_BASE = 1_000_000
VIDEO = ROOT / "sample_vids/DJI_20260711124046_0021_D.MP4"
GOLD = ROOT / "goldens"


def build_args():
    return argparse.Namespace(
        output=None, gcsv=False, severe=8.0, severe_pad=0.2, severe_merge=0.2,
        ramp=0.3, plot=False, orientation="XYZ", light_cutoff=25.0,
        strong_cutoff=2.5, noise_low=1.5, noise_high=5.0,
        noise_band=[30.0, 180.0], noise_window=100.0, hampel_window=7,
        hampel_sigma=6.0, lpf=None, no_optical=False, optical_cutoff=8.0,
        fast_handback=[100.0, 250.0], patch_pad=0.5, patch_merge=1.0,
        optical_noise=None, handback_cutoff=None, fast_wide_cutoff=0.0,
        fast_wide_ramp=[150.0, 300.0], fast_wide_accel=1500.0,
        anchor_mode=False, anchor_cutoff=1.5)


def seeded_video_rates(video_path, intervals, meta):
    """o4fix.video_rates (o4fix.py:273-318) with per-frame-pair RNG seed."""
    cap = cv2.VideoCapture(str(video_path))
    fps = cap.get(cv2.CAP_PROP_FPS)
    W = int(cap.get(cv2.CAP_PROP_FRAME_WIDTH))
    H = int(cap.get(cv2.CAP_PROP_FRAME_HEIGHT))
    fp = meta.get("fisheye_params", {})
    K = np.array(fp.get("camera_matrix",
                        [[546.4027, 0, W / 2], [0, 546.4027, H / 2], [0, 0, 1]]),
                 dtype=np.float64)
    D = np.array(fp.get("distortion_coeffs",
                        [0.1551311, 0.1371409, -0.0938614, 0.0041704]),
                 dtype=np.float64)
    cd = meta.get("calib_dimension", {"w": W, "h": H})
    K = K.copy()
    K[0] *= W / cd["w"]
    K[1] *= H / cd["h"]
    feat = dict(maxCorners=600, qualityLevel=0.01, minDistance=12, blockSize=7)
    lk = dict(winSize=(21, 21), maxLevel=3,
              criteria=(cv2.TERM_CRITERIA_EPS | cv2.TERM_CRITERIA_COUNT, 30, 0.01))
    ts, oms, qs = [], [], []
    for (a, b) in intervals:
        f0 = max(0, int(a * fps))
        f1 = int(b * fps) + 1
        cap.set(cv2.CAP_PROP_POS_FRAMES, f0)
        prev = None
        for fidx in range(f0, f1 + 1):
            ok, frame = cap.read()
            if not ok:
                break
            gray = cv2.resize(cv2.cvtColor(frame, cv2.COLOR_BGR2GRAY),
                              (W // 2, H // 2))
            if prev is not None:
                cv2.setRNGSeed(SEED_BASE + fidx)        # <-- the one change
                rvec, q = o4fix._pair_rotation(prev, gray, K, D, feat, lk)
                ts.append((fidx - 0.5) / fps)
                oms.append(rvec * fps)
                qs.append(q)
            prev = gray
    cap.release()
    return np.array(ts), np.array(oms), np.array(qs)


def main():
    GOLD.mkdir(exist_ok=True)
    args = build_args()

    t, q, meta = o4fix.extract_quats(VIDEO)
    np.savez(GOLD / "extract.npz", t=t, q=q)
    json.dump({k: meta.get(k) for k in
               ("camera", "model", "frame_readout_time", "calib_dimension",
                "fisheye_params")},
              open(GOLD / "meta.json", "w"), default=str, indent=1)

    fs = 1.0 / np.median(np.diff(t))
    tm, omega = o4fix.quats_to_rates(t, q)
    clean, diag = o4fix.adaptive_clean(omega, fs, args)
    np.savez(GOLD / "clean.npz", tm=tm, omega=omega, cleaned=clean,
             alpha=diag["alpha"], noise=diag["noise"], light=diag["light"],
             strong=diag["strong"], spike_frac=diag["spikes"].mean())

    noisy = o4fix.find_intervals(diag["alpha"] > 0.15, tm,
                                 pad_s=args.patch_pad,
                                 merge_s=args.patch_merge, min_s=0.2)
    calib_all = o4fix.find_intervals(diag["alpha"] < 0.02, tm, -0.2, 0.0, 3.0)
    motion = np.degrees(np.linalg.norm(clean, axis=1))
    scored = []
    for (a, b) in calib_all:
        m = (tm >= a) & (tm <= b)
        scored.append((motion[m].std(), a, min(b, a + 4.0)))
    scored.sort(reverse=True)
    calib = [(a, b) for _, a, b in scored[:6]]
    severe = o4fix.find_intervals(diag["noise"] > args.severe, tm,
                                  args.severe_pad, args.severe_merge, 0.2)
    np.savez(GOLD / "intervals.npz", noisy=np.array(noisy),
             calib=np.array(calib), severe=np.array(severe))

    tvc, ovc, qvc = seeded_video_rates(VIDEO, calib, meta)
    np.savez(GOLD / "optical_calib.npz", t=tvc, omega=ovc, quality=qvc)
    fit = o4fix.fit_video_alignment(tvc, ovc, qvc, tm, np.degrees(clean), fs)
    shift, N, r2 = fit
    np.savez(GOLD / "fit.npz", shift=shift, n=N, r2=r2)
    tvn, ovn, qvn = seeded_video_rates(VIDEO, noisy, meta)
    np.savez(GOLD / "optical_noisy.npz", t=tvn, omega=ovn, quality=qvn)

    o4fix.video_rates = seeded_video_rates       # monkeypatch, then full stage
    patched = o4fix.optical_patch(VIDEO, tm, clean, diag, fs, args, meta)
    np.savez(GOLD / "patched.npz", rates=patched)

    q_out, stats = o4fix.splice_orientation(t, q, patched, severe, args.ramp)
    np.savez(GOLD / "splice.npz", q_out=q_out,
             drifts=np.array([(a, b, d) for a, b, d in stats]))

    # SEEDED python-reference MP4 for the Task 15 e2e gate. The user's
    # sample_vids/..._fixed.MP4 was made with unseeded RANSAC and is NOT
    # bit-comparable; this one shares the rust seed scheme.
    ok = mp4patch.inject_and_check(str(VIDEO), q_out, str(GOLD / "ref_fixed.MP4"))
    assert ok, "python reference round-trip failed"

    so, qf, ts_ms, q_ref = mp4patch._aligned_slots(str(VIDEO))
    np.savez(GOLD / "slots.npz", offs=so, q_file=qf, ts_ms=ts_ms, q_ref=q_ref)
    print("goldens written to", GOLD)


if __name__ == "__main__":
    main()
```

- [ ] **Step 2: Run it**

Run: `python tools/dump_goldens.py` (expect 10–20 min; optical sections run twice by design — once standalone for stage goldens, once inside `optical_patch`; both are identically seeded so results match).
Expected output files: `goldens/{extract,clean,intervals,optical_calib,fit,optical_noisy,patched,splice,slots}.npz`, `goldens/meta.json`, `goldens/ref_fixed.MP4` (seeded python-reference output for Task 15's e2e gate). Sanity: printed fit line inside optical_patch shows `R2=0.99x, time offset ~-4 ms` (per CLAUDE.md).

- [ ] **Step 3: Write failing golden test**

`rust/o4core/tests/golden_telemetry.rs`:
```rust
use ndarray::{Array1, Array2};
use ndarray_npy::NpzReader;
use std::fs::File;

pub fn repo(p: &str) -> std::path::PathBuf {   // pub: later tests import via #[path]
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(p)
}
pub fn npz(name: &str) -> NpzReader<File> {
    NpzReader::new(File::open(repo(&format!("goldens/{name}"))).unwrap()).unwrap()
}

#[test]
#[ignore] // needs test clip + goldens
fn extraction_matches_python() {
    let tel = o4core::telemetry::extract_quats(&repo("sample_vids/DJI_20260711124046_0021_D.MP4")).unwrap();
    let mut z = npz("extract.npz");
    let t: Array1<f64> = z.by_name("t").unwrap();
    let q: Array2<f64> = z.by_name("q").unwrap();
    assert_eq!(tel.t.len(), t.len(), "sample count");
    for i in 0..t.len() {
        assert!((tel.t[i] - t[i]).abs() <= 1e-12, "t[{i}]");
        for k in 0..4 {
            assert!((tel.q[i][k] - q[[i, k]]).abs() <= 1e-12, "q[{i}][{k}]");
        }
    }
    assert_eq!(tel.meta.model.as_str(), "O4P");
    assert!(tel.meta.camera_matrix.is_some() || tel.meta.calib_w.is_none());
}
```

- [ ] **Step 4: Run to verify failure** — `cargo test -p o4core --test golden_telemetry -- --ignored`: FAIL (no `telemetry`).

- [ ] **Step 5: Implement `rust/o4core/src/telemetry.rs`**

```rust
//! extract_quats / flat stream via the telemetry-parser crate.
//! Ports o4fix.extract_quats (o4fix.py:55-94) and mp4patch._flat_reference.
use crate::error::O4Error;
use std::path::Path;

#[derive(Clone, Debug, Default)]
pub struct Meta {
    pub camera: String,
    pub model: String,
    pub camera_matrix: Option<[[f64; 3]; 3]>,
    pub distortion: Option<[f64; 4]>,
    pub calib_w: Option<f64>,
    pub calib_h: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct Telemetry {
    pub t: Vec<f64>,          // seconds
    pub q: Vec<[f64; 4]>,     // wxyz, sorted/deduped/continuous/normalized
    pub meta: Meta,
}

/// Emission-order stream: (t_ms, q wxyz). Exact mirror of the parser output
/// (verified byte-for-byte against Python in Task 3).
pub fn flat_quat_stream(path: &Path) -> Result<(Vec<f64>, Vec<[f64; 4]>), O4Error> {
    // Uses the crate API recorded in rust/README.md by Task 3:
    // Input::from_stream -> samples -> tag_map[GroupId::Quaternion][TagId::Data]
    // -> Vec<TimeQuaternion<f64>> { t (ms), v {w,x,y,z} }.
    // Meta extraction: GroupId::Lens / TagId::Data JSON for fisheye_params
    // { camera_matrix 3x3, distortion_coeffs [4] } and calib_dimension {w,h}.
    todo!("body per Task 3 API notes; ~40 lines")
}

pub fn read_meta(path: &Path) -> Result<Meta, O4Error> {
    todo!("camera/model + Lens JSON per Task 3 API notes")
}

pub fn extract_quats(path: &Path) -> Result<Telemetry, O4Error> {
    let (ts_ms, qs) = flat_quat_stream(path)?;
    let meta = read_meta(path)?;
    if ts_ms.is_empty() {
        return Err(O4Error::NoTelemetry(format!(
            "camera={}, model={}", meta.camera, meta.model)));
    }
    // stable argsort by t
    let mut idx: Vec<usize> = (0..ts_ms.len()).collect();
    idx.sort_by(|&a, &b| ts_ms[a].total_cmp(&ts_ms[b]));   // stable sort
    let ts: Vec<f64> = idx.iter().map(|&i| ts_ms[i]).collect();
    let qs_s: Vec<[f64; 4]> = idx.iter().map(|&i| qs[i]).collect();
    // dedupe consecutive equal quats (2 kHz stream duplicates each 1 kHz value)
    let mut t_out = vec![ts[0]];
    let mut q_out = vec![qs_s[0]];
    for i in 1..qs_s.len() {
        if qs_s[i] != qs_s[i - 1] {
            t_out.push(ts[i]);
            q_out.push(qs_s[i]);
        }
    }
    // hemisphere continuity + normalize
    let mut flip = false;
    for i in 0..q_out.len() {
        if i > 0 {
            let dot: f64 = (0..4).map(|k| q_out[i][k] * {
                // compare against the PRE-normalization previous value with
                // its flip applied, matching numpy's vectorized cumsum flips
                let mut p = q_out[i - 1];
                if false { p[0] = p[0]; }
                p[k]
            }).sum();
            if dot < 0.0 { flip = !flip; }
        }
        if flip { for c in q_out[i].iter_mut() { *c = -*c; } }
    }
    for q in q_out.iter_mut() { *q = crate::quat::qnorm(*q); }
    for t in t_out.iter_mut() { *t /= 1000.0; }
    Ok(Telemetry { t: t_out, q: q_out, meta })
}
```
**Porting caution (matches numpy exactly):** python computes flips from the *deduped, unflipped* stream (`flips = cumsum(dot(q[i],q[i-1]) < 0) % 2`), i.e., each dot product uses the ORIGINAL neighbor values, not flipped ones; then applies flips, then normalizes. Implement that ordering literally: first collect dedup'd raw quats, then compute the dot-sign array on raw neighbors, then cumulative-xor flips, then negate flagged rows, then normalize. (Rewrite the loop above accordingly — the golden test fails on any deviation. This trap is called out because session 1 died on transform-order bugs.)

- [ ] **Step 6: Fill the two `todo!` bodies** against the Task 3 API notes; run `cargo test -p o4core --test golden_telemetry -- --ignored` until `extraction_matches_python` passes (exact to 1e-12).

- [ ] **Step 7: Commit**

```powershell
git add rust tools; git commit -m @'
feat: telemetry extraction with golden parity; golden dump script

Claude-Session: https://claude.ai/code/session_01Y4QB81pia8MGdXyUZKkdQU
'@
```

---

### Task 8: detect.rs — adaptive_clean + find_intervals

**Files:**
- Create: `rust/o4core/src/detect.rs`, `rust/o4core/tests/golden_detect.rs`
- Modify: `rust/o4core/src/lib.rs` (uncomment `pub mod detect;`)

**Interfaces:**
- Consumes: `dsp::*`, `quat::quats_to_rates`, `config::Config`.
- Produces: `detect::CleanDiag { alpha: Vec<f64>, noise: Vec<f64>, light: Vec<[f64;3]> /*deg/s*/, strong: Vec<[f64;3]> /*deg/s*/, spike_frac: f64 }`, `detect::adaptive_clean(omega: &[[f64;3]] /*rad/s*/, fs: f64, cfg: &Config) -> (Vec<[f64;3]> /*rad/s*/, CleanDiag)`, `detect::find_intervals(mask: &[bool], t: &[f64], pad_s: f64, merge_s: f64, min_s: f64) -> Vec<(f64,f64)>`.

- [ ] **Step 1: Write failing golden test**

`rust/o4core/tests/golden_detect.rs`:
```rust
#[path = "golden_telemetry.rs"] mod gt; // reuse repo()/npz() helpers via pub fns
use ndarray::{Array1, Array2};
use o4core::{config::Config, detect, quat};

fn r3(a: &Array2<f64>) -> Vec<[f64; 3]> {
    (0..a.nrows()).map(|i| [a[[i, 0]], a[[i, 1]], a[[i, 2]]]).collect()
}

#[test]
#[ignore]
fn clean_stage_matches_python() {
    let tel = o4core::telemetry::extract_quats(
        &gt::repo("sample_vids/DJI_20260711124046_0021_D.MP4")).unwrap();
    let mut z = gt::npz("clean.npz");
    let tm_g: Array1<f64> = z.by_name("tm").unwrap();
    let omega_g: Array2<f64> = z.by_name("omega").unwrap();
    let cleaned_g: Array2<f64> = z.by_name("cleaned").unwrap();
    let alpha_g: Array1<f64> = z.by_name("alpha").unwrap();
    let noise_g: Array1<f64> = z.by_name("noise").unwrap();
    let light_g: Array2<f64> = z.by_name("light").unwrap();
    let strong_g: Array2<f64> = z.by_name("strong").unwrap();

    let fs = {
        let mut d: Vec<f64> = tel.t.windows(2).map(|w| w[1] - w[0]).collect();
        d.sort_by(f64::total_cmp);
        1.0 / d[d.len() / 2]  // CAREFUL: np.median averages middle two for even n
    };
    let (tm, omega) = quat::quats_to_rates(&tel.t, &tel.q);
    let cfg = Config::default();
    let (cleaned, diag) = detect::adaptive_clean(&omega, fs, &cfg);

    assert_eq!(tm.len(), tm_g.len());
    for i in 0..tm.len() {
        assert!((tm[i] - tm_g[i]).abs() <= 1e-12);
        for k in 0..3 {
            assert!((omega[i][k] - omega_g[[i, k]]).abs() <= 1e-11, "omega[{i}][{k}]");
            assert!((cleaned[i][k] - cleaned_g[[i, k]]).abs() <= 1e-9, "cleaned[{i}][{k}]");
            assert!((diag.light[i][k] - light_g[[i, k]]).abs() <= 1e-9);
            assert!((diag.strong[i][k] - strong_g[[i, k]]).abs() <= 1e-9);
        }
        assert!((diag.alpha[i] - alpha_g[i]).abs() <= 1e-9, "alpha[{i}]");
        assert!((diag.noise[i] - noise_g[i]).abs() <= 1e-9, "noise[{i}]");
    }

    // intervals: exact same values
    let mut zi = gt::npz("intervals.npz");
    let noisy_g: Array2<f64> = zi.by_name("noisy").unwrap();
    let severe_g: Array2<f64> = zi.by_name("severe").unwrap();
    let noisy = detect::find_intervals(
        &diag.alpha.iter().map(|&a| a > 0.15).collect::<Vec<_>>(),
        &tm, cfg.patch_pad, cfg.patch_merge, 0.2);
    let severe = detect::find_intervals(
        &diag.noise.iter().map(|&n| n > cfg.severe).collect::<Vec<_>>(),
        &tm, cfg.severe_pad, cfg.severe_merge, 0.2);
    assert_eq!(noisy.len(), noisy_g.nrows());
    for (i, (a, b)) in noisy.iter().enumerate() {
        assert!((a - noisy_g[[i, 0]]).abs() <= 1e-12 && (b - noisy_g[[i, 1]]).abs() <= 1e-12);
    }
    assert_eq!(severe.len(), severe_g.nrows());
    for (i, (a, b)) in severe.iter().enumerate() {
        assert!((a - severe_g[[i, 0]]).abs() <= 1e-12 && (b - severe_g[[i, 1]]).abs() <= 1e-12);
    }
}
```
(Make `repo`/`npz` in golden_telemetry.rs `pub`. The median note: `np.median` of an even-length array averages the two middle values — implement `fs` in pipeline.rs later with that exact semantic; here the test computes it the same way it will.)

- [ ] **Step 2: Run to verify failure** — FAIL (no `detect`).

- [ ] **Step 3: Implement `rust/o4core/src/detect.rs`**

```rust
//! adaptive_clean (o4fix.py:209-241) + find_intervals (o4fix.py:253-270).
use crate::config::Config;
use crate::dsp;

pub struct CleanDiag {
    pub alpha: Vec<f64>,
    pub noise: Vec<f64>,
    pub light: Vec<[f64; 3]>,    // deg/s
    pub strong: Vec<[f64; 3]>,   // deg/s
    pub spike_frac: f64,
}

const R2D: f64 = 180.0 / std::f64::consts::PI;
const D2R: f64 = std::f64::consts::PI / 180.0;

pub fn adaptive_clean(omega: &[[f64; 3]], fs: f64, cfg: &Config)
    -> (Vec<[f64; 3]>, CleanDiag)
{
    let deg: Vec<[f64; 3]> = omega.iter()
        .map(|r| [r[0] * R2D, r[1] * R2D, r[2] * R2D]).collect();
    let (x, spike_frac) = dsp::hampel(&deg, cfg.hampel_window, cfg.hampel_sigma);

    // 30-180 Hz band-RMS noise estimate, max across axes (o4fix.py:222-229)
    let ba = dsp::butter_band(2, cfg.noise_band.0 / (fs / 2.0),
                              (cfg.noise_band.1).min(0.95 * fs / 2.0) / (fs / 2.0));
    let hf = dsp::filtfilt3(&ba, &x);
    let win = ((cfg.noise_window_ms * fs / 1000.0).round() as usize).max(3);
    let mut noise = vec![0.0f64; x.len()];
    for ax in 0..3 {
        let sq: Vec<f64> = hf.iter().map(|r| r[ax] * r[ax]).collect();
        let sm = dsp::uniform_filter1d(&sq, win);
        for i in 0..noise.len() { noise[i] = noise[i].max(sm[i].sqrt()); }
    }

    let mut alpha: Vec<f64> = noise.iter()
        .map(|&n| ((n - cfg.noise_low) / (cfg.noise_high - cfg.noise_low)).clamp(0.0, 1.0))
        .collect();
    alpha = dsp::uniform_filter1d(&alpha, ((0.2 * fs) as usize).max(3));

    let light = dsp::filtfilt3(&dsp::butter_low(2, cfg.light_cutoff / (fs / 2.0)), &x);
    let strong = dsp::filtfilt3(&dsp::butter_low(2, cfg.strong_cutoff / (fs / 2.0)), &x);
    let out: Vec<[f64; 3]> = (0..x.len()).map(|i| {
        core::array::from_fn(|k| {
            (light[i][k] * (1.0 - alpha[i]) + strong[i][k] * alpha[i]) * D2R
        })
    }).collect();
    (out, CleanDiag { alpha, noise, light, strong, spike_frac })
}

/// Time intervals where mask is true, padded/merged/pruned (o4fix.py:253-270).
pub fn find_intervals(mask: &[bool], t: &[f64], pad_s: f64, merge_s: f64,
                      min_s: f64) -> Vec<(f64, f64)>
{
    let n = mask.len();
    let mut starts = Vec::new();
    let mut ends = Vec::new();
    if mask[0] { starts.push(0usize); }
    for i in 1..n {
        if mask[i] && !mask[i - 1] { starts.push(i); }
        if !mask[i] && mask[i - 1] { ends.push(i); }
    }
    if mask[n - 1] { ends.push(n); }
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for (s, e) in starts.iter().zip(&ends) {
        let a = t[*s] - pad_s;
        let b = t[(*e).min(n - 1)] + pad_s;
        if let Some(last) = merged.last_mut() {
            if a - last.1 < merge_s { last.1 = b; continue; }
        }
        merged.push((a, b));
    }
    merged.into_iter().filter(|(a, b)| b - a >= min_s).collect()
}
```
Porting notes: python's `int(0.2 * fs)` truncates — use `as usize` (truncation), NOT `.round()`; `round()` is only for `noise_window` per `int(round(...))`. Python's edge-detection uses `np.diff(mask.astype(i8))` with start indices `+1` — the loop above is the equivalent; the interval golden test proves both off-by-one candidates wrong except the right one.

- [ ] **Step 4: Run golden test, expect pass** — `cargo test -p o4core --test golden_detect -- --ignored --nocapture` (~1 min). PASS.

- [ ] **Step 5: Commit**

```powershell
git add rust; git commit -m @'
feat: adaptive_clean + find_intervals with stage-golden parity

Claude-Session: https://claude.ai/code/session_01Y4QB81pia8MGdXyUZKkdQU
'@
```

---

### Task 9: mp4.rs part 1 — box walk, sample table, protobuf scan

**Files:**
- Create: `rust/o4core/src/mp4.rs`, `rust/o4core/tests/golden_mp4.rs`
- Modify: `rust/o4core/src/lib.rs` (uncomment `pub mod mp4;`)

**Interfaces:**
- Produces: `mp4::read_meta_track_samples(&Path) -> Result<Vec<(u64, u32)>, O4Error>` (offset, size per sample), `mp4::ScanSample { offset: u64, size: u32, frame_ts: Option<u64>, atts: Vec<([Option<u64>;4], [f32;4])>, att_offset: f32 }`, `mp4::scan_file(&Path) -> Result<Vec<ScanSample>, O4Error>`.

- [ ] **Step 1: Write failing golden test**

`rust/o4core/tests/golden_mp4.rs` (first test only; Task 10 appends more):
```rust
#[path = "golden_telemetry.rs"] mod gt;
use ndarray::Array2;

#[test]
#[ignore]
fn slot_scan_matches_python() {
    let video = gt::repo("sample_vids/DJI_20260711124046_0021_D.MP4");
    let scanned = o4core::mp4::scan_file(&video).unwrap();
    // flatten to slots exactly like mp4patch._aligned_slots' pre-filter form
    let mut offs = Vec::new();
    let mut vals = Vec::new();
    for s in &scanned {
        for (o, v) in &s.atts {
            if v.iter().any(|x| x.is_nan()) { continue; }
            let q64: [f64; 4] = core::array::from_fn(|i| v[i] as f64);
            let qo = o4core::mp4::file_to_out(q64);
            if qo == [0.0; 4] { continue; }
            assert!(o.iter().all(|x| x.is_some()), "omitted field in scanned quat");
            offs.push([o[0].unwrap(), o[1].unwrap(), o[2].unwrap(), o[3].unwrap()]);
            vals.push(q64);
        }
    }
    let mut z = gt::npz("slots.npz");
    let offs_g: Array2<i64> = z.by_name("offs").unwrap();
    let qf_g: Array2<f64> = z.by_name("q_file").unwrap();
    assert_eq!(offs.len(), offs_g.nrows(), "slot count");
    for i in 0..offs.len() {
        for k in 0..4 {
            assert_eq!(offs[i][k], offs_g[[i, k]] as u64, "off[{i}][{k}]");
            assert_eq!(vals[i][k], qf_g[[i, k]], "val[{i}][{k}]"); // exact f32->f64
        }
    }
}
```
(`file_to_out` is implemented in this task since the filter rule needs it.)

- [ ] **Step 2: Run to verify failure** — FAIL (no `mp4`).

- [ ] **Step 3: Implement `rust/o4core/src/mp4.rs` (part 1)**

```rust
//! In-place O4P quat patching. Ports mp4patch.py (read-only reference).
//! Layout notes at mp4patch.py:1-17; wire format: protobuf inside 'meta'
//! handler track samples; quats = float32 LE fixed32 fields 1..4.
use crate::error::O4Error;
use crate::quat::qmul;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

pub const C_RIGHT: [f64; 4] = [0.5, -0.5, -0.5, 0.5];
pub const C_RIGHT_INV: [f64; 4] = [0.5, 0.5, 0.5, -0.5];
pub const Y180: [f64; 4] = [0.0, 0.0, 1.0, 0.0];
pub const Y180_INV: [f64; 4] = [0.0, 0.0, -1.0, 0.0];

/// Stored file quat -> telemetry-parser output frame (no sign continuity).
pub fn file_to_out(q: [f64; 4]) -> [f64; 4] { qmul(Y180, qmul(q, C_RIGHT)) }
pub fn out_to_file(q: [f64; 4]) -> [f64; 4] { qmul(Y180_INV, qmul(q, C_RIGHT_INV)) }

fn be32(b: &[u8], p: usize) -> u64 { u32::from_be_bytes(b[p..p+4].try_into().unwrap()) as u64 }
fn be64(b: &[u8], p: usize) -> u64 { u64::from_be_bytes(b[p..p+8].try_into().unwrap()) }

/// Yield (type, payload_start, payload_end) of boxes in buf[start..end].
fn walk_boxes(buf: &[u8], start: usize, end: usize) -> Vec<([u8; 4], usize, usize)> {
    let mut out = Vec::new();
    let mut pos = start;
    while pos + 8 <= end {
        let mut size = be32(buf, pos) as usize;
        let btype: [u8; 4] = buf[pos + 4..pos + 8].try_into().unwrap();
        let mut hdr = 8;
        if size == 1 { size = be64(buf, pos + 8) as usize; hdr = 16; }
        else if size == 0 { size = end - pos; }
        if size < hdr { break; }
        out.push((btype, pos + hdr, (pos + size).min(end)));
        pos += size;
    }
    out
}

fn find_box(buf: &[u8], start: usize, end: usize, path: &[&[u8; 4]])
    -> Option<(usize, usize)>
{
    if path.is_empty() { return Some((start, end)); }
    for (t, ps, pe) in walk_boxes(buf, start, end) {
        if &t == path[0] { return find_box(buf, ps, pe, &path[1..]); }
    }
    None
}

/// (abs_offset, size) of every sample in the 'meta'-handler track.
/// Ports mp4patch.read_meta_track_samples (mp4patch.py:81-172).
pub fn read_meta_track_samples(path: &Path) -> Result<Vec<(u64, u32)>, O4Error> {
    let mut f = std::fs::File::open(path)?;
    let flen = f.metadata()?.len();
    // find + load moov
    let mut pos = 0u64;
    let mut moov: Option<Vec<u8>> = None;
    let mut moov_payload = (0usize, 0usize);
    let mut hdr = [0u8; 16];
    while pos + 8 <= flen {
        f.seek(SeekFrom::Start(pos))?;
        f.read_exact(&mut hdr[..8.min((flen - pos) as usize)]).ok();
        let mut size = u32::from_be_bytes(hdr[0..4].try_into().unwrap()) as u64;
        let btype = &hdr[4..8];
        let mut hsz = 8usize;
        if size == 1 {
            f.read_exact(&mut hdr[8..16])?;
            size = u64::from_be_bytes(hdr[8..16].try_into().unwrap());
            hsz = 16;
        } else if size == 0 { size = flen - pos; }
        if btype == b"moov" {
            f.seek(SeekFrom::Start(pos))?;
            let mut buf = vec![0u8; size as usize];
            f.read_exact(&mut buf)?;
            moov_payload = (hsz, size as usize);
            moov = Some(buf);
            break;
        }
        pos += size;
    }
    let moov = moov.ok_or_else(|| O4Error::Mp4("no moov box".into()))?;
    let (ps, pe) = moov_payload;

    for (t, tps, tpe) in walk_boxes(&moov, ps, pe) {
        if &t != b"trak" { continue; }
        let Some(mdia) = find_box(&moov, tps, tpe, &[b"mdia"]) else { continue };
        let Some(hdlr) = find_box(&moov, mdia.0, mdia.1, &[b"hdlr"]) else { continue };
        if &moov[hdlr.0 + 8..hdlr.0 + 12] != b"meta" { continue; }
        let stbl = find_box(&moov, mdia.0, mdia.1, &[b"minf", b"stbl"])
            .ok_or_else(|| O4Error::Mp4("no stbl".into()))?;
        let mut boxes = std::collections::HashMap::new();
        for (bt, bps, bpe) in walk_boxes(&moov, stbl.0, stbl.1) {
            boxes.insert(bt, (bps, bpe));
        }
        // stsz
        let (sps, _) = boxes[b"stsz"];
        let sample_size = be32(&moov, sps + 4) as u32;
        let count = be32(&moov, sps + 8) as usize;
        let sizes: Vec<u32> = if sample_size != 0 {
            vec![sample_size; count]
        } else {
            (0..count).map(|i| be32(&moov, sps + 12 + 4 * i) as u32).collect()
        };
        // stco / co64
        let chunk_offsets: Vec<u64> = if let Some(&(cps, _)) = boxes.get(b"stco") {
            let n = be32(&moov, cps + 4) as usize;
            (0..n).map(|i| be32(&moov, cps + 8 + 4 * i)).collect()
        } else {
            let (cps, _) = boxes[b"co64"];
            let n = be32(&moov, cps + 4) as usize;
            (0..n).map(|i| be64(&moov, cps + 8 + 8 * i)).collect()
        };
        // stsc
        let (cps, _) = boxes[b"stsc"];
        let n = be32(&moov, cps + 4) as usize;
        let stsc: Vec<(u64, u64)> = (0..n)
            .map(|i| (be32(&moov, cps + 8 + 12 * i), be32(&moov, cps + 12 + 12 * i)))
            .collect();
        let mut samples = Vec::with_capacity(count);
        let mut si = 0usize;
        for (i, &(first_chunk, per_chunk)) in stsc.iter().enumerate() {
            let last_chunk = if i + 1 < stsc.len() { stsc[i + 1].0 - 1 }
                             else { chunk_offsets.len() as u64 };
            for c in (first_chunk - 1)..last_chunk {
                let mut off = chunk_offsets[c as usize];
                for _ in 0..per_chunk {
                    if si >= count { break; }
                    samples.push((off, sizes[si]));
                    off += sizes[si] as u64;
                    si += 1;
                }
            }
        }
        if samples.len() != count {
            return Err(O4Error::Mp4(format!("stsc walk mismatch: {} != {count}", samples.len())));
        }
        return Ok(samples);
    }
    Err(O4Error::Mp4("no 'meta' handler track".into()))
}

// ---------------- protobuf field scan (wire types 0/1/2/5 only)

fn read_varint(buf: &[u8], mut pos: usize) -> (u64, usize) {
    let (mut val, mut shift) = (0u64, 0u32);
    loop {
        let b = buf[pos];
        val |= ((b & 0x7f) as u64) << shift;
        pos += 1;
        if b & 0x80 == 0 { return (val, pos); }
        shift += 7;
    }
}

/// (field_no, wire_type, payload_start, payload_end) over buf[start..end].
fn fields(buf: &[u8], start: usize, end: usize) -> Vec<(u64, u8, usize, usize)> {
    let mut out = Vec::new();
    let mut pos = start;
    while pos < end {
        let (key, np) = read_varint(buf, pos);
        pos = np;
        let (fno, wt) = (key >> 3, (key & 7) as u8);
        match wt {
            0 => { let (_, np2) = read_varint(buf, pos); out.push((fno, 0, pos, np2)); pos = np2; }
            1 => { out.push((fno, 1, pos, pos + 8)); pos += 8; }
            2 => { let (ln, np2) = read_varint(buf, pos); out.push((fno, 2, np2, np2 + ln as usize)); pos = np2 + ln as usize; }
            5 => { out.push((fno, 5, pos, pos + 4)); pos += 4; }
            w => panic!("unsupported wire type {w} at {pos}"),
        }
    }
    out
}

fn sub(buf: &[u8], span: (usize, usize), field_no: u64) -> Option<(usize, usize)> {
    fields(buf, span.0, span.1).into_iter()
        .find(|&(f, wt, _, _)| f == field_no && wt == 2)
        .map(|(_, _, ps, pe)| (ps, pe))
}

pub struct ScanSample {
    pub offset: u64,
    pub size: u32,
    pub frame_ts: Option<u64>,
    /// (absolute file offsets of the 4 float payloads, wxyz f32 values)
    pub atts: Vec<([Option<u64>; 4], [f32; 4])>,
    pub att_offset: f32,
}

/// Ports mp4patch.scan_sample (mp4patch.py:221-257).
fn scan_sample(data: &[u8], base_off: u64) -> (Option<u64>, Vec<([Option<u64>; 4], [f32; 4])>, f32) {
    let Some(fm) = sub(data, (0, data.len()), 3) else { return (None, vec![], 0.0) };
    let mut frame_ts = None;
    if let Some(hdr) = sub(data, fm, 1) {
        for (fno, wt, ps, _) in fields(data, hdr.0, hdr.1) {
            if fno == 2 && wt == 0 { frame_ts = Some(read_varint(data, ps).0); }
        }
    }
    let Some(imu) = sub(data, fm, 3) else { return (frame_ts, vec![], 0.0) };
    let Some(fusion) = sub(data, imu, 2) else { return (frame_ts, vec![], 0.0) };
    let mut atts = Vec::new();
    let mut att_offset = 0.0f32;
    for (fno, wt, ps, pe) in fields(data, fusion.0, fusion.1) {
        if fno == 4 && wt == 5 {
            att_offset = f32::from_le_bytes(data[ps..ps + 4].try_into().unwrap());
        }
        if fno == 3 && wt == 2 {
            let mut offs = [None; 4];
            let mut vals = [0.0f32; 4];
            for (qf, qwt, qps, _) in fields(data, ps, pe) {
                if (1..=4).contains(&qf) && qwt == 5 {
                    offs[(qf - 1) as usize] = Some(base_off + qps as u64);
                    vals[(qf - 1) as usize] =
                        f32::from_le_bytes(data[qps..qps + 4].try_into().unwrap());
                }
            }
            atts.push((offs, vals));
        }
    }
    (frame_ts, atts, att_offset)
}

pub fn scan_file(path: &Path) -> Result<Vec<ScanSample>, O4Error> {
    let samples = read_meta_track_samples(path)?;
    let mut f = std::fs::File::open(path)?;
    let mut out = Vec::with_capacity(samples.len());
    for (off, size) in samples {
        f.seek(SeekFrom::Start(off))?;
        let mut data = vec![0u8; size as usize];
        f.read_exact(&mut data)?;
        let (frame_ts, atts, att_offset) = scan_sample(&data, off);
        out.push(ScanSample { offset: off, size, frame_ts, atts, att_offset });
    }
    Ok(out)
}
```

- [ ] **Step 4: Run golden test, expect pass** — `cargo test -p o4core --test golden_mp4 -- --ignored`: `slot_scan_matches_python` PASS (slot count and every offset/value exact).

- [ ] **Step 5: Commit**

```powershell
git add rust; git commit -m @'
feat: mp4 box walk + protobuf quat scan with golden slot parity

Claude-Session: https://claude.ai/code/session_01Y4QB81pia8MGdXyUZKkdQU
'@
```

---

### Task 10: mp4.rs part 2 — aligned slots, patch_video, byte-level gates

**Files:**
- Modify: `rust/o4core/src/mp4.rs` (append), `rust/o4core/tests/golden_mp4.rs` (append tests)

**Interfaces:**
- Consumes: `telemetry::flat_quat_stream`.
- Produces: `mp4::SlotTable { offs: Vec<[u64;4]>, q_file: Vec<[f64;4]>, ts_ms: Vec<f64>, q_ref: Vec<[f64;4]>, d_file: Vec<usize>, n_dedup: usize }`, `mp4::aligned_slots(&Path) -> Result<SlotTable, O4Error>`, `mp4::PatchReport { slots: usize, unchanged: usize, new_f32: Vec<[f32;4]>, ts_ms: Vec<f64> }`, `mp4::patch_video(video, out, q_target: Option<&[[f64;4]]>) -> Result<PatchReport, O4Error>`, `mp4::inject_and_check(video, out, q_target, log: &dyn Fn(&str)) -> Result<bool, O4Error>`.

- [ ] **Step 1: Append failing gate tests to `golden_mp4.rs`**

```rust
#[test]
#[ignore]
fn nullpatch_is_byte_identical() {
    use sha2::{Digest, Sha256};
    let video = gt::repo("sample_vids/DJI_20260711124046_0021_D.MP4");
    let out = std::env::temp_dir().join("o4fix_nullpatch_test.MP4");
    o4core::mp4::patch_video(&video, &out, None).unwrap();
    let h = |p: &std::path::Path| {
        let mut hasher = Sha256::new();
        let mut f = std::fs::File::open(p).unwrap();
        std::io::copy(&mut f, &mut hasher).unwrap();
        hasher.finalize()
    };
    assert_eq!(h(&video), h(&out), "NULLPATCH must be byte-identical");
    std::fs::remove_file(&out).ok();
}

#[test]
#[ignore]
fn inject_round_trip_exact() {
    let video = gt::repo("sample_vids/DJI_20260711124046_0021_D.MP4");
    let out = std::env::temp_dir().join("o4fix_inject_test.MP4");
    let st = o4core::mp4::aligned_slots(&video).unwrap();
    // deduped reference processed like extract_quats (sign continuity + norm)
    let mut q_target = o4core::mp4::deduped_reference(&st);
    // perturb rows 1000..2000 by a small fixed rotation
    let d = o4core::quat::qexp([0.001, -0.002, 0.0015]);
    for q in q_target[1000..2000].iter_mut() {
        *q = o4core::quat::qmul(*q, d);
    }
    let ok = o4core::mp4::inject_and_check(&video, &out, &q_target, &|s| println!("{s}")).unwrap();
    assert!(ok, "round-trip must return exactly the injected values");
    std::fs::remove_file(&out).ok();
}
```

- [ ] **Step 2: Run to verify failure** — FAIL (missing fns).

- [ ] **Step 3: Append to `rust/o4core/src/mp4.rs`**

```rust
use crate::quat::qnorm;

pub struct SlotTable {
    pub offs: Vec<[u64; 4]>,
    pub q_file: Vec<[f64; 4]>,
    pub ts_ms: Vec<f64>,
    pub q_ref: Vec<[f64; 4]>,
    pub d_file: Vec<usize>,
    pub n_dedup: usize,
}

/// Ports mp4patch._aligned_slots + _dedup_index (mp4patch.py:386-432).
pub fn aligned_slots(video: &Path) -> Result<SlotTable, O4Error> {
    let scanned = scan_file(video)?;
    let mut offs = Vec::new();
    let mut q_file = Vec::new();
    for s in &scanned {
        for (o, v) in &s.atts {
            if v.iter().any(|x| x.is_nan()) { continue; }
            let q64: [f64; 4] = core::array::from_fn(|i| v[i] as f64);
            if file_to_out(q64) == [0.0; 4] { continue; }
            if o.iter().any(|x| x.is_none()) {
                return Err(O4Error::Mp4("quat with omitted fields; cannot patch in place".into()));
            }
            offs.push([o[0].unwrap(), o[1].unwrap(), o[2].unwrap(), o[3].unwrap()]);
            q_file.push(q64);
        }
    }
    let (ts_ms, q_ref) = crate::telemetry::flat_quat_stream(video)?;
    if q_ref.len() != q_file.len() {
        return Err(O4Error::Mp4(format!(
            "slot count {} != parser stream {}", q_file.len(), q_ref.len())));
    }
    for i in 0..q_file.len() {
        let qo = file_to_out(q_file[i]);
        let e1: f64 = (0..4).map(|k| (qo[k] - q_ref[i][k]).abs()).fold(0.0, f64::max);
        let e2: f64 = (0..4).map(|k| (qo[k] + q_ref[i][k]).abs()).fold(0.0, f64::max);
        if e1.min(e2) > 1e-12 {
            return Err(O4Error::Mp4(format!("slot/parser alignment broken at {i}")));
        }
    }
    // dedup index: map each flat slot to its sorted+deduped 1 kHz row
    let mut order: Vec<usize> = (0..ts_ms.len()).collect();
    order.sort_by(|&a, &b| ts_ms[a].total_cmp(&ts_ms[b]));
    let mut d_sorted = vec![0usize; order.len()];
    for i in 1..order.len() {
        let changed = q_ref[order[i]] != q_ref[order[i - 1]];
        d_sorted[i] = d_sorted[i - 1] + usize::from(changed);
    }
    let mut d_file = vec![0usize; order.len()];
    for (i, &oi) in order.iter().enumerate() { d_file[oi] = d_sorted[i]; }
    let n_dedup = d_sorted[order.len() - 1] + 1;
    Ok(SlotTable { offs, q_file, ts_ms, q_ref, d_file, n_dedup })
}

/// Deduped reference stream processed exactly like o4fix.extract_quats
/// (sorted, deduped, sign-continuous, normalized) — the patching base.
pub fn deduped_reference(st: &SlotTable) -> Vec<[f64; 4]> {
    let mut order: Vec<usize> = (0..st.ts_ms.len()).collect();
    order.sort_by(|&a, &b| st.ts_ms[a].total_cmp(&st.ts_ms[b]));
    let mut qd: Vec<[f64; 4]> = Vec::with_capacity(st.n_dedup);
    for (i, &oi) in order.iter().enumerate() {
        if i == 0 || st.q_ref[oi] != st.q_ref[order[i - 1]] {
            qd.push(st.q_ref[oi]);
        }
    }
    // hemisphere continuity on RAW neighbors, then flips, then normalize
    let mut flips = vec![false; qd.len()];
    let mut cum = false;
    for i in 1..qd.len() {
        let dot: f64 = (0..4).map(|k| qd[i][k] * qd[i - 1][k]).sum();
        if dot < 0.0 { cum = !cum; }
        flips[i] = cum;
    }
    for i in 0..qd.len() {
        if flips[i] { for c in qd[i].iter_mut() { *c = -*c; } }
        qd[i] = qnorm(qd[i]);
    }
    qd
}

pub struct PatchReport {
    pub slots: usize,
    pub unchanged: usize,
    pub new_f32: Vec<[f32; 4]>,
    pub ts_ms: Vec<f64>,
}

/// Ports mp4patch.patch_video (mp4patch.py:435-503). q_target rows are in
/// telemetry-parser output frame, one per deduped 1 kHz sample; None = null
/// patch (must produce a byte-identical file).
pub fn patch_video(video: &Path, out: &Path, q_target: Option<&[[f64; 4]]>)
    -> Result<PatchReport, O4Error>
{
    let st = aligned_slots(video)?;
    let mut unchanged_count = st.n_dedup;
    let new_file: Vec<[f64; 4]> = match q_target {
        None => st.q_file.clone(),
        Some(qt) => {
            if qt.len() != st.n_dedup {
                return Err(O4Error::Mp4(format!(
                    "target has {} rows, file has {} deduped samples", qt.len(), st.n_dedup)));
            }
            let qd_ref = deduped_reference(&st);
            let unchanged: Vec<bool> = (0..st.n_dedup)
                .map(|d| qt[d] == qd_ref[d]).collect();
            unchanged_count = unchanged.iter().filter(|&&u| u).count();
            // original file value per deduped row: first slot occurrence wins
            let mut qf_orig = vec![[0.0f64; 4]; st.n_dedup];
            for i in (0..st.q_file.len()).rev() { qf_orig[st.d_file[i]] = st.q_file[i]; }
            let mut merged = vec![[0.0f64; 4]; st.n_dedup];
            for d in 0..st.n_dedup {
                merged[d] = if unchanged[d] { qf_orig[d] }
                            else { out_to_file(qnorm(qt[d])) };
            }
            // per-row sign pinned to previous WRITTEN value (mp4patch.py:478-486)
            let mut prev = merged[0];
            for d in 1..st.n_dedup {
                if unchanged[d] { prev = merged[d]; continue; }
                let dot: f64 = (0..4).map(|k| merged[d][k] * prev[k]).sum();
                if dot < 0.0 { for c in merged[d].iter_mut() { *c = -*c; } }
                prev = merged[d];
            }
            st.d_file.iter().map(|&d| merged[d]).collect()
        }
    };
    let new_f32: Vec<[f32; 4]> = new_file.iter()
        .map(|q| core::array::from_fn(|k| q[k] as f32)).collect();
    for q in &new_f32 {
        if q.iter().any(|x| x.is_nan()) { return Err(O4Error::Mp4("NaN in injected values".into())); }
        if *q == [0.0f32; 4] { return Err(O4Error::Mp4("all-zero quat in injected values".into())); }
    }
    std::fs::copy(video, out)?;
    let f = std::fs::OpenOptions::new().read(true).write(true).open(out)?;
    let mut w = std::io::BufWriter::new(f);
    use std::io::Write;
    // NOTE: slots are written in file order; unchanged rows re-write their
    // original f32 bytes (f64 came from f32, so the cast is lossless).
    let mut ordered: Vec<(u64, f32)> = Vec::with_capacity(st.offs.len() * 4);
    for (o4, v4) in st.offs.iter().zip(&new_f32) {
        for k in 0..4 { ordered.push((o4[k], v4[k])); }
    }
    ordered.sort_by_key(|&(o, _)| o);
    for (o, v) in ordered {
        w.get_ref().seek(SeekFrom::Start(o))?;
        w.get_mut().write_all(&v.to_le_bytes())?;
    }
    w.flush()?;
    Ok(PatchReport { slots: st.offs.len(), unchanged: unchanged_count,
                     new_f32, ts_ms: st.ts_ms })
}

/// Ports mp4patch.inject_and_check (mp4patch.py:515-534): the shipping gate.
pub fn inject_and_check(video: &Path, out: &Path, q_target: &[[f64; 4]],
                        log: &dyn Fn(&str)) -> Result<bool, O4Error>
{
    let rep = patch_video(video, out, Some(q_target))?;
    log(&format!("  {}/{} samples unchanged (original bytes kept)",
                 rep.unchanged, q_target.len()));
    let (ts2, q2) = crate::telemetry::flat_quat_stream(out)?;
    if ts2.len() != rep.new_f32.len() {
        log("ROUND-TRIP FAILED: slot count changed");
        return Ok(false);
    }
    let mut dt_max = 0.0f64;
    let mut err_max = 0.0f64;
    for i in 0..ts2.len() {
        dt_max = dt_max.max((ts2[i] - rep.ts_ms[i]).abs());
        let qe = file_to_out(core::array::from_fn(|k| rep.new_f32[i][k] as f64));
        let e1: f64 = (0..4).map(|k| (qe[k] - q2[i][k]).abs()).fold(0.0, f64::max);
        let e2: f64 = (0..4).map(|k| (qe[k] + q2[i][k]).abs()).fold(0.0, f64::max);
        err_max = err_max.max(e1.min(e2));
    }
    log(&format!("timestamps: max diff {dt_max} ms; values: max diff {err_max} (sign-folded)"));
    Ok(dt_max == 0.0 && err_max == 0.0)
}
```
Porting caution: buffered writer + seek is fragile — if the BufWriter/seek interaction misbehaves, drop to plain `File` writes (this is exactly what the nullpatch gate exists to catch). The `ordered.sort_by_key` write order is an optimization over Python's as-encountered order; byte result is identical.

- [ ] **Step 4: Run gates, expect pass**

Run: `cargo test -p o4core --test golden_mp4 -- --ignored --nocapture`
Expected: all three tests PASS. `nullpatch_is_byte_identical` failing means a scan/offset bug; `inject_round_trip_exact` failing means transform/sign-pinning bug. Do not proceed past this task until both gates are green (CLAUDE.md: gates must keep passing).

- [ ] **Step 5: Commit**

```powershell
git add rust; git commit -m @'
feat: in-place quat patching with nullpatch + inject round-trip gates

Claude-Session: https://claude.ai/code/session_01Y4QB81pia8MGdXyUZKkdQU
'@
```

---

### Task 11: OpenCV setup + optical.rs — video_rates / pair_rotation

**Files:**
- Create: `rust/o4core/src/optical.rs`, `rust/o4core/tests/golden_optical.rs`
- Modify: `rust/o4core/Cargo.toml` (add `opencv`), `rust/o4core/src/lib.rs` (add `pub mod optical;`), `rust/README.md` (OpenCV setup section)

**Interfaces:**
- Consumes: `telemetry::Meta`, `dsp::*`.
- Produces: `optical::OpticalRates { t: Vec<f64>, omega: Vec<[f64;3]> /*rad/s camera frame*/, quality: Vec<f64> }`, `optical::video_rates(video: &Path, intervals: &[(f64,f64)], meta: &Meta, cancel: &AtomicBool, on_interval: &(dyn Fn(usize, usize) + Sync)) -> Result<OpticalRates, O4Error>` (rayon over intervals, results concatenated in interval order).

- [ ] **Step 1: Environment setup (record everything in rust/README.md §OpenCV)**

```powershell
python -c "import cv2; print(cv2.__version__)"   # e.g. 4.10.0 — call it $V
winget install LLVM.LLVM                          # opencv-rust needs libclang
curl -L -o opencv.exe https://github.com/opencv/opencv/releases/download/$V/opencv-$V-windows.exe
.\opencv.exe -o"C:\" -y                           # self-extracting to C:\opencv
```
Set user env vars (then restart shell): `OPENCV_INCLUDE_PATHS=C:\opencv\build\include`, `OPENCV_LINK_PATHS=C:\opencv\build\x64\vc16\lib`, `OPENCV_LINK_LIBS=opencv_world<VER>` (e.g. `opencv_world4100`), `LIBCLANG_PATH=C:\Program Files\LLVM\bin`, and append `C:\opencv\build\x64\vc16\bin` to `PATH` (runtime DLLs incl. `opencv_videoio_ffmpeg*.dll`). If the exact Python minor has no prebuilt Windows release, install the matching `opencv-python` version into the Python env instead (`pip install opencv-python==$V.*`) and regenerate goldens (`python tools/dump_goldens.py`) — the two sides must decode identical frames.

Then: `cargo add opencv --package o4core` and `cargo build -p o4core` (first build is slow — bindgen).

- [ ] **Step 2: Write failing golden test**

`rust/o4core/tests/golden_optical.rs`:
```rust
#[path = "golden_telemetry.rs"] mod gt;
use ndarray::{Array1, Array2};
use std::sync::atomic::AtomicBool;

#[test]
#[ignore]
fn seeded_optical_rates_match_python_on_calib_intervals() {
    let video = gt::repo("sample_vids/DJI_20260711124046_0021_D.MP4");
    let tel = o4core::telemetry::extract_quats(&video).unwrap();
    let mut zi = gt::npz("intervals.npz");
    let calib_g: Array2<f64> = zi.by_name("calib").unwrap();
    let calib: Vec<(f64, f64)> = (0..calib_g.nrows())
        .map(|i| (calib_g[[i, 0]], calib_g[[i, 1]])).collect();
    let opt = o4core::optical::video_rates(
        &video, &calib, &tel.meta, &AtomicBool::new(false), &|_, _| ()).unwrap();
    let mut z = gt::npz("optical_calib.npz");
    let t_g: Array1<f64> = z.by_name("t").unwrap();
    let om_g: Array2<f64> = z.by_name("omega").unwrap();
    let q_g: Array1<f64> = z.by_name("quality").unwrap();
    assert_eq!(opt.t.len(), t_g.len(), "sample count");
    for i in 0..opt.t.len() {
        assert!((opt.t[i] - t_g[i]).abs() <= 1e-9, "t[{i}]");
        assert!((opt.quality[i] - q_g[i]).abs() <= 1e-9, "quality[{i}]");
        for k in 0..3 {
            assert!((opt.omega[i][k] - om_g[[i, k]]).abs() <= 1e-9,
                    "omega[{i}][{k}]: {} vs {}", opt.omega[i][k], om_g[[i, k]]);
        }
    }
}
```

- [ ] **Step 3: Run to verify failure** — FAIL (no `optical`).

- [ ] **Step 4: Implement `rust/o4core/src/optical.rs` (video_rates + pair_rotation)**

```rust
//! Optical rotation measurement. Ports o4fix.video_rates/_pair_rotation
//! (o4fix.py:273-349). RNG seeded per frame pair: set_rng_seed(1_000_000+fidx)
//! — OpenCV's theRNG is thread-local, so this is deterministic under rayon.
use crate::error::O4Error;
use crate::telemetry::Meta;
use opencv::core::{self, Mat, Point2f, Scalar, Size, TermCriteria, Vector, no_array};
use opencv::prelude::*;
use opencv::{calib3d, imgproc, video, videoio};
use rayon::prelude::*;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct OpticalRates {
    pub t: Vec<f64>,
    pub omega: Vec<[f64; 3]>,   // rad/s, camera frame
    pub quality: Vec<f64>,
}

impl From<opencv::Error> for O4Error {
    fn from(e: opencv::Error) -> Self { O4Error::Cv(e.to_string()) }
}

fn k_d(meta: &Meta, w: i32, h: i32) -> (Mat, Mat) {
    let km = meta.camera_matrix.unwrap_or(
        [[546.4027, 0.0, w as f64 / 2.0], [0.0, 546.4027, h as f64 / 2.0], [0.0, 0.0, 1.0]]);
    let d = meta.distortion.unwrap_or([0.1551311, 0.1371409, -0.0938614, 0.0041704]);
    let (cw, ch) = (meta.calib_w.unwrap_or(w as f64), meta.calib_h.unwrap_or(h as f64));
    let mut k = km;
    for c in 0..3 { k[0][c] *= w as f64 / cw; k[1][c] *= h as f64 / ch; }
    let k_mat = Mat::from_slice_2d(&k).unwrap();
    let d_mat = Mat::from_slice(&d).unwrap();
    (k_mat, d_mat)
}

pub fn video_rates(video_path: &Path, intervals: &[(f64, f64)], meta: &Meta,
                   cancel: &AtomicBool,
                   on_interval: &(dyn Fn(usize, usize) + Sync))
    -> Result<OpticalRates, O4Error>
{
    // probe fps/size once
    let cap = videoio::VideoCapture::from_file(video_path.to_str().unwrap(),
                                               videoio::CAP_ANY)?;
    let fps = cap.get(videoio::CAP_PROP_FPS)?;
    let w = cap.get(videoio::CAP_PROP_FRAME_WIDTH)? as i32;
    let h = cap.get(videoio::CAP_PROP_FRAME_HEIGHT)? as i32;
    drop(cap);
    let done = std::sync::atomic::AtomicUsize::new(0);
    let results: Result<Vec<_>, O4Error> = intervals.par_iter().map(|&(a, b)| {
        if cancel.load(Ordering::Relaxed) { return Err(O4Error::Cancelled); }
        let r = interval_rates(video_path, a, b, fps, w, h, meta);
        on_interval(done.fetch_add(1, Ordering::Relaxed) + 1, intervals.len());
        r
    }).collect();
    let mut out = OpticalRates { t: vec![], omega: vec![], quality: vec![] };
    for part in results? {                          // interval order preserved
        out.t.extend(part.t);
        out.omega.extend(part.omega);
        out.quality.extend(part.quality);
    }
    Ok(out)
}

fn interval_rates(video_path: &Path, a: f64, b: f64, fps: f64, w: i32, h: i32,
                  meta: &Meta) -> Result<OpticalRates, O4Error>
{
    let (k, d) = k_d(meta, w, h);
    let mut cap = videoio::VideoCapture::from_file(video_path.to_str().unwrap(),
                                                   videoio::CAP_ANY)?;
    let f0 = (a * fps).max(0.0) as i64;
    let f1 = (b * fps) as i64 + 1;
    cap.set(videoio::CAP_PROP_POS_FRAMES, f0 as f64)?;
    let mut out = OpticalRates { t: vec![], omega: vec![], quality: vec![] };
    let mut prev: Option<Mat> = None;
    let mut frame = Mat::default();
    for fidx in f0..=f1 {
        if !cap.read(&mut frame)? { break; }
        let mut gray = Mat::default();
        imgproc::cvt_color(&frame, &mut gray, imgproc::COLOR_BGR2GRAY, 0)?;
        let mut half = Mat::default();
        imgproc::resize(&gray, &mut half, Size::new(w / 2, h / 2), 0.0, 0.0,
                        imgproc::INTER_LINEAR)?;
        if let Some(p) = &prev {
            core::set_rng_seed(1_000_000 + fidx as i32)?;
            let (rvec, q) = pair_rotation(p, &half, &k, &d)?;
            out.t.push((fidx as f64 - 0.5) / fps);
            out.omega.push([rvec[0] * fps, rvec[1] * fps, rvec[2] * fps]);
            out.quality.push(q);
        }
        prev = Some(half);
    }
    Ok(out)
}

/// Ports o4fix._pair_rotation (o4fix.py:321-349).
fn pair_rotation(prev: &Mat, gray: &Mat, k: &Mat, d: &Mat)
    -> Result<([f64; 3], f64), O4Error>
{
    let mut p0 = Vector::<Point2f>::new();
    imgproc::good_features_to_track(prev, &mut p0, 600, 0.01, 12.0, &no_array(),
                                    7, false, 0.04)?;
    if p0.len() < 40 { return Ok(([0.0; 3], 0.0)); }
    let mut p1 = Vector::<Point2f>::new();
    let mut st = Vector::<u8>::new();
    let mut err = Vector::<f32>::new();
    video::calc_optical_flow_pyr_lk(
        prev, gray, &p0, &mut p1, &mut st, &mut err, Size::new(21, 21), 3,
        TermCriteria::new(core::TermCriteria_COUNT + core::TermCriteria_EPS, 30, 0.01)?,
        0, 1e-4)?;
    let good: Vec<usize> = (0..st.len()).filter(|&i| st.get(i).unwrap() == 1).collect();
    if good.len() < 40 { return Ok(([0.0; 3], 0.0)); }
    // x2 back to full res, then fisheye-undistort to normalized coords
    let mk = |src: &Vector<Point2f>| -> Vector<Point2f> {
        good.iter().map(|&i| { let p = src.get(i).unwrap(); Point2f::new(p.x * 2.0, p.y * 2.0) }).collect()
    };
    let (g0, g1) = (mk(&p0), mk(&p1));
    let mut u0 = Vector::<Point2f>::new();
    let mut u1 = Vector::<Point2f>::new();
    calib3d::fisheye_undistort_points(&g0, &mut u0, k, d, &no_array(), &no_array())?;
    calib3d::fisheye_undistort_points(&g1, &mut u1, k, d, &no_array(), &no_array())?;
    let eye = Mat::eye(3, 3, core::CV_64F)?.to_mat()?;
    let mut inliers = Mat::default();
    let e = calib3d::find_essential_mat(&u0, &u1, &eye, calib3d::RANSAC, 0.999,
                                        0.002, 1000, &mut inliers)?;
    if e.rows() != 3 || e.cols() != 3 { return Ok(([0.0; 3], 0.0)); }
    let inl: i32 = (0..inliers.rows())
        .map(|i| *inliers.at::<u8>(i).unwrap() as i32).sum();
    if inl < 60 { return Ok(([0.0; 3], 0.0)); }
    let (mut r1, mut r2, mut t) = (Mat::default(), Mat::default(), Mat::default());
    calib3d::decompose_essential_mat(&e, &mut r1, &mut r2, &mut t)?;
    let rod = |r: &Mat| -> Result<[f64; 3], O4Error> {
        let mut rv = Mat::default();
        calib3d::rodrigues(r, &mut rv, &mut no_array())?;
        Ok([*rv.at::<f64>(0)?, *rv.at::<f64>(1)?, *rv.at::<f64>(2)?])
    };
    let (v1, v2) = (rod(&r1)?, rod(&r2)?);
    let n = |v: &[f64; 3]| (v[0]*v[0] + v[1]*v[1] + v[2]*v[2]).sqrt();
    let rvec = if n(&v1) <= n(&v2) { v1 } else { v2 };
    let quality = (((inl - 60) as f64) / 150.0).min(1.0).max(0.0);
    Ok((rvec, quality))
}
```
API-fix note: exact `opencv` crate signatures (arg counts of `calc_optical_flow_pyr_lk`, `find_essential_mat` variants, `fisheye_undistort_points` module path — it may be `calib3d::fisheye_undistort_points` or under a `fisheye` submodule depending on crate version) WILL need adjustment against docs.rs for the pinned version. Semantics that must not change: maxCorners 600 / quality 0.01 / minDistance 12 / blockSize 7; LK win 21×21 / 3 levels / criteria (COUNT|EPS, 30, 0.01); RANSAC prob 0.999 / threshold 0.002; inlier gates 40/40/60; quality formula; smaller-angle rotation candidate.

- [ ] **Step 5: Run golden test**

Run: `cargo test -p o4core --test golden_optical -- --ignored --nocapture` (several minutes).
Expected: PASS with ≤1e-9 agreement. **If values differ beyond 1e-9:** (a) run the Rust test twice — if Rust differs from itself, the seeding is wrong (fix: seed inside the pair, check thread-local RNG); (b) if Rust is self-consistent but differs from Python, decode parity or RANSAC internals differ cross-language. STOP and report to the user with the observed error distribution before touching the tolerance; the sanctioned fallback (user must approve) is: exact t/quality equality + omega within 0.05 °/s RMS per axis, plus Task 12/13 switching to their documented fallback tolerances.

- [ ] **Step 6: Commit**

```powershell
git add rust; git commit -m @'
feat: seeded optical rotation measurement (video_rates/pair_rotation)

Claude-Session: https://claude.ai/code/session_01Y4QB81pia8MGdXyUZKkdQU
'@
```

---

### Task 12: optical.rs — fit_video_alignment

**Files:**
- Modify: `rust/o4core/src/optical.rs` (append), `rust/o4core/tests/golden_optical.rs` (append test)

**Interfaces:**
- Consumes: `dsp::{butter_low, filtfilt, uniform_filter1d, interp}`.
- Produces: `optical::Alignment { shift: f64, n: [[f64;3];3], r2: f64 }`, `optical::fit_video_alignment(opt: &OpticalRates, tm: &[f64], gyro_deg: &[[f64;3]], fs: f64) -> Option<Alignment>`.

- [ ] **Step 1: Append failing test**

```rust
#[test]
#[ignore]
fn alignment_fit_matches_python() {
    // rebuild inputs from goldens (independent of Task 11's runtime)
    let mut zc = gt::npz("optical_calib.npz");
    let t: Array1<f64> = zc.by_name("t").unwrap();
    let om: Array2<f64> = zc.by_name("omega").unwrap();
    let q: Array1<f64> = zc.by_name("quality").unwrap();
    let opt = o4core::optical::OpticalRates {
        t: t.to_vec(),
        omega: (0..om.nrows()).map(|i| [om[[i,0]], om[[i,1]], om[[i,2]]]).collect(),
        quality: q.to_vec(),
    };
    let mut zl = gt::npz("clean.npz");
    let tm: Array1<f64> = zl.by_name("tm").unwrap();
    let cleaned: Array2<f64> = zl.by_name("cleaned").unwrap();
    const R2D: f64 = 180.0 / std::f64::consts::PI;
    let gyro_deg: Vec<[f64; 3]> = (0..cleaned.nrows())
        .map(|i| [cleaned[[i,0]]*R2D, cleaned[[i,1]]*R2D, cleaned[[i,2]]*R2D]).collect();
    // python used fs from np.median(diff(t_extract)); recompute identically:
    let te = { let mut z = gt::npz("extract.npz"); let t: Array1<f64> = z.by_name("t").unwrap(); t };
    let mut d: Vec<f64> = te.windows(2).into_iter().map(|w| w[1] - w[0]).collect();
    d.sort_by(f64::total_cmp);
    let fs = 1.0 / ((d[d.len()/2 - 1] + d[d.len()/2]) / 2.0); // np.median, even n
    let al = o4core::optical::fit_video_alignment(&opt, &tm.to_vec(), &gyro_deg, fs).unwrap();
    let mut zf = gt::npz("fit.npz");
    let shift_g: Array1<f64> = zf.by_name("shift").unwrap();
    let n_g: Array2<f64> = zf.by_name("n").unwrap();
    let r2_g: Array1<f64> = zf.by_name("r2").unwrap();
    assert!((al.shift - shift_g[0]).abs() <= 1e-9, "shift");
    for r in 0..3 { for c in 0..3 {
        assert!((al.n[r][c] - n_g[[r, c]]).abs() <= 1e-9, "N[{r}][{c}]");
    }}
    assert!((al.r2 - r2_g[0]).abs() <= 1e-9, "r2");
}
```
(If Task 11 ended in fallback-tolerance mode: compare with |Δshift| ≤ 0.002 (one scan step), N ≤ 1e-3, r2 ≤ 0.01 instead — and only with prior user signoff per Task 11.)
Careful: np.median for even-length arrays averages the two middle elements — the `fs` recomputation above and pipeline.rs (Task 15) must both use that exact semantic. Fix the Task 8 test's `fs` line to match this (it used the odd-length shortcut; the clip has an even diff count only if sample count is odd — implement one shared helper `pipeline::median(&mut Vec<f64>)` in Task 15 and use it in tests from here on).

- [ ] **Step 2: Run to verify failure** — FAIL (missing fn).

- [ ] **Step 3: Append implementation**

```rust
use nalgebra::Matrix3;

pub struct Alignment { pub shift: f64, pub n: [[f64; 3]; 3], pub r2: f64 }

/// Ports o4fix.fit_video_alignment (o4fix.py:352-400).
pub fn fit_video_alignment(opt: &OpticalRates, tm: &[f64],
                           gyro_deg: &[[f64; 3]], fs: f64) -> Option<Alignment>
{
    use crate::dsp;
    let good: Vec<usize> = (0..opt.quality.len())
        .filter(|&i| opt.quality[i] > 0.5).collect();
    if good.len() < 200 { return None; }
    const R2D: f64 = 180.0 / std::f64::consts::PI;
    let ovd: Vec<[f64; 3]> = good.iter()
        .map(|&i| core::array::from_fn(|k| opt.omega[i][k] * R2D)).collect();
    let tv: Vec<f64> = good.iter().map(|&i| opt.t[i]).collect();

    // gyro_at: 10 ms-smoothed gyro sampled at query times (o4fix.py:363-366)
    let sm = dsp::uniform_filter3(gyro_deg, ((fs / 100.0) as usize).max(1));
    let cols: Vec<Vec<f64>> = (0..3).map(|k| sm.iter().map(|r| r[k]).collect()).collect();
    let gyro_at = |tq: &[f64]| -> Vec<[f64; 3]> {
        let per: Vec<Vec<f64>> = (0..3).map(|k| dsp::interp(tq, tm, &cols[k])).collect();
        (0..tq.len()).map(|i| [per[0][i], per[1][i], per[2][i]]).collect()
    };
    let ba = dsp::butter_low(2, 5.0 / 50.0);
    let lowf = |x: &[[f64; 3]]| dsp::filtfilt3(&ba, x);

    let procrustes = |b: &[[f64; 3]], a: &[[f64; 3]]| -> [[f64; 3]; 3] {
        let mut m = Matrix3::<f64>::zeros();   // B^T A
        for i in 0..b.len() {
            for r in 0..3 { for c in 0..3 { m[(r, c)] += b[i][r] * a[i][c]; } }
        }
        let svd = m.svd(true, true);
        let n = svd.u.unwrap() * svd.v_t.unwrap();
        core::array::from_fn(|r| core::array::from_fn(|c| n[(r, c)]))
    };
    let apply = |b: &[[f64; 3]], n: &[[f64; 3]; 3]| -> Vec<[f64; 3]> {
        b.iter().map(|row| core::array::from_fn(|c| {
            (0..3).map(|r| row[r] * n[r][c]).sum()
        })).collect()
    };
    let pearson = |x: &[f64], y: &[f64]| -> f64 {
        let n = x.len() as f64;
        let (mx, my) = (x.iter().sum::<f64>() / n, y.iter().sum::<f64>() / n);
        let mut (sxy, sxx, syy) = (0.0, 0.0, 0.0);
        for i in 0..x.len() {
            let (dx, dy) = (x[i] - mx, y[i] - my);
            sxy += dx * dy; sxx += dx * dx; syy += dy * dy;
        }
        sxy / (sxx.sqrt() * syy.sqrt())
    };

    let b_low = lowf(&ovd);
    let mut shift = 0.0f64;
    let mut n_mat = [[0.0; 3]; 3];
    for _ in 0..3 {
        let tq: Vec<f64> = tv.iter().map(|t| t + shift).collect();
        let a_low = lowf(&gyro_at(&tq));
        n_mat = procrustes(&b_low, &a_low);
        let p = apply(&b_low, &n_mat);
        let mut best = (f64::NEG_INFINITY, shift);
        for k in 0..=60 {                       // scan shift-0.06 .. +0.06 by 2 ms
            let sh = shift - 0.06 + 0.002 * k as f64;
            let tq: Vec<f64> = tv.iter().map(|t| t + sh).collect();
            let g = lowf(&gyro_at(&tq));
            let r: f64 = (0..3).map(|ax| pearson(
                &g.iter().map(|r| r[ax]).collect::<Vec<_>>(),
                &p.iter().map(|r| r[ax]).collect::<Vec<_>>(),
            )).sum::<f64>() / 3.0;
            if r > best.0 { best = (r, sh); }
        }
        shift = best.1;
    }
    let tq: Vec<f64> = tv.iter().map(|t| t + shift).collect();
    let g = lowf(&gyro_at(&tq));
    let p = apply(&b_low, &n_mat);
    let (mut ss_res, mut ss_tot) = (0.0, 0.0);
    let mean: [f64; 3] = core::array::from_fn(|k| {
        g.iter().map(|r| r[k]).sum::<f64>() / g.len() as f64
    });
    for i in 0..g.len() {
        for k in 0..3 {
            ss_res += (g[i][k] - p[i][k]).powi(2);
            ss_tot += (g[i][k] - mean[k]).powi(2);
        }
    }
    Some(Alignment { shift, n: n_mat, r2: 1.0 - ss_res / ss_tot })
}
```
Porting cautions: (1) python's `np.arange(shift-0.06, shift+0.061, 0.002)` yields 61 values → `0..=60`; (2) `B @ N` means rows·columns — `apply` above computes `sum_r b[r]*n[r][c]`, matching; (3) the r2 mean is per-column (`g.mean(0)`), as coded; (4) `int(fs/100)` truncates → `as usize`. (5) fix the `mut` tuple syntax — `let (mut sxy, mut sxx, mut syy)`.

- [ ] **Step 4: Run test, expect pass** — `cargo test -p o4core --test golden_optical -- --ignored` → both tests PASS.

- [ ] **Step 5: Commit**

```powershell
git add rust; git commit -m @'
feat: procrustes video-to-gyro alignment fit with golden parity

Claude-Session: https://claude.ai/code/session_01Y4QB81pia8MGdXyUZKkdQU
'@
```

---

### Task 13: patch.rs — optical_patch

**Files:**
- Create: `rust/o4core/src/patch.rs`, `rust/o4core/tests/golden_patch.rs`
- Modify: `rust/o4core/src/lib.rs` (uncomment `pub mod patch;`)

**Interfaces:**
- Consumes: `optical::{video_rates, fit_video_alignment, OpticalRates, Alignment}`, `detect::{CleanDiag, find_intervals}`, `dsp::*`, `config::Config`, `telemetry::Meta`.
- Produces: `patch::optical_patch(video: &Path, tm: &[f64], cleaned: &[[f64;3]], diag: &CleanDiag, fs: f64, cfg: &Config, meta: &Meta, log: &(dyn Fn(&str) + Sync), cancel: &AtomicBool) -> Result<Vec<[f64;3]>, O4Error>` — Err(CalibrationFailed) on no-calib/failed-fit/R²<0.8; per-burst low-quality skips are logged, not errors.

- [ ] **Step 1: Write failing golden test**

`rust/o4core/tests/golden_patch.rs`:
```rust
#[path = "golden_telemetry.rs"] mod gt;
use ndarray::{Array1, Array2};
use std::sync::atomic::AtomicBool;

#[test]
#[ignore]
fn patched_rates_match_python() {
    let video = gt::repo("sample_vids/DJI_20260711124046_0021_D.MP4");
    let tel = o4core::telemetry::extract_quats(&video).unwrap();
    let cfg = o4core::config::Config::default();
    let fs = o4core::pipeline_fs_placeholder(&tel.t); // replaced by pipeline::fs in Task 15
    let (tm, omega) = o4core::quat::quats_to_rates(&tel.t, &tel.q);
    let (cleaned, diag) = o4core::detect::adaptive_clean(&omega, fs, &cfg);
    let patched = o4core::patch::optical_patch(
        &video, &tm, &cleaned, &diag, fs, &cfg, &tel.meta,
        &|s: &str| println!("{s}"), &AtomicBool::new(false)).unwrap();
    let mut z = gt::npz("patched.npz");
    let rates_g: Array2<f64> = z.by_name("rates").unwrap();
    assert_eq!(patched.len(), rates_g.nrows());
    for i in 0..patched.len() {
        for k in 0..3 {
            assert!((patched[i][k] - rates_g[[i, k]]).abs() <= 1e-9,
                    "patched[{i}][{k}]: {} vs {}", patched[i][k], rates_g[[i, k]]);
        }
    }
}
```
(Add `pub fn pipeline_fs_placeholder(t: &[f64]) -> f64` to lib.rs now — np.median semantics incl. even-length averaging — and have Task 15 move it into `pipeline::fs` re-exported; keeps this test compiling before pipeline.rs exists. Fallback-tolerance mode, only with Task 11 signoff: ≤ 0.05 °/s RMS per axis over patched samples and identical per-burst skip decisions per the log lines.)

- [ ] **Step 2: Run to verify failure** — FAIL (no `patch`).

- [ ] **Step 3: Implement `rust/o4core/src/patch.rs` — optical_patch (ports o4fix.py:403-535 literally)**

```rust
use crate::config::Config;
use crate::detect::{find_intervals, CleanDiag};
use crate::dsp;
use crate::error::O4Error;
use crate::optical;
use crate::telemetry::Meta;
use std::path::Path;
use std::sync::atomic::AtomicBool;

const R2D: f64 = 180.0 / std::f64::consts::PI;
const D2R: f64 = std::f64::consts::PI / 180.0;

pub fn optical_patch(video: &Path, tm: &[f64], cleaned: &[[f64; 3]],
                     diag: &CleanDiag, fs: f64, cfg: &Config, meta: &Meta,
                     log: &(dyn Fn(&str) + Sync), cancel: &AtomicBool)
    -> Result<Vec<[f64; 3]>, O4Error>
{
    // alpha_opt: separate optical trigger if configured (o4fix.py:405-412)
    let alpha_opt: Vec<f64> = match cfg.optical_noise {
        Some((lo, hi)) => {
            let a: Vec<f64> = diag.noise.iter()
                .map(|&n| ((n - lo) / (hi - lo)).clamp(0.0, 1.0)).collect();
            dsp::uniform_filter1d(&a, ((0.2 * fs) as usize).max(3))
        }
        None => diag.alpha.clone(),
    };
    let noisy = find_intervals(
        &alpha_opt.iter().map(|&a| a > 0.15).collect::<Vec<_>>(),
        tm, cfg.patch_pad, cfg.patch_merge, 0.2);
    if noisy.is_empty() {
        log("   optical patch: no noisy sections detected, skipping");
        return Ok(cleaned.to_vec());
    }

    // calibration sections (o4fix.py:420-431)
    let calib_all = find_intervals(
        &diag.alpha.iter().map(|&a| a < 0.02).collect::<Vec<_>>(),
        tm, -0.2, 0.0, 3.0);
    let motion: Vec<f64> = cleaned.iter()
        .map(|r| (r[0]*r[0] + r[1]*r[1] + r[2]*r[2]).sqrt() * R2D).collect();
    let mut scored: Vec<(f64, f64, f64)> = calib_all.iter().map(|&(a, b)| {
        let vals: Vec<f64> = tm.iter().zip(&motion)
            .filter(|(t, _)| **t >= a && **t <= b).map(|(_, m)| *m).collect();
        let mean = vals.iter().sum::<f64>() / vals.len() as f64;
        let std = (vals.iter().map(|v| (v - mean).powi(2)).sum::<f64>()
                   / vals.len() as f64).sqrt();     // np.std: population, ddof=0
        (std, a, b.min(a + 4.0))
    }).collect();
    scored.sort_by(|x, y| y.partial_cmp(x).unwrap());  // sort(reverse=True), tuple order
    let calib: Vec<(f64, f64)> = scored.iter().take(6).map(|&(_, a, b)| (a, b)).collect();
    if calib.is_empty() {
        log("   optical patch: no clean calibration sections");
        return Err(O4Error::CalibrationFailed { r2: None });
    }

    let total: f64 = noisy.iter().chain(&calib).map(|(a, b)| b - a).sum();
    log(&format!("   optical patch: analyzing {} noisy + {} calibration sections ({:.0} s of video)...",
                 noisy.len(), calib.len(), total));
    let opt_c = optical::video_rates(video, &calib, meta, cancel, &|_, _| ())?;
    let gyro_deg: Vec<[f64; 3]> = cleaned.iter()
        .map(|r| core::array::from_fn(|k| r[k] * R2D)).collect();
    let Some(al) = optical::fit_video_alignment(&opt_c, tm, &gyro_deg, fs) else {
        log("   optical patch: calibration failed");
        return Err(O4Error::CalibrationFailed { r2: None });
    };
    log(&format!("   optical patch: video/gyro alignment R2={:.3}, time offset {:.0} ms",
                 al.r2, al.shift * 1000.0));
    if al.r2 < 0.8 {
        return Err(O4Error::CalibrationFailed { r2: Some(al.r2) });
    }

    let opt_n = optical::video_rates(video, &noisy, meta, cancel, &|_, _| ())?;
    if opt_n.t.is_empty() {
        // DEVIATION from o4fix.py:456 (`return clean`): python's caller detects
        // that via `patched is clean` and refuses to write output; Rust makes
        // the refusal explicit so pipeline::process can't silently splice
        // unrepaired rates. Same user-visible outcome: no output, clear message.
        log("   optical patch: no optical samples in noisy sections");
        return Err(O4Error::CalibrationFailed { r2: None });
    }
    // patch_deg = degrees(ov) @ N ; tv += shift (o4fix.py:451-452)
    let patch_deg: Vec<[f64; 3]> = opt_n.omega.iter().map(|o| {
        core::array::from_fn(|c| (0..3).map(|r| o[r] * R2D * al.n[r][c]).sum())
    }).collect();
    let tv: Vec<f64> = opt_n.t.iter().map(|t| t + al.shift).collect();

    let mut out: Vec<[f64; 3]> = cleaned.iter()
        .map(|r| core::array::from_fn(|k| r[k] * R2D)).collect();
    let light = &diag.light;

    // rate-aware handback (o4fix.py:458-468)
    let hb_cut = cfg.handback_cutoff.unwrap_or(cfg.optical_cutoff);
    let mut medium = dsp::filtfilt3(&dsp::butter_low(2, hb_cut / (fs / 2.0)), light);
    let mag: Vec<f64> = medium.iter()
        .map(|r| (r[0]*r[0] + r[1]*r[1] + r[2]*r[2]).sqrt()).collect();
    let rate_mag = dsp::uniform_filter1d(&mag, ((0.1 * fs) as usize).max(3));
    let (lo_r, hi_r) = cfg.fast_handback;
    let wf0: Vec<f64> = rate_mag.iter()
        .map(|&m| ((m - lo_r) / (hi_r - lo_r).max(1e-6)).clamp(0.0, 1.0)).collect();
    let w_fast = dsp::uniform_filter1d(&wf0, ((0.15 * fs) as usize).max(3));

    // fast-wide branch (M4; o4fix.py:474-487)
    if cfg.fast_wide_cutoff != 0.0 {
        let wide = dsp::filtfilt3(&dsp::butter_low(2, cfg.fast_wide_cutoff / (fs / 2.0)), light);
        let (lo_w, hi_w) = cfg.fast_wide_ramp;
        let mut w_wide: Vec<f64> = rate_mag.iter()
            .map(|&m| ((m - lo_w) / (hi_w - lo_w).max(1e-6)).clamp(0.0, 1.0)).collect();
        if cfg.fast_wide_accel != 0.0 {
            let grad: Vec<f64> = dsp::gradient(&rate_mag).iter()
                .map(|g| (g * fs).abs()).collect();
            let acc = dsp::uniform_filter1d(&grad, ((0.1 * fs) as usize).max(3));
            for i in 0..w_wide.len() {
                w_wide[i] *= (1.0 - acc[i] / cfg.fast_wide_accel).clamp(0.0, 1.0);
            }
        }
        let w_wide = dsp::uniform_filter1d(&w_wide, ((0.15 * fs) as usize).max(3));
        for i in 0..medium.len() {
            for k in 0..3 {
                medium[i][k] = (1.0 - w_wide[i]) * medium[i][k] + w_wide[i] * wide[i][k];
            }
        }
    }

    // per-burst optical splice-in (o4fix.py:489-534)
    let vfps = if tv.len() > 1 {
        let m = tv.len().min(100);
        let mut d: Vec<f64> = tv[..m].windows(2).map(|w| w[1] - w[0]).collect();
        d.sort_by(f64::total_cmp);
        let mid = d.len() / 2;
        let med = if d.len() % 2 == 0 { (d[mid - 1] + d[mid]) / 2.0 } else { d[mid] };
        1.0 / med
    } else { 100.0 };
    let bq = dsp::butter_low(2, cfg.optical_cutoff.min(0.45 * vfps) / (vfps / 2.0));
    let strong = &diag.strong;
    for &(a, b) in &noisy {
        let midx: Vec<usize> = (0..tv.len())
            .filter(|&i| tv[i] >= a - 0.3 && tv[i] <= b + 0.3).collect();
        if midx.len() < 30 { continue; }
        let seg_t: Vec<f64> = midx.iter().map(|&i| tv[i]).collect();
        let mut seg_o: Vec<[f64; 3]> = midx.iter().map(|&i| patch_deg[i]).collect();
        let seg_q: Vec<f64> = midx.iter().map(|&i| opt_n.quality[i]).collect();
        let frac_bad = seg_q.iter().filter(|&&q| q < 0.3).count() as f64 / seg_q.len() as f64;
        if frac_bad > 0.3 {
            log(&format!("   optical patch: {a:.1}-{b:.1}s skipped ({:.0}% low-quality flow), keeping filtered gyro",
                         frac_bad * 100.0));
            continue;
        }
        let bad: Vec<bool> = seg_q.iter().map(|&q| q < 0.3).collect();
        if bad.iter().any(|&x| x) && !bad.iter().all(|&x| x) {
            let gt: Vec<f64> = seg_t.iter().zip(&bad).filter(|(_, &b)| !b).map(|(t, _)| *t).collect();
            for k in 0..3 {
                let gv: Vec<f64> = seg_o.iter().zip(&bad).filter(|(_, &b)| !b).map(|(o, _)| o[k]).collect();
                let bt: Vec<f64> = seg_t.iter().zip(&bad).filter(|(_, &b)| *b).map(|(t, _)| *t).collect();
                let fill = dsp::interp(&bt, &gt, &gv);
                let mut fi = 0;
                for (i, &isbad) in bad.iter().enumerate() {
                    if isbad { seg_o[i][k] = fill[fi]; fi += 1; }
                }
            }
        }
        seg_o = dsp::filtfilt3(&bq, &seg_o);
        let gm: Vec<usize> = (0..tm.len()).filter(|&i| tm[i] >= a && tm[i] <= b).collect();
        let tq: Vec<f64> = gm.iter().map(|&i| tm[i]).collect();
        let video_1k: Vec<[f64; 3]> = {
            let per: Vec<Vec<f64>> = (0..3).map(|k| dsp::interp(
                &tq, &seg_t, &seg_o.iter().map(|r| r[k]).collect::<Vec<_>>())).collect();
            (0..tq.len()).map(|i| [per[0][i], per[1][i], per[2][i]]).collect()
        };
        let burst: Vec<[f64; 3]> = if cfg.anchor_mode {
            // optical = LF drift anchor on band-limited gyro (o4fix.py:513-525)
            let g: Vec<[f64; 3]> = gm.iter().map(|&i| strong[i]).collect();
            let mut corr: Vec<[f64; 3]> = (0..g.len())
                .map(|i| core::array::from_fn(|k| video_1k[i][k] - g[i][k])).collect();
            let ba = dsp::butter_low(2, cfg.anchor_cutoff / (fs / 2.0));
            let nseg = corr.len();
            let taps = ba.b.len().max(ba.a.len());
            if nseg > 3 * taps * 10 {
                let padlen = (nseg - 1).min((2.0 * fs) as usize);
                let cols: Vec<Vec<f64>> = (0..3).map(|k| dsp::filtfilt_padlen(
                    &ba, &corr.iter().map(|r| r[k]).collect::<Vec<_>>(), padlen)).collect();
                corr = (0..nseg).map(|i| [cols[0][i], cols[1][i], cols[2][i]]).collect();
            }
            (0..g.len()).map(|i| core::array::from_fn(|k| {
                g[i][k] + (1.0 - w_fast[gm[i]]) * corr[i][k]
            })).collect()
        } else {
            (0..gm.len()).map(|i| core::array::from_fn(|k| {
                let wf = w_fast[gm[i]];
                (1.0 - wf) * video_1k[i][k] + wf * medium[gm[i]][k]
            })).collect()
        };
        // steep ramp + partner blend (o4fix.py:528-534)
        for (i, &g) in gm.iter().enumerate() {
            let w = (alpha_opt[g] / 0.35).clamp(0.0, 1.0);
            for k in 0..3 {
                let partner = if cfg.optical_noise.is_some() { out[g][k] } else { light[g][k] };
                out[g][k] = (1.0 - w) * partner + w * burst[i][k];
            }
        }
    }
    Ok(out.iter().map(|r| core::array::from_fn(|k| r[k] * D2R)).collect())
}
```
Porting cautions: `np.std` is population std (ddof=0); python tuple sort compares (std, a, b) lexicographically — `partial_cmp` on the tuple matches; `int(b*fps)+1` frame ranges live in optical.rs; the `m.sum() < 30` python check counts mask samples on `tv`, as here; `out` accumulates across bursts (later bursts see earlier writes) — the sequential loop preserves that.

- [ ] **Step 4: Run test, expect pass** — `cargo test -p o4core --test golden_patch -- --ignored --nocapture` (~10 min: optical over noisy+calib). PASS at ≤1e-9.

- [ ] **Step 5: Commit**

```powershell
git add rust; git commit -m @'
feat: optical_patch with stage-golden parity

Claude-Session: https://claude.ai/code/session_01Y4QB81pia8MGdXyUZKkdQU
'@
```

---

### Task 14: patch.rs — splice_orientation

**Files:**
- Modify: `rust/o4core/src/patch.rs` (add `BurstStat` + `splice_orientation`)
- Create: `rust/o4core/tests/golden_splice.rs`

**Interfaces:**
- Consumes: `quat::{qmul, qconj, qnorm, qexp, qlog, slerp, smoothstep}`, `dsp::{searchsorted_left, searchsorted_right}`.
- Produces:
  - `patch::BurstStat { pub start: f64, pub end: f64, pub drift_deg: f64 }`
  - `patch::splice_orientation(t: &[f64], q_raw: &[[f64;4]], omega_patch: &[[f64;3]], intervals: &[(f64,f64)], ramp_s: f64) -> (Vec<[f64;4]>, Vec<BurstStat>)` — pure function, no I/O. `omega_patch` is rad/s with `len(t)-1` rows; row `k` applies to step `t[k] -> t[k+1]` (this is exactly what Task 13's `optical_patch` returns).

- [ ] **Step 1: Write failing golden test**

`rust/o4core/tests/golden_splice.rs`:
```rust
#[path = "golden_telemetry.rs"] mod gt;
use ndarray::{Array1, Array2};

#[test]
#[ignore] // needs test clip + goldens
fn splice_matches_python() {
    let (t, q) = gt::npz_extract(); // helper added below: (Vec<f64>, Vec<[f64;4]>) from extract.npz
    let mut zp = gt::npz("patched.npz");
    let rates: Array2<f64> = zp.by_name("rates").unwrap();
    let omega: Vec<[f64; 3]> = (0..rates.nrows())
        .map(|i| [rates[[i, 0]], rates[[i, 1]], rates[[i, 2]]]).collect();
    let mut zi = gt::npz("intervals.npz");
    let sev: Array2<f64> = zi.by_name("severe").unwrap();
    let intervals: Vec<(f64, f64)> = (0..sev.nrows())
        .map(|i| (sev[[i, 0]], sev[[i, 1]])).collect();

    let ramp = o4core::config::Config::default().ramp;
    let (q_out, stats) = o4core::patch::splice_orientation(&t, &q, &omega, &intervals, ramp);

    let mut zs = gt::npz("splice.npz");
    let qg: Array2<f64> = zs.by_name("q_out").unwrap();
    let drifts: Array2<f64> = zs.by_name("drifts").unwrap(); // rows (a, b, drift_deg)
    assert_eq!(q_out.len(), qg.nrows());
    for i in 0..q_out.len() {
        let d = (0..4).map(|k| (q_out[i][k] - qg[[i, k]]).abs()).fold(0.0, f64::max);
        let dn = (0..4).map(|k| (q_out[i][k] + qg[[i, k]]).abs()).fold(0.0, f64::max);
        assert!(d.min(dn) <= 1e-9, "q_out[{i}]: folded diff {}", d.min(dn));
    }
    assert_eq!(stats.len(), drifts.nrows());
    for (i, s) in stats.iter().enumerate() {
        assert!((s.start - drifts[[i, 0]]).abs() <= 1e-9);
        assert!((s.end - drifts[[i, 1]]).abs() <= 1e-9);
        assert!((s.drift_deg - drifts[[i, 2]]).abs() <= 1e-6,
                "drift[{i}]: {} vs {}", s.drift_deg, drifts[[i, 2]]);
    }
}
```
Add to `golden_telemetry.rs` a `pub fn npz_extract() -> (Vec<f64>, Vec<[f64;4]>)` helper that loads `extract.npz` t/q into plain vecs (`repo()`/`npz()` there are already `pub` for exactly this cross-file use).

- [ ] **Step 2: Run to verify failure** — FAIL (no `splice_orientation`).

- [ ] **Step 3: Implement in `rust/o4core/src/patch.rs` (ports o4fix.py:137-173 literally)**

```rust
#[derive(Clone, Copy, Debug)]
pub struct BurstStat {
    pub start: f64,
    pub end: f64,
    pub drift_deg: f64,
}

/// Replace q_raw inside each interval with integrated omega_patch, pinned to
/// raw at both edges (o4fix.py:137-173). Accumulated drift is spread across
/// the interval as a smoothstep rotation-vector correction; ramp_s slerp
/// cross-fades at the edges. Samples outside intervals are returned
/// bit-identical (clean-zone guarantee — feeds mp4::patch_video's
/// unchanged-row original-bytes path).
pub fn splice_orientation(t: &[f64], q_raw: &[[f64; 4]], omega_patch: &[[f64; 3]],
                          intervals: &[(f64, f64)], ramp_s: f64)
    -> (Vec<[f64; 4]>, Vec<BurstStat>)
{
    use crate::quat::{qconj, qexp, qlog, qmul, qnorm, slerp, smoothstep};
    let mut q_out = q_raw.to_vec();
    let mut stats = Vec::new();
    for &(a, b) in intervals {
        let i0 = crate::dsp::searchsorted_left(t, a);
        let i1 = crate::dsp::searchsorted_right(t, b)
            .saturating_sub(1).min(t.len() - 1);
        if i1 < i0 + 8 { continue; }            // python: if i1 - i0 < 8
        let n = i1 - i0;

        // sequential integration: qs[k+1] = qs[k] * qexp(omega*dt)
        let mut qs: Vec<[f64; 4]> = Vec::with_capacity(n + 1);
        qs.push(q_raw[i0]);
        for k in 0..n {
            let dt = t[i0 + k + 1] - t[i0 + k];
            let o = omega_patch[i0 + k];
            qs.push(qmul(qs[k], qexp([o[0] * dt, o[1] * dt, o[2] * dt])));
        }
        for q in qs.iter_mut() { *q = qnorm(*q); }  // python normalizes ONCE, after the loop

        // endpoint drift, spread as smoothstep rotation-vector correction
        let e = qlog(qmul(qconj(qs[n]), q_raw[i1]));
        let drift_deg = (e[0] * e[0] + e[1] * e[1] + e[2] * e[2]).sqrt().to_degrees();
        let dur = (t[i1] - t[i0]).max(1e-9);
        for k in 0..=n {
            let s = smoothstep((t[i0 + k] - t[i0]) / dur);
            qs[k] = qmul(qs[k], qexp([s * e[0], s * e[1], s * e[2]]));
            // NOTE: python does NOT renormalize after this multiply — neither do we
        }

        // edge cross-fade, then write back
        for k in 0..=n {
            let tt = t[i0 + k];
            let r = smoothstep((tt - t[i0]) / ramp_s)
                .min(smoothstep((t[i1] - tt) / ramp_s));
            q_out[i0 + k] = slerp(q_raw[i0 + k], qs[k], r);
        }
        stats.push(BurstStat { start: a, end: b, drift_deg });
    }
    (q_out, stats)
}
```
Porting cautions: (1) integration is inherently sequential — do not rayon this loop; (2) Python's `max(int(searchsorted), 0)` is a no-op (searchsorted ≥ 0) and its `min(searchsorted - 1, len - 1)` can only go negative for an interval entirely before `t[0]`, which `find_intervals` can't produce — `i1 < i0 + 8` covers that corner in Rust; (3) normalization order matters: qexp returns unit quats but the qmul chain accumulates rounding, and Python normalizes the whole path once after integration, before computing `e` — replicate exactly; (4) slerp already normalizes its output; the drift-correction multiply is NOT followed by a normalize in Python.

- [ ] **Step 4: Run test, expect pass** — `cargo test -p o4core --test golden_splice -- --ignored --nocapture` (fast; no video decode). PASS: q_out ≤ 1e-9 sign-folded, drifts ≤ 1e-6 deg.

- [ ] **Step 5: Commit**

```powershell
git add rust; git commit -m @'
feat: splice_orientation with stage-golden parity

Claude-Session: https://claude.ai/code/session_01Y4QB81pia8MGdXyUZKkdQU
'@
```

---

### Task 15: pipeline.rs — process() orchestration + e2e gate

**Files:**
- Create: `rust/o4core/src/pipeline.rs`, `rust/o4core/tests/e2e.rs`
- Modify: `rust/o4core/src/lib.rs` (add `pub mod pipeline;`, DELETE `pipeline_fs_placeholder`), `rust/o4core/tests/golden_patch.rs` (call site → `o4core::pipeline::fs(&tel.t)`)

**Interfaces:**
- Consumes: everything.
- Produces:
  - `pipeline::median(x: &[f64]) -> f64` — np.median semantics (sort, average the two middle elements for even n)
  - `pipeline::fs(t: &[f64]) -> f64` — `1/median(diff(t))`, o4fix.py:590
  - `pipeline::Stage { Extract, Analyze, Optical, Splice, Write }`
  - `pipeline::Progress { pub stage: Stage, pub message: String }` (fraction/percent plumbing is Plan 2 — the GUI; adding a field later is additive)
  - `pipeline::Outcome::{ Repaired { out: PathBuf, bursts: Vec<BurstStat> }, Healthy }`
  - `pipeline::process(video: &Path, out: Option<&Path>, cfg: &Config, on_progress: &(dyn Fn(Progress) + Sync), cancel: &AtomicBool) -> Result<Outcome, O4Error>`

- [ ] **Step 1: Write failing tests**

Unit (in-module, no clip):
```rust
#[test]
fn median_matches_numpy() {
    assert_eq!(median(&[1.0, 2.0, 3.0, 4.0]), 2.5);   // even: average middle two
    assert_eq!(median(&[1.0, 3.0, 2.0]), 2.0);        // odd: middle after sort
    assert_eq!(median(&[5.0]), 5.0);
    assert!((fs(&[0.0, 0.001, 0.002, 0.0035]) - 1000.0).abs() < 1e-9); // median dt = 1 ms
}
```

`rust/o4core/tests/e2e.rs` (all `#[ignore]`, need clip + goldens):
```rust
#[path = "golden_telemetry.rs"] mod gt;
use std::sync::atomic::AtomicBool;
use o4core::pipeline::{process, Outcome, Progress};
use o4core::config::Config;

fn run(cfg: &Config, out: &std::path::Path) -> Result<Outcome, o4core::error::O4Error> {
    let video = gt::repo("sample_vids/DJI_20260711124046_0021_D.MP4");
    process(&video, Some(out), cfg,
            &|p: Progress| println!("{}", p.message), &AtomicBool::new(false))
}

#[test]
#[ignore]
fn healthy_clip_short_circuits() {
    let out = std::env::temp_dir().join("o4fix_healthy_test.MP4");
    let _ = std::fs::remove_file(&out);
    let cfg = Config { severe: f64::MAX, ..Config::default() };
    let r = run(&cfg, &out).unwrap();
    assert!(matches!(r, Outcome::Healthy));
    assert!(!out.exists(), "healthy path must not write an output file");
}

#[test]
#[ignore]
fn no_calibration_sections_is_actionable_error() {
    let out = std::env::temp_dir().join("o4fix_nocalib_test.MP4");
    let _ = std::fs::remove_file(&out);
    // noise_low/high < 0 forces alpha == 1 everywhere -> no alpha<0.02 calib windows
    let cfg = Config { noise_low: -2.0, noise_high: -1.0, ..Config::default() };
    let e = run(&cfg, &out).unwrap_err();
    assert!(matches!(e, o4core::error::O4Error::CalibrationFailed { .. }));
    assert!(!out.exists());
}

#[test]
#[ignore] // ~10 min: full pipeline incl. optical
fn e2e_matches_seeded_python_reference() {
    let out = std::env::temp_dir().join("o4fix_e2e_test.MP4");
    let _ = std::fs::remove_file(&out);
    let r = run(&Config::default(), &out).unwrap();
    let Outcome::Repaired { bursts, .. } = r else { panic!("expected Repaired") };
    assert!(!bursts.is_empty());

    // compare against the SEEDED python reference written by dump_goldens.py
    let (t_r, q_r) = stream(&out);
    let (t_p, q_p) = stream(&gt::repo("goldens/ref_fixed.MP4"));
    assert_eq!(t_r, t_p, "timestamps must be identical");
    let mut zi = gt::npz("intervals.npz");
    let sev: ndarray::Array2<f64> = zi.by_name("severe").unwrap();
    for i in 0..q_r.len() {
        let d = (0..4).map(|k| (q_r[i][k] - q_p[i][k]).abs()).fold(0.0, f64::max);
        let dn = (0..4).map(|k| (q_r[i][k] + q_p[i][k]).abs()).fold(0.0, f64::max);
        let t_s = t_r[i] / 1000.0;
        let inside = (0..sev.nrows()).any(|j| t_s >= sev[[j, 0]] && t_s <= sev[[j, 1]]);
        if inside {
            assert!(d.min(dn) <= 1e-6, "repaired sample {i}: {}", d.min(dn));
        } else {
            assert_eq!(d.min(dn), 0.0, "clean-zone sample {i} must be bit-exact");
        }
    }
}

fn stream(p: &std::path::Path) -> (Vec<f64>, Vec<[f64; 4]>) {
    o4core::telemetry::flat_quat_stream(p).unwrap()
}
```
The clean-zone `assert_eq!(…, 0.0)` is the parser-level restatement of the byte-exactness guarantee: outside severe intervals `splice_orientation` returns `q_raw` untouched, `patch_video` keeps original bytes, and both files decode identically. Sign-fold anyway — the parser's own continuity pass could legally flip a whole run's sign if an upstream repaired sample moved across an antipode.

**Prerequisite — seeded Python reference MP4:** `goldens/ref_fixed.MP4` is written by Task 7's dump script; if it's missing (dump ran from an older script revision), rerun `python tools/dump_goldens.py`. Do NOT compare against `sample_vids/DJI_20260711124046_0021_D_fixed.MP4` at tight tolerance: that file was produced by unseeded Python RANSAC, so its repaired zones legitimately differ beyond 1e-6. It stays as Task 17's render-level reference only.

- [ ] **Step 2: Run to verify failure** — FAIL (no `pipeline`).

- [ ] **Step 3: Implement `rust/o4core/src/pipeline.rs`**

```rust
//! Orchestration: ports o4fix.process/process_mp4 (o4fix.py:586-662) with one
//! DELIBERATE REORDER (spec: pipeline deviation): severe intervals are
//! computed FIRST, so healthy clips return Outcome::Healthy before any
//! OpenCV work. For repairable clips the outputs are identical to Python;
//! only console-message order differs (burst count prints before optical
//! progress).
use crate::config::Config;
use crate::error::O4Error;
use crate::patch::BurstStat;
use crate::{detect, mp4, patch, quat, telemetry};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stage { Extract, Analyze, Optical, Splice, Write }

#[derive(Clone, Debug)]
pub struct Progress {
    pub stage: Stage,
    pub message: String,
}

#[derive(Debug)]
pub enum Outcome {
    Repaired { out: PathBuf, bursts: Vec<BurstStat> },
    /// No severe bursts found — telemetry healthy, no output written.
    Healthy,
}

/// np.median: sort, average the two middle elements when n is even.
pub fn median(x: &[f64]) -> f64 {
    let mut v = x.to_vec();
    v.sort_by(f64::total_cmp);
    let m = v.len() / 2;
    if v.len() % 2 == 0 { (v[m - 1] + v[m]) / 2.0 } else { v[m] }
}

/// Sample rate, identical to o4fix.py:590 `1/np.median(np.diff(t))`.
pub fn fs(t: &[f64]) -> f64 {
    1.0 / median(&t.windows(2).map(|w| w[1] - w[0]).collect::<Vec<_>>())
}

pub fn process(video: &Path, out: Option<&Path>, cfg: &Config,
               on_progress: &(dyn Fn(Progress) + Sync), cancel: &AtomicBool)
    -> Result<Outcome, O4Error>
{
    let say = |stage: Stage, message: String|
        on_progress(Progress { stage, message });
    let check = || -> Result<(), O4Error> {
        if cancel.load(Ordering::Relaxed) { Err(O4Error::Cancelled) } else { Ok(()) }
    };

    check()?;
    let tel = telemetry::extract_quats(video)?;
    let fs = fs(&tel.t);
    say(Stage::Extract, format!(
        "   {} {}, {} quat samples @ {:.0} Hz, {:.1} s",
        tel.meta.camera, tel.meta.model, tel.q.len(), fs,
        tel.t[tel.t.len() - 1] - tel.t[0]));

    check()?;
    let (tm, omega) = quat::quats_to_rates(&tel.t, &tel.q);
    let (cleaned, diag) = detect::adaptive_clean(&omega, fs, cfg);
    let frac = diag.alpha.iter().filter(|&&a| a > 0.5).count() as f64
        / diag.alpha.len() as f64;
    say(Stage::Analyze, format!(
        "   spikes replaced: {:.1}% of samples, noise bursts cover: {:.1}% of flight",
        diag.spike_frac * 100.0, frac * 100.0));

    // ---- deliberate reorder: severe gate BEFORE optical ----
    let severe_mask: Vec<bool> = diag.noise.iter().map(|&n| n > cfg.severe).collect();
    let intervals = detect::find_intervals(&severe_mask, &tm,
                                           cfg.severe_pad, cfg.severe_merge, 0.2);
    if intervals.is_empty() {
        say(Stage::Analyze, format!(
            "   no severe bursts (> {:?} deg/s band-RMS) found - telemetry \
             looks healthy, nothing to repair", cfg.severe));
        return Ok(Outcome::Healthy);
    }
    let tot: f64 = intervals.iter().map(|(a, b)| b - a).sum();
    say(Stage::Analyze, format!(
        "   replacing orientation in {} severe bursts ({tot:.1} s)", intervals.len()));

    check()?;
    let log = |s: &str| say(Stage::Optical, s.to_string());
    let patched = patch::optical_patch(video, &tm, &cleaned, &diag, fs, cfg,
                                       &tel.meta, &log, cancel)?;

    check()?;
    let (q_out, bursts) = patch::splice_orientation(
        &tel.t, &tel.q, &patched, &intervals, cfg.ramp);
    for b in &bursts {
        say(Stage::Splice, format!(
            "     [{:7.2}, {:7.2}] optical drift over burst: {:5.2} deg",
            b.start, b.end, b.drift_deg));
    }

    check()?;
    let out_path: PathBuf = match out {
        Some(p) => p.to_path_buf(),
        None => {
            let stem = video.file_stem().unwrap_or_default().to_string_lossy();
            let ext = video.extension().map(|e| e.to_string_lossy()).unwrap_or_default();
            video.with_file_name(format!("{stem}_fixed.{ext}"))  // preserves .MP4 vs .mp4
        }
    };
    let wlog = |s: &str| say(Stage::Write, s.to_string());
    match mp4::inject_and_check(video, &out_path, &q_out, &wlog) {
        Ok(true) => {
            say(Stage::Write, format!(
                "   wrote {} - load it in Gyroflow like a stock recording",
                out_path.display()));
            Ok(Outcome::Repaired { out: out_path, bursts })
        }
        Ok(false) => {
            let _ = std::fs::remove_file(&out_path);
            Err(O4Error::VerifyFailed)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&out_path); // partial copy possible
            Err(e)
        }
    }
}
```
Notes:
- `{:?}` on `cfg.severe` prints `8.0` matching Python's `str(8.0)`; `{}` would print `8`.
- Python's `patched is clean` identity check (o4fix.py:634, "optical patch unavailable … not writing an output file") maps to Rust errors instead: the ImportError arm doesn't exist (OpenCV is compiled in), the "no noisy sections" arm is unreachable when severe intervals are non-empty (severe 8 > noise_high 5 ⇒ alpha = 1 ⇒ noisy covers every severe sample), and the empty-optical-samples arm returns `Err(CalibrationFailed)` (already in Task 13's `optical_patch`, see the DEVIATION comment there). Net behavior matches: no output file, actionable message.
- lib.rs: delete `pipeline_fs_placeholder`; `tests/golden_patch.rs` switches to `o4core::pipeline::fs(&tel.t)`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p o4core` (unit) and `cargo test -p o4core --test e2e -- --ignored --nocapture` (~15 min total; regenerate goldens first if `goldens/ref_fixed.MP4` is missing).
Expected: all pass. The e2e clean-zone assert is the whole project's keystone — if it fails, suspect splice interval indexing (Task 14) or unchanged-row detection (Task 10), NOT tolerances. Do not loosen `1e-6`/bit-exact without stopping for user signoff.

- [ ] **Step 5: Commit**

```powershell
git add rust; git commit -m @'
feat: pipeline::process with healthy short-circuit and e2e parity gate

Claude-Session: https://claude.ai/code/session_01Y4QB81pia8MGdXyUZKkdQU
'@
```

---

### Task 16: o4fix-cli — CLI parity for MP4-repair mode

**Files:**
- Modify: `rust/o4fix-cli/Cargo.toml` (add `clap = { version = "4", features = ["derive"] }`, `o4core = { path = "../o4core" }`), `rust/o4fix-cli/src/main.rs` (replace stub)
- Create: `rust/o4fix-cli/src/args.rs`, `rust/o4fix-cli/tests/cli_args.rs`

**Interfaces:**
- Consumes: `pipeline::process`, `config::Config`.
- Produces: `o4fix` binary. MP4-repair flags mirror `o4fix.py` exactly (names, defaults, help text abbreviated is fine). NOT ported: `--gcsv`, `--plot`, `--orientation`, `--lpf`, `--no-optical` (legacy gcsv pipeline stays Python-only per spec) — passing them errors via clap's unknown-flag handling, which is the correct signal.

- [ ] **Step 1: Write failing test**

`rust/o4fix-cli/tests/cli_args.rs` — parse-level only (no clip needed):
```rust
use clap::Parser;
// includes the REAL parser source (same file main.rs declares as `mod args;`)
#[path = "../src/args.rs"] mod args;
use args::Cli;

#[test]
fn defaults_map_to_default_config() {
    let c = Cli::parse_from(["o4fix", "a.MP4"]).to_config();
    let d = o4core::config::Config::default();
    assert_eq!(c.severe, d.severe);
    assert_eq!(c.noise_band, d.noise_band);
    assert_eq!(c.fast_wide_ramp, d.fast_wide_ramp);
    assert_eq!(c.fast_wide_accel, d.fast_wide_accel);
    assert!(c.handback_cutoff.is_none() && c.optical_noise.is_none());
    assert!(!c.anchor_mode);
}

#[test]
fn m4_profile_via_flags() {
    let c = Cli::parse_from(["o4fix", "a.MP4", "--fast-wide-cutoff", "16"]).to_config();
    assert_eq!(c.fast_wide_cutoff, 16.0);
    assert_eq!(c.fast_wide_accel, 1500.0); // accel gate defaults ON
}

#[test]
fn output_with_multiple_videos_rejected() {
    assert!(Cli::try_parse_from(["o4fix", "a.MP4", "b.MP4", "-o", "x.MP4"])
        .and_then(|c| c.validate()).is_err());
}
```

- [ ] **Step 2: Run to verify failure** — FAIL (stub main, no args module).

- [ ] **Step 3: Implement**

`rust/o4fix-cli/src/args.rs` (shape; keep every default literally equal to o4fix.py's argparse):
```rust
use clap::Parser;
use o4core::config::Config;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "o4fix", version,
          about = "Repair DJI O4 Pro gyro noise: writes VIDEO_fixed.MP4 with \
                   clean embedded telemetry - load it in Gyroflow like a stock recording")]
pub struct Cli {
    /// DJI O4 Pro .MP4 file(s)
    #[arg(required = true)]
    pub videos: Vec<PathBuf>,
    /// output path (single video only); default VIDEO_fixed.MP4
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    // MP4 repair tuning
    #[arg(long, default_value_t = 8.0)]  pub severe: f64,
    #[arg(long, default_value_t = 0.2)]  pub severe_pad: f64,
    #[arg(long, default_value_t = 0.2)]  pub severe_merge: f64,
    #[arg(long, default_value_t = 0.3)]  pub ramp: f64,

    // filter tuning (defaults tuned on O4 Pro test flight)
    #[arg(long, default_value_t = 25.0)] pub light_cutoff: f64,
    #[arg(long, default_value_t = 2.5)]  pub strong_cutoff: f64,
    #[arg(long, default_value_t = 1.5)]  pub noise_low: f64,
    #[arg(long, default_value_t = 5.0)]  pub noise_high: f64,
    #[arg(long, num_args = 2, value_names = ["LO", "HI"],
          default_values_t = [30.0, 180.0])]
    pub noise_band: Vec<f64>,
    #[arg(long, default_value_t = 100.0)] pub noise_window: f64,
    #[arg(long, default_value_t = 7)]     pub hampel_window: usize,
    #[arg(long, default_value_t = 6.0)]   pub hampel_sigma: f64,
    #[arg(long, default_value_t = 8.0)]   pub optical_cutoff: f64,
    #[arg(long, num_args = 2, value_names = ["LO", "HI"],
          default_values_t = [100.0, 250.0])]
    pub fast_handback: Vec<f64>,
    #[arg(long, default_value_t = 0.5)]   pub patch_pad: f64,
    #[arg(long, default_value_t = 1.0)]   pub patch_merge: f64,
    #[arg(long, num_args = 2, value_names = ["LO", "HI"])]
    pub optical_noise: Option<Vec<f64>>,
    #[arg(long)]                          pub handback_cutoff: Option<f64>,
    #[arg(long, default_value_t = 0.0)]   pub fast_wide_cutoff: f64,
    #[arg(long, num_args = 2, value_names = ["LO", "HI"],
          default_values_t = [150.0, 300.0])]
    pub fast_wide_ramp: Vec<f64>,
    #[arg(long, default_value_t = 1500.0)] pub fast_wide_accel: f64,
    #[arg(long)]                          pub anchor_mode: bool,
    #[arg(long, default_value_t = 1.5)]   pub anchor_cutoff: f64,
}

impl Cli {
    pub fn validate(self) -> Result<Self, clap::Error> {
        if self.output.is_some() && self.videos.len() > 1 {
            // mirrors argparse p.error(...): usage error, exit code 2
            return Err(clap::Error::raw(clap::error::ErrorKind::ArgumentConflict,
                "-o/--output only works with a single video\n"));
        }
        Ok(self)
    }
    pub fn to_config(&self) -> Config {
        Config {
            severe: self.severe, severe_pad: self.severe_pad,
            severe_merge: self.severe_merge, ramp: self.ramp,
            light_cutoff: self.light_cutoff, strong_cutoff: self.strong_cutoff,
            noise_low: self.noise_low, noise_high: self.noise_high,
            noise_band: (self.noise_band[0], self.noise_band[1]),
            noise_window_ms: self.noise_window,
            hampel_window: self.hampel_window, hampel_sigma: self.hampel_sigma,
            optical_cutoff: self.optical_cutoff,
            handback_cutoff: self.handback_cutoff,
            fast_handback: (self.fast_handback[0], self.fast_handback[1]),
            patch_pad: self.patch_pad, patch_merge: self.patch_merge,
            optical_noise: self.optical_noise.as_ref().map(|v| (v[0], v[1])),
            fast_wide_cutoff: self.fast_wide_cutoff,
            fast_wide_ramp: (self.fast_wide_ramp[0], self.fast_wide_ramp[1]),
            fast_wide_accel: self.fast_wide_accel,
            anchor_mode: self.anchor_mode, anchor_cutoff: self.anchor_cutoff,
        }
    }
}
```

`rust/o4fix-cli/src/main.rs`:
```rust
mod args;
use clap::Parser;
use std::sync::atomic::AtomicBool;

fn main() -> std::process::ExitCode {
    let cli = match args::Cli::parse().validate() {
        Ok(c) => c,
        Err(e) => e.exit(), // usage errors: exit 2, argparse-compatible
    };
    let cfg = cli.to_config();
    let cancel = AtomicBool::new(false); // v1: Ctrl+C kills the process; GUI adds real cancel
    let mut failed = false;
    for video in &cli.videos {
        println!("== {}", video.file_name().unwrap_or_default().to_string_lossy());
        let r = o4core::pipeline::process(
            video, cli.output.as_deref(), &cfg,
            &|p| println!("{}", p.message), &cancel);
        match r {
            Ok(_) => {}                       // Repaired and Healthy both print their own lines
            Err(e) => { eprintln!("   ERROR: {e}"); failed = true; }
        }
    }
    if failed { std::process::ExitCode::FAILURE } else { std::process::ExitCode::SUCCESS }
}
```
DEVIATION (document in rust/README.md): python o4fix.py always exits 0 even when a video fails; the Rust CLI exits 1 if any video failed — better for scripting, and the GUI (Plan 2) relies on `process()`'s Result anyway. Message text and order match Python except the reorder note from Task 15.

- [ ] **Step 4: Run tests + smoke run**

Run: `cargo test -p o4fix-cli`, then
`cargo run -p o4fix-cli --release -- sample_vids/DJI_20260711124046_0021_D.MP4 -o "$env:TEMP\cli_smoke_fixed.MP4"` (~10 min)
and `python o4fix.py sample_vids/DJI_20260711124046_0021_D.MP4 -o "$env:TEMP\cli_smoke_py.MP4"` if a fresh side-by-side transcript is wanted.
Expected: arg tests pass; smoke run prints the same lines as Python (allowing the severe-burst line to appear earlier, and optical drift values to differ in the last digits — python here is unseeded), writes the output, exits 0. `--help` lists every flag above.

- [ ] **Step 5: Commit**

```powershell
git add rust; git commit -m @'
feat: o4fix CLI with python-parity flags and messages

Claude-Session: https://claude.ai/code/session_01Y4QB81pia8MGdXyUZKkdQU
'@
```

---

### Task 17: Acceptance — Gyroflow render of Rust output sits on the M2 row

**Files:**
- Create: `sample_vids/eval_RUST.gyroflow` (copy of `sample_vids/eval_M2_tight.gyroflow`, JSON-edited)
- No source changes. This is the STOP-for-user gate.

- [ ] **Step 1: Produce the Rust-repaired clip**

Run: `cargo run -p o4fix-cli --release -- sample_vids/DJI_20260711124046_0021_D.MP4 -o sample_vids/DJI_20260711124046_0021_D_RUST.MP4`
(Skip if the Task 16 smoke output is still around — but keep the file in `sample_vids/` so the .gyroflow path is stable.)

- [ ] **Step 2: Build the render project**

Copy `sample_vids/eval_M2_tight.gyroflow` → `sample_vids/eval_RUST.gyroflow`, then JSON-edit (CLAUDE.md harness rules):
- `videofile` → `DJI_20260711124046_0021_D_RUST.MP4` path
- `gyro_source.filepath` → same path, and **DROP `gyro_source.file_metadata` entirely** (gyro file changed)
- `offsets` → `{}` (never autosync patched data)
- `output.output_filename` → `eval_RUST.mp4`, bitrate 30

- [ ] **Step 3: Render + evaluate**

Run (~90 s): `& "C:\Program Files\WindowsApps\29160AdrianRoss.Gyroflow_1.63.2453.0_x64__q81n4e8pq4bra\Gyroflow.exe" sample_vids\eval_RUST.gyroflow -f --stdout-progress`
Then (~7 min): `python analysis\eval_render.py sample_vids\eval_RUST.mp4`
Then: `python analysis\rank_renders.py eval_A_embedded eval_M2_tight eval_RUST`

- [ ] **Step 4: Check against the M2 row**

Expected (CLAUDE.md M2 row, residual RMS °/s wobble/shake): clean 3.3/4.8, mild 6.7/7.5, severe 12.5/17.0, flicks 17.8/20.3. The eval_RUST row must sit within eval-coupling noise of eval_M2_tight: ±0.5 clean/mild, ±1.0 severe/flicks. If Task 15's e2e gate passed, the two MP4s are near-bit-identical and the numbers should be equal to the first decimal; any larger gap means a real defect upstream — go back, do not rationalize it as render noise.

- [ ] **Step 5: STOP — user signoff**

Present the rank_renders table to the user. The user eyeballs `eval_RUST.mp4` vs `eval_M2_tight.mp4`. **Plan 1 is done only when the user says the numbers and footage look right.** Do not start Plan 2 (Tauri GUI, packaging, CI) before that signoff.

- [ ] **Step 6: Commit + push**

```powershell
git add sample_vids/eval_RUST.gyroflow rust; git commit -m @'
test: acceptance render project for rust pipeline output

Claude-Session: https://claude.ai/code/session_01Y4QB81pia8MGdXyUZKkdQU
'@; git push
```

---

## Definition of done (Plan 1)

- [ ] All committed-fixture unit tests green: `cargo test -p o4core -p o4fix-cli` (no clip needed — this is what CI will run in Plan 2).
- [ ] All clip-gated goldens green: `cargo test -p o4core -- --ignored` (extraction, clean, mp4 gates, optical, fit, patch, splice, e2e).
- [ ] Byte gates green: nullpatch byte-identical, inject round-trip exact (Task 10) — CLAUDE.md says keep them passing, forever.
- [ ] CLI smoke run output matches Python transcript (Task 16).
- [ ] Acceptance render sits on the M2 row and the user signed off (Task 17).
- [ ] `rust/README.md` documents: OpenCV env setup, telemetry-parser pin, the two documented deviations (pipeline reorder, CLI exit codes), and the dev commands.
- [ ] Everything pushed to `ThaumielSparrow/o4fix` main.

Out of scope for this plan (Plan 2, written after signoff): Tauri GUI, job queue + `concurrent_files`, advanced-settings panel with M2/M4 switch, portable packaging, GitHub Actions release CI.
