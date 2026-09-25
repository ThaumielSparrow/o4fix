"""Per-burst residual + correction-angle composition from the whole-clip probe.
  bursts.py PROBE_GLOB RUN_LOG"""
import sys, glob
from pathlib import Path
import numpy as np
from scipy.signal import butter, filtfilt
sys.path.insert(0, str(Path(__file__).parent))
import analyze as A
t, r, n = A.probe(sys.argv[1])
bad = ~np.isfinite(r).all(1)
for k in range(3):
    r[bad, k] = np.interp(t[bad], t[~bad], r[~bad, k])
hp = filtfilt(*butter(2, 1 / 50, 'high'), r, axis=0)
lo = np.degrees(A.band(r, 1, 8)); hi = np.degrees(A.band(r, 8, 45))
conf = np.convolve(np.clip((n - 60) / 140, 0, 1), np.ones(9) / 9, 'same')
rms = lambda x, m: np.sqrt((x[m] ** 2).sum(1).mean())
print(f'{"burst":>16s} {"ref":>4s} {"1-8":>6s} {"8-45":>6s} {"ang":>6s} {"angLF":>6s} {"angHF":>6s} {"minN":>5s} {"medN":>5s}')
for a, b, applied in A.bursts(sys.argv[2]):
    m = (t >= a) & (t <= b)
    if m.sum() < 10:
        continue
    g = np.clip(np.minimum(t - (a - .25), (b + .25) - t) / .15, 0, 1); g = g * g * (3 - 2 * g)
    w = (t > a - 1.5) & (t < b + 1.5)
    ang = np.degrees(np.cumsum(-(g * conf)[w, None] * hp[w], 0) * 0.01)
    angLF = filtfilt(*butter(2, 1 / 50, 'low'), ang, axis=0) if w.sum() > 20 else ang
    mx = lambda x: np.linalg.norm(x - x[0], axis=1).max()
    print(f'{a:7.2f}-{b:7.2f} {"Y" if applied else "n":>4s} {rms(lo, m):6.2f} {rms(hi, m):6.2f} {mx(ang):6.2f} {mx(angLF):6.2f} {np.linalg.norm(ang-angLF,axis=1).max():6.2f} {n[m].min():5d} {int(np.median(n[m])):5d}')
