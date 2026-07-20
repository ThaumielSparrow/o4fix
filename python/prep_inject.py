#!/usr/bin/env python3
"""Build the quaternion stream to inject into the O4 Pro MP4 (see mp4patch.py).

Design (session-3 plan): base = RAW embedded quats everywhere; ONLY inside
severe noise bursts (30-180 Hz band-RMS > ~8 deg/s) replace orientation with
the integral of the B-style optical patch rates (LP8 optical + rate-aware
handback), pinned to the raw path at burst edges:
  - the accumulated optical drift over each burst is spread across the whole
    burst as a slow rotation-vector correction (smoothstep), so both endpoints
    match raw exactly with no fast fake motion, and
  - a ~0.3 s slerp cross-fade at each edge blends the rate content smoothly.
Mild zones stay bit-exact raw (measured best; optical injects fake pan/tilt
there).

Output: .npz with t (s) and q (Nx4 wxyz, telemetry-parser output frame),
one row per deduped 1 kHz sample - feed to `mp4patch.py inject`.
"""
import argparse
import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).parent))
from o4fix import (extract_quats, quats_to_rates, adaptive_clean,
                   optical_patch, find_intervals, splice_orientation)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("video")
    p.add_argument("-o", "--output", required=True, help="output .npz")
    p.add_argument("--severe", type=float, default=8.0,
                   help="deg/s 30-180 Hz band-RMS threshold for orientation "
                        "replacement (default 8)")
    p.add_argument("--severe-pad", type=float, default=0.2)
    p.add_argument("--severe-merge", type=float, default=0.2)
    p.add_argument("--ramp", type=float, default=0.3,
                   help="s, slerp cross-fade length at burst edges (default 0.3)")
    args_local, extra = p.parse_known_args()

    # o4fix args with B-style defaults (LP8 optical + rate-aware handback);
    # reuse o4fix's own parser for tuning flags so defaults stay in sync
    o4args = _build_o4fix_args(args_local.video, extra)

    video = Path(args_local.video)
    print(f"== {video.name}")
    t, q_raw, meta = extract_quats(video)
    fs = 1.0 / np.median(np.diff(t))
    print(f"   {len(q_raw)} quats @ {fs:.0f} Hz, {t[-1] - t[0]:.1f} s")

    tm, omega = quats_to_rates(t, q_raw)
    clean, diag = adaptive_clean(omega, fs, o4args)
    print("   running B-style optical patch (this decodes video sections) ...")
    patched = optical_patch(video, tm, clean, diag, fs, o4args, meta)

    # severe intervals from the same noise estimate the cleaner used
    severe_mask = diag["noise"] > args_local.severe
    intervals = find_intervals(severe_mask, tm, pad_s=args_local.severe_pad,
                               merge_s=args_local.severe_merge, min_s=0.2)
    tot = sum(b - a for a, b in intervals)
    print(f"   severe intervals (> {args_local.severe} deg/s): "
          f"{len(intervals)} covering {tot:.1f} s")

    q_out, stats = splice_orientation(t, q_raw, patched, intervals,
                                      args_local.ramp)
    for a, b, drift in stats:
        print(f"     [{a:7.2f}, {b:7.2f}] optical drift over burst: {drift:5.2f} deg")

    changed = np.any(q_out != q_raw, axis=1).mean()
    np.savez(args_local.output, t=t, q=q_out)
    print(f"   wrote {args_local.output} ({changed * 100:.1f}% of samples modified)")


def _build_o4fix_args(video, extra):
    """Get an o4fix args namespace (its own defaults + `extra` overrides) by
    running o4fix.main() with process() intercepted."""
    import o4fix as _o4
    argv_save = sys.argv
    proc_save = _o4.process
    try:
        sys.argv = ["o4fix.py", video] + extra
        captured = {}
        _o4.process = lambda v, a: captured.setdefault("args", a)
        _o4.main()
        return captured["args"]
    finally:
        _o4.process = proc_save
        sys.argv = argv_save


if __name__ == "__main__":
    main()
