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
