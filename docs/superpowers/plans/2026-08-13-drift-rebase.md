# Drift Rebase with Capped Decay — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the monster-burst drift bridge (the ~150 °/s sub-2 Hz fake
rotation `splice_orientation` injects to land each optical patch back on the
raw endpoint) by carrying the drift forward as a slowly-decaying constant
orientation offset that is invisible to Gyroflow.

**Architecture:** `splice_orientation` (Python `python/o4fix.py` and Rust
`o4core/src/patch.rs`) gains a running offset quat `O`, applied as
`q_out = O(t) ⊗ q_raw` outside bursts and decaying toward identity at a
capped rate. Bursts whose implied bridge rate `1.5·drift/duration` exceeds a
gate skip the smoothstep endpoint correction and fold their drift into `O`.
Two new knobs (`--drift-rebase-above`, `--drift-decay-rate`) flow through
CLI/Config/GUI exactly like `--gyro-trust-noise` did. A Gyroflow render
kill-test runs before any implementation; the full render harness gates the
default-on decision.

**Tech Stack:** Python 3.12 (numpy/scipy/opencv/telemetry-parser), Rust
(cargo workspace at repo root), Gyroflow 1.6.3 CLI renders, existing
analysis harness under `python/analysis/`.

**Spec:** `docs/superpowers/specs/2026-08-13-drift-rebase-design.md`

## Global Constraints

- Repo root: `C:\Users\lzhan\Desktop\o4prostab`. Run cargo from the root.
- Cargo builds need LLVM appended to PATH or the opencv build script dies
  with STATUS_DLL_NOT_FOUND: bash `PATH="$PATH:/c/Program Files/LLVM/bin"`.
- Gyroflow store exe refuses direct CreateProcess. Render with:
  `cmd /c start "" /wait "C:\Program Files\WindowsApps\29160AdrianRoss.Gyroflow_1.63.2453.0_x64__q81n4e8pq4bra\Gyroflow.exe" PROJECT.gyroflow -f --stdout-progress`
  (~90 s per 176 s clip on this machine; wait for exit, check output file).
- NEVER overwrite files in `sample_vids/` except the four `_fixed.MP4`
  regenerations explicitly listed in Task 6. All experiment outputs go to
  the session scratchpad or `python/analysis/cache/`.
- Bit-exactness invariant: with `--drift-rebase-above 0` the pipeline output
  must be byte-identical to today's. Samples where the offset is identity
  keep original bytes.
- Python is the reference implementation; Rust must match at quat level
  (goldens). Regenerate goldens only in Task 6 after defaults are final.
- Decimal defaults from the spec: decay cap 1.5 °/s; gate default chosen in
  Task 5 (candidate 30 °/s), constraint: no 0021 burst may qualify.
