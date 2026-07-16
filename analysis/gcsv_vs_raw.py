"""Data-level check: banded |gcsv - raw_gyro| per zone for B, D, E gcsv.

If a variant's mild zones keep the gyro (as intended for D/E), the 2-8 Hz
difference from raw should be ~0 there. Catches construction bugs directly.
"""
import sys
from pathlib import Path

import numpy as np
from scipy.signal import butter, filtfilt
from scipy.ndimage import uniform_filter1d

root = Path(r"C:\Users\lzhan\Desktop\o4prostab")
sys.path.insert(0, str(root))
import o4fix

d = np.load(root / "analysis/cache/DJI_20260711124046_0021_D_rates.npz")
tm, deg = d["tm"], d["om_deg"]
fs = 1.0 / np.median(np.diff(tm))
x, _ = o4fix.hampel(deg, 7, 6.0)
b, a = butter(2, [30 / (fs / 2), min(180, 0.95 * fs / 2) / (fs / 2)], "band")
hf = filtfilt(b, a, x, axis=0)
noise = np.sqrt(uniform_filter1d(hf**2, size=int(0.1 * fs), axis=0,
                                 mode="nearest")).max(axis=1)
alpha = np.clip((noise - 1.5) / 3.5, 0, 1)
alpha = uniform_filter1d(alpha, size=int(0.2 * fs), mode="nearest")
piv = o4fix.find_intervals(alpha > 0.15, tm, 0.5, 1.0, 0.2)
patched = np.zeros(len(tm), bool)
for a_, b_ in piv:
    patched |= (tm >= a_) & (tm <= b_)
masks = {"clean": ~patched, "mild": patched & (noise <= 4.5),
         "severe": patched & (noise > 4.5)}

def load_gcsv(p):
    rows = np.loadtxt(p, delimiter=",", skiprows=12)
    return rows[:, 0] / 1000.0, np.degrees(rows[:, 1:4])

for name in ["DJI_20260711124046_0021_D.gcsv", "eval_D_v2.gcsv",
             "eval_E_anchor.gcsv", "eval_G_raw.gcsv"]:
    tg, g = load_gcsv(root / "sample_vids" / name)
    n = min(len(tg), len(tm))
    diff = g[:n] - x[:n]
    for lo, hi, lab in [(2, 8, "2-8"), (8, 25, "8-25")]:
        bb, ab = butter(2, [lo / (fs / 2), hi / (fs / 2)], "band")
        db = filtfilt(bb, ab, diff, axis=0)
        line = f"{name:<36} {lab:>5} Hz |gcsv-raw|: "
        for lab2, m in masks.items():
            mm = m[:n]
            line += f"{lab2}={np.sqrt((db[mm]**2).sum(1).mean()):6.2f}  "
        print(line)
