"""Banded residual RMS in given windows from eval_render series caches.

Adapted from the task-5 brief's template to the ACTUAL eval_render.py npz
schema (verified by inspection): the cache stores a single 'series' array
with columns [t, wx, wy, wroll, dlogscale, quality] (deg/s apparent rates
in columns 1:4, i.e. wx/wy/wroll -- this IS the "resid" 3-vector the brief
assumed under a 'resid' key). No separate 't'/'resid' keys exist.
"""
import sys
from pathlib import Path

import numpy as np
from scipy.signal import butter, filtfilt

cache = Path(__file__).resolve().parent / "cache"


def series(stem):
    d = np.load(cache / f"eval_{stem}.npz")
    s = d["series"]
    return s[:, 0], s[:, 1:4]  # t, (wx,wy,wroll) deg/s


def band(t, x, a, b, lo, hi):
    fs = 1.0 / np.median(np.diff(t))
    m = (t >= a) & (t <= b)
    if m.sum() < 30:
        return np.nan
    hi = min(hi, 0.45 * fs)
    bb, ba = butter(2, [lo / (fs / 2), hi / (fs / 2)], "band")
    seg = filtfilt(bb, ba, x[m] - x[m].mean(0), axis=0)
    return float(np.sqrt((seg ** 2).sum(-1).mean()))


WINDOWS_0060 = {
    "monster_100": (99.9, 102.0), "monster_106": (105.3, 107.1),
    "monster_225": (224.5, 226.7), "monster_248": (247.8, 249.8),
    "monster_308": (307.5, 309.8),
    "clean_ctrl": (117.0, 128.0), "mild_ctrl": (181.0, 211.0),
    "monster_248_postburst": (249.3, 290.0),  # 0.1-1 Hz post-burst decay check
}

WINDOWS_0027 = {
    # exactly the bursts o4fix tagged REBASED at gate=30, +-0.5s
    "burst_5.5": (4.96, 7.44), "burst_99.8": (99.34, 102.84),
    "burst_114.6": (114.10, 116.29), "burst_219.5_flick": (219.0, 221.14),
    "clean_ctrl": (55.0, 65.0), "mild_ctrl": (160.0, 199.0),
}


def run(windows, stems, post_burst_key=None):
    for stem in stems:
        t, r = series(stem)
        print(f"== {stem}")
        for name, (a, b) in windows.items():
            if name == post_burst_key:
                w = band(t, r, a, b, 0.1, 1.0)
                s = float("nan")
            else:
                w = band(t, r, a, b, 0.3, 8)
                s = band(t, r, a, b, 8, 30)
            print(f"  {name:24} {a:6.1f}-{b:6.1f}  wob {w:6.2f}  shk {s:6.2f}")


if __name__ == "__main__":
    which = sys.argv[1]
    stems = sys.argv[2:]
    if which == "0060":
        run(WINDOWS_0060, stems, post_burst_key="monster_248_postburst")
    elif which == "0027":
        run(WINDOWS_0027, stems)
    else:
        print("usage: eval_windows.py {0060|0027} stem [stem ...]")
