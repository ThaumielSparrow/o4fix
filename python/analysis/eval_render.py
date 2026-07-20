#!/usr/bin/env python3
"""Measure stabilization quality of rendered (stabilized) videos.

Tracks features frame-to-frame on the stabilized output and fits a
similarity transform per frame pair. The residual apparent motion IS what
the viewer perceives: a perfect render moves only along the smoothed
camera path (very low frequency), so
  - micro-panning  = low-frequency (0.05-1 Hz) wander of the integrated
                     apparent angle, degrees
  - non-flowiness  = high-frequency residual rate, deg/s (2-8 Hz wobble,
                     8-30 Hz shake)
  - zoom pumping   = LF RMS of log(scale), percent

Usage:
    python analysis/eval_render.py RENDER1.mp4 [RENDER2.mp4 ...]
                                   [--csv out.csv] [--half]

Reports metrics per named window (clean cruise / powerloop / burst /
flicks, from the 0021_D test clip) and per patched/clean masks derived
from the cached gyro rates (analysis/cache/*_rates.npz).
"""
import argparse
import sys
from pathlib import Path

import cv2
import numpy as np
from scipy.signal import butter, filtfilt

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import o4fix  # noqa: E402  (hampel, find_intervals)
from scipy.ndimage import uniform_filter1d  # noqa: E402

F_PX = 546.4  # calib focal length @1440w; constant is fine for comparisons

# named windows in the 0021_D test clip (seconds)
WINDOWS = [
    ("powerloop 7-12", 7, 12),
    ("clean 60-65", 60, 65),
    ("clean 100-105", 100, 105),
    ("burst 140-145", 140, 145),
    ("flick 22.1", 21.1, 23.1),
    ("flick 31.1", 30.1, 32.1),
    ("flick 109.8", 108.8, 110.8),
    ("flick 161.4", 160.4, 162.4),
]


def patched_intervals_from_cache(cache_npz):
    d = np.load(cache_npz)
    tm, deg = d["tm"], d["om_deg"]
    fs = 1.0 / np.median(np.diff(tm))
    x, _ = o4fix.hampel(deg, 7, 6.0)
    b, a = butter(2, [30.0 / (fs / 2), min(180.0, 0.95 * fs / 2) / (fs / 2)],
                  "band")
    hf = filtfilt(b, a, x, axis=0)
    win = max(3, int(round(100.0 * fs / 1000)))
    noise = np.sqrt(uniform_filter1d(hf ** 2, size=win, axis=0,
                                     mode="nearest")).max(axis=1)
    alpha = np.clip((noise - 1.5) / 3.5, 0.0, 1.0)
    alpha = uniform_filter1d(alpha, size=max(3, int(0.2 * fs)), mode="nearest")
    return o4fix.find_intervals(alpha > 0.15, tm, 0.5, 1.0, 0.2)


def track_video(path, half=True):
    """Per-frame-pair similarity transform -> apparent rates (deg/s)."""
    cap = cv2.VideoCapture(str(path))
    if not cap.isOpened():
        raise RuntimeError(f"cannot open {path}")
    fps = cap.get(cv2.CAP_PROP_FPS) or 100.0
    n = int(cap.get(cv2.CAP_PROP_FRAME_COUNT))
    feat = dict(maxCorners=400, qualityLevel=0.01, minDistance=16,
                blockSize=7)
    lk = dict(winSize=(21, 21), maxLevel=3,
              criteria=(cv2.TERM_CRITERIA_EPS | cv2.TERM_CRITERIA_COUNT,
                        30, 0.01))
    ok, frame = cap.read()
    if not ok:
        raise RuntimeError(f"cannot read {path}")
    scale = 0.5 if half else 1.0
    f_px = F_PX * (frame.shape[1] * scale / 1440.0)

    def prep(fr):
        g = cv2.cvtColor(fr, cv2.COLOR_BGR2GRAY)
        if half:
            g = cv2.resize(g, None, fx=0.5, fy=0.5,
                           interpolation=cv2.INTER_AREA)
        return g

    prev = prep(frame)
    p0 = cv2.goodFeaturesToTrack(prev, **feat)
    rows = []  # t, dx_deg/s, dy_deg/s, droll_deg/s, dlogscale/s, quality
    i = 0
    while True:
        ok, frame = cap.read()
        if not ok:
            break
        i += 1
        gray = prep(frame)
        if p0 is None or len(p0) < 60:
            p0 = cv2.goodFeaturesToTrack(prev, **feat)
        if p0 is None or len(p0) < 20:
            rows.append((i / fps, np.nan, np.nan, np.nan, np.nan, 0.0))
            prev = gray
            p0 = cv2.goodFeaturesToTrack(prev, **feat)
            continue
        p1, st, _ = cv2.calcOpticalFlowPyrLK(prev, gray, p0, None, **lk)
        good = st.ravel() == 1
        a, b = p0[good].reshape(-1, 2), p1[good].reshape(-1, 2)
        if len(a) < 20:
            rows.append((i / fps, np.nan, np.nan, np.nan, np.nan, 0.0))
        else:
            M, inl = cv2.estimateAffinePartial2D(
                a, b, method=cv2.RANSAC, ransacReprojThreshold=1.0)
            if M is None:
                rows.append((i / fps, np.nan, np.nan, np.nan, np.nan, 0.0))
            else:
                s = np.hypot(M[0, 0], M[0, 1])
                dth = np.arctan2(M[0, 1], M[0, 0])
                # remove rotation/scale contribution at frame center to get
                # pure translation of the view center
                h, w = gray.shape
                c = np.array([w / 2, h / 2])
                cc = M[:, :2] @ c + M[:, 2] - c
                q = inl.sum() / max(len(a), 1)
                rows.append((i / fps,
                             np.degrees(np.arctan(cc[0] / f_px)) * fps,
                             np.degrees(np.arctan(cc[1] / f_px)) * fps,
                             np.degrees(dth) * fps,
                             np.log(max(s, 1e-6)) * fps,
                             q))
        prev = gray
        p0 = cv2.goodFeaturesToTrack(gray, **feat) if i % 5 == 0 else \
            p1[good].reshape(-1, 1, 2)
        if i % 2000 == 0:
            print(f"    {i}/{n} frames", flush=True)
    cap.release()
    r = np.array(rows)
    # fill occasional NaNs by interpolation
    for k in range(1, 5):
        bad = np.isnan(r[:, k])
        if bad.any() and not bad.all():
            r[bad, k] = np.interp(r[bad, 0], r[~bad, 0], r[~bad, k])
    return r  # columns: t, wx, wy, wroll (deg/s), dlogs/s, quality


