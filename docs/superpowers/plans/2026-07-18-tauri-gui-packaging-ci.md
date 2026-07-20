# Plan 2: Tauri GUI, Packaging, CI — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship `o4fix-app` (Tauri 2 drop-zone GUI over `o4core`), GitHub Actions CI, and a portable-zip v0.1.0 GitHub Release, plus the debt carried from Plan 1 (fmt, M4/anchor runtime validation, golden-harness restructure).

**Architecture:** Phases prep → CI → GUI → release per `docs/superpowers/specs/2026-07-18-tauri-gui-design.md` (parent: `2026-07-14-rust-refactor-design.md` §5–8). The GUI is a thin Tauri 2 shell: plain HTML/CSS/JS frontend (no Node), a worker-pool queue in Rust calling `o4core::pipeline::process`, progress re-emitted as Tauri events. One additive o4core change: `Progress.pct` + an `on_interval` callback threaded through `optical_patch`.

**Tech Stack:** Rust (stable), Tauri 2 (`tauri = "2"`, `tauri-build = "2"`, `tauri-plugin-dialog = "2"`), serde/serde_json, existing `o4core` (opencv 0.99.0 / OpenCV 4.12.0, pinned telemetry-parser), GitHub Actions `windows-latest`, PowerShell.

## Global Constraints

- **Pins are frozen for this plan**: OpenCV **4.12.0** (`opencv_world4120`), `opencv` crate **0.99.0**, `telemetry-parser` git rev `4abe30846be4da7d4e9dbbb55002f6d9cfd86ae5`. Never bump any of them here.
- **OpenCV env vars must be set inline** in every shell that builds/tests/runs anything touching `o4core` (see `rust/README.md`):
  ```powershell
  $env:OPENCV_INCLUDE_PATHS='C:\opencv\build\include'; $env:OPENCV_LINK_PATHS='C:\opencv\build\x64\vc16\lib'; $env:OPENCV_LINK_LIBS='opencv_world4120'; $env:LIBCLANG_PATH='C:\Program Files\LLVM\bin'; $env:PATH="$env:PATH;C:\opencv\build\x64\vc16\bin;C:\Program Files\LLVM\bin"
  ```
- **Working dir for cargo commands is `rust/`** (the workspace root). Repo root is `C:\Users\lzhan\Desktop\o4prostab`.
- `o4fix.py` and `mp4patch.py` are the **read-only golden reference** — never edit them. `tools/dump_goldens.py` MAY be extended (Task 3).
- **CLI stdout parity**: `o4fix.exe` output must stay identical to `o4fix.py`'s except the two documented deviations (pipeline reorder, exit codes). New `Progress` events carrying only a pct MUST have `message == ""` and the CLI MUST NOT print them.
- **Never `git add`**: `goldens/`, `sample_vids/`, `rust/target/`, `*_fixed.MP4`, `test_gyro.csv`, `.superpowers/`. Check `git status` before every commit.
- **Never overwrite `sample_vids/DJI_20260711124046_0021_D_fixed.MP4`** (Python session-3 reference output). GUI/CLI smoke runs write to the scratchpad dir, never next to the source clip.
- Test clip: `sample_vids/DJI_20260711124046_0021_D.MP4`. Goldens live in untracked `goldens/` (currently present, dumped 2026-07-17); regenerate with `python tools/dump_goldens.py` (~10–20 min) if missing.
- **Byte gates stay green forever**: `nullpatch_is_byte_identical`, `inject_round_trip_exact`, e2e clean zones bit-exact.
- **Subagents must never run long jobs in background shells** (torn down at turn end). Run builds/tests foreground with `timeout` up to 600000 ms; the controller runs anything longer.
- Store Gyroflow / renders are NOT needed anywhere in this plan.
- Commit style: conventional (`feat:`/`fix:`/`test:`/`docs:`/`refactor:`/`ci:`), matching existing history.
- Wherever a command says `<scratchpad>`, substitute the session's scratchpad directory (any local temp dir OUTSIDE the repo works). Scratch MP4s are ~1.7 GB each — delete them when a step says so.

## File Structure

```
.git-blame-ignore-revs                      Task 1  fmt commit hash
.github/actions/setup-opencv/action.yml     Task 4  composite: install+cache OpenCV, env
.github/workflows/ci.yml                    Task 4  fmt + build/clippy/test on push/PR
.github/workflows/release.yml               Task 10 tag → portable zip → GitHub Release
docs/release-checklist.md                   Task 10 pre-tag local gate list
packaging/README.txt                        Task 10 shipped inside the zip
tools/make_icon.py                          Task 6  generates rust/o4fix-app/icons/icon.ico
tools/dump_goldens.py                       Task 3  MODIFY: + ref_fixed_m4.MP4, --m4-only
rust/o4core/tests/common/mod.rs             Task 2  merged helpers (replaces helpers.rs + helper half of golden_telemetry.rs)
rust/o4core/src/pipeline.rs                 Task 5  MODIFY: Progress.pct, optical pct plumbing
rust/o4core/src/patch.rs                    Task 5  MODIFY: optical_patch gains on_interval
rust/o4fix-cli/src/main.rs                  Task 5  MODIFY: skip empty-message progress
rust/o4fix-app/Cargo.toml                   Task 6
rust/o4fix-app/build.rs                     Task 6
rust/o4fix-app/tauri.conf.json              Task 6
rust/o4fix-app/capabilities/default.json    Task 6
rust/o4fix-app/icons/icon.ico               Task 6  (committed binary, generated once)
rust/o4fix-app/src/main.rs                  Task 6  webview2 check + builder; Task 7 wires commands
rust/o4fix-app/src/settings.rs              Task 7  ConfigDto/GuiSettings + load/save
rust/o4fix-app/src/queue.rs                 Task 7  worker pool, events, catch_unwind
rust/o4fix-app/ui/index.html                Task 8
rust/o4fix-app/ui/style.css                 Task 8
rust/o4fix-app/ui/app.js                    Task 8
rust/o4fix-app/ui/help.js                   Task 8  tooltip texts (verbatim from o4fix.py argparse)
rust/Cargo.toml                             Task 6  MODIFY: members += "o4fix-app"
rust/README.md                              Tasks 1,3,6,11 MODIFY
README.md                                   Task 11 MODIFY: pilot-facing
CLAUDE.md                                   Task 11 MODIFY: one-line status update
```

---

### Task 1: Workspace `cargo fmt` + blame-ignore

**Files:**
- Modify: everything under `rust/` that rustfmt touches (mechanical only)
- Create: `.git-blame-ignore-revs` (repo root)
- Modify: `rust/README.md` (blame note)

**Interfaces:** none (no behavior change; later tasks assume `cargo fmt --check` passes).

- [ ] **Step 1: Branch, confirm clean tree, then format**

```powershell
git checkout -b tauri-gui   # all Plan 2 work lands here; merge to main in Task 11
git status --porcelain      # expect: empty (or only untracked junk listed in Global Constraints)
cd rust; cargo fmt
```

- [ ] **Step 2: Verify no semantic change + fmt now passes**

```powershell
cd rust; cargo fmt --check   # expect: exit 0, no output
# fast suite (needs OpenCV env inline, see Global Constraints):
cargo test -p o4core -p o4fix-cli
```
Expected: all unit tests pass, 0 failures (same count as before the fmt: run `cargo test -p o4core -p o4fix-cli 2>&1 | Select-String "test result"` before and after if in doubt).

- [ ] **Step 3: Commit the reformat alone**

```powershell
git add rust
git status   # verify ONLY rust/*.rs (and possibly Cargo.toml whitespace) staged
git commit -m "style: cargo fmt across workspace (mechanical, no behavior change)"
```

- [ ] **Step 4: Create `.git-blame-ignore-revs` + README note**

```powershell
git rev-parse HEAD   # call this <FMT_SHA>
```
Create `.git-blame-ignore-revs` at repo root:
```
# mechanical cargo fmt of the Plan 1 verbatim-transcribed port (Plan 2 Task 1)
<FMT_SHA>
```
Append to `rust/README.md` under "Dev commands":
```markdown
### Formatting

The workspace is rustfmt-clean and CI enforces `cargo fmt --check`.
The initial mechanical reformat is listed in `.git-blame-ignore-revs`;
run `git config blame.ignoreRevsFile .git-blame-ignore-revs` once locally
so `git blame` skips it.
```

- [ ] **Step 5: Commit**

```powershell
git add .git-blame-ignore-revs rust/README.md
git commit -m "chore: blame-ignore the fmt commit; document ignoreRevsFile"
```

---

### Task 2: Golden-test harness → `tests/common/mod.rs`

Kills the `#[path = "..."] mod gt;` cross-include hack (each `tests/*.rs` is its own binary crate, so shared helpers included by `#[path]` produce dead-code warnings and duplicate test runs — `extraction_matches_python` currently compiles into 6 binaries). `tests/common/mod.rs` is the Cargo convention: a subdirectory module is NOT compiled as a test binary.

**Files:**
- Create: `rust/o4core/tests/common/mod.rs`
- Delete: `rust/o4core/tests/helpers.rs`
- Modify: `rust/o4core/tests/{golden_telemetry,golden_detect,golden_mp4,golden_optical,golden_patch,golden_splice,e2e,quat_fixtures,dsp_kernels,dsp_filters}.rs` (include lines only)

**Interfaces:**
- Produces: `common::{repo, npz, npz_extract, fx, col, rows3, rows4, close, assert_close}` — exact bodies moved from `tests/helpers.rs` and `tests/golden_telemetry.rs:5-23`, signatures unchanged. Task 3's new e2e test uses `common` via the same `use self::common as gt;` alias.

- [ ] **Step 1: Create `rust/o4core/tests/common/mod.rs`**

Content = concatenation, verbatim bodies (only attributes/docs adjusted):

```rust
//! Shared test helpers. Lives in a subdirectory so Cargo does NOT compile
//! it as its own test binary; each tests/*.rs declares `mod common;`.
#![allow(dead_code)] // each test binary compiles its own copy and uses a subset
use ndarray::{Array1, Array2};
use ndarray_npy::NpzReader;
use std::fs::File;

pub fn repo(p: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../").join(p)
}
pub fn npz(name: &str) -> NpzReader<File> {
    NpzReader::new(File::open(repo(&format!("goldens/{name}"))).unwrap()).unwrap()
}
pub fn npz_extract() -> (Vec<f64>, Vec<[f64; 4]>) {
    let mut z = npz("extract.npz");
    let t: Array1<f64> = z.by_name("t").unwrap();
    let q: Array2<f64> = z.by_name("q").unwrap();
    let t_vec = t.to_vec();
    let q_vec: Vec<[f64; 4]> = (0..q.nrows())
        .map(|i| [q[[i, 0]], q[[i, 1]], q[[i, 2]], q[[i, 3]]])
        .collect();
    (t_vec, q_vec)
}
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
Run `cargo fmt` after creating (Task 1 made the tree fmt-clean; keep it that way in every task).

- [ ] **Step 2: Rewrite include lines in all ten test files**

Minimal-diff aliasing (edition-2018 `use` needs `self::`):
- In `golden_detect.rs`, `golden_mp4.rs`, `golden_optical.rs`, `golden_patch.rs`, `golden_splice.rs`, `e2e.rs` replace the first line `#[path = "golden_telemetry.rs"] mod gt;` (with any trailing comment) with:
  ```rust
  mod common;
  use self::common as gt;
  ```
- In `quat_fixtures.rs`, `dsp_kernels.rs`, `dsp_filters.rs` replace `#[path = "helpers.rs"] mod helpers;` with:
  ```rust
  mod common;
  use self::common as helpers;
  ```
