"""Intended-motion model from Gyroflow's exported smoothed path (feedback-v1)."""
import json, sys
from pathlib import Path
import numpy as np
sys.path.insert(0, str(Path(__file__).parent))
from fb import OUT, ROOT, load
from fb2 import gate, Minv, filtfilt, butter2_hp

CAM = ROOT / 'target/experiments/bounce-localize-v1/camera.json'


def qmul(a, b):
    w1, x1, y1, z1 = a.T; w2, x2, y2, z2 = b.T
    return np.stack([w1*w2-x1*x2-y1*y2-z1*z2, w1*x2+x1*w2+y1*z2-z1*y2,
                     w1*y2-x1*z2+y1*w2+z1*x2, w1*z2+x1*y2-y1*x2+z1*w2], 1)


def rates(q):
    q = q.copy()
    for i in range(1, len(q)):
        if (q[i] * q[i-1]).sum() < 0: q[i] = -q[i]
    c = q[:-1] * np.array([1, -1, -1, -1])
    d = qmul(c, q[1:])
    d[d[:, 0] < 0] *= -1
    v = d[:, 1:]; n = np.linalg.norm(v, axis=1)
    th = 2 * np.arcsin(np.clip(n, 0, 1)); s = np.where(n > 1e-12, th / np.maximum(n, 1e-12), 2)
    return v * s[:, None] * 100.0


def camera(path=CAM):
    d = json.loads(Path(path).read_text())
    S = np.array([f['stab_quat'] for f in d]); O = np.array([f['org_quat'] for f in d])
    fov = np.array([f['fov_scale'] for f in d]); ts = np.array([f['timestamp_ms'] for f in d]) / 1000
    return ts, rates(S), rates(O), fov


def pair_features(t, cam):
    ts, wS, wO, fov = cam
    k = np.round((t - 0.005) * 100).astype(int)          # pair (k, k+1)
    f = 0.5 * (fov[k] + fov[k + 1])
    return wS[k], wO[k], f


def design(wS, f):
    return np.concatenate([wS / f[:, None], wS], 1)       # scale-aware + plain


def fit(names, cam):
    X, Y = [], []
    for nm in names:
        t, r, n = load(OUT / f'base-{nm}.json')
        wS, wO, f = pair_features(t, cam)
        ok = np.isfinite(r).all(1) & (n >= 80) & (gate(t) < 0.01)
        X.append(design(wS, f)[ok]); Y.append(r[ok, :3])
    X = np.concatenate(X); Y = np.concatenate(Y)
    w = np.ones(len(X))
    for _ in range(10):  # IRLS Huber
        B = np.linalg.lstsq(X * w[:, None], Y * w[:, None], rcond=None)[0]
        res = Y - X @ B
        s = np.median(np.abs(res), 0) * 1.48
        z = np.abs(res / s).max(1)
        w = np.sqrt(np.where(z < 2, 1, 2 / z))
    r2 = 1 - (res ** 2).sum(0) / ((Y - Y.mean(0)) ** 2).sum(0)
    return B, r2


def residual(name, cam, B):
    t, r, n = load(OUT / f'base-{name}.json')
    wS, wO, f = pair_features(t, cam)
    res = r[:, :3] - design(wS, f) @ B
    return t, res, n


if __name__ == '__main__':
    cam = camera()
    B, r2 = fit(['cal1', 'cal2', 'late', 'early'], cam)
    print('r2 intended-motion fit', r2.round(4))
    np.save(OUT / 'intended_B.npy', B)
    for nm in ['late', 'early']:
        t, res, n = residual(nm, cam, B)
        e = np.degrees(res @ Minv.T); w = gate(t)
        print(nm)
        for s in np.arange(t[0], t[-1], 0.5):
            m = (t >= s) & (t < s + 0.5)
            rms = np.sqrt((e[m] ** 2).mean(0))
            print(f"{s:6.1f} w={w[m].mean():.2f} n={int(np.median(n[m])):4d} e=[{rms[0]:5.1f} {rms[1]:5.1f} {rms[2]:5.1f}]")
