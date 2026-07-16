#!/usr/bin/env python3
"""Cache quat-derived rates (and optional optical ground truth) for analysis.

Usage:
    python analysis/make_cache.py sample_vids/CLIP.MP4 [--gt 7 12]

Writes analysis/cache/<stem>_rates.npz  (tm seconds, om_deg Nx3 deg/s @1kHz)
and, with --gt, <stem>_gt_<T0>_<T1>.npz (tv, ov_rad_s camera frame, quality).
"""
import argparse
import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import o4fix


def main():
    p = argparse.ArgumentParser()
    p.add_argument("video")
    p.add_argument("--gt", nargs=2, type=float, metavar=("T0", "T1"),
                   help="also extract optical GT rates for this time window (s)")
    p.add_argument("--out", default=str(Path(__file__).parent / "cache"))
    args = p.parse_args()

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    stem = Path(args.video).stem

    t, q, meta = o4fix.extract_quats(args.video)
    tm, om = o4fix.quats_to_rates(t, q)
    np.savez(out / f"{stem}_rates.npz", tm=tm, om_deg=np.degrees(om))
    print(f"wrote {stem}_rates.npz ({len(tm)} samples, "
          f"{1 / np.median(np.diff(tm)):.0f} Hz)")

    if args.gt:
        a, b = args.gt
        tv, ov, qv = o4fix.video_rates(args.video, [(a, b)], meta)
        np.savez(out / f"{stem}_gt_{a:.0f}_{b:.0f}.npz", tv=tv, ov=ov, qv=qv)
        print(f"wrote GT for {a}-{b}s ({len(tv)} frame pairs, "
              f"median quality {np.median(qv):.2f})")


if __name__ == "__main__":
    main()
