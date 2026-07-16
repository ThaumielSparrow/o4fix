"""Raw banded metrics for all tracked renders, incl. severe/mild masks."""
import sys
from pathlib import Path

import numpy as np
from scipy.signal import butter, filtfilt
from scipy.ndimage import uniform_filter1d

root = Path(r"C:\Users\lzhan\Desktop\o4prostab")
sys.path.insert(0, str(root))
import o4fix

CACHE = root / "analysis/cache"
STEMS = ["DJI_20260711124046_0021_D", "eval_A_embedded",
         "eval_B_gcsv_current", "eval_C_lpf50", "eval_D_v2",
         "eval_R_noreadout", "eval_I_integrator"]
import sys as _sys
if len(_sys.argv) > 1:
    STEMS += _sys.argv[1:]

def bandpass(x, fs, lo, hi):
    b, a = butter(2, [lo / (fs / 2), hi / (fs / 2)], "band")
    return filtfilt(b, a, x, axis=0)

# masks
d = np.load(CACHE / "DJI_20260711124046_0021_D_rates.npz")
tm, deg = d["tm"], d["om_deg"]
fsm = 1.0 / np.median(np.diff(tm))
x, _ = o4fix.hampel(deg, 7, 6.0)
b, a = butter(2, [30 / (fsm / 2), min(180, 0.95 * fsm / 2) / (fsm / 2)], "band")
hf = filtfilt(b, a, x, axis=0)
noise = np.sqrt(uniform_filter1d(hf**2, size=int(0.1 * fsm), axis=0,
                                 mode="nearest")).max(axis=1)
alpha = np.clip((noise - 1.5) / 3.5, 0, 1)
alpha = uniform_filter1d(alpha, size=int(0.2 * fsm), mode="nearest")
piv = o4fix.find_intervals(alpha > 0.15, tm, 0.5, 1.0, 0.2)

rows = {}
for s in STEMS:
    r = np.load(CACHE / f"eval_{s}.npz")["series"]
    t, rates = r[:, 0], r[:, 1:4]
    fs = 1.0 / np.median(np.diff(t))
    patched = np.zeros(len(t), bool)
    for a_, b_ in piv:
        patched |= (t >= a_) & (t <= b_)
    nz = np.interp(t, tm, noise)
    masks = {"clean": ~patched, "mild": patched & (nz <= 4.5),
             "severe": patched & (nz > 4.5)}
    for lab, a2, b2 in [("flicks", 0, 0)]:
        pass
    fl = np.zeros(len(t), bool)
    for c in (22.1, 31.1, 109.8, 161.4):
        fl |= (t >= c - 1) & (t <= c + 1)
    masks["flicks"] = fl
    wob = bandpass(rates, fs, 2.0, 8.0)
    shk = bandpass(rates, fs, 8.0, min(30.0, 0.45 * fs))
    rows[s] = {lab: (np.sqrt((wob[m]**2).sum(1).mean()),
                     np.sqrt((shk[m]**2).sum(1).mean()))
               for lab, m in masks.items()}

print(f"{'render':<24}" + "".join(f"{lab:>16}" for lab in
                                  ["clean", "mild", "severe", "flicks"]))
print(f"{'':<24}" + "  wobble  shake" * 4)
for s in STEMS:
    line = f"{s:<24}"
    for lab in ["clean", "mild", "severe", "flicks"]:
        w, k = rows[s][lab]
        line += f"{w:8.2f}{k:7.2f} "
    print(line)