- `golden_telemetry.rs`: delete lines 1–23 (the imports + `repo`/`npz`/`npz_extract` helpers and the `#[allow(dead_code)]` block), keep only the `extraction_matches_python` test, and prepend:
  ```rust
  mod common;
  use self::common::{npz, repo};
  use ndarray::{Array1, Array2};
  ```
- Delete `rust/o4core/tests/helpers.rs`.

- [ ] **Step 3: Verify: compile everything, run fast + one cheap golden**

```powershell
cd rust  # env vars inline
cargo test -p o4core --no-run          # compiles ALL test binaries incl. --ignored ones
cargo test -p o4core -p o4fix-cli      # fast suite: expect all green, ZERO dead_code warnings
cargo test -p o4core --test golden_telemetry -- --ignored   # extraction golden, ~2 min
```
Expected: all pass; warning count for o4core/o4fix-cli is zero (the old `npz_extract is never used` warning is gone). `extraction_matches_python` now runs in exactly ONE binary.

- [ ] **Step 4: Commit**

```powershell
git add rust/o4core/tests
git commit -m "refactor: golden-test helpers into tests/common (drop #[path] includes)"
```

---

### Task 3: M4 seeded golden + permanent e2e test; anchor one-off smoke

The M4 (`fast_wide_cutoff 16`) and anchor-mode branches were statically verified in Plan 1 but never executed in Rust. This task runtime-validates both against seeded Python. M4 becomes a permanent clip-gated test; anchor is a documented one-off (off-by-default, experimental).

**Files:**
- Modify: `tools/dump_goldens.py`
- Modify: `rust/o4core/tests/e2e.rs` (new test)
- Modify: `rust/README.md` (goldens note)
- Scratchpad only (not committed): `smoke_anchor.py`

**Interfaces:**
- Consumes: `common` helpers via `use self::common as gt;` (Task 2); existing `stream()` helper in `e2e.rs`; `o4core::config::Config::m4()` (exists: `config.rs:47-49`, = `fast_wide_cutoff: 16.0` over defaults).
- Produces: `goldens/ref_fixed_m4.MP4` (untracked, regenerable); test `e2e_m4_matches_seeded_python_reference`.

- [ ] **Step 1: Extend `tools/dump_goldens.py`**

Add an `--m4-only` CLI flag and an M4 section. Edits:

1. Replace the `def build_args():` signature line and body reference with a parameterized version:
```python
def build_args(fast_wide_cutoff=0.0):
    return argparse.Namespace(
        output=None, gcsv=False, severe=8.0, severe_pad=0.2, severe_merge=0.2,
        ramp=0.3, plot=False, orientation="XYZ", light_cutoff=25.0,
        strong_cutoff=2.5, noise_low=1.5, noise_high=5.0,
        noise_band=[30.0, 180.0], noise_window=100.0, hampel_window=7,
        hampel_sigma=6.0, lpf=None, no_optical=False, optical_cutoff=8.0,
        fast_handback=[100.0, 250.0], patch_pad=0.5, patch_merge=1.0,
        optical_noise=None, handback_cutoff=None,
        fast_wide_cutoff=fast_wide_cutoff,
        fast_wide_ramp=[150.0, 300.0], fast_wide_accel=1500.0,
        anchor_mode=False, anchor_cutoff=1.5)
```
2. In `main()`, before `GOLD.mkdir`, parse the flag:
```python
    ap = argparse.ArgumentParser()
    ap.add_argument("--m4-only", action="store_true",
                    help="only (re)generate goldens/ref_fixed_m4.MP4 (~10 min); "
                         "skips the M2 stage dumps")
    opts = ap.parse_args()
```
3. Wrap the entire existing M2 body of `main()` (from `t, q, meta = o4fix.extract_quats(VIDEO)` through the `slots.npz` dump) in `if not opts.m4_only:`, and hoist a shared prelude so the M4 section can run standalone. Final structure of `main()`:
```python
def main():
    GOLD.mkdir(exist_ok=True)
    opts = ...            # as above
    args = build_args()
    t, q, meta = o4fix.extract_quats(VIDEO)
    fs = 1.0 / np.median(np.diff(t))
    tm, omega = o4fix.quats_to_rates(t, q)
    clean, diag = o4fix.adaptive_clean(omega, fs, args)
    severe = o4fix.find_intervals(diag["noise"] > args.severe, tm,
                                  args.severe_pad, args.severe_merge, 0.2)
    o4fix.video_rates = seeded_video_rates      # monkeypatch once, both modes

    if not opts.m4_only:
        # ... the existing dumps, UNCHANGED, minus the lines now in the
        # prelude above (extract/fs/tm-omega/clean-diag/severe/monkeypatch);
        # note `noisy`, `calib_all`/`calib`, optical dumps, fit, patched,
        # splice, ref_fixed.MP4 inject, slots dump all stay byte-identical
        # in behavior — pure code motion.
        ...

    # SEEDED M4 reference (Plan 2 Task 3): same clip, fast_wide_cutoff=16
    args_m4 = build_args(fast_wide_cutoff=16.0)
    patched_m4 = o4fix.optical_patch(VIDEO, tm, clean, diag, fs, args_m4, meta)
    q_out_m4, _ = o4fix.splice_orientation(t, q, patched_m4, severe, args_m4.ramp)
    ok = mp4patch.inject_and_check(str(VIDEO), q_out_m4,
                                   str(GOLD / "ref_fixed_m4.MP4"))
    assert ok, "python M4 reference round-trip failed"
    print("goldens written to", GOLD)
```
The code motion MUST NOT change what the M2 dumps contain — `adaptive_clean`/`find_intervals`/monkeypatch order is preserved exactly as in the current file (compare against the current `main()` line by line; the only behavioral addition is the M4 section).

- [ ] **Step 2: Generate the M4 reference**

```powershell
cd C:\Users\lzhan\Desktop\o4prostab
python tools/dump_goldens.py --m4-only    # ~8-12 min (optical runs once)
```
Expected: prints the optical-patch log lines then `goldens written to ...`; `goldens/ref_fixed_m4.MP4` exists, ~1.7 GB. Existing M2 goldens untouched (check `git status` irrelevant — untracked — but verify `goldens/ref_fixed.MP4` mtime unchanged).

- [ ] **Step 3: Guard against M2-dump drift (regression check on the refactor)**

Because Step 1 moved code, prove the M2 path still produces identical goldens without waiting 20 min: the cheap deterministic stages suffice. Write `<scratchpad>\check_m2_refactor.py`:

