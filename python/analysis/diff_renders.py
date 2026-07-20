#!/usr/bin/env python3
"""Banded difference of apparent motion between two renders of the SAME clip.

Parallax and intended-path motion cancel to first order (same scene, nearly
same crop), so the difference isolates how differently the two renders move:
  LF 0.05-1 Hz of the integrated difference -> extra slow wander (deg)
  2-8 Hz  -> wobble difference (deg/s)
  8-30 Hz -> shake difference (deg/s)
A sub-frame time shift between the series is scanned out automatically.

Usage:
  python analysis/diff_renders.py REF_STEM STEM2 [STEM3 ...]
        (stems of analysis/cache/eval_<stem>.npz saved by eval_render.py)
"""
import argparse
import sys
from pathlib import Path

import numpy as np
from scipy.signal import butter, filtfilt
from scipy.ndimage import uniform_filter1d

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import o4fix  # noqa: E402

CACHE = Path(__file__).parent / "cache"

WINDOWS = [
    ("powerloop 7-12", 7, 12),
    ("burst 140-145", 140, 145),
    ("flick 22.1", 21.1, 23.1),
    ("flick 109.8", 108.8, 110.8),
]


def masks_from_cache(t):
    d = np.load(CACHE / "DJI_20260711124046_0021_D_rates.npz")
    tm, deg = d["tm"], d["om_deg"]
    fs = 1.0 / np.median(np.diff(tm))
    x, _ = o4fix.hampel(deg, 7, 6.0)
    b, a = butter(2, [30 / (fs / 2), min(180, 0.95 * fs / 2) / (fs / 2)],
                  "band")
    hf = filtfilt(b, a, x, axis=0)
    noise = np.sqrt(uniform_filter1d(hf ** 2, size=int(0.1 * fs), axis=0,
                                     mode="nearest")).max(axis=1)
    alpha = np.clip((noise - 1.5) / 3.5, 0, 1)
    alpha = uniform_filter1d(alpha, size=int(0.2 * fs), mode="nearest")
    piv = o4fix.find_intervals(alpha > 0.15, tm, 0.5, 1.0, 0.2)
    patched = np.zeros(len(t), bool)
    for a_, b_ in piv:
        patched |= (t >= a_) & (t <= b_)
    nz = np.interp(t, tm, noise)
    severe = patched & (nz > 4.5)
    return {"clean": ~patched, "patched-mild": patched & ~severe,
            "patched-severe": severe}


def load_series(stem):
    r = np.load(CACHE / f"eval_{stem}.npz")["series"]
    return r[:, 0], r[:, 1:4]


def best_shift(t, a, b, fs):
    """sub-frame shift of b minimizing overall |a-b|, +-3 frames, 0.2 steps"""
    best = (None, np.inf)
    for s in np.arange(-3, 3.01, 0.2):
        bs = np.stack([np.interp(t, t + s / fs, b[:, k]) for k in range(3)],
                      axis=1)
        v = np.nanmedian(np.abs(a - bs))
        if v < best[1]:
            best = (s, v)
    s = best[0]
    return np.stack([np.interp(t, t + s / fs, b[:, k]) for k in range(3)],
                    axis=1), s


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("stems", nargs="+", help="first is the reference")
    args = ap.parse_args()
    ref, others = args.stems[0], args.stems[1:]
    t, ra = load_series(ref)
    fs = 1.0 / np.median(np.diff(t))
    masks = masks_from_cache(t)

    print(f"\n=== render minus {ref}: wander deg | wobble 2-8 | shake 8-30 "
          f"(RMS of difference) ===")
    for stem in others:
        t2, rb = load_series(stem)
        n = min(len(t), len(t2))
        rb2, shift = best_shift(t[:n], ra[:n], rb[:n], fs)
        d = rb2 - ra[:n]
        blf = bandpass(d, fs, 0.05, 1.0)
        wander = np.cumsum(blf, axis=0) / fs
        wob = bandpass(d, fs, 2.0, 8.0)
        shk = bandpass(d, fs, 8.0, min(30.0, 0.45 * fs))
        print(f"\n{stem}  (shift {shift * 1000 / fs:+.0f} ms)")
        for lab, m in masks.items():
            mm = m[:n]
            print(f"  {lab:<16} wander={np.sqrt((wander[mm]**2).sum(1).mean()):6.2f} "
                  f"wobble={np.sqrt((wob[mm]**2).sum(1).mean()):6.2f} "
                  f"shake={np.sqrt((shk[mm]**2).sum(1).mean()):6.2f}")
        for lab, a_, b_ in WINDOWS:
            mm = (t[:n] >= a_) & (t[:n] <= b_)
            print(f"  {lab:<16} wander={np.sqrt((wander[mm]**2).sum(1).mean()):6.2f} "
                  f"wobble={np.sqrt((wob[mm]**2).sum(1).mean()):6.2f} "
                  f"shake={np.sqrt((shk[mm]**2).sum(1).mean()):6.2f}")


def bandpass(x, fs, lo, hi):
    b, a = butter(2, [lo / (fs / 2), hi / (fs / 2)], "band")
    return filtfilt(b, a, x, axis=0)


if __name__ == "__main__":
    main()