- After any o4core change, rebuild `target\release` (stale-build incident
  2026-08-13: the user launches the repo's target\release GUI).
- Commit messages end with:
  `Claude-Session: https://claude.ai/code/session_01XCfP7bCJb4qFCXAiGCs7Bp`

---

### Task 1: Kill-test — constant-offset invariance of Gyroflow stabilization

Confirms the design premise: left-multiplying the whole quat trajectory by a
constant rotation leaves Gyroflow's per-frame corrections unchanged. If this
fails, STOP THE PLAN and report — every later task depends on it.

**Files:**
- Create: `python/tools/offset_killtest.py` (kept in repo — it re-validates
  the premise against future Gyroflow versions)
- Scratch outputs: `<scratchpad>/killtest/` (offset MP4, projects, renders,
  metadata JSONs)

**Interfaces:**
- Consumes: `o4fix.extract_quats(path) -> (t, q, meta)`,
  `o4fix.quat_mul/quat_exp`, `mp4patch.inject_and_check(video, q_target, out) -> bool`
- Produces: PASS/FAIL verdict printed by the script; no code consumed later.

- [ ] **Step 1: Write the offset-injection + comparison script**

```python
#!/usr/bin/env python3
"""Kill-test: a constant orientation offset must not change Gyroflow output.

Usage:
  python python/tools/offset_killtest.py build SRC.MP4 OUT.MP4
      # writes OUT.MP4 = SRC with ALL quats left-multiplied by a fixed 30 deg
      # rotation about (1,1,1)/sqrt(3)
  python python/tools/offset_killtest.py compare BASE.json OFFSET.json
      # BASE/OFFSET = Gyroflow --export-metadata 3 outputs for the original
      # and offset files; PASSes when per-frame corrections match
"""
import json
import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import mp4patch
import o4fix


def build(src, out):
    t, q, meta = o4fix.extract_quats(src)
    axis = np.array([1.0, 1.0, 1.0]) / np.sqrt(3.0)
    R = o4fix.quat_exp(np.radians(30.0) * axis[None, :])[0]
    qo = o4fix.quat_mul(np.broadcast_to(R, q.shape).copy(), q)
    qo /= np.linalg.norm(qo, axis=1, keepdims=True)
    ok = mp4patch.inject_and_check(str(src), qo, str(out))
    print("inject:", "OK" if ok else "FAILED")
    sys.exit(0 if ok else 1)


def _corrections(path):
    """Per-frame correction quats q_smooth^-1 (x) q_actual, wxyz array."""
    d = json.loads(Path(path).read_text())
    # export-metadata 3 carries per-frame original and stabilized quats;
    # accept both naming variants seen across Gyroflow versions
    frames = d["frames"] if "frames" in d else d
    org, stab = [], []
    for f in frames:
        org.append(f.get("original_quat") or f.get("org_quat") or f["org"])
        stab.append(f.get("stab_quat") or f.get("smoothed_quat") or f["stab"])
    org = np.asarray(org, dtype=np.float64)
    stab = np.asarray(stab, dtype=np.float64)
    return o4fix.quat_mul(o4fix.quat_conj(stab), org)


def compare(base_json, offset_json):
    ca = _corrections(base_json)
    cb = _corrections(offset_json)
    n = min(len(ca), len(cb))
    dots = np.abs(np.sum(ca[:n] * cb[:n], axis=1)).clip(-1, 1)
    ang = np.degrees(2 * np.arccos(dots))
    print(f"frames compared: {n}")
    print(f"correction mismatch: max {ang.max():.5f} deg, "
          f"p99 {np.percentile(ang, 99):.5f} deg")
    ok = ang.max() < 0.05
    print("KILL-TEST:", "PASS" if ok else "FAIL - STOP THE PLAN")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    cmd = sys.argv[1]
    if cmd == "build":
        build(sys.argv[2], sys.argv[3])
    elif cmd == "compare":
        compare(sys.argv[2], sys.argv[3])
    else:
        sys.exit(f"unknown command {cmd}")
```

Note for the implementer: before relying on the key names in
`_corrections`, render ONE metadata export (Step 3) and inspect the JSON's
actual schema (`python -c "import json;d=json.load(open('base.json'));..."`).
Adjust `_corrections` to the real field names — the variants listed are
best-effort guesses and adjusting them is expected, not a deviation.

- [ ] **Step 2: Build the offset clip from 0021**

```bash
cd "C:\Users\lzhan\Desktop\o4prostab"
python python/tools/offset_killtest.py build \
  sample_vids/DJI_20260711124046_0021_D.MP4 \
  "<scratchpad>/killtest/0021_offset30.MP4"
```
Expected: mp4patch round-trip prints OK / `inject: OK` (the inject gate
already verifies the parser returns exactly the injected values).

- [ ] **Step 3: Make two Gyroflow projects and render both with metadata export**

Copy `sample_vids/eval_A_embedded.gyroflow` twice into
`<scratchpad>/killtest/` as `kt_base.gyroflow` and `kt_offset.gyroflow`,
then JSON-edit (python one-liner or editor):
- both: `output.output_filename` → `kt_base.mp4` / `kt_offset.mp4`,
  `output.output_folder` → `<scratchpad>/killtest/`, bitrate 30,
  `offsets` → `{}`
- `kt_offset.gyroflow` only: `videofile` and `gyro_source.filepath` →
  `<scratchpad>/killtest/0021_offset30.MP4`, and DELETE
  `gyro_source.file_metadata` (forces re-read of the patched telemetry).

Render each (about 90 s each):
```powershell
cmd /c start "" /wait "C:\Program Files\WindowsApps\29160AdrianRoss.Gyroflow_1.63.2453.0_x64__q81n4e8pq4bra\Gyroflow.exe" "<scratchpad>\killtest\kt_base.gyroflow" -f --stdout-progress --export-metadata "3:<scratchpad>\killtest\base.json"
cmd /c start "" /wait "C:\Program Files\WindowsApps\29160AdrianRoss.Gyroflow_1.63.2453.0_x64__q81n4e8pq4bra\Gyroflow.exe" "<scratchpad>\killtest\kt_offset.gyroflow" -f --stdout-progress --export-metadata "3:<scratchpad>\killtest\offset.json"
```
Expected: both `kt_base.mp4` and `kt_offset.mp4` exist and both JSONs are
non-empty. If `--export-metadata` is rejected by 1.6.3, fall back to
`python/analysis/eval_render.py` on both renders (cache
`DJI_20260711124046_0021_D_rates.npz` already exists) and compare banded
residuals instead — pass criterion: every band in every named window equal
within 0.3 °/s.

- [ ] **Step 4: Compare corrections**

```bash
python python/tools/offset_killtest.py compare \
  "<scratchpad>/killtest/base.json" "<scratchpad>/killtest/offset.json"
```
Expected: `KILL-TEST: PASS` (max mismatch < 0.05°).
**If FAIL: stop the plan, report to the user that Gyroflow has an
absolute-attitude dependency and the rebase design is dead.**

- [ ] **Step 5: Commit the tool**

```bash
git add python/tools/offset_killtest.py
git commit -m "test: constant-offset invariance kill-test for drift rebase

Claude-Session: https://claude.ai/code/session_01XCfP7bCJb4qFCXAiGCs7Bp"
```

---

### Task 2: Python core — offset-carrying `splice_orientation`

**Files:**
- Modify: `python/o4fix.py:137-176` (`splice_orientation`; add helper
  `_apply_offset_span` above it)
- Test: `python/tests/test_splice_rebase.py` (new; first Python test file —
  create the directory, no `__init__.py` needed for pytest)

**Interfaces:**
- Consumes: existing `quat_mul, quat_conj, quat_exp, quat_log, slerp,
  smoothstep` from `o4fix.py`.
- Produces: `splice_orientation(t, q_raw, omega_patch_rad, intervals,
  ramp_s, rebase_above=0.0, decay_rate=1.5) -> (q_out, stats)` where
  `stats` items are now 4-tuples `(a, b, drift_deg, rebased: bool)`.
  Task 3 (CLI) and Task 4 (Rust) rely on exactly this signature.

- [ ] **Step 1: Write the failing tests**

```python
"""Unit tests for the drift-rebase splice (spec 2026-08-13)."""
import sys
from pathlib import Path

import numpy as np
import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import o4fix


def make_case(drift_axis=(0.0, 0.0, 1.0), drift_deg=60.0):
    """1 kHz still-camera clip, one 1 s burst [2, 3] whose raw endpoint is
    drift_deg away from where zero patch rates integrate to."""
    fs = 1000
    t = np.arange(0, 10.0, 1.0 / fs)
    n = len(t)
    # raw quats rotate to the drifted attitude linearly across the burst
    ax = np.asarray(drift_axis, dtype=float)
    ax /= np.linalg.norm(ax)
    in_burst = (t >= 2.0) & (t <= 3.0)
    frac = np.clip((t - 2.0) / 1.0, 0.0, 1.0)
    q_raw = o4fix.quat_exp(np.radians(drift_deg) * frac[:, None] * ax[None, :])
    omega = np.zeros((n - 1, 3))  # optical says: camera did not move
    return t, q_raw, omega


def rates_deg(t, q):
    tm, om = o4fix.quats_to_rates(t, q)
    return tm, np.degrees(om)


def test_rebase_off_is_todays_behavior():
    t, q_raw, omega = make_case()
    q_out, stats = o4fix.splice_orientation(
        t, q_raw, omega, [(2.0, 3.0)], 0.3, rebase_above=0.0)
    # endpoints pinned to raw (1-ulp slerp wobble allowed: only samples
    # OUTSIDE intervals carry the byte guarantee), outside bit-identical
    assert stats[0][3] is False
    i1 = np.searchsorted(t, 3.0, "right") - 1
    dot = abs(float(np.dot(q_out[i1], q_raw[i1])))
    assert np.degrees(2 * np.arccos(min(dot, 1.0))) < 1e-7
    outside = (t < 2.0) | (t > 3.0)
    assert np.array_equal(q_out[outside], q_raw[outside])
    # the bridge injects fake rate inside the burst (that's the old floor)
    tm, om = rates_deg(t, q_out)
    mid = (tm > 2.2) & (tm < 2.8)
    assert np.linalg.norm(om[mid], axis=1).max() > 30.0


def test_rebase_kills_in_burst_fake_rate():
    t, q_raw, omega = make_case(drift_deg=60.0)  # implied 1.5*60/1 = 90 deg/s
    q_out, stats = o4fix.splice_orientation(
        t, q_raw, omega, [(2.0, 3.0)], 0.3,
        rebase_above=30.0, decay_rate=1.5)
    (a, b, drift, rebased) = stats[0]
    assert rebased is True
    assert drift == pytest.approx(60.0, abs=1.0)
    tm, om = rates_deg(t, q_out)
    # inside the burst: no bridge -> output rate ~ optical (zero), except
    # the 0.3 s edge ramps which blend real raw rate content
    mid = (tm > 2.35) & (tm < 2.65)
    assert np.linalg.norm(om[mid], axis=1).max() < 2.0
    # after the burst: offset decays at <= decay_rate (fake pan capped)
    post = (tm > 3.4) & (tm < 40.0)
    assert np.linalg.norm(om[post], axis=1).max() < 1.5 + 0.1


def test_offset_decays_to_bit_identical_raw():
    t, q_raw, omega = make_case(drift_deg=6.0)   # implied 9 deg/s
    q_out, stats = o4fix.splice_orientation(
        t, q_raw, omega, [(2.0, 3.0)], 0.3,
        rebase_above=5.0, decay_rate=1.5)
    assert stats[0][3] is True
    # 6 deg at 1.5 deg/s -> gone 4 s after the burst; far samples bit-exact
    far = t > 3.0 + 6.0 / 1.5 + 0.5
    assert np.array_equal(q_out[far], q_raw[far])


def test_offsets_compose_across_bursts():
    fs = 1000
    t = np.arange(0, 20.0, 1.0 / fs)
    n = len(t)
    ax = np.array([0.0, 0.0, 1.0])
    # two bursts, each drifting raw 40 deg further about z
    frac = (np.clip((t - 2.0), 0, 1) + np.clip((t - 8.0), 0, 1))
    q_raw = o4fix.quat_exp(np.radians(40.0) * frac[:, None] * ax[None, :])
    omega = np.zeros((n - 1, 3))
    q_out, stats = o4fix.splice_orientation(
        t, q_raw, omega, [(2.0, 3.0), (8.0, 9.0)], 0.3,
        rebase_above=30.0, decay_rate=0.0)   # carry forever
    assert [s[3] for s in stats] == [True, True]
    # with zero decay the offset after burst 2 is the composed 80 deg:
    # q_out stays at identity attitude (optical said "no motion") forever
    tm, om = rates_deg(t, q_out)
    quiet = (tm > 9.4)
    assert np.linalg.norm(om[quiet], axis=1).max() < 0.5
    end_err = o4fix.quat_log(q_out[-1:])  # attitude still ~identity
    assert np.degrees(np.linalg.norm(end_err)) < 1.0


def test_identity_offset_spans_bit_identical():
    t, q_raw, omega = make_case()
    q_out, _ = o4fix.splice_orientation(
        t, q_raw, omega, [(2.0, 3.0)], 0.3,
        rebase_above=1000.0, decay_rate=1.5)  # gate never trips
    outside = (t < 2.0) | (t > 3.0)
    assert np.array_equal(q_out[outside], q_raw[outside])
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd "C:\Users\lzhan\Desktop\o4prostab" && python -m pytest python/tests/test_splice_rebase.py -v
```
Expected: FAIL — `splice_orientation() got an unexpected keyword argument
'rebase_above'` (5 failures/errors). (`pip install pytest` first if absent.)

- [ ] **Step 3: Implement the rebase**

Replace `splice_orientation` in `python/o4fix.py` with (docstring: keep the
existing one, append the two paragraphs shown):

```python
def _apply_offset_span(q_out, q_raw, t, off, k0, k1, decay_rate):
    """Write q_out[k0:k1] = O(t) (x) q_raw with the offset O decaying toward
    identity at decay_rate deg/s (0 = carry forever). Samples reached after
    the decay completes are left untouched (bit-identical raw). Returns the
    offset remaining at index k1."""
    v = quat_log(off[None, :])[0]
    ang = np.linalg.norm(v)
    if ang < 1e-12 or k1 <= k0:
        return off if ang >= 1e-12 else np.array([1.0, 0.0, 0.0, 0.0])
    axis = v / ang
    if decay_rate > 0:
        angs = np.maximum(ang - np.radians(decay_rate) * (t[k0:k1] - t[k0]),
                          0.0)
        rem = max(ang - np.radians(decay_rate) *
                  (t[min(k1, len(t) - 1)] - t[k0]), 0.0)
    else:
        angs = np.full(k1 - k0, ang)
        rem = ang
    live = angs > 0
    if live.any():
        offs = quat_exp(angs[live, None] * axis[None, :])
        q_out[k0:k1][live] = quat_mul(offs, q_raw[k0:k1][live])
    return quat_exp((rem * axis)[None, :])[0]


def splice_orientation(t, q_raw, omega_patch_rad, intervals, ramp_s,
                       rebase_above=0.0, decay_rate=1.5):
    """<existing docstring unchanged, then append:>

    Drift rebase (spec 2026-08-13): bursts whose implied bridge rate
    1.5*drift/duration exceeds rebase_above (deg/s; 0 disables) skip the
    smoothstep endpoint correction entirely - the path lands on the optical
    endpoint and the drift is carried forward as a constant orientation
    offset on the following samples (invisible to stabilization, which only
    sees rate of orientation error). The offset decays toward identity at
    decay_rate deg/s (0 = carry forever) and composes across bursts.
    Samples under an identity offset keep their original bit patterns.
    """
    q_out = q_raw.copy()
    stats = []
    off = np.array([1.0, 0.0, 0.0, 0.0])
    prev_end = 0
    for (a, b) in intervals:
        i0 = max(int(np.searchsorted(t, a, "left")), 0)
        i1 = min(int(np.searchsorted(t, b, "right")) - 1, len(t) - 1)
        if i1 - i0 < 8:
            continue
        off = _apply_offset_span(q_out, q_raw, t, off, prev_end, i0,
                                 decay_rate)
        off_pre = off
        n = i1 - i0
        dt = np.diff(t[i0:i1 + 1])
        dq = quat_exp(omega_patch_rad[i0:i1] * dt[:, None])
        qs = np.empty((n + 1, 4))
        qs[0] = quat_mul(off_pre[None, :], q_raw[i0:i0 + 1])[0]
        for k in range(n):
            qs[k + 1] = quat_mul(qs[k], dq[k])
        qs /= np.linalg.norm(qs, axis=1, keepdims=True)

        q_end_base = quat_mul(off_pre[None, :], q_raw[i1:i1 + 1])
        e = quat_log(quat_mul(quat_conj(qs[-1:]), q_end_base))[0]
        drift_deg = np.degrees(np.linalg.norm(e))
        dur = max(t[i1] - t[i0], 1e-9)
        rebased = bool(rebase_above > 0
                       and 1.5 * drift_deg / dur > rebase_above)
        s = smoothstep((t[i0:i1 + 1] - t[i0]) / dur)
        if rebased:
            off = quat_mul(qs[-1:], quat_conj(q_raw[i1:i1 + 1]))[0]
            off /= np.linalg.norm(off)
        else:
            qs = quat_mul(qs, quat_exp(s[:, None] * e[None, :]))

        # base path the edge ramps blend toward: pre-offset frame at entry,
        # post-offset frame at exit (identical unless rebased); smoothstep
        # interpolation keeps it continuous and flat at both edges
        d_off = quat_log(quat_mul(off[None, :], quat_conj(off_pre[None, :])))
        offs = quat_mul(quat_exp(s[:, None] * d_off),
                        np.broadcast_to(off_pre, (n + 1, 4)))
        base = quat_mul(offs, q_raw[i0:i1 + 1])
        if not rebased and np.linalg.norm(quat_log(off_pre[None, :])) < 1e-12:
            base = q_raw[i0:i1 + 1]  # exact original path (bit-identical)

        tt = t[i0:i1 + 1]
        r = np.minimum(smoothstep((tt - tt[0]) / ramp_s),
                       smoothstep((tt[-1] - tt) / ramp_s))
        q_out[i0:i1 + 1] = slerp(base, qs, r)
        stats.append((a, b, drift_deg, rebased))
        prev_end = i1 + 1
    _apply_offset_span(q_out, q_raw, t, off, prev_end, len(t), decay_rate)
    return q_out, stats
```

Care points (why the code is shaped this way — keep these properties):
- With `rebase_above=0` and identity offset, every operation reduces to the
  pre-change code path (`base = q_raw` slice, same `e`, same slerp), so the
  output is bit-identical to today's — `test_rebase_off_is_todays_behavior`
  plus Task 3's byte-diff gate enforce this.
- `_apply_offset_span` never touches samples once the decay hits zero, and
  is skipped entirely for identity offsets — that is what keeps clean clips
  byte-exact.
- `prev_end` tracking freezes decay inside bursts (spec §2).

- [ ] **Step 4: Run tests to verify they pass**

```bash
python -m pytest python/tests/test_splice_rebase.py -v
```
Expected: 5 passed.

- [ ] **Step 5: Commit**

```bash
git add python/o4fix.py python/tests/test_splice_rebase.py
git commit -m "feat: drift rebase with capped decay in splice_orientation

Claude-Session: https://claude.ai/code/session_01XCfP7bCJb4qFCXAiGCs7Bp"
```

---

### Task 3: Python CLI flags + pipeline wiring + regression byte-gate

**Files:**
- Modify: `python/o4fix.py` — `process_mp4` (call site + stats print,
  currently lines ~677-681) and the argparse MP4-repair group (~line 703).

**Interfaces:**
- Consumes: Task 2's `splice_orientation(..., rebase_above, decay_rate)`
  and 4-tuple stats.
- Produces: CLI flags `--drift-rebase-above` (default **0.0** until Task 5
  flips it) and `--drift-decay-rate` (default 1.5); per-burst print gains a
  `REBASED` suffix. Rust Task 4 mirrors these names exactly.

- [ ] **Step 1: Wire the flags**

In the `m = p.add_argument_group("MP4 repair tuning (default mode)")` block
add:

```python
    m.add_argument("--drift-rebase-above", type=float, default=0.0,
                   metavar="DEG_S",
                   help="deg/s implied bridge rate (1.5*drift/duration) "
                        "above which a burst's optical drift is carried "
                        "forward as a constant orientation offset instead "
                        "of being bridged inside the burst (0 = always "
                        "bridge in-burst). A constant offset is invisible "
                        "to stabilization; do not enable with Gyroflow "
                        "horizon lock ON")
    m.add_argument("--drift-decay-rate", type=float, default=1.5,
                   metavar="DEG_S",
                   help="deg/s cap at which a carried drift offset bleeds "
                        "back to identity in the following clean zone "
                        "(0 = carry forever; default 1.5)")
```

In `process_mp4` replace the splice call + stats loop with:

```python
    q_out, stats = splice_orientation(t, q_raw, patched, intervals,
                                      args.ramp,
                                      rebase_above=args.drift_rebase_above,
                                      decay_rate=args.drift_decay_rate)
    for a, b, drift, rebased in stats:
        tag = "  REBASED" if rebased else ""
        print(f"     [{a:7.2f}, {b:7.2f}] optical drift over burst: "
              f"{drift:5.2f} deg{tag}")
```

- [ ] **Step 2: Byte-identity regression gate (flags off)**

The 0060 fixed file in `sample_vids` was regenerated 2026-08-13 by the
current (pre-rebase) pipeline — it is the exact regression reference.

```bash
cd "C:\Users\lzhan\Desktop\o4prostab"
python python/o4fix.py sample_vids/DJI_20260808151831_0060_D.MP4 \
  -o "<scratchpad>/0060_pyoff.MP4"
cmp "<scratchpad>/0060_pyoff.MP4" sample_vids/DJI_20260808151831_0060_D_fixed.MP4 && echo BYTE-IDENTICAL
```
Expected: `BYTE-IDENTICAL` (Python vs Rust output parity was locked by
goldens; if a stray RANSAC draw makes `cmp` fail, fall back to comparing
extracted quats with `np.array_equal` outside severe bursts and max
angular diff < 0.01° inside — that is the Python/Rust seeded-vs-unseeded
allowance, not a rebase bug. Investigate anything larger.)

- [ ] **Step 3: Smoke the rebase end-to-end on 0060**

```bash
python python/o4fix.py sample_vids/DJI_20260808151831_0060_D.MP4 \
  --drift-rebase-above 30 -o "<scratchpad>/0060_rebase.MP4"
```
Expected: the 5 monster bursts (100.38, 105.83, 225.03, 248.34, 308.01 s;
drifts 125/58/103/103/175°) print `REBASED`; the other 22 do not
(largest non-monster implied rate on this clip is well under 30). Then
verify in telemetry (script `<scratchpad>/check_rebase.py`, pattern below):

```python
import sys
sys.path.insert(0, r"C:\Users\lzhan\Desktop\o4prostab\python")
import numpy as np, o4fix
t, q, _ = o4fix.extract_quats(r"<scratchpad>/0060_rebase.MP4")
tm, om = o4fix.quats_to_rates(t, q)
deg = np.degrees(om)
for (a, b) in [(100.38, 101.49), (248.34, 249.32)]:
    m = (tm >= a + 0.35) & (tm <= b - 0.35)   # inside the edge ramps
    mag = np.linalg.norm(deg[m], axis=1)
    print(f"[{a},{b}] in-burst |rate| max {mag.max():.1f} deg/s")
# post-burst decay: rate magnitude in the 60 s after 249.32 must never
# exceed real motion + 1.6 deg/s of decay pan -- eyeball against the raw
# rates printed alongside:
```
Expected: in-burst |rate| tracks the optical patch (tens of °/s, no
100°+/s bridge sweep). Compare the same windows in the Task-2-era fixed
file to see the bridge gone.

- [ ] **Step 4: Run the unit tests once more, then commit**

```bash
python -m pytest python/tests/test_splice_rebase.py -v
git add python/o4fix.py
git commit -m "feat: --drift-rebase-above / --drift-decay-rate CLI flags

Claude-Session: https://claude.ai/code/session_01XCfP7bCJb4qFCXAiGCs7Bp"
```

---

### Task 4: Rust port — o4core + CLI + GUI (flag parity, default still off)

**Files:**
- Modify: `o4core/src/config.rs` (Config + Default), `o4core/src/patch.rs`
  (`splice_orientation`, `BurstStat`), `o4core/src/pipeline.rs:199-209`
  (call + print), `o4fix-cli/src/args.rs` (2 flags + `to_config`),
  `o4fix-app/src/settings.rs` (both conversions), `o4fix-app/ui/help.js`
  (DEFAULTS + HELP + FIELDS).
- Test: `o4core/src/patch.rs` `#[cfg(test)]` module (mirror the Python
  unit tests).

**Interfaces:**
- Consumes: Task 2's Python semantics (reference implementation).
- Produces: `Config { drift_rebase_above: f64, drift_decay_rate: f64, .. }`
  (defaults 0.0 / 1.5); `splice_orientation(t, q_raw, omega_patch,
  intervals, ramp_s, rebase_above, decay_rate)`;
  `BurstStat { start, end, drift_deg, rebased: bool }`. Task 6 flips the
  defaults.

- [ ] **Step 1: Config fields**

In `o4core/src/config.rs` add to the struct (after `anchor_cutoff`):

```rust
    pub drift_rebase_above: f64,
    pub drift_decay_rate: f64,
```
and to `Default::default()`:
```rust
            drift_rebase_above: 0.0,
            drift_decay_rate: 1.5,
```
Extend `defaults_match_spec` with
`assert_eq!(c.drift_rebase_above, 0.0); assert_eq!(c.drift_decay_rate, 1.5);`

- [ ] **Step 2: Port `splice_orientation` (with failing tests first)**

Add tests to `o4core/src/patch.rs`'s test module that mirror the Python
tests exactly (same synthetic construction, same assertions —
`make_case`: 1 kHz `t` over 10 s, raw quats `qexp(radians(60)*frac*z)`,
zero `omega_patch`, burst (2.0, 3.0)):
`rebase_off_is_previous_behavior`, `rebase_kills_in_burst_fake_rate`,
`offset_decays_to_bit_identical_raw` (use `==` on the arrays — f64
bit-compare), `offsets_compose_across_bursts`,
`identity_offset_spans_bit_identical`. Run
`cargo test -p o4core splice` → compile FAIL (missing args), then port:

```rust
pub struct BurstStat {
    pub start: f64,
    pub end: f64,
    pub drift_deg: f64,
    pub rebased: bool,
}

fn apply_offset_span(
    q_out: &mut [[f64; 4]],
    q_raw: &[[f64; 4]],
    t: &[f64],
    off: [f64; 4],
    k0: usize,
    k1: usize,
    decay_rate: f64,
) -> [f64; 4] {
    use crate::quat::{qexp, qlog, qmul, qnorm};
    let v = qlog(off);
    let ang = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if ang < 1e-12 || k1 <= k0 {
        return if ang < 1e-12 { [1.0, 0.0, 0.0, 0.0] } else { off };
    }
    let axis = [v[0] / ang, v[1] / ang, v[2] / ang];
    let rate = decay_rate.to_radians();
    let mut rem = ang;
    for k in k0..k1 {
        let a = if decay_rate > 0.0 {
            (ang - rate * (t[k] - t[k0])).max(0.0)
        } else {
            ang
        };
        if a > 0.0 {
            let o = qexp([a * axis[0], a * axis[1], a * axis[2]]);
            q_out[k] = qmul(o, q_raw[k]);
        }
    }
    if decay_rate > 0.0 {
        rem = (ang - rate * (t[k1.min(t.len() - 1)] - t[k0])).max(0.0);
    }
    qnorm(qexp([rem * axis[0], rem * axis[1], rem * axis[2]]))
}

pub fn splice_orientation(
    t: &[f64],
    q_raw: &[[f64; 4]],
    omega_patch: &[[f64; 3]],
    intervals: &[(f64, f64)],
    ramp_s: f64,
    rebase_above: f64,
    decay_rate: f64,
) -> (Vec<[f64; 4]>, Vec<BurstStat>) {
    use crate::quat::{qconj, qexp, qlog, qmul, qnorm, slerp, smoothstep};
    let mut q_out = q_raw.to_vec();
    let mut stats = Vec::new();
    let mut off = [1.0, 0.0, 0.0, 0.0];
    let mut prev_end = 0usize;
    for &(a, b) in intervals {
        let i0 = crate::dsp::searchsorted_left(t, a);
        let i1 = crate::dsp::searchsorted_right(t, b)
            .saturating_sub(1)
            .min(t.len() - 1);
        if i1 < i0 + 8 {
            continue;
        }
        off = apply_offset_span(&mut q_out, q_raw, t, off, prev_end, i0,
                                decay_rate);
        let off_pre = off;
        let n = i1 - i0;
        let mut qs: Vec<[f64; 4]> = Vec::with_capacity(n + 1);
        qs.push(qmul(off_pre, q_raw[i0]));
        for k in 0..n {
            let dt = t[i0 + k + 1] - t[i0 + k];
            let o = omega_patch[i0 + k];
            qs.push(qmul(qs[k], qexp([o[0] * dt, o[1] * dt, o[2] * dt])));
        }
        for q in qs.iter_mut() {
            *q = qnorm(*q);
        } // python normalizes once, after the loop

        let e = qlog(qmul(qconj(qs[n]), qmul(off_pre, q_raw[i1])));
        let drift_deg = (e[0] * e[0] + e[1] * e[1] + e[2] * e[2])
            .sqrt()
            .to_degrees();
        let dur = (t[i1] - t[i0]).max(1e-9);
        let rebased = rebase_above > 0.0
            && 1.5 * drift_deg / dur > rebase_above;
        if rebased {
            off = qnorm(qmul(qs[n], qconj(q_raw[i1])));
        } else {
            for k in 0..=n {
                let s = smoothstep((t[i0 + k] - t[i0]) / dur);
                qs[k] = qmul(qs[k], qexp([s * e[0], s * e[1], s * e[2]]));
                // python does NOT renormalize here - neither do we
            }
        }

        let d_off = qlog(qmul(off, qconj(off_pre)));
        let off_pre_ang = {
            let v = qlog(off_pre);
            (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
        };
        for k in 0..=n {
            let tt = t[i0 + k];
            let s = smoothstep((tt - t[i0]) / dur);
            let base = if !rebased && off_pre_ang < 1e-12 {
                q_raw[i0 + k] // exact original path (bit-identical branch)
            } else {
                let o = qmul(
                    qexp([s * d_off[0], s * d_off[1], s * d_off[2]]),
                    off_pre,
                );
                qmul(o, q_raw[i0 + k])
            };
            let r = smoothstep((tt - t[i0]) / ramp_s)
                .min(smoothstep((t[i1] - tt) / ramp_s));
            q_out[i0 + k] = slerp(base, qs[k], r);
        }
        stats.push(BurstStat { start: a, end: b, drift_deg, rebased });
        prev_end = i1 + 1;
    }
    apply_offset_span(&mut q_out, q_raw, t, off, prev_end, t.len(),
                      decay_rate);
    (q_out, stats)
}
```

Port care point: keep operation ORDER identical to Python (qs normalized
once after integration; smoothstep correction not renormalized; slerp
last) — quat-level parity with the seeded goldens depends on it.

- [ ] **Step 3: Pipeline call + print**

`o4core/src/pipeline.rs` (call at line ~199):

```rust
    let (q_out, bursts) = patch::splice_orientation(
        &tel.t, &tel.q, &patched, &intervals, cfg.ramp,
        cfg.drift_rebase_above, cfg.drift_decay_rate,
    );
    for b in &bursts {
        say(
            Stage::Splice,
            0.87,
            format!(
                "     [{:7.2}, {:7.2}] optical drift over burst: {:5.2} deg{}",
                b.start, b.end, b.drift_deg,
                if b.rebased { "  REBASED" } else { "" }
            ),
        );
    }
```

`BurstStat` gained a field — the compiler will flag every other consumer
(GUI event serialization in o4fix-app, any Clone/serde derives): extend
them to carry `rebased` through rather than dropping it.

- [ ] **Step 4: CLI args (`o4fix-cli/src/args.rs`)**

After the `anchor_cutoff` field (match the file's clap style):
```rust
    #[arg(long, default_value_t = 0.0, value_name = "DEG_S")]
    pub drift_rebase_above: f64,
    #[arg(long, default_value_t = 1.5, value_name = "DEG_S")]
    pub drift_decay_rate: f64,
```
(copy the help strings verbatim from the Python argparse text in Task 3 —
this file mirrors o4fix.py field-for-field) and in `to_config()`:
```rust
            drift_rebase_above: self.drift_rebase_above,
            drift_decay_rate: self.drift_decay_rate,
```

- [ ] **Step 5: GUI (`o4fix-app/src/settings.rs`, `o4fix-app/ui/help.js`)**

settings.rs — struct field pair, both conversion impls:
```rust
    pub drift_rebase_above: f64,
    pub drift_decay_rate: f64,
```
```rust
            drift_rebase_above: c.drift_rebase_above,
            drift_decay_rate: c.drift_decay_rate,
```
```rust
            drift_rebase_above: self.drift_rebase_above,
            drift_decay_rate: self.drift_decay_rate,
```
help.js — DEFAULTS: `drift_rebase_above: 0.0, drift_decay_rate: 1.5,`
HELP entries (reuse the argparse help text), FIELDS rows after the
`gyro_trust_noise` row:
```js
  ["Optical", "drift_rebase_above", "Drift rebase above (°/s implied, 0 = off)", "num"],
  ["Optical", "drift_decay_rate", "Drift decay rate (°/s)", "num"],
```
Also handle settings.json migration: the app must tolerate a settings file
missing the new keys (serde `#[serde(default = ...)]` with functions
returning 0.0 / 1.5 — check how existing Option fields deserialize and
follow that pattern; the 2026-08-13 incident shows old settings files WILL
be loaded by the new app).

- [ ] **Step 6: Build + tests + CLI parity smoke**

```bash
cd "C:\Users\lzhan\Desktop\o4prostab"
PATH="$PATH:/c/Program Files/LLVM/bin" cargo build --release
cargo test -p o4core splice
cargo test -p o4fix-cli
./target/release/o4fix.exe sample_vids/DJI_20260808151831_0060_D.MP4 \
  --drift-rebase-above 30 -o "<scratchpad>/0060_rebase_rs.MP4"
```
Expected: build clean, splice tests pass, same 5 bursts print `REBASED`.
Quat-parity vs the Python run from Task 3 Step 3
(`<scratchpad>/0060_rebase.MP4`): extract both, `np.array_equal` outside
severe bursts, max angular diff < 0.01° inside (RANSAC draw allowance).

- [ ] **Step 7: Commit**

```bash
git add o4core o4fix-cli o4fix-app
git commit -m "feat: drift rebase ported to o4core + CLI + GUI

Claude-Session: https://claude.ai/code/session_01XCfP7bCJb4qFCXAiGCs7Bp"
```

---

### Task 5: Harness validation + gate-threshold decision

The render loop that decides whether the feature ships default-on and at
what gate. Everything renders from `<scratchpad>/renders/`; keep the two
final A/B renders for the user.

**Files:**
- Create: `<scratchpad>/renders/` projects + renders,
  `<scratchpad>/eval_windows.py` (window-metric script, code below)
- Uses: `python/analysis/make_cache.py`, `python/analysis/eval_render.py`

**Interfaces:**
- Consumes: Task 3/4 outputs (`0060_rebase.MP4` etc.).
- Produces: the default gate value (written into Task 6), renders
  `eval60_BRIDGE.mp4` / `eval60_REBASE.mp4`, `eval27_REBASE.mp4` for user
  eyeballing.

- [ ] **Step 1: Analytic sweep — implied bridge rates per clip**

Script pattern (extend `<scratchpad>/check_rebase.py`): for each of 0021,
0027, 0057-0060, run the pipeline stages up to `splice_orientation` (or
simply re-run `o4fix.exe` with `--drift-rebase-above 999999` and parse the
printed drift/duration per burst) and tabulate `1.5*drift/dur`. Pick the
default gate as the largest round value that (a) trips every burst with
implied rate ≥ 100 °/s on the monster clips and (b) trips NOTHING on 0021.
Candidate 30; adjust from the table (0021's max implied is ~17 °/s from
powerloop 55°/5 s — verify).
Gate check: `python python/o4fix.py sample_vids/DJI_20260711124046_0021_D.MP4 --drift-rebase-above <chosen> -o "<scratchpad>/0021_rebase.MP4"`
must print zero `REBASED` lines and the output must be byte-identical
(`cmp`) to a flags-off run — that is the 0021 no-regression proof, no
render needed.

- [ ] **Step 2: Render 0060 A/B**

Regenerate the fixed file at the chosen gate (Rust exe, faster), then make
two projects by copying `sample_vids/eval_M2_tight.gyroflow` and JSON-edit
each: `videofile` + `gyro_source.filepath` → the bridge-mode fixed file
(`sample_vids/DJI_20260808151831_0060_D_fixed.MP4`) resp. the rebase file
(`<scratchpad>/renders/0060_rebase.MP4`); DELETE `gyro_source.file_metadata`;
`offsets` → `{}`; output filename `eval60_BRIDGE.mp4` / `eval60_REBASE.mp4`
in `<scratchpad>/renders/`; bitrate 30. Render both via the
`cmd /c start "" /wait` pattern (~4 min each for a 383 s clip).

- [ ] **Step 3: Evaluate**

```bash
python python/analysis/make_cache.py sample_vids/DJI_20260808151831_0060_D.MP4
python python/analysis/eval_render.py "<scratchpad>/renders/eval60_BRIDGE.mp4" --cache python/analysis/cache/DJI_20260808151831_0060_D_rates.npz
python python/analysis/eval_render.py "<scratchpad>/renders/eval60_REBASE.mp4" --cache python/analysis/cache/DJI_20260808151831_0060_D_rates.npz
```
(~7-15 min each; the named WINDOWS in eval_render.py are 0021-specific —
ignore them, use the series npz.) Then window metrics, script
`<scratchpad>/eval_windows.py`:

```python
"""Banded residual RMS in given windows from eval_render series caches."""
import sys
from pathlib import Path

import numpy as np
from scipy.signal import butter, filtfilt

cache = Path(r"C:\Users\lzhan\Desktop\o4prostab\python\analysis\cache")


def series(stem):
    d = np.load(cache / f"eval_{stem}.npz")   # keys: t, resid_deg_s (Nx3-ish)
    return d["t"], d["resid"] if "resid" in d else d[d.files[1]]


def band(t, x, a, b, lo, hi):
    fs = 1.0 / np.median(np.diff(t))
    m = (t >= a) & (t <= b)
    if m.sum() < 30:
        return np.nan
    bb, ba = butter(2, [lo / (fs / 2), min(hi, 0.45 * fs) / (fs / 2)], "band")
    seg = filtfilt(bb, ba, x[m] - x[m].mean(0), axis=0)
    return float(np.sqrt((seg ** 2).sum(-1).mean()))


WINDOWS = {  # 0060: the 5 monster bursts (+0.5 s each side) + controls
    "monster_100": (99.9, 102.0), "monster_106": (105.3, 107.1),
    "monster_225": (224.5, 226.7), "monster_248": (247.8, 249.8),
    "monster_308": (307.5, 309.8),
    "clean_ctrl": (117.0, 128.0), "mild_ctrl": (181.0, 211.0),
}
for stem in sys.argv[1:]:
    t, r = series(stem)
    print(f"== {stem}")
    for name, (a, b) in WINDOWS.items():
        w = band(t, r, a, b, 0.3, 8)   # includes the sub-2 Hz bridge band
        s = band(t, r, a, b, 8, 30)
        print(f"  {name:12} {a:6.1f}-{b:6.1f}  wob {w:6.1f}  shk {s:6.1f}")
```
(Adapt the npz key names to what eval_render.py actually writes — open one
cache file first and list `d.files`.)

Success criteria (spec §6): every monster window's 0.3-8 Hz residual drops
materially for REBASE vs BRIDGE (target: at least halved); `clean_ctrl` /
`mild_ctrl` deltas ≤ 1.5 °/s (eval coupling noise). Post-burst drift check:
widen `monster_248` to (249.3, 290) with band 0.1-1 Hz — REBASE may exceed
BRIDGE by at most the decay pan (≤ ~1.6 °/s).

- [ ] **Step 4: Render + evaluate 0027 (drift-floor clip) the same way**

Same recipe: regenerate `<scratchpad>/renders/0027_rebase.MP4` at the
chosen gate, project from a copy of `sample_vids/eval27_V3.gyroflow`
(already points at the 0027 fixed file — only change `videofile` +
`gyro_source.filepath` to the rebase file, drop `file_metadata`, set output
`eval27_REBASE.mp4`), render, eval with
`python/analysis/cache/DJI_20260721141550_0027_D_rates.npz` (already
exists from session 7 — regenerate with make_cache.py if missing), window
metrics on its monster bursts (5.5 s and the 219.5 s flick from the
session-7 notes plus every burst the pipeline prints as REBASED, ±0.5 s)
vs the existing `eval27_V3` eval cache (`eval_eval27_V3.npz`).
Success: monster windows improve or hold; the 219.5 s flick window must
NOT regress (that window punished fake-rate ideas before).

- [ ] **Step 5: Decision + user checkpoint**

Assemble the numbers into a table and STOP for user review: present
BRIDGE vs REBASE per window, the chosen gate, and links to the two 0060
renders for eyeballing. Default-on requires user sign-off (M2 precedent:
the user decides perceptual trade-offs). If numbers are bad → keep
defaults off, ship as opt-in flag, and record findings in CLAUDE.md.

---

### Task 6: Flip defaults, goldens, docs, rebuild, release prep

Only after Task 5 sign-off. If the decision was "opt-in only", do
everything here EXCEPT the default flips.

**Files:**
- Modify: `python/o4fix.py` (argparse default), `o4core/src/config.rs`
  (Default + test), `o4fix-app/ui/help.js` (DEFAULTS), `CLAUDE.md`,
  `README.md` (horizon-lock note), `Cargo.toml` (workspace version → 0.1.2)
- Regenerate: `goldens/` via `python python/tools/dump_goldens.py`

- [ ] **Step 1: Flip the default** — `--drift-rebase-above` default 0.0 →
  the Task 5 value in all three places (argparse `default=`, Rust
  `Default::default()` + `defaults_match_spec` assert, help.js DEFAULTS).
- [ ] **Step 2: Docs** — README + GUI help gain: "If you stabilize with
  horizon lock ON, set Drift rebase above to 0 — carried orientation
  offsets would fight the horizon reference." CLAUDE.md gets a short
  section: mechanism, chosen gate, measured table, renders kept.
- [ ] **Step 3: Regenerate goldens + full test suite**

```bash
python python/tools/dump_goldens.py        # ~10-20 min
PATH="$PATH:/c/Program Files/LLVM/bin" cargo build --release
cargo test -p o4core
cargo test -p o4core -- --ignored          # golden + e2e parity
cargo test -p o4fix-cli
python -m pytest python/tests -v
```
Expected: all green. The goldens run also refreshes `goldens/ref_fixed.MP4`
with the new defaults — that is intended.
- [ ] **Step 4: Regenerate the four Aug-08 `_fixed.MP4` + spot telemetry check**

```bash
./target/release/o4fix.exe \
  sample_vids/DJI_20260808145055_0057_D.MP4 \
  sample_vids/DJI_20260808145758_0058_D.MP4 \
  sample_vids/DJI_20260808151054_0059_D.MP4 \
  sample_vids/DJI_20260808151831_0060_D.MP4
```
(default output = overwrite the `_fixed.MP4` next to each input — the ONLY
sanctioned sample_vids overwrite). Re-run the Task 3 Step 3 telemetry
check on 0060's two complaint bursts.
- [ ] **Step 5: Commit + version bump; ASK THE USER before tagging v0.1.2**
  (tag push triggers the release workflow — outward-facing, needs explicit
  go-ahead).

```bash
git add -A && git commit -m "feat: drift rebase default-on at gate <N>; docs + goldens

Claude-Session: https://claude.ai/code/session_01XCfP7bCJb4qFCXAiGCs7Bp"
```

---

## Self-review notes

- Spec §6.1 kill-test → Task 1 (STOP gate). §2 mechanism → Tasks 2/4.
  §3 gating + sweep → Tasks 2 (gate arithmetic) + 5 (value). §4 on-disk →
  bit-identity tests (Task 2) + byte gates (Tasks 3.2, 5.1). §5 surface →
  Tasks 3/4 + defaults in 6. §6 validation → Tasks 5/6. §7 horizon-lock
  doc → Task 6.2; EKF follow-up stays out of scope.
- `<scratchpad>` = the session scratchpad directory printed in the system
  prompt of whoever executes this; substitute the absolute path.
- Known judgment calls left to the implementer, explicitly allowed:
  metadata-JSON field names (Task 1 Step 1 note), eval-cache npz key names
  (Task 5 Step 3 note), serde default style (Task 4 Step 5).