def bandpass(x, fs, lo, hi, order=2):
    if lo <= 0:
        b, a = butter(order, hi / (fs / 2), "low")
    else:
        b, a = butter(order, [lo / (fs / 2), hi / (fs / 2)], "band")
    return filtfilt(b, a, x, axis=0)


def metrics_for_mask(t, rates, logs_rate, fs, mask):
    """(pan_deg, wobble_dps, shake_dps, zoom_pct) within mask."""
    if mask.sum() < int(2 * fs):
        return (np.nan,) * 4
    # integrated apparent angle, LF wander 0.05-1 Hz
    ang = np.cumsum(rates, axis=0) / fs
    pan = bandpass(ang, fs, 0.05, 1.0)
    pan_m = np.sqrt((pan[mask] ** 2).sum(axis=1).mean())
    wob = bandpass(rates, fs, 2.0, 8.0)
    wob_m = np.sqrt((wob[mask] ** 2).sum(axis=1).mean())
    shk = bandpass(rates, fs, 8.0, 30.0)
    shk_m = np.sqrt((shk[mask] ** 2).sum(axis=1).mean())
    zoom = bandpass(np.cumsum(logs_rate) / fs, fs, 0.05, 1.0)
    zoom_m = np.sqrt((zoom[mask] ** 2).mean()) * 100
    return pan_m, wob_m, shk_m, zoom_m


def evaluate_series(r, patched):
    t = r[:, 0]
    fs = 1.0 / np.median(np.diff(t))
    rates = r[:, 1:4]
    logsr = r[:, 4]
    out = {}
    pmask = np.zeros(len(t), bool)
    for a, b in patched:
        pmask |= (t >= a) & (t <= b)
    out["ALL patched (64%)"] = metrics_for_mask(t, rates, logsr, fs, pmask)
    out["ALL clean"] = metrics_for_mask(t, rates, logsr, fs, ~pmask)
    for name, a, b in WINDOWS:
        m = (t >= a) & (t <= b)
        out[name] = metrics_for_mask(t, rates, logsr, fs, m)
    out["_quality"] = float(np.median(r[:, 5]))
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("videos", nargs="+")
    ap.add_argument("--cache", default=str(Path(__file__).parent / "cache" /
                    "DJI_20260711124046_0021_D_rates.npz"))
    ap.add_argument("--full-res", action="store_true")
    ap.add_argument("--npz-dir", default=str(Path(__file__).parent / "cache"))
    args = ap.parse_args()

    patched = patched_intervals_from_cache(args.cache)
    results = {}
    for v in args.videos:
        print(f"  tracking {Path(v).name} ...", flush=True)
        series = track_video(v, half=not args.full_res)
        res = evaluate_series(series, patched)
        results[Path(v).stem] = res
        np.savez(Path(args.npz_dir) / f"eval_{Path(v).stem}.npz",
                 series=series,
                 **{k: np.array(val) for k, val in res.items()})

    names = list(results)
    labels = [k for k in results[names[0]] if not k.startswith("_")]
    print("\n=== pan = LF wander deg (0.05-1 Hz) | wobble 2-8 Hz deg/s | "
          "shake 8-30 Hz deg/s | zoom LF % ===")
    header = f"{'window':<18}" + "".join(f"{n[:26]:>28}" for n in names)
    print(header)
    for lab in labels:
        row = f"{lab:<18}"
        for n in names:
            p, w, s, z = results[n][lab]
            row += f"{p:6.2f} {w:6.2f} {s:6.2f} {z:5.2f} |".rjust(28)
        print(row)
    print("\nmedian track quality: " +
          ", ".join(f"{n}: {results[n]['_quality']:.2f}" for n in names))


if __name__ == "__main__":
    main()
