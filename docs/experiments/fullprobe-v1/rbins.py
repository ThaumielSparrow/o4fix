"""Render-measured residual (fbclip pipeline) in 0.25 s bins.  rbins.py RENDER_JSON CAMERA_JSON [A B]"""
import sys
from pathlib import Path
import numpy as np
from scipy.signal import butter, filtfilt
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'feedback-v1'))
import fbclip as F
def series(rj, cj):
    cam = F.camera(cj); B = np.load(F.FB / 'intended_B.npy')
    t, r, n = F.load(Path(rj))
    res = r[:, :3] - F.design(t, cam) @ B
    bad = ~np.isfinite(res).all(1)
    for k in range(3): res[bad, k] = np.interp(t[bad], t[~bad], res[~bad, k])
    e = np.degrees(F.hp3(res @ F.MINV.T, 1.0))
    lo = filtfilt(*butter(2, [1 / 50, 8 / 50], 'band'), e, axis=0); hi = filtfilt(*butter(2, [8 / 50, 45 / 50], 'band'), e, axis=0)
    return t, lo, hi, n
if __name__ == '__main__':
    t, lo, hi, n = series(sys.argv[1], sys.argv[2])
    a, b = (float(sys.argv[3]), float(sys.argv[4])) if len(sys.argv) > 4 else (t[0], t[-1])
    for s in np.arange(a, b, 0.25):
        m = (t >= s) & (t < s + .25)
        print(f'{s:7.2f}  1-8Hz {np.sqrt((lo[m]**2).sum(1).mean()):6.2f}  8-45Hz {np.sqrt((hi[m]**2).sum(1).mean()):6.2f}  n {int(n[m].min())}')
