"""Correlate the source-frame probe residual with the render-measured residual
(fbclip.residual pipeline) outside bursts, 0073 production v1 render."""
import sys, json
from pathlib import Path
import numpy as np
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'feedback-v1'))
import fbclip as F
O = F.E / 'fullprobe-v1/0073'
J = np.load(O / 'joined.npz')
cam = F.camera(F.E / 'release-v013/0073-release-camera.json')
B = np.load(F.FB / 'intended_B.npy')
for w in ['0', '55', '288', '338']:
    t, r, n = F.load(O / f'render-{w}.json')
    res = r[:, :3] - F.design(t, cam) @ B
    bad = ~np.isfinite(res).all(1)
    for k in range(3):
        res[bad, k] = np.interp(t[bad], t[~bad], res[~bad, k])
    er = np.degrees(F.hp3(res @ F.MINV.T, 1.0))
    m0 = (np.interp(t, J['t'], J['near'].astype(float)) < 0.01) & (n >= 150) & ~bad
    for sh in [-0.004, 0.0, 0.004]:
        ew = np.stack([np.interp(t, J['t'] + sh, J['r'][:, k]) for k in range(3)], 1)
        ew = F.hp3(ew, 1.0)
        for lab, (lo, hi) in [('1-8', (1, 8)), ('8-45', (8, 45))]:
            from scipy.signal import butter, filtfilt
            b, a = butter(2, [lo / 50, hi / 50], 'band')
            A = filtfilt(b, a, er, axis=0)[m0]; Bw = filtfilt(b, a, ew, axis=0)[m0]
            cs = [np.corrcoef(A[:, k], Bw[:, k])[0, 1] for k in range(3)]
            sl = [(A[:, k] * Bw[:, k]).sum() / (Bw[:, k] ** 2).sum() for k in range(3)]
            print(f'w{w:>3s} sh{sh*1000:+.0f} {lab:5s} n{m0.sum():5d} corr {np.round(cs,2)} slope {np.round(sl,2)} '
                  f'rms render {np.round(A.std(0),2)} probe {np.round(Bw.std(0),2)}')
