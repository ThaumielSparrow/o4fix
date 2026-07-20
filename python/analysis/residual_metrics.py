#!/usr/bin/env python3
"""Stabilization ERROR of a render = tracked apparent motion minus the
intended (smoothed) camera path from Gyroflow's camera-data export.

Inputs per render: eval_<stem>.npz (from eval_render.py, contains `series`)
and <name>_camera.json (from Gyroflow --export-metadata 3). The constant
axis/scale mapping between the stab-path rotvec rates and the tracker's
(dx, dy, droll) image rates is fit per render on high-quality samples, so
conventions/focal factors self-calibrate.

Residual bands:
  LF 0.05-1 Hz  -> slow mis-correction; its integral is perceived as
                   micro-panning (also reported in degrees)
  2-8 Hz        -> wobble
  8-30 Hz       -> shake / shudder
  zoom LF       -> zoom pumping error, % (tracker scale minus fov_scale)

Usage:
  python analysis/residual_metrics.py STEM:CAMERA.json [STEM2:CAM2.json ...]
"""
import argparse
import json
import sys
from pathlib import Path

import numpy as np
from scipy.signal import butter, filtfilt
from scipy.ndimage import uniform_filter1d

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import o4fix  # noqa: E402

CACHE = Path(__file__).parent / "cache"


def qmul(a, b):
    w1, x1, y1, z1 = a[..., 0], a[..., 1], a[..., 2], a[..., 3]
    w2, x2, y2, z2 = b[..., 0], b[..., 1], b[..., 2], b[..., 3]
    return np.stack([
        w1*w2 - x1*x2 - y1*y2 - z1*z2,
        w1*x2 + x1*w2 + y1*z2 - z1*y2,
        w1*y2 - x1*z2 + y1*w2 + z1*x2,
        w1*z2 + x1*y2 - y1*x2 + z1*w2], axis=-1)


def qconj(q):
    o = q.copy()
    o[..., 1:] *= -1
    return o


def rotvec(dq):
    s = np.sign(dq[..., 0:1])
    s[s == 0] = 1
    dq = dq * s
    v = dq[..., 1:]
    n = np.linalg.norm(v, axis=-1, keepdims=True)
    a = 2 * np.arcsin(np.clip(n, 0, 1))
    return np.degrees(np.where(n > 1e-12, v / np.maximum(n, 1e-12) * a, v))


def masks_from_cache(cache_npz, t):
    d = np.load(cache_npz)
    tm, deg = d["tm"], d["om_deg"]
    fs = 1.0 / np.median(np.diff(tm))
    x, _ = o4fix.hampel(deg, 7, 6.0)
    b, a = butter(2, [30 / (fs / 2), min(180, 0.95 * fs / 2) / (fs / 2)], "band")
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


def bandpass(x, fs, lo, hi):
    b, a = butter(2, [lo / (fs / 2), hi / (fs / 2)], "band")
    return filtfilt(b, a, x, axis=0)


def analyze(stem, camjson):
    npz = np.load(CACHE / f"eval_{stem}.npz")
    r = npz["series"]
    t, rates, logsr, q = r[:, 0], r[:, 1:4], r[:, 4], r[:, 5]
    fs = 1.0 / np.median(np.diff(t))

    cam = json.load(open(camjson))
    sq = np.array([f["stab_quat"] for f in cam])
    fov = np.array([f["fov_scale"] for f in cam])
    sv = rotvec(qmul(qconj(sq[:-1]), sq[1:])) * fs      # deg/s
    n = min(len(sv), len(rates))
    sv, rates, logsr, t, q = sv[:n], rates[:n], logsr[:n], t[:n], q[:n]
    zoom_rate = np.diff(np.log(fov), prepend=np.log(fov[0]))[:n] * fs

    # fit mapping M (3x3) stab-path -> tracker rates, high-quality samples
    good = q > 0.4
    M, *_ = np.linalg.lstsq(sv[good], rates[good], rcond=None)
    pred = sv @ M
    ss = 1 - np.sum((rates[good] - pred[good]) ** 2) / \
        np.sum((rates[good] - rates[good].mean(0)) ** 2)
    res = rates - pred
    zres = logsr - zoom_rate

    masks = masks_from_cache(
        CACHE / "DJI_20260711124046_0021_D_rates.npz", t)
    out = {"_fitR2": ss}
    lf = bandpass(res, fs, 0.05, 1.0)
    lf_ang = np.cumsum(lf, axis=0) / fs   # deg, wander
    wob = bandpass(res, fs, 2.0, 8.0)
    shk = bandpass(res, fs, 8.0, min(30.0, 0.45 * fs))
    zlf = bandpass(zres[:, None], fs, 0.05, 1.0)[:, 0]
    zwander = np.cumsum(zlf) / fs
    for lab, m in masks.items():
        out[lab] = (
            np.sqrt((lf_ang[m] ** 2).sum(1).mean()),          # pan deg
            np.sqrt((wob[m] ** 2).sum(1).mean()),             # wobble deg/s
            np.sqrt((shk[m] ** 2).sum(1).mean()),             # shake deg/s
            np.sqrt((zwander[m] ** 2).mean()) * 100,          # zoom %
        )
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("pairs", nargs="+", help="STEM:CAMERA.json")
    args = ap.parse_args()
    rows = {}
    for pair in args.pairs:
        stem, cam = pair.split(":", 1)
        rows[stem] = analyze(stem, cam)
    labs = ["clean", "patched-mild", "patched-severe"]
    print("\n=== residual vs intended path: pan-wander deg | wobble 2-8 deg/s"
          " | shake 8-30 deg/s | zoom-wander % ===")
    print(f"{'mask':<16}" + "".join(f"{s[:26]:>30}" for s in rows))
    for lab in labs:
        line = f"{lab:<16}"
        for s in rows:
            p, w, k, z = rows[s][lab]
            line += f"{p:6.2f} {w:6.2f} {k:6.2f} {z:6.2f} |".rjust(30)
        print(line)
    print("mapping fit R2: " +
          ", ".join(f"{s}: {rows[s]['_fitR2']:.3f}" for s in rows))


if __name__ == "__main__":
    main()
