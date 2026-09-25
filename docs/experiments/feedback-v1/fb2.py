"""Correction synthesis from measured render residuals (feedback-v1)."""
import json, sys, math
from pathlib import Path
import numpy as np
sys.path.insert(0, str(Path(__file__).parent))
from fb import OUT, ROOT, READOUT_MID, load, smooth_env

STAGES = ROOT / 'target/experiments/residual-stage-v1/0073/stages.json'
BURSTS = [tuple(b['interval']) for b in json.loads(STAGES.read_text())['bursts']]
CAL = json.loads((OUT / 'calibration.json').read_text())
M = np.array(CAL['M']); LAG = CAL['lag']
Minv = np.linalg.inv(M)
WINDOWS = {'late': 130, 'early': 12}


def butter2_hp(fc, fs):
    # 2nd-order Butterworth high-pass (bilinear), coefficients b, a
    k = math.tan(math.pi * fc / fs); q = math.sqrt(2)
    norm = 1 / (1 + q * k + k * k)
    b = np.array([1, -2, 1]) * norm
    a = np.array([1, 2 * (k * k - 1) * norm, (1 - q * k + k * k) * norm])
    return b, a


def lfilter(b, a, x):
    y = np.zeros_like(x)
    for i in range(len(x)):
        y[i] = b[0] * x[i] + (b[1] * x[i-1] if i > 0 else 0) + (b[2] * x[i-2] if i > 1 else 0) \
            - (a[1] * y[i-1] if i > 0 else 0) - (a[2] * y[i-2] if i > 1 else 0)
    return y


def filtfilt(b, a, x, pad=150):
    xp = np.concatenate([2 * x[0] - x[pad:0:-1], x, 2 * x[-1] - x[-2:-pad-2:-1]])
    y = lfilter(b, a, xp); y = lfilter(b, a, y[::-1])[::-1]
    return y[pad:-pad]


def gate(t, pad=0.25, fade=0.15):
    w = np.zeros_like(t)
    for a, b in BURSTS:
        w = np.maximum(w, smooth_env(t, a - pad, b + pad, fade))
    return w


def body_error(path, fc=1.0):
    t, r, n = load(path)
    bad = ~np.isfinite(r).all(1) | (n < 40)
    for k in range(3):
        r[bad, k] = np.interp(t[bad], t[~bad], r[~bad, k])
    e = r[:, :3] @ Minv.T                       # body error rate (rad/s)
    ehp = np.stack([filtfilt(*butter2_hp(fc, 100.0), e[:, k]) for k in range(3)], 1)
    return t, ehp, bad


def stats(tag, path):
    t, e, bad = body_error(path)
    w = gate(t)
    ins, out = w > 0.99, w < 0.01
    deg = np.degrees(e)
    f = lambda m: np.sqrt((deg[m] ** 2).mean(0)).round(2).tolist()
    print(tag, 'in-burst rms deg/s', f(ins), ' outside', f(out), ' bad', int(bad.sum()))


if __name__ == '__main__':
    for p in sys.argv[1:]:
        stats(Path(p).stem, p)
