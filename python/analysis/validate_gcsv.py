#!/usr/bin/env python3
"""Validate a generated .gcsv against the raw telemetry (and optical GT).

Usage:
    python analysis/make_cache.py sample_vids/CLIP.MP4 [--gt 7 12]   # once
    python analysis/validate_gcsv.py sample_vids/CLIP.MP4 [--window 7 12]

Reports:
- band-by-band RMS raw vs gcsv in the window (is noise removed?)
- fast-event tracking ratios (are flicks/pans preserved? want ~1.00)
- continuity / NaN checks
- if a cached GT window overlaps: per-axis correlation of gcsv vs the
  video-measured real motion, band by band (is what remains REAL?)
"""
import argparse
import sys
from pathlib import Path

import numpy as np
from scipy.ndimage import uniform_filter1d
from scipy.signal import butter, filtfilt

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import o4fix  # noqa: F401  (kept for API parity; fit helpers live here)

FS = 1000.0


def lpf(x, fc, fs=FS):
    b, a = butter(2, fc / (fs / 2), "low")
    return filtfilt(b, a, x, axis=0)


def bpf(x, lo, hi, fs=FS):
    b, a = butter(2, [lo / (fs / 2), hi / (fs / 2)], "band")
    return filtfilt(b, a, x, axis=0)


def read_gcsv(path):
    rows, started = [], False
    with open(path) as f:
        for line in f:
            if started:
                rows.append([float(x) for x in line.strip().split(",")])
            elif line.startswith("t,"):
                started = True
    arr = np.array(rows)
    return arr[:, 0] / 1000.0, np.degrees(arr[:, 1:4])


def main():
    p = argparse.ArgumentParser()
    p.add_argument("video")
    p.add_argument("--gcsv", help="default: video path with .gcsv suffix")
    p.add_argument("--window", nargs=2, type=float, default=[7.0, 12.0])
    p.add_argument("--cache", default=str(Path(__file__).parent / "cache"))
    args = p.parse_args()

    stem = Path(args.video).stem
    cache = Path(args.cache)
    rates_file = cache / f"{stem}_rates.npz"
    if not rates_file.exists():
        sys.exit(f"missing {rates_file} - run make_cache.py first")
    d = np.load(rates_file)
    tm, om = d["tm"], d["om_deg"]

    gcsv = args.gcsv or str(Path(args.video).with_suffix(".gcsv"))
    tg, gg = read_gcsv(gcsv)
    print(f"gcsv: {len(tg)} samples, NaN: {np.isnan(gg).sum()}, "
          f"abs max {np.abs(gg).max():.1f} deg/s, "
          f"max jump {np.abs(np.diff(gg, axis=0)).max():.1f} deg/s")

    out_tm = np.stack([np.interp(tm, tg, gg[:, k]) for k in range(3)], axis=1)

    a, b = args.window
    m = (tm >= a) & (tm <= b)
    print(f"\nbands in {a}-{b}s (deg/s RMS avg axes): raw -> gcsv")
    for lo, hi in [(0.5, 3), (3, 10), (10, 30), (30, 120)]:
        print(f"  {lo}-{hi}Hz: {bpf(om[m], lo, hi).std():6.2f} -> "
              f"{bpf(out_tm[m], lo, hi).std():6.2f}")

    raw_lf, out_lf = lpf(om, 3.0), lpf(out_tm, 3.0)
    n = min(len(raw_lf), len(out_lf))
    cors = [np.corrcoef(raw_lf[m][:, i], out_lf[m][:, i])[0, 1] for i in range(3)]
    print(f"<3Hz corr gcsv vs raw in window: {np.round(cors, 3)}")

    # fast events over the whole flight
    mag = np.linalg.norm(raw_lf, axis=1)
    events, order = [], np.argsort(-mag)
    for i in order:
        if all(abs(tm[i] - tm[j]) > 2.0 for j in events):
            events.append(i)
        if len(events) >= 12:
            break
    print("\nfast events (want ratio ~1.00 = intentional motion preserved):")
    for i in sorted(events, key=lambda i: tm[i]):
        r = np.linalg.norm(out_lf[i]) / max(mag[i], 1e-9)
        flag = "  <-- under-reported" if r < 0.9 else ""
        print(f"  t={tm[i]:6.1f}s |w|={mag[i]:6.1f} ratio={r:.2f}{flag}")

    # optional GT comparison
    for gtf in sorted(cache.glob(f"{stem}_gt_*.npz")):
        g = np.load(gtf)
        tv, ov, qv = g["tv"], g["ov"], g["qv"]
        if tv[0] > b or tv[-1] < a:
            continue
        fit = o4fix.fit_video_alignment(tv, ov, qv, tm, om, FS)
        if fit is None:
            print(f"\nGT {gtf.name}: alignment fit failed (too few good frames)")
            continue
        shift, N, r2 = fit
        gt = np.degrees(ov) @ N
        tvs = tv + shift
        fps = 1.0 / np.median(np.diff(tvs))
        sm = uniform_filter1d(out_tm, size=int(FS / fps), axis=0, mode="nearest")
        out_f = np.stack([np.interp(tvs, tm, sm[:, k]) for k in range(3)], axis=1)
        print(f"\nGT {gtf.name} (fit R2={r2:.3f}, shift {shift * 1000:.0f} ms) "
              f"- gcsv vs REAL motion:")
        for lo, hi in [(0.5, 3), (3, 6), (6, 10)]:
            bg = bpf(gt, lo, hi, fps)
            bo = bpf(out_f, lo, hi, fps)
            c = np.mean([np.corrcoef(bg[:, i], bo[:, i])[0, 1] for i in range(3)])
            print(f"  {lo}-{hi}Hz: GT {bg.std():5.2f}  gcsv {bo.std():5.2f} "
                  f" corr {c:.2f}")


if __name__ == "__main__":
    main()