```python
import sys
import numpy as np
ROOT = r"C:\Users\lzhan\Desktop\o4prostab"
sys.path.insert(0, ROOT)
sys.path.insert(0, ROOT + r"\tools")
import o4fix
from dump_goldens import build_args, VIDEO

z = np.load(ROOT + r"\goldens\clean.npz")
args = build_args()
t, q, meta = o4fix.extract_quats(VIDEO)
fs = 1.0 / np.median(np.diff(t))
tm, om = o4fix.quats_to_rates(t, q)
clean, diag = o4fix.adaptive_clean(om, fs, args)
assert np.array_equal(z["cleaned"], clean) and np.array_equal(z["noise"], diag["noise"])
print("M2 clean-stage outputs identical after refactor")
```
```powershell
python <scratchpad>\check_m2_refactor.py
```
Expected: `M2 clean-stage outputs identical after refactor`. (If importing `dump_goldens` runs argparse at import time, that's a Step 1 bug — the parser must stay INSIDE `main()`; module import stays side-effect-free.)

- [ ] **Step 4: Add the permanent Rust M4 e2e test**

Append to `rust/o4core/tests/e2e.rs`:

```rust
#[test]
#[ignore] // ~10 min: full pipeline incl. optical, M4 profile
fn e2e_m4_matches_seeded_python_reference() {
    let out = std::env::temp_dir().join("o4fix_e2e_m4_test.MP4");
    let _ = std::fs::remove_file(&out);
    let r = run(&Config::m4(), &out).unwrap();
    let Outcome::Repaired { bursts, .. } = r else { panic!("expected Repaired") };
    assert!(!bursts.is_empty());
    let (t_r, q_r) = stream(&out);
    let (t_p, q_p) = stream(&gt::repo("goldens/ref_fixed_m4.MP4"));
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
    std::fs::remove_file(&out).ok();
}
```
(Same tolerances/severe-window logic as the existing M2 e2e — the severe intervals are config-identical between M2 and M4, so reusing `intervals.npz` is correct: `fast_wide_cutoff` only changes handback content inside patches, not interval detection.)

- [ ] **Step 5: Run it**

```powershell
cd rust  # env vars inline
cargo test -p o4core --test e2e e2e_m4 -- --ignored --nocapture   # ~10 min
```
Expected: PASS. The transcript's optical/splice lines will differ slightly from the M2 run (fast-wide handback active) — that's the point: the branch executed.

- [ ] **Step 6: Anchor one-off smoke (not committed)**

Write to scratchpad `smoke_anchor.py` (adapt ROOT if scratchpad differs):

```python
"""One-off: seeded Python anchor-mode reference vs Rust CLI --anchor-mode."""
import sys, numpy as np
from pathlib import Path
ROOT = Path(r"C:\Users\lzhan\Desktop\o4prostab")
sys.path.insert(0, str(ROOT)); sys.path.insert(0, str(ROOT / "tools"))
import o4fix, mp4patch
from dump_goldens import build_args, seeded_video_rates, VIDEO

OUT = Path(sys.argv[1])          # e.g. <scratchpad>/anchor_py.MP4
args = build_args(); args.anchor_mode = True
t, q, meta = o4fix.extract_quats(VIDEO)
fs = 1.0/np.median(np.diff(t)); tm, om = o4fix.quats_to_rates(t, q)
clean, diag = o4fix.adaptive_clean(om, fs, args)
severe = o4fix.find_intervals(diag["noise"] > args.severe, tm,
                              args.severe_pad, args.severe_merge, 0.2)
o4fix.video_rates = seeded_video_rates
patched = o4fix.optical_patch(VIDEO, tm, clean, diag, fs, args, meta)
q_out, _ = o4fix.splice_orientation(t, q, patched, severe, args.ramp)
assert mp4patch.inject_and_check(str(VIDEO), q_out, str(OUT))
print("python anchor reference written:", OUT)
```

Also write `<scratchpad>\compare_quats.py` (reused by anyone re-running this smoke):

```python
import sys
import numpy as np
sys.path.insert(0, r"C:\Users\lzhan\Desktop\o4prostab")
import o4fix

t1, q1, _ = o4fix.extract_quats(sys.argv[1])
t2, q2, _ = o4fix.extract_quats(sys.argv[2])
assert np.array_equal(t1, t2), "timestamps differ"
d = np.minimum(np.abs(q1 - q2).max(1), np.abs(q1 + q2).max(1))
print("max sign-folded quat delta:", d.max())
assert d.max() <= 1e-6
```

Run both sides and compare (Rust optical is always seeded with the same per-frame scheme, so outputs are directly comparable):
```powershell
python <scratchpad>\smoke_anchor.py <scratchpad>\anchor_py.MP4          # ~10 min
cd rust   # env vars inline
cargo build -p o4fix-cli --release
.\target\release\o4fix.exe ..\sample_vids\DJI_20260711124046_0021_D.MP4 --anchor-mode -o <scratchpad>\anchor_rs.MP4
python <scratchpad>\compare_quats.py <scratchpad>\anchor_py.MP4 <scratchpad>\anchor_rs.MP4
```
Expected: `max sign-folded quat delta:` ≤ 1e-6. Paste the number into the task report. Delete both ~1.7 GB scratch MP4s afterwards.

- [ ] **Step 7: Document + commit**

Append to `rust/README.md` "Goldens" section: `dump_goldens.py --m4-only regenerates only goldens/ref_fixed_m4.MP4 (M4-profile e2e reference; the full run now writes both).`

```powershell
git add tools/dump_goldens.py rust/o4core/tests/e2e.rs rust/README.md
git commit -m "test: seeded M4 e2e golden + anchor-mode parity smoke (branches now runtime-verified)"
```

---

### Task 4: GitHub Actions CI

**Files:**
- Create: `.github/actions/setup-opencv/action.yml`
- Create: `.github/workflows/ci.yml`

**Interfaces:**
- Produces: composite action `./.github/actions/setup-opencv` (no inputs; installs cached OpenCV 4.12.0 to `C:\opencv`, exports `OPENCV_*`/`LIBCLANG_PATH` env + PATH entries). Task 10's release workflow reuses it verbatim.

- [ ] **Step 1: Local clippy dry-run (CI will gate on it)**

```powershell
cd rust  # env vars inline
cargo clippy --workspace --all-targets -- -D warnings
```
Expected: probably a handful of pre-existing style lints in test files (e.g. `needless_range_loop` in golden tests where loops mirror numpy indexing). Fix trivially-fixable ones; where the loop shape deliberately mirrors the Python reference, add a targeted `#![allow(clippy::needless_range_loop)]` (or the specific lint) at the top of that test file with a one-line comment `// mirrors numpy indexing in the golden reference`. Re-run until exit 0. Do NOT blanket-allow at workspace level.

- [ ] **Step 2: Write the composite action**

`.github/actions/setup-opencv/action.yml`:
```yaml
name: Setup OpenCV 4.12.0
description: Cached prebuilt OpenCV 4.12.0 for the opencv crate, plus libclang env
runs:
  using: composite
  steps:
    - name: Cache OpenCV
      id: cache-opencv
      uses: actions/cache@v4
      with:
        path: C:\opencv
        key: opencv-4.12.0-windows
    - name: Download and extract OpenCV
      if: steps.cache-opencv.outputs.cache-hit != 'true'
      shell: pwsh
      run: |
        curl.exe -L -o "$env:TEMP\opencv-4.12.0-windows.exe" https://github.com/opencv/opencv/releases/download/4.12.0/opencv-4.12.0-windows.exe
        & "$env:TEMP\opencv-4.12.0-windows.exe" -oC:\ -y
        # self-extractor returns before files are flushed (rust/README.md gotcha): poll
        $dll = 'C:\opencv\build\x64\vc16\bin\opencv_world4120.dll'
        for ($i = 0; $i -lt 60 -and -not (Test-Path $dll); $i++) { Start-Sleep 2 }
        if (-not (Test-Path $dll)) { throw 'OpenCV extraction incomplete' }
    - name: Export build env
      shell: pwsh
      run: |
        Add-Content $env:GITHUB_ENV 'OPENCV_INCLUDE_PATHS=C:\opencv\build\include'
        Add-Content $env:GITHUB_ENV 'OPENCV_LINK_PATHS=C:\opencv\build\x64\vc16\lib'
        Add-Content $env:GITHUB_ENV 'OPENCV_LINK_LIBS=opencv_world4120'
        Add-Content $env:GITHUB_ENV 'LIBCLANG_PATH=C:\Program Files\LLVM\bin'
        Add-Content $env:GITHUB_PATH 'C:\opencv\build\x64\vc16\bin'
        Add-Content $env:GITHUB_PATH 'C:\Program Files\LLVM\bin'
```
(`windows-latest` images ship LLVM at `C:\Program Files\LLVM`. If the CI log later shows the `0xc0000135` STATUS_DLL_NOT_FOUND build-script failure from `rust/README.md` gotcha #1, add a step here: `choco install llvm -y --no-progress` before "Export build env" — do not guess in advance.)

- [ ] **Step 3: Write the CI workflow**

`.github/workflows/ci.yml`:
```yaml
name: CI
on: [push]          # all branches; solo repo, no PR duplication concerns
jobs:
  fmt:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt
      - run: cargo fmt --check
        working-directory: rust
  test:
    runs-on: windows-latest
    timeout-minutes: 45
    steps:
      - uses: actions/checkout@v4
      - uses: ./.github/actions/setup-opencv
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: rust
      - name: Clippy
        run: cargo clippy --workspace --all-targets -- -D warnings
        working-directory: rust
      - name: Unit tests
        run: cargo test -p o4core -p o4fix-cli
        working-directory: rust
```
Notes: `cargo test` without `--ignored` runs only the fixture-based layer-1 tests — no clip, no goldens needed on CI. The `nom v6.1.2` future-incompat notice (transitive from pinned telemetry-parser) is informational, not a failure. When Task 6 adds `o4fix-app` to the workspace, `--workspace` clippy picks it up automatically; `cargo test` package list stays as-is (the app's tests are added to this line in Task 7).

- [ ] **Step 4: Commit, push, watch**

```powershell
git add .github rust   # rust: any clippy-fix edits from Step 1
git commit -m "ci: fmt + clippy + unit tests on windows-latest with cached OpenCV"
git push -u origin HEAD
gh run list --limit 3
gh run watch   # pick the newest run id if prompted
```
Expected: `fmt` green in <1 min; `test` green — cold ~20–30 min (OpenCV download + full build), warm re-run ≤ ~8 min. If the opencv crate's build script fails, diagnose against the two gotchas in `rust/README.md` (libclang PATH, extractor flush) before changing pins — do not bump any versions.

- [ ] **Step 5: Confirm cache effectiveness**

Push a trivial follow-up commit (e.g. the Task 5 work, or amend nothing and re-run via `gh run rerun <id>`), verify the `Cache OpenCV` step reports a hit and total wall time drops. Record cold/warm times in the task report (success criterion: warm ≤ ~5–8 min).

---

### Task 5: `Progress.pct` + optical interval progress (additive)

The optical stage is minutes-long; the GUI needs in-stage progress. `optical::video_rates` already takes `on_interval: &(dyn Fn(usize, usize) + Sync)` — `optical_patch` currently discards it with `&|_, _| ()` at `patch.rs:59` (calib) and `patch.rs:72` (noisy). Thread it out to the pipeline and put a `pct` on every `Progress`.

**Files:**
- Modify: `rust/o4core/src/patch.rs` (enum + signature + 2 call sites)
- Modify: `rust/o4core/src/pipeline.rs` (Progress struct, say(), anchors, optical_pct)
- Modify: `rust/o4fix-cli/src/main.rs` (skip empty messages)
- Modify: `rust/o4core/tests/golden_patch.rs`, `rust/o4core/tests/e2e.rs` (call-site updates)

**Interfaces:**
- Produces (Task 7 consumes):
  - `pipeline::Progress { pub stage: Stage, pub message: String, pub pct: f64 }` — `pct` is the overall job fraction [0,1]; events with `message.is_empty()` are pct-only ticks (CLI ignores them, GUI uses them for the bar).
  - `patch::OptPhase { Calib, Noisy }` (Copy, Clone, Debug).
  - `pipeline::optical_pct(phase: OptPhase, done: usize, total: usize) -> f64`.
  - `pipeline::process` signature UNCHANGED.
  - Stage→pct anchors: Extract line 0.05, Analyze 0.08, burst-count 0.10, optical Calib 0.10→0.30, Noisy 0.30→0.85, Splice 0.87, Write logs 0.92, final/healthy 1.0.

- [ ] **Step 1: Capture the baseline CLI transcript**

```powershell
cd rust  # env vars inline
cargo build -p o4fix-cli --release
.\target\release\o4fix.exe ..\sample_vids\DJI_20260711124046_0021_D.MP4 -o <scratchpad>\pct_base.MP4 > <scratchpad>\transcript_base.txt
```
Expected: exit 0, ~5 min, transcript matches the known session-3 numbers (31 bursts / 52.1 s, R2=0.995 / -4 ms, 124551/176372 unchanged).

- [ ] **Step 2: patch.rs — OptPhase + on_interval**

Add near the top of `patch.rs`:
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptPhase { Calib, Noisy }
```
Change `optical_patch`'s signature: insert `on_interval: &(dyn Fn(OptPhase, usize, usize) + Sync),` immediately after the `log` parameter. Replace the two discard closures:
- `patch.rs:59`: `&|_, _| ()` → `&|d, n| on_interval(OptPhase::Calib, d, n)`
- `patch.rs:72`: `&|_, _| ()` → `&|d, n| on_interval(OptPhase::Noisy, d, n)`

- [ ] **Step 3: pipeline.rs — pct everywhere**

```rust
#[derive(Clone, Debug)]
pub struct Progress {
    pub stage: Stage,
    pub message: String,
    /// Overall job fraction [0,1]. Events with an empty `message` are
    /// pct-only ticks: CLI skips them, GUI drives the bar with them.
    pub pct: f64,
}

/// Optical-stage share of overall progress: calib intervals 0.10-0.30,
/// noisy intervals 0.30-0.85 (optical dominates wall time).
pub fn optical_pct(phase: crate::patch::OptPhase, done: usize, total: usize) -> f64 {
    let f = done as f64 / total.max(1) as f64;
    match phase {
        crate::patch::OptPhase::Calib => 0.10 + 0.20 * f,
        crate::patch::OptPhase::Noisy => 0.30 + 0.55 * f,
    }
}
```
In `process()`: `let say = |stage: Stage, pct: f64, message: String| on_progress(Progress { stage, message, pct });` and update every call site with the anchor values from Interfaces (healthy short-circuit line = 1.0). For the optical stage:
```rust
    check()?;
    let opt_pct = std::sync::atomic::AtomicU64::new(0.10f64.to_bits());
    let log = |s: &str| say(Stage::Optical,
                            f64::from_bits(opt_pct.load(Ordering::Relaxed)),
                            s.to_string());
    let on_interval = |ph, d, n| {
        let p = optical_pct(ph, d, n);
        opt_pct.store(p.to_bits(), Ordering::Relaxed);
        say(Stage::Optical, p, String::new());          // pct-only tick
    };
    let patched = patch::optical_patch(video, &tm, &cleaned, &diag, fs, cfg,
                                       &tel.meta, &log, &on_interval, cancel)?;
```
Splice loop lines: pct 0.87. `wlog` closure: 0.92. Final "wrote ..." line: 1.0. Add a unit test next to `median_matches_numpy`:
```rust
    #[test]
    fn optical_pct_anchors() {
        use crate::patch::OptPhase::*;
        assert!((optical_pct(Calib, 0, 4) - 0.10).abs() < 1e-12);
        assert!((optical_pct(Calib, 4, 4) - 0.30).abs() < 1e-12);
        assert!((optical_pct(Noisy, 0, 23) - 0.30).abs() < 1e-12);
        assert!((optical_pct(Noisy, 23, 23) - 0.85).abs() < 1e-12);
        assert!(optical_pct(Noisy, 0, 0).is_finite()); // total=0 guarded by max(1)
        assert!(optical_pct(Calib, 1, 4) < optical_pct(Calib, 2, 4)); // monotone
    }
```

- [ ] **Step 4: Update the three consumers**

- `o4fix-cli/src/main.rs`: `&|p| println!("{}", p.message)` → `&|p| if !p.message.is_empty() { println!("{}", p.message) }`.
- `tests/e2e.rs` `run()`: same guard on its print closure.
- `tests/golden_patch.rs`: the direct `optical_patch(...)` call gains `&|_, _, _| ()` in the new parameter position.

- [ ] **Step 5: Verify — compile, fast suite, transcript parity**

```powershell
cd rust  # env vars inline
cargo test -p o4core --no-run
cargo test -p o4core -p o4fix-cli          # incl. new optical_pct_anchors
cargo build -p o4fix-cli --release
.\target\release\o4fix.exe ..\sample_vids\DJI_20260711124046_0021_D.MP4 -o <scratchpad>\pct_new.MP4 > <scratchpad>\transcript_new.txt
fc <scratchpad>\transcript_base.txt <scratchpad>\transcript_new.txt
fc /b <scratchpad>\pct_base.MP4 <scratchpad>\pct_new.MP4
```
Expected: both `fc` report no differences — stdout byte-identical (pct-only ticks filtered), output MP4 byte-identical (pure plumbing). Delete the two scratch MP4s.

- [ ] **Step 6: Commit**

```powershell
git add rust
git commit -m "feat: Progress.pct with optical interval ticks (CLI output unchanged)"
git push
gh run watch   # CI from Task 4 must stay green
```

---

### Task 6: `o4fix-app` scaffold (Tauri 2 window boots)

**Files:**
- Create: `tools/make_icon.py`, `rust/o4fix-app/icons/icon.ico` (generated, committed)
- Create: `rust/o4fix-app/{Cargo.toml,build.rs,tauri.conf.json,capabilities/default.json,src/main.rs,ui/index.html}`
- Modify: `rust/Cargo.toml` (workspace members)

**Interfaces:**
- Produces: bootable `o4fix-app.exe` shell. Task 7 replaces the `tauri::Builder` line with `.manage(...)`/`.invoke_handler(...)`; Task 8 replaces `ui/index.html`.

- [ ] **Step 1: Generate the icon (tauri-build on Windows requires an .ico)**

`tools/make_icon.py`:
```python
"""One-off: generate rust/o4fix-app/icons/icon.ico (output is committed)."""
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

out = Path(__file__).resolve().parents[1] / "rust/o4fix-app/icons/icon.ico"
out.parent.mkdir(parents=True, exist_ok=True)
img = Image.new("RGBA", (256, 256), (0, 0, 0, 0))
d = ImageDraw.Draw(img)
d.rounded_rectangle([8, 8, 248, 248], radius=48, fill=(18, 22, 30, 255),
                    outline=(90, 200, 250, 255), width=8)
try:
    big = ImageFont.truetype("arialbd.ttf", 110)
    small = ImageFont.truetype("arial.ttf", 54)
except OSError:
    big = small = ImageFont.load_default()
d.text((128, 112), "O4", font=big, anchor="mm", fill=(90, 200, 250, 255))
d.text((128, 196), "fix", font=small, anchor="mm", fill=(230, 235, 240, 255))
img.save(out, sizes=[(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
print("wrote", out)
```
```powershell
pip install pillow
python tools/make_icon.py    # expect: wrote ...icon.ico
```

- [ ] **Step 2: Workspace + crate files**

`rust/Cargo.toml`: `members = ["o4core", "o4fix-cli", "o4fix-app"]`.

`rust/o4fix-app/Cargo.toml`:
```toml
[package]
name = "o4fix-app"
version = "0.1.0"
edition = "2021"

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
o4core = { path = "../o4core" }
tauri = { version = "2", features = [] }
tauri-plugin-dialog = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
rfd = "0.15"
```

`rust/o4fix-app/build.rs`:
```rust
fn main() {
    tauri_build::build()
}
```

`rust/o4fix-app/tauri.conf.json`:
```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "o4fix",
  "version": "0.1.0",
  "identifier": "com.thaumielsparrow.o4fix",
  "build": { "frontendDist": "ui" },
  "app": {
    "withGlobalTauri": true,
    "windows": [
      {
        "title": "o4fix — DJI O4 Pro gyro repair",
        "width": 920,
        "height": 680,
        "minWidth": 720,
        "minHeight": 520,
        "dragDropEnabled": true
      }
    ],
    "security": { "csp": null }
  },
  "bundle": { "active": false, "icon": ["icons/icon.ico"] }
}
```

`rust/o4fix-app/capabilities/default.json`:
```json
{
  "identifier": "default",
  "description": "o4fix main window",
  "windows": ["main"],
  "permissions": ["core:default", "dialog:default"]
}
```

`rust/o4fix-app/src/main.rs`:
```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Portable zip = no installer bootstrapper; spec: runtime check + link.
    if tauri::webview_version().is_err() {
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("o4fix — WebView2 required")
            .set_description(
                "Microsoft WebView2 Runtime was not found.\n\n\
                 Install it from:\n\
                 https://developer.microsoft.com/microsoft-edge/webview2/\n\
                 then run o4fix again. (Windows 11 includes it by default.)")
            .show();
        return;
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .run(tauri::generate_context!())
        .expect("error while running o4fix");
}
```

`rust/o4fix-app/ui/index.html` (placeholder until Task 8):
```html
<!doctype html>
<meta charset="utf-8">
<title>o4fix</title>
<body style="background:#12161e;color:#e6ebf0;font-family:sans-serif">
  <h1>o4fix scaffold OK</h1>
</body>
```

- [ ] **Step 3: Boot it**

```powershell
cd rust  # env vars inline (o4core dependency needs OpenCV)
cargo run -p o4fix-app
```
Expected: first build several minutes; a 920×680 window titled "o4fix — DJI O4 Pro gyro repair" opens showing "o4fix scaffold OK". Close it (exit 0). If `tauri_build` errors about a missing icon, the icon path in `tauri.conf.json` `bundle.icon` vs `icons/icon.ico` on disk is the first thing to check.

- [ ] **Step 4: Lint + commit**

```powershell
cargo clippy -p o4fix-app -- -D warnings   # env vars inline
cargo fmt --check
git add rust/Cargo.toml rust/o4fix-app tools/make_icon.py
git status   # icons/icon.ico must be staged; nothing from target/
git commit -m "feat: o4fix-app Tauri 2 scaffold with WebView2 check"
git push
gh run watch   # CI: clippy --workspace now covers o4fix-app; must be green
```

---

### Task 7: Queue backend, settings persistence, Tauri commands

**Files:**
- Modify: `rust/o4core/src/config.rs` (derive `PartialEq` — one line)
- Create: `rust/o4fix-app/src/settings.rs`
- Create: `rust/o4fix-app/src/queue.rs`
- Modify: `rust/o4fix-app/src/main.rs` (manage + handlers)
- Modify: `.github/workflows/ci.yml` (test package list)

**Interfaces:**
- Consumes: `pipeline::process` (unchanged signature), `Progress { stage, message, pct }` from Task 5, `Config`/`Config::m4()` from `o4core::config`.
- Produces (Task 8's JS calls these — names are load-bearing):
  - Commands: `start_queue(files: Vec<String>, settings: GuiSettings) -> Vec<u64>`, `cancel_job(id: u64)`, `pick_files() -> Vec<String>`, `load_settings() -> GuiSettings`, `save_settings(settings: GuiSettings)`.
  - Events: `job_progress {id, file, stage, pct, detail}` (stage ∈ "analyzing"|"measuring motion"|"patching"|"verifying"), `job_log {id, line}`, `job_done {id, status, message}` (status ∈ "done"|"healthy"|"error"|"cancelled"; message = output path for "done", human text otherwise).
  - `GuiSettings { profile: String ("m2"|"m4"|"custom"), config: ConfigDto, output_dir: Option<String>, concurrent_files: usize }`; `ConfigDto` field names = CLI flag names with underscores (`noise_window`, not `noise_window_ms`), tuples as 2-arrays, Options nullable.

- [ ] **Step 1: `Config` gets `PartialEq`**

`rust/o4core/src/config.rs:2`: `#[derive(Clone, Debug)]` → `#[derive(Clone, Debug, PartialEq)]`. (Needed for DTO round-trip assertions; additive.)

- [ ] **Step 2: Write `settings.rs` (tests included in-file)**

```rust
use o4core::config::Config;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// JSON mirror of o4core::config::Config. Field names follow the CLI flags
/// (o4fix-cli/src/args.rs), so `noise_window` here maps to
/// `Config.noise_window_ms`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ConfigDto {
    pub severe: f64,
    pub severe_pad: f64,
    pub severe_merge: f64,
    pub ramp: f64,
    pub light_cutoff: f64,
    pub strong_cutoff: f64,
    pub noise_low: f64,
    pub noise_high: f64,
    pub noise_band: [f64; 2],
    pub noise_window: f64,
    pub hampel_window: usize,
    pub hampel_sigma: f64,
    pub optical_cutoff: f64,
    pub handback_cutoff: Option<f64>,
    pub fast_handback: [f64; 2],
    pub patch_pad: f64,
    pub patch_merge: f64,
    pub optical_noise: Option<[f64; 2]>,
    pub fast_wide_cutoff: f64,
    pub fast_wide_ramp: [f64; 2],
    pub fast_wide_accel: f64,
    pub anchor_mode: bool,
    pub anchor_cutoff: f64,
}

impl Default for ConfigDto {
    fn default() -> Self {
        Self::from_config(&Config::default())
    }
}

impl ConfigDto {
    pub fn from_config(c: &Config) -> Self {
        Self {
            severe: c.severe,
            severe_pad: c.severe_pad,
            severe_merge: c.severe_merge,
            ramp: c.ramp,
            light_cutoff: c.light_cutoff,
            strong_cutoff: c.strong_cutoff,
            noise_low: c.noise_low,
            noise_high: c.noise_high,
            noise_band: [c.noise_band.0, c.noise_band.1],
            noise_window: c.noise_window_ms,
            hampel_window: c.hampel_window,
            hampel_sigma: c.hampel_sigma,
            optical_cutoff: c.optical_cutoff,
            handback_cutoff: c.handback_cutoff,
            fast_handback: [c.fast_handback.0, c.fast_handback.1],
            patch_pad: c.patch_pad,
            patch_merge: c.patch_merge,
            optical_noise: c.optical_noise.map(|(a, b)| [a, b]),
            fast_wide_cutoff: c.fast_wide_cutoff,
            fast_wide_ramp: [c.fast_wide_ramp.0, c.fast_wide_ramp.1],
            fast_wide_accel: c.fast_wide_accel,
            anchor_mode: c.anchor_mode,
            anchor_cutoff: c.anchor_cutoff,
        }
    }

    /// Exhaustive struct literal: adding a Config field breaks this at
    /// compile time (same guarantee as o4fix-cli's to_config).
    pub fn to_config(&self) -> Config {
        Config {
            severe: self.severe,
            severe_pad: self.severe_pad,
            severe_merge: self.severe_merge,
            ramp: self.ramp,
            light_cutoff: self.light_cutoff,
            strong_cutoff: self.strong_cutoff,
            noise_low: self.noise_low,
            noise_high: self.noise_high,
            noise_band: (self.noise_band[0], self.noise_band[1]),
            noise_window_ms: self.noise_window,
            hampel_window: self.hampel_window,
            hampel_sigma: self.hampel_sigma,
            optical_cutoff: self.optical_cutoff,
            handback_cutoff: self.handback_cutoff,
            fast_handback: (self.fast_handback[0], self.fast_handback[1]),
            patch_pad: self.patch_pad,
            patch_merge: self.patch_merge,
            optical_noise: self.optical_noise.map(|a| (a[0], a[1])),
            fast_wide_cutoff: self.fast_wide_cutoff,
            fast_wide_ramp: (self.fast_wide_ramp[0], self.fast_wide_ramp[1]),
            fast_wide_accel: self.fast_wide_accel,
            anchor_mode: self.anchor_mode,
            anchor_cutoff: self.anchor_cutoff,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct GuiSettings {
    pub profile: String,
    pub config: ConfigDto,
    pub output_dir: Option<String>,
    pub concurrent_files: usize,
}

impl Default for GuiSettings {
    fn default() -> Self {
        Self { profile: "m2".into(), config: ConfigDto::default(),
               output_dir: None, concurrent_files: 1 }
    }
}

// Pure file I/O (unit-testable without an AppHandle).
pub fn load_from(path: &Path) -> GuiSettings {
    std::fs::read_to_string(path).ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}
pub fn save_to(path: &Path, s: &GuiSettings) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, serde_json::to_string_pretty(s).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

pub fn settings_path(app: &tauri::AppHandle) -> PathBuf {
    use tauri::Manager;
    app.path().app_config_dir().expect("app config dir").join("settings.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dto_default_round_trips_to_config_default() {
        assert_eq!(ConfigDto::default().to_config(), Config::default());
        assert_eq!(ConfigDto::from_config(&Config::m4()).to_config(), Config::m4());
    }

    #[test]
    fn settings_serde_round_trip() {
        let s = GuiSettings { profile: "m4".into(),
            config: ConfigDto::from_config(&Config::m4()),
            output_dir: Some("D:\\out".into()), concurrent_files: 3 };
        let j = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<GuiSettings>(&j).unwrap(), s);
    }

    #[test]
    fn load_missing_or_corrupt_falls_back_to_default() {
        let dir = std::env::temp_dir().join("o4fix_settings_test");
        let p = dir.join("settings.json");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(load_from(&p), GuiSettings::default());       // missing
        save_to(&p, &GuiSettings::default()).unwrap();
        assert_eq!(load_from(&p), GuiSettings::default());       // round trip
        std::fs::write(&p, "{not json").unwrap();
        assert_eq!(load_from(&p), GuiSettings::default());       // corrupt
        let _ = std::fs::remove_dir_all(&dir);
    }
}
```

- [ ] **Step 3: Run the settings tests**

```powershell
cd rust  # env vars inline
cargo test -p o4fix-app
```
Expected: 3 tests pass. (They compile `main.rs` too — add `mod settings;` and `mod queue;` declarations as you create the files; an empty `queue.rs` placeholder is fine for this step.)

- [ ] **Step 4: Write `queue.rs`**

```rust
use crate::settings::GuiSettings;
use o4core::pipeline::{self, Outcome, Progress, Stage};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};

#[derive(Default)]
pub struct AppState {
    next_id: AtomicU64,
    jobs: Mutex<HashMap<u64, Arc<AtomicBool>>>, // id -> cancel flag
}

#[derive(Clone, Serialize)]
pub struct JobProgress {
    pub id: u64,
    pub file: String,
    pub stage: &'static str,
    pub pct: f64,
    pub detail: String,
}

#[derive(Clone, Serialize)]
pub struct JobDone {
    pub id: u64,
    pub status: &'static str,
    pub message: String,
}

pub fn stage_name(s: Stage) -> &'static str {
    match s {
        Stage::Extract | Stage::Analyze => "analyzing",
        Stage::Optical => "measuring motion",
        Stage::Splice => "patching",
        Stage::Write => "verifying",
    }
}

struct Job {
    id: u64,
    file: PathBuf,
    cancel: Arc<AtomicBool>,
}

#[tauri::command]
pub fn start_queue(app: AppHandle, state: State<'_, AppState>,
                   files: Vec<String>, settings: GuiSettings) -> Vec<u64> {
    let cfg = settings.config.to_config();
    let out_dir = settings.output_dir.clone().map(PathBuf::from);
    let queue: Arc<Mutex<VecDeque<Job>>> = Arc::default();
    let mut ids = Vec::new();
    {
        let mut q = queue.lock().unwrap();
        let mut jobs = state.jobs.lock().unwrap();
        for f in files {
            let id = state.next_id.fetch_add(1, Ordering::Relaxed);
            let cancel = Arc::new(AtomicBool::new(false));
            jobs.insert(id, cancel.clone());
            q.push_back(Job { id, file: PathBuf::from(f), cancel });
            ids.push(id);
        }
    }
    let workers = settings.concurrent_files.clamp(1, 3).min(ids.len());
    for _ in 0..workers {
        let app = app.clone();
        let queue = queue.clone();
        let cfg = cfg.clone();
        let out_dir = out_dir.clone();
        std::thread::spawn(move || loop {
            let job = queue.lock().unwrap().pop_front();
            let Some(job) = job else { break };
            run_job(&app, job, &cfg, out_dir.as_deref());
        });
    }
    ids
}

fn run_job(app: &AppHandle, job: Job, cfg: &o4core::config::Config,
           out_dir: Option<&Path>) {
    let fname = job.file.file_name().unwrap_or_default()
        .to_string_lossy().to_string();
    let finish = |status: &'static str, message: String| {
        let _ = app.emit("job_done", JobDone { id: job.id, status, message });
    };
    if !fname.to_ascii_lowercase().ends_with(".mp4") {
        finish("error", "not an .MP4 file".into());
        return;
    }
    // output-folder override reuses pipeline's default naming scheme
    let out = out_dir.map(|d| {
        let stem = job.file.file_stem().unwrap_or_default().to_string_lossy();
        let ext = job.file.extension()
            .map(|e| e.to_string_lossy()).unwrap_or_default();
        if ext.is_empty() { d.join(format!("{stem}_fixed")) }
        else { d.join(format!("{stem}_fixed.{ext}")) }
    });
    let emit_progress = |p: Progress| {
        if !p.message.is_empty() {
            let _ = app.emit("job_log",
                serde_json::json!({ "id": job.id, "line": p.message }));
        }
        let _ = app.emit("job_progress", JobProgress {
            id: job.id, file: fname.clone(), stage: stage_name(p.stage),
            pct: p.pct, detail: p.message,
        });
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pipeline::process(&job.file, out.as_deref(), cfg, &emit_progress, &job.cancel)
    }));
    match result {
        Ok(Ok(Outcome::Repaired { out, .. })) => finish("done", out.display().to_string()),
        Ok(Ok(Outcome::Healthy)) =>
            finish("healthy", "telemetry looks healthy, nothing to repair".into()),
        Ok(Err(o4core::error::O4Error::Cancelled)) => finish("cancelled", String::new()),
        Ok(Err(e)) => finish("error", e.to_string()),
        Err(panic) => {
            let msg = panic.downcast_ref::<&str>().map(|s| s.to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".into());
            finish("error", format!("internal error: {msg}"));
        }
    }
}

#[tauri::command]
pub fn cancel_job(state: State<'_, AppState>, id: u64) {
    if let Some(c) = state.jobs.lock().unwrap().get(&id) {
        c.store(true, Ordering::Relaxed);
    }
}

#[tauri::command]
pub async fn pick_files(app: AppHandle) -> Vec<String> {
    use tauri_plugin_dialog::DialogExt;
    app.dialog().file()
        .add_filter("MP4 video", &["mp4", "MP4"])
        .blocking_pick_files()
        .map(|files| files.into_iter().map(|f| f.to_string()).collect())
        .unwrap_or_default()
}

#[tauri::command]
pub fn load_settings(app: AppHandle) -> GuiSettings {
    crate::settings::load_from(&crate::settings::settings_path(&app))
}

#[tauri::command]
pub fn save_settings(app: AppHandle, settings: GuiSettings) -> Result<(), String> {
    crate::settings::save_to(&crate::settings::settings_path(&app), &settings)
}
```
Notes for the implementer: partial-output deletion on cancel/verify-fail/error already happens inside `pipeline::process` (`pipeline.rs:138-145`) — the GUI never cleans up files itself. Jobs cancelled while still queued terminate at `process`'s first `check()?`.

- [ ] **Step 5: Wire `main.rs`**

Replace the `tauri::Builder` call:
```rust
mod queue;
mod settings;

// ... webview2 check unchanged ...
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(queue::AppState::default())
        .invoke_handler(tauri::generate_handler![
            queue::start_queue,
            queue::cancel_job,
            queue::pick_files,
            queue::load_settings,
            queue::save_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running o4fix");
```

- [ ] **Step 6: Verify + CI package list**

```powershell
cd rust  # env vars inline
cargo test -p o4fix-app
cargo clippy -p o4fix-app -- -D warnings
cargo fmt --check
```
Expected: settings tests green, clippy clean. In `.github/workflows/ci.yml` change the test line to `cargo test -p o4core -p o4fix-cli -p o4fix-app`.
Optional runtime sanity (UI comes in Task 8): `cargo run -p o4fix-app`, right-click → Inspect (devtools are enabled in debug builds), console: `window.__TAURI__.core.invoke('load_settings').then(console.log)` → prints the default GuiSettings JSON.

- [ ] **Step 7: Commit**

```powershell
git add rust .github/workflows/ci.yml
git commit -m "feat: job queue, cancel, settings persistence + Tauri commands"
git push
gh run watch
```

---

### Task 8: Frontend UI

**Files:**
- Create: `rust/o4fix-app/ui/index.html` (replaces placeholder), `ui/style.css`, `ui/help.js`, `ui/app.js`
- Modify: `rust/o4fix-app/src/queue.rs` + `src/main.rs` (add `pick_folder` command)

**Interfaces:**
- Consumes: Task 7's commands/events exactly as named there.
- Produces: `pick_folder() -> Option<String>` command (added here because only the folder-override UI needs it).

- [ ] **Step 1: Add `pick_folder` to the backend**

Append to `queue.rs`:
```rust
#[tauri::command]
pub async fn pick_folder(app: AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    app.dialog().file().blocking_pick_folder().map(|f| f.to_string())
}
```
Add `queue::pick_folder` to the `generate_handler![...]` list in `main.rs`.

- [ ] **Step 2: `ui/help.js` — defaults + tooltips**

Classic scripts (no modules/bundler); load order: `help.js` then `app.js`. `DEFAULTS` mirrors `ConfigDto::default()` (numbers verbatim from `o4core/src/config.rs`); tooltip texts verbatim from `o4fix.py`'s argparse help (`o4fix.py:677-765` — copy the full help strings, collapsing line wraps to single spaces; entries below marked `/*…*/` are abbreviations in THIS PLAN ONLY, the file must carry the full text):

```js
window.DEFAULTS = {
  severe: 8.0, severe_pad: 0.2, severe_merge: 0.2, ramp: 0.3,
  light_cutoff: 25.0, strong_cutoff: 2.5, noise_low: 1.5, noise_high: 5.0,
  noise_band: [30.0, 180.0], noise_window: 100.0,
  hampel_window: 7, hampel_sigma: 6.0, optical_cutoff: 8.0,
  handback_cutoff: null, fast_handback: [100.0, 250.0],
  patch_pad: 0.5, patch_merge: 1.0, optical_noise: null,
  fast_wide_cutoff: 0.0, fast_wide_ramp: [150.0, 300.0],
  fast_wide_accel: 1500.0, anchor_mode: false, anchor_cutoff: 1.5,
};
window.M4 = Object.assign({}, window.DEFAULTS, { fast_wide_cutoff: 16.0 });

window.HELP = {
  severe: "deg/s 30-180 Hz band-RMS above which orientation is replaced with integrated optical motion (default 8)",
  severe_pad: "s, padding around each severe burst (default 0.2)",
  severe_merge: "s, gap below which severe bursts merge (default 0.2)",
  ramp: "s, slerp cross-fade to the raw path at burst edges (default 0.3)",
  light_cutoff: /* full text from o4fix.py:693-696 */ "",
  strong_cutoff: /* o4fix.py:697-700 */ "",
  noise_low: /* o4fix.py:701-703 */ "",
  noise_high: /* o4fix.py:704-705 */ "",
  noise_band: /* o4fix.py:706-708 */ "",
  noise_window: /* o4fix.py:709-710 */ "",
  hampel_window: /* o4fix.py:711-712 */ "",
  hampel_sigma: /* o4fix.py:713-714 */ "",
  optical_cutoff: /* o4fix.py:719-721 */ "",
  fast_handback: /* o4fix.py:722-727 */ "",
  patch_pad: /* o4fix.py:728-730 */ "",
  patch_merge: /* o4fix.py:731-733 */ "",
  optical_noise: /* o4fix.py:734-738 */ "",
  handback_cutoff: /* o4fix.py:739-741 */ "",
  fast_wide_cutoff: /* o4fix.py:742-748 */ "",
  fast_wide_ramp: /* o4fix.py:749-752 */ "",
  fast_wide_accel: /* o4fix.py:753-757 */ "",
  anchor_mode: /* o4fix.py:758-762 */ "",
  anchor_cutoff: /* o4fix.py:763-765 */ "",
};

// field descriptors driving the settings form
window.FIELDS = [
  // group, key, label, kind: "num" | "pair" | "pair_opt" | "num_opt" | "bool"
  ["Repair thresholds", "severe", "Severe threshold (°/s)", "num"],
  ["Repair thresholds", "severe_pad", "Burst padding (s)", "num"],
  ["Repair thresholds", "severe_merge", "Burst merge gap (s)", "num"],
  ["Repair thresholds", "ramp", "Edge cross-fade (s)", "num"],
  ["Filter tuning", "light_cutoff", "Light low-pass (Hz)", "num"],
  ["Filter tuning", "strong_cutoff", "Strong low-pass (Hz)", "num"],
  ["Filter tuning", "noise_low", "Noise blend start (°/s)", "num"],
  ["Filter tuning", "noise_high", "Noise blend full (°/s)", "num"],
  ["Filter tuning", "noise_band", "Noise band (Hz lo/hi)", "pair"],
  ["Filter tuning", "noise_window", "Noise window (ms)", "num"],
  ["Filter tuning", "hampel_window", "Hampel half-window", "num"],
  ["Filter tuning", "hampel_sigma", "Hampel sigma", "num"],
  ["Optical", "optical_cutoff", "Optical bandwidth (Hz)", "num"],
  ["Optical", "handback_cutoff", "Handback cutoff (Hz, blank = optical)", "num_opt"],
  ["Optical", "fast_handback", "Fast handback ramp (°/s lo/hi)", "pair"],
  ["Optical", "patch_pad", "Patch padding (s)", "num"],
  ["Optical", "patch_merge", "Patch merge gap (s)", "num"],
  ["Optical", "optical_noise", "Optical threshold (°/s lo/hi, blank = noise)", "pair_opt"],
  ["Optical", "fast_wide_cutoff", "Fast-wide cutoff (Hz, 0 = off)", "num"],
  ["Optical", "fast_wide_ramp", "Fast-wide ramp (°/s lo/hi)", "pair"],
  ["Optical", "fast_wide_accel", "Fast-wide accel gate (°/s², 0 = off)", "num"],
  ["Optical", "anchor_mode", "Anchor mode (experimental)", "bool"],
  ["Optical", "anchor_cutoff", "Anchor cutoff (Hz)", "num"],
];
```
(The GUI's third group "Optical" splits argparse's long "filter tuning" group at `optical_cutoff` for scannability — grouping is presentational only, every MP4-mode flag appears exactly once.)

- [ ] **Step 3: `ui/index.html`**

```html
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>o4fix</title>
<link rel="stylesheet" href="style.css">
</head>
<body>
<header>
  <h1>o4fix <span class="sub">DJI O4 Pro gyro repair</span></h1>
  <button id="gear" title="Advanced settings">⚙</button>
</header>
<main>
  <div id="dropzone" tabindex="0">
    <p><strong>Drop DJI O4 Pro videos here</strong></p>
    <p>or click to browse</p>
  </div>
  <div class="bar">
    <button id="start" disabled>Start repair</button>
    <button id="clear" disabled>Clear finished</button>
    <span id="profile-badge">Profile: Default (M2)</span>
  </div>
  <ul id="queue"></ul>
  <details id="logpane">
    <summary>Log</summary>
    <pre id="log"></pre>
  </details>
</main>
<aside id="settings" hidden>
  <h2>Advanced settings <button id="close-settings">✕</button></h2>
  <fieldset id="profiles">
    <legend>Profile</legend>
    <label><input type="radio" name="profile" value="m2"> Default (M2)</label>
    <label><input type="radio" name="profile" value="m4"> Sharp-turn (M4)</label>
    <label><input type="radio" name="profile" value="custom"> Custom</label>
    <button id="reset">Reset to defaults</button>
  </fieldset>
  <div id="fields"></div>
  <fieldset>
    <legend>App</legend>
    <label>Output folder <input id="output-dir" type="text"
           placeholder="next to source video">
           <button id="browse-out">…</button>
           <button id="clear-out" title="back to next-to-source">✕</button></label>
    <label>Parallel files
      <select id="concurrent"><option>1</option><option>2</option><option>3</option></select>
    </label>
  </fieldset>
</aside>
<script src="help.js"></script>
<script src="app.js"></script>
</body>
</html>
```

- [ ] **Step 4: `ui/style.css`**

```css
:root {
  --bg: #12161e; --panel: #1a2029; --edge: #2a3342; --fg: #e6ebf0;
  --dim: #8b96a5; --accent: #5ac8fa; --ok: #4cd964; --err: #ff5f57;
  --warn: #ffbd2e;
}
* { box-sizing: border-box; }
body { margin: 0; background: var(--bg); color: var(--fg);
       font: 14px/1.45 "Segoe UI", sans-serif; }
header { display: flex; justify-content: space-between; align-items: center;
         padding: 10px 16px; border-bottom: 1px solid var(--edge); }
h1 { font-size: 18px; margin: 0; } .sub { color: var(--dim); font-weight: 400; font-size: 13px; }
#gear { font-size: 18px; background: none; border: none; color: var(--dim); cursor: pointer; }
#gear:hover { color: var(--fg); }
main { padding: 14px 16px; }
#dropzone { border: 2px dashed var(--edge); border-radius: 10px; padding: 26px;
            text-align: center; color: var(--dim); cursor: pointer; }
#dropzone.hover, #dropzone:hover { border-color: var(--accent); color: var(--fg); }
#dropzone p { margin: 4px; }
.bar { display: flex; gap: 10px; align-items: center; margin: 12px 0; }
button { background: var(--panel); color: var(--fg); border: 1px solid var(--edge);
         border-radius: 6px; padding: 6px 14px; cursor: pointer; }
button:disabled { opacity: .4; cursor: default; }
#start:not(:disabled) { border-color: var(--accent); color: var(--accent); }
#profile-badge { color: var(--dim); margin-left: auto; font-size: 12px; }
#queue { list-style: none; padding: 0; margin: 0; }
#queue li { background: var(--panel); border: 1px solid var(--edge); border-radius: 8px;
            padding: 8px 12px; margin-bottom: 8px; display: grid;
            grid-template-columns: 1fr auto auto; gap: 4px 10px; align-items: center; }
.fname { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.chip { font-size: 12px; padding: 2px 10px; border-radius: 999px;
        border: 1px solid var(--edge); color: var(--dim); white-space: nowrap; }
.chip.analyzing, .chip.measuring-motion, .chip.patching, .chip.verifying
  { border-color: var(--accent); color: var(--accent); }
.chip.done { border-color: var(--ok); color: var(--ok); }
.chip.healthy { border-color: var(--ok); color: var(--ok); }
.chip.error { border-color: var(--err); color: var(--err); }
.chip.cancelled { border-color: var(--warn); color: var(--warn); }
.cancel { padding: 2px 8px; font-size: 12px; }
progress { grid-column: 1 / -1; width: 100%; height: 6px; }
.msg { grid-column: 1 / -1; font-size: 12px; color: var(--dim);
       overflow-wrap: anywhere; }
#logpane { margin-top: 14px; color: var(--dim); }
#log { background: var(--panel); border: 1px solid var(--edge); border-radius: 8px;
       padding: 10px; max-height: 220px; overflow: auto; font-size: 12px;
       white-space: pre-wrap; }
#settings { position: fixed; top: 0; right: 0; bottom: 0; width: 380px;
            background: var(--panel); border-left: 1px solid var(--edge);
            padding: 14px 16px; overflow-y: auto; z-index: 5; }
#settings h2 { display: flex; justify-content: space-between; font-size: 15px; margin-top: 0; }
fieldset { border: 1px solid var(--edge); border-radius: 8px; margin-bottom: 12px; }
legend { color: var(--dim); padding: 0 6px; }
#settings label { display: flex; align-items: center; gap: 6px; margin: 6px 0;
                  justify-content: space-between; }
#settings input[type="number"], #settings input[type="text"]
  { background: var(--bg); color: var(--fg); border: 1px solid var(--edge);
    border-radius: 4px; padding: 3px 6px; width: 90px; }
#settings input.pair { width: 58px; }
#output-dir { width: 170px !important; }
```

- [ ] **Step 5: `ui/app.js`**

```js
const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

let settings = null;              // GuiSettings from backend
let pending = [];                 // absolute paths not yet queued
const rows = new Map();           // job id -> DOM refs
let activeJobs = 0;

const $ = (id) => document.getElementById(id);
const busy = () => activeJobs > 0;

function baseName(p) { return p.split(/[\\/]/).pop(); }

function setControls() {
  $("start").disabled = busy() || pending.length === 0;
  $("clear").disabled = busy() || rows.size === 0;
}

function addFiles(paths) {
  if (busy()) return;
  for (const p of paths || []) {
    if (pending.includes(p)) continue;
    pending.push(p);
    const li = document.createElement("li");
    li.innerHTML = `<span class="fname"></span><span class="chip">queued</span>
      <button class="cancel" hidden>cancel</button>
      <progress max="1" value="0"></progress><span class="msg"></span>`;
    li.querySelector(".fname").textContent = baseName(p);
    li.dataset.path = p;
    $("queue").appendChild(li);
  }
  setControls();
}

async function start() {
  const files = pending.slice();
  const ids = await invoke("start_queue", { files, settings });
  activeJobs = ids.length;
  const lis = [...$("queue").querySelectorAll("li")].filter(li => !li.dataset.id);
  ids.forEach((id, i) => {
    const li = lis[i];
    li.dataset.id = id;
    const cancelBtn = li.querySelector(".cancel");
    cancelBtn.hidden = false;
    cancelBtn.onclick = () => invoke("cancel_job", { id });
    rows.set(id, { li, chip: li.querySelector(".chip"),
                   bar: li.querySelector("progress"),
                   msg: li.querySelector(".msg"), cancel: cancelBtn });
  });
  pending = [];
  setControls();
}

function onProgress(e) {
  const r = rows.get(e.payload.id);
  if (!r) return;
  r.chip.textContent = e.payload.stage;
  r.chip.className = "chip " + e.payload.stage.replace(/ /g, "-");
  r.bar.value = e.payload.pct;
}

function onLog(e) {
  const r = rows.get(e.payload.id);
  const name = r ? r.li.querySelector(".fname").textContent : e.payload.id;
  $("log").textContent += `[${name}]${e.payload.line}\n`;
  $("log").scrollTop = $("log").scrollHeight;
}

function onDone(e) {
  const { id, status, message } = e.payload;
  const r = rows.get(id);
  if (!r) return;
  r.chip.textContent = status === "healthy" ? "healthy — nothing to repair" : status;
  r.chip.className = "chip " + status;
  r.cancel.hidden = true;
  if (status === "done") { r.bar.value = 1; r.msg.textContent = "→ " + message; }
  else { r.bar.hidden = true; if (message) r.msg.textContent = message; }
  activeJobs -= 1;
  setControls();
}

// ---------- settings ----------
function cfgEquals(a, b) {
  // key-by-key: independent of JSON key order in settings.json
  return Object.keys(window.DEFAULTS)
    .every(k => JSON.stringify(a[k]) === JSON.stringify(b[k]));
}
function detectProfile(cfg) {
  if (cfgEquals(cfg, window.DEFAULTS)) return "m2";
  if (cfgEquals(cfg, window.M4)) return "m4";
  return "custom";
}
function profileLabel(p) {
  return { m2: "Default (M2)", m4: "Sharp-turn (M4)", custom: "Custom" }[p];
}

function save() {
  settings.profile = detectProfile(settings.config);
  document.querySelector(`input[name=profile][value=${settings.profile}]`).checked = true;
  $("profile-badge").textContent = "Profile: " + profileLabel(settings.profile);
  invoke("save_settings", { settings });
}

function buildFields() {
  const wrap = $("fields");
  wrap.innerHTML = "";
  let fs = null, lastGroup = null;
  for (const [group, key, label, kind] of window.FIELDS) {
    if (group !== lastGroup) {
      fs = document.createElement("fieldset");
      fs.innerHTML = `<legend>${group}</legend>`;
      wrap.appendChild(fs);
      lastGroup = group;
    }
    const lab = document.createElement("label");
    lab.title = window.HELP[key];
    lab.append(label + " ");
    const val = settings.config[key];
    if (kind === "bool") {
      const cb = Object.assign(document.createElement("input"),
                               { type: "checkbox", checked: val });
      cb.onchange = () => { settings.config[key] = cb.checked; save(); };
      lab.appendChild(cb);
    } else if (kind === "pair" || kind === "pair_opt") {
      const box = document.createElement("span");
      const inputs = [0, 1].map(i => {
        const inp = Object.assign(document.createElement("input"),
          { type: "number", className: "pair", step: "any",
            value: val === null ? "" : val[i] });
        box.appendChild(inp);
        return inp;
      });
      const commit = () => {
        const a = inputs.map(x => x.value.trim());
        if (kind === "pair_opt" && a.every(x => x === "")) settings.config[key] = null;
        else settings.config[key] = [parseFloat(a[0]) || 0, parseFloat(a[1]) || 0];
        save();
      };
      inputs.forEach(x => x.onchange = commit);
      lab.appendChild(box);
    } else { // num | num_opt
      const inp = Object.assign(document.createElement("input"),
        { type: "number", step: "any", value: val === null ? "" : val });
      inp.onchange = () => {
        const v = inp.value.trim();
        settings.config[key] = (kind === "num_opt" && v === "") ? null
          : (key === "hampel_window" ? parseInt(v, 10) || 0 : parseFloat(v) || 0);
        save();
      };
      lab.appendChild(inp);
    }
    fs.appendChild(lab);
  }
}

function renderSettings() {
  buildFields();
  $("output-dir").value = settings.output_dir || "";
  $("concurrent").value = settings.concurrent_files;
  save(); // also syncs profile radio + badge
}

async function init() {
  settings = await invoke("load_settings");
  renderSettings();

  $("dropzone").onclick = async () => addFiles(await invoke("pick_files"));
  await listen("tauri://drag-drop", (e) => addFiles(e.payload.paths));
  await listen("tauri://drag-enter", () => $("dropzone").classList.add("hover"));
  await listen("tauri://drag-leave", () => $("dropzone").classList.remove("hover"));
  await listen("job_progress", onProgress);
  await listen("job_log", onLog);
  await listen("job_done", onDone);

  $("start").onclick = start;
  $("clear").onclick = () => {
    for (const [id, r] of [...rows]) {
      if (!r.cancel.hidden) continue;           // still running
      r.li.remove(); rows.delete(id);
    }
    setControls();
  };
  $("gear").onclick = () => $("settings").hidden = false;
  $("close-settings").onclick = () => $("settings").hidden = true;
  $("reset").onclick = () => {
    settings.config = JSON.parse(JSON.stringify(window.DEFAULTS));
    renderSettings();
  };
  for (const radio of document.querySelectorAll("input[name=profile]")) {
    radio.onchange = () => {
      if (radio.value === "m2")
        settings.config = JSON.parse(JSON.stringify(window.DEFAULTS));
      else if (radio.value === "m4")
        settings.config = JSON.parse(JSON.stringify(window.M4));
      renderSettings();                          // custom: keep as-is
    };
  }
  $("browse-out").onclick = async () => {
    const dir = await invoke("pick_folder");
    if (dir) { settings.output_dir = dir; $("output-dir").value = dir; save(); }
  };
  $("clear-out").onclick = () => {
    settings.output_dir = null; $("output-dir").value = ""; save();
  };
  $("output-dir").onchange = () => {
    settings.output_dir = $("output-dir").value.trim() || null; save();
  };
  $("concurrent").onchange = () => {
    settings.concurrent_files = parseInt($("concurrent").value, 10); save();
  };
}

init();
```
Implementation note: `cfgEquals` compares key-by-key against the `DEFAULTS` key set, so it is independent of the JSON key order the backend serializes (or a hand-edited settings.json uses).

- [ ] **Step 6: Quick functional checks (no long pipeline run yet)**

```powershell
cd rust  # env vars inline
cargo run -p o4fix-app
```
Checklist (record each):
1. Window opens on the drop-zone UI (not the scaffold page).
2. ⚙ opens the drawer: 3 profile radios (M2 checked), 23 tuning fields in 3 groups, tooltips on hover show argparse help text, App group present.
3. Select M4 → `fast_wide_cutoff` field shows 16, badge "Profile: Sharp-turn (M4)". Edit any field → badge flips to Custom. Reset → back to M2.
4. Close and relaunch the app: M-profile choice survived (settings.json written under `%APPDATA%\com.thaumielsparrow.o4fix`).
5. Drag `test_gyro.csv` (repo root) onto the window → row appears; Start → row goes red "error — not an .MP4 file" almost instantly; log pane stays usable; Start re-enables.
6. Click-to-browse opens the native MP4-filtered picker; Cancel closes it without a row.

- [ ] **Step 7: Lint + commit**

```powershell
cargo clippy -p o4fix-app -- -D warnings; cargo fmt --check   # env vars inline
git add rust/o4fix-app
git commit -m "feat: drop-zone UI, queue rows, log pane, advanced settings drawer"
git push
gh run watch
```

---

### Task 9: GUI end-to-end verification on the real clip

No new code (fixes only, if the checklist fails). This is the manual smoke checklist from the spec §7 — run it with the RELEASE build (debug o4core is far too slow for the optical stage). **Set the output folder override to the scratchpad first — the default would overwrite `sample_vids/DJI_20260711124046_0021_D_fixed.MP4`, the protected Python reference output.**

**Files:** none (report only; any fix goes through its own commit + re-run).

- [ ] **Step 1: Build + CLI reference output**

```powershell
cd rust  # env vars inline
cargo build --release -p o4fix-app -p o4fix-cli
.\target\release\o4fix.exe ..\sample_vids\DJI_20260711124046_0021_D.MP4 -o <scratchpad>\cli_ref.MP4
```
Expected: ~5 min; transcript shows the known numbers (31 bursts / 52.1 s, R2=0.995 / -4 ms, 124551/176372 unchanged, round-trip max err 0).

- [ ] **Step 2: Happy path + determinism cross-check**

```powershell
.\target\release\o4fix-app.exe   # same shell (needs OpenCV DLLs on PATH)
```
1. ⚙ → Output folder → browse to the scratchpad. Profile = Default (M2).
2. Drop the test clip. Start. Chips must walk analyzing → measuring motion (bar advancing smoothly through the interval ticks) → patching → verifying → done, with the log pane mirroring the CLI lines (burst intervals, per-burst drift, R²).
3. `fc /b <scratchpad>\cli_ref.MP4 <scratchpad>\DJI_20260711124046_0021_D_fixed.MP4` → **no differences** (GUI and CLI runs are seeded-deterministic and share Config defaults).

- [ ] **Step 3: Cancel, healthy, error rows**

1. Re-queue the clip; hit cancel during "measuring motion" → chip "cancelled", no partial file left in the scratchpad output folder.
2. Settings → set `severe` to `1e9` (badge flips to Custom) → queue clip → green "healthy — nothing to repair", no output written. Reset to defaults afterwards (badge back to Default (M2)).
3. Queue `test_gyro.csv` → red error row "not an .MP4 file"; other rows unaffected.

- [ ] **Step 4: Batch + concurrency**

```powershell
cd C:\Users\lzhan\Desktop\o4prostab
Copy-Item sample_vids\DJI_20260711124046_0021_D.MP4 <scratchpad>\copy_a.MP4
Copy-Item sample_vids\DJI_20260711124046_0021_D.MP4 <scratchpad>\copy_b.MP4
```
Settings → Parallel files = 2. Queue the original + both copies (3 rows). Start: exactly two rows progress simultaneously, third starts when a slot frees; all three end "done"; drop zone/Start disabled while running, re-enabled after. Then set Parallel files back to 1.

- [ ] **Step 5: Clean up + report**

Delete all scratchpad MP4s (`copy_a/copy_b`, their `_fixed` outputs, `cli_ref.MP4`, the GUI `_fixed` output). Record every checklist item's result in the task report. Any failure = fix, commit, re-run the affected step before proceeding.

---

### Task 10: Release workflow, zip contents, checklist doc

**Files:**
- Create: `.github/workflows/release.yml`
- Create: `packaging/README.txt`
- Create: `docs/release-checklist.md`

**Interfaces:**
- Consumes: `./.github/actions/setup-opencv` from Task 4.
- Produces: tag `v*` → GitHub Release with `o4fix-vX.Y.Z-windows-x64.zip` containing `o4fix-app.exe`, `o4fix.exe`, `opencv_world4120.dll`, `opencv_videoio_ffmpeg4120_64.dll`, `README.txt`.

- [ ] **Step 1: `packaging/README.txt`**

```
o4fix — DJI O4 Pro gyro noise repair
====================================

Early-2026 DJI O4 Pro air units record noisy motion data during
high-throttle flight, which makes Gyroflow-stabilized footage shudder
and wobble. o4fix rewrites the noisy sections of the video's embedded
motion data in place. The repaired file loads into Gyroflow like a
stock recording.

Quick start
-----------
1. Run o4fix-app.exe
2. Drop your DJI .MP4 files onto the window
3. Click "Start repair"
4. Load the new VIDEO_fixed.MP4 in Gyroflow as usual

Notes
-----
- Your original files are never modified.
- "healthy — nothing to repair" means the recording's motion data is
  fine; just use the original in Gyroflow.
- "Couldn't calibrate motion from this clip" means the clip has no
  calm flight sections to calibrate against; the file is left
  unrepaired rather than risk making it worse.
- o4fix.exe is the command-line version (run: o4fix.exe VIDEO.MP4).
- Keep all files from this zip in one folder (the .dll files are
  required).
- If the app does not start: it needs Microsoft WebView2 (included in
  Windows 11; the app shows a download link on Windows 10 if missing)
  and the Microsoft Visual C++ Runtime
  (https://aka.ms/vs/17/release/vc_redist.x64.exe).

Project: https://github.com/ThaumielSparrow/o4fix
```

- [ ] **Step 2: `.github/workflows/release.yml`**

```yaml
name: Release
on:
  push:
    tags: ['v*']
jobs:
  release:
    runs-on: windows-latest
    timeout-minutes: 60
    permissions:
      contents: write
    steps:
      - uses: actions/checkout@v4
      - uses: ./.github/actions/setup-opencv
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: rust
      - name: Check version sync
        shell: pwsh
        run: |
          $tag  = $env:GITHUB_REF_NAME.TrimStart('v')
          $conf = (Get-Content rust/o4fix-app/tauri.conf.json | ConvertFrom-Json).version
          $cli  = (Select-String '^version = "(.+)"' rust/o4fix-cli/Cargo.toml).Matches[0].Groups[1].Value
          $app  = (Select-String '^version = "(.+)"' rust/o4fix-app/Cargo.toml).Matches[0].Groups[1].Value
          if ($conf -ne $tag -or $cli -ne $tag -or $app -ne $tag) {
            throw "version mismatch: tag=$tag tauri.conf=$conf cli=$cli app=$app"
          }
      - name: Build
        run: cargo build --release -p o4fix-cli -p o4fix-app
        working-directory: rust
      - name: Assemble portable zip
        shell: pwsh
        run: |
          $dist = "o4fix-$env:GITHUB_REF_NAME-windows-x64"
          New-Item -ItemType Directory $dist | Out-Null
          Copy-Item rust/target/release/o4fix-app.exe $dist
          Copy-Item rust/target/release/o4fix.exe $dist
          Copy-Item C:\opencv\build\x64\vc16\bin\opencv_world4120.dll $dist
          Copy-Item C:\opencv\build\x64\vc16\bin\opencv_videoio_ffmpeg4120_64.dll $dist
          Copy-Item packaging/README.txt $dist
          Compress-Archive -Path $dist -DestinationPath "$dist.zip"
      - name: Create GitHub Release
        shell: pwsh
        env:
          GH_TOKEN: ${{ github.token }}
        run: >
          gh release create $env:GITHUB_REF_NAME
          "o4fix-$env:GITHUB_REF_NAME-windows-x64.zip"
          --title "o4fix $env:GITHUB_REF_NAME"
          --notes "Portable Windows build - unzip, run o4fix-app.exe (GUI) or o4fix.exe (CLI). See README.txt inside the zip."
```

- [ ] **Step 3: `docs/release-checklist.md`**

```markdown
# Release checklist

CI cannot run the clip-gated tests (1.7 GB clip + Python goldens), so a
release REQUIRES this local gate first. All on the release commit:

1. `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
   fast suite `cargo test -p o4core -p o4fix-cli -p o4fix-app` — green.
2. Goldens present (else `python tools/dump_goldens.py`, ~25-30 min incl. M4).
3. Full clip-gated suite green: `cargo test -p o4core -- --ignored`
   (extraction, detect, mp4 byte gates incl. nullpatch/inject, optical,
   fit, patch, splice, e2e M2 **and** e2e M4). ~45-60 min.
4. GUI smoke (plan Task 9 checklist) passed on this commit.
5. Versions synced: `rust/o4fix-cli/Cargo.toml`, `rust/o4fix-app/Cargo.toml`,
   `rust/o4fix-app/tauri.conf.json` all equal the tag (minus the `v`).
6. CI green on `main` for the commit being tagged.
7. `git tag vX.Y.Z && git push origin vX.Y.Z`; watch the Release workflow.
8. Download the published zip; smoke it from a CLEAN environment
   (PATH stripped to System32 so the zip's own DLLs must resolve):
   `cmd /c "set PATH=C:\Windows\System32;C:\Windows&& o4fix.exe <clip> -o <tmp out>"`
   then launch `o4fix-app.exe` the same way and repair a clip via the GUI.
9. Sanity-check the README download link still points at the release.
```

- [ ] **Step 4: Commit**

```powershell
git add .github/workflows/release.yml packaging docs/release-checklist.md
git commit -m "ci: tag-triggered portable-zip release workflow + checklist"
git push
```
(The workflow itself is exercised by the v0.1.0 tag in Task 11 — tags only fire it once it exists on the tagged commit, so no dry-run is possible; the version-sync and assemble steps are the risky bits and both fail loudly.)

---

### Task 11: Docs, merge, v0.1.0 release, clean-machine smoke

**Files:**
- Modify: `README.md` (repo root — pilot-facing rewrite)
- Modify: `rust/README.md` (app dev section)
- Modify: `CLAUDE.md` (status line)

- [ ] **Step 1: Root `README.md`**

Read the existing file first and keep anything still-relevant (problem description, credits). Replace/restructure to:

```markdown
# o4fix — DJI O4 Pro gyro noise repair

Early-2026 DJI O4 Pro air units (fw 01.00.07.00, camera "O4P") record
noisy gyro data during high-throttle flight; Gyroflow output shudders,
wobbles and micro-pans. o4fix repairs the MP4's embedded orientation
telemetry in place — severe noise bursts are replaced with motion
measured optically from the video frames — and writes `VIDEO_fixed.MP4`,
which loads in Gyroflow like a stock recording. Clean sections keep
their original bytes.

## Download (Windows)

Grab the latest `o4fix-vX.Y.Z-windows-x64.zip` from
[Releases](https://github.com/ThaumielSparrow/o4fix/releases), unzip,
run `o4fix-app.exe`, drop your videos, click Start repair. No install
needed (Windows 10 may prompt once for Microsoft WebView2).

- "healthy — nothing to repair": the clip's telemetry is fine, use the
  original.
- "Couldn't calibrate motion from this clip": no calm flight sections to
  calibrate against; the file is left untouched.
- Advanced settings: Default (M2) suits most flying; Sharp-turn (M4)
  recovers flip/roll crispness at the cost of slight extra high-frequency
  shake — try both on your own footage.

## CLI

`o4fix.exe VIDEO.MP4 [VIDEO2.MP4 ...] [-o OUT.MP4] [--severe 8 ...]`
Same pipeline and defaults as the GUI; `o4fix.exe --help` lists the
tuning flags.

## Repository layout

- `rust/` — the shipped implementation (`o4core` library, `o4fix` CLI,
  `o4fix-app` Tauri GUI). See `rust/README.md` for building.
- `o4fix.py`, `mp4patch.py` — the original Python research pipeline,
  kept as the golden reference the Rust port is verified against.
- `analysis/`, `tools/` — evaluation harness and golden-fixture dev
  tools (Python; not needed to use o4fix).
- `docs/` — design specs, implementation plans, release checklist.

Details of the underlying problem and the verification methodology live
in `CLAUDE.md` and `docs/superpowers/specs/`.
```

- [ ] **Step 2: `rust/README.md` app section + `CLAUDE.md` line**

Append to `rust/README.md` layout list: `- o4fix-app/ — Tauri 2 GUI ("o4fix-app.exe"; plain HTML/CSS/JS in ui/, no Node build step; settings persist to %APPDATA%\com.thaumielsparrow.o4fix\settings.json)` and under Dev commands: `cargo run -p o4fix-app` (debug, devtools via right-click → Inspect) / `cargo build --release -p o4fix-app`.

In `CLAUDE.md`, under the "## Possible follow-ups" heading, prepend a line: `- DONE (Plan 2, 2026-07-XX): Rust port shipped as o4fix-app GUI + CLI, portable zip on GitHub Releases (v0.1.0), CI on GitHub Actions. Multi-clip validation still open (deferred post-release).` (fill the date).

```powershell
git add README.md rust/README.md CLAUDE.md
git commit -m "docs: pilot-facing README, app dev docs, status update"
git push
gh run watch
```

- [ ] **Step 3: Merge to main**

Use the superpowers:finishing-a-development-branch flow (user decides merge vs PR). After merge: CI green on `main`, branch deleted local+remote.

- [ ] **Step 4: Run the release checklist, then tag**

Work through `docs/release-checklist.md` items 1-6 literally (the full `--ignored` suite is the long pole, ~45-60 min — run it foreground in chunks per test binary if the 10-min tool timeout bites: `cargo test -p o4core --test golden_mp4 -- --ignored` etc., finishing with the two e2e tests). Then:

```powershell
git tag v0.1.0
git push origin v0.1.0
gh run watch     # Release workflow: expect green, ~20-40 min warm
gh release view v0.1.0   # asset o4fix-v0.1.0-windows-x64.zip present
```

- [ ] **Step 5: Clean-environment zip smoke (checklist items 8-9)**

```powershell
cd <scratchpad>
gh release download v0.1.0 -p "*.zip"
Expand-Archive o4fix-v0.1.0-windows-x64.zip -DestinationPath .
cd o4fix-v0.1.0-windows-x64
cmd /c "set PATH=C:\Windows\System32;C:\Windows&& o4fix.exe C:\Users\lzhan\Desktop\o4prostab\sample_vids\DJI_20260711124046_0021_D.MP4 -o <scratchpad>\zip_smoke.MP4"
```
Expected: full transcript with the known numbers, exit 0 — proves the zip is self-contained (exe-dir DLLs, no dev env). Then `cmd /c "set PATH=C:\Windows\System32;C:\Windows&& start o4fix-app.exe"`, repair the clip once via the GUI (output folder = scratchpad), verify done-chip + output file. Delete all scratch MP4s and the extracted folder.

- [ ] **Step 6: Close out**

Task report: release URL, zip size (expect ~50-80 MB), cold/warm CI times, any deviations. The plan is done when the Definition of Done below is fully checked.

---

## Definition of done (Plan 2)

- [ ] `cargo fmt --check` clean; fmt commit in `.git-blame-ignore-revs` (spec Phase 1).
- [ ] M4 e2e golden test green; anchor smoke delta ≤ 1e-6 recorded in a task report (spec: branches runtime-verified).
- [ ] Golden helpers live in `tests/common/`; zero `#[path]` includes remain in `rust/o4core/tests/`; fast suite + clip-gated suite green.
- [ ] CI green on `main`: fmt + clippy `-D warnings` + fast tests, cached OpenCV, warm run ≤ ~8 min (spec Phase 2 criterion).
- [ ] Byte gates still green (`nullpatch_is_byte_identical`, `inject_round_trip_exact`, e2e clean zones bit-exact) — CLAUDE.md: keep passing, forever.
- [ ] CLI stdout on the test clip byte-identical to pre-Plan-2 (Task 5 transcript diff).
- [ ] GUI: Task 9 checklist fully green (drop/queue/progress/log, M2=CLI byte-identical output, cancel/healthy/error rows, batch of 3 with concurrency 2, settings persistence).
- [ ] v0.1.0 GitHub Release exists with the portable zip; clean-PATH CLI + GUI smoke passed from the downloaded zip (spec Phase 4 / parent criterion 4).
- [ ] Docs: root README (pilot-facing), rust/README (app), packaging/README.txt, docs/release-checklist.md, CLAUDE.md status line — all committed.
- [ ] Multi-clip validation explicitly NOT done (deferred post-release by user decision, 2026-07-18) — noted in CLAUDE.md line above.
