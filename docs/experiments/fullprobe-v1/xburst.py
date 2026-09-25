"""Probe-vs-render residual agreement inside given bursts.  xburst.py CLIP_DIR CAMERA_JSON WIN:A:B ..."""
import sys
from pathlib import Path
import numpy as np
from scipy.signal import butter, filtfilt
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'feedback-v1')); sys.path.insert(0, str(Path(__file__).parent))
import fbclip as F, analyze as A
O = Path(sys.argv[1]); cam = F.camera(sys.argv[2]); B = np.load(F.FB / 'intended_B.npy')
for spec in sys.argv[3:]:
    w, a, b = spec.split(':'); a, b = float(a), float(b)
    t, r, n = F.load(O / f'render-{w}.json')
    res = r[:, :3] - F.design(t, cam) @ B
    bad = ~np.isfinite(res).all(1)
    for k in range(3): res[bad, k] = np.interp(t[bad], t[~bad], res[~bad, k])
    er = np.degrees(F.hp3(res @ F.MINV.T, 1.0))
    tp, rp, npr = A.probe(str(O / f'prod-{w}.json'))
    ep = np.degrees(F.hp3(rp, 1.0))
    m = (t > a) & (t < b)
    for sh in [-0.004, 0, 0.004]:
        ew = np.stack([np.interp(t, tp + sh, ep[:, k]) for k in range(3)], 1)
        bb = butter(2, [1 / 50, 8 / 50], 'band')
        X = filtfilt(*bb, er, axis=0)[m]; Y = filtfilt(*bb, ew, axis=0)[m]
        print(w, f'sh{sh*1e3:+.0f}', 'corr', np.round([np.corrcoef(X[:, k], Y[:, k])[0, 1] for k in range(3)], 2),
              'slope', np.round([(X[:, k] * Y[:, k]).sum() / (Y[:, k] ** 2).sum() for k in range(3)], 2),
              'rms render', X.std(0).round(1), 'probe', Y.std(0).round(1))
