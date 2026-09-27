"""regions-v2 remap: what are the worst bins OUTSIDE severe bursts? For each, the source gyro
noise (30-180 Hz band RMS, o4fix's detector), the repaired telemetry's rotation rate, and the
render measurement's inlier count."""
import json, subprocess, sys
from pathlib import Path
import numpy as np
from scipy.signal import butter, sosfiltfilt
sys.path.insert(0, str(Path(__file__).resolve().parent))
import gatecheck as g

EXE = g.ROOT / 'target/release/examples/dump_quats.exe'


def rates(p):
    out = subprocess.run([str(EXE), str(p)], capture_output=True, text=True, check=True).stdout
    a = np.array([[float(x) for x in l.split(',')] for l in out.splitlines()[1:] if l])
    t, keep = np.unique(a[:, 0], return_index=True)
    q = a[keep, 1:]; t = t / 1000
    # 1 kHz grid (2 kHz slots carry duplicated values)
    tt = np.arange(t[0], t[-1], 0.001); idx = np.searchsorted(t, tt).clip(0, len(t) - 1); q = q[idx]
    for i in range(1, len(q)):
        if (q[i] * q[i - 1]).sum() < 0: q[i] = -q[i]
    w1, x1, y1, z1 = (q[:-1] * [1, -1, -1, -1]).T; w2, x2, y2, z2 = q[1:].T
    v = np.stack([w1 * x2 + x1 * w2 + y1 * z2 - z1 * y2, w1 * y2 - x1 * z2 + y1 * w2 + z1 * x2,
                  w1 * z2 + x1 * y2 - y1 * x2 + z1 * w2], 1)
    return tt[1:], np.degrees(2 * v) * 1000


R = json.load(open(g.RV2 / 'remap_classified.json'))
out = []
for clip in sorted(set(r['clip'] for r in R)):
    worst = [r for r in R if r['clip'] == clip and r['state'].startswith('outside') and r['inliers'] >= 150 and r['lo'] > 15]
    if not worst: continue
    ts, ws = rates(g.ROOT / f'sample_vids/{g.SRC[clip]}.MP4')
    tp, wp = rates(g.d(clip + 'p') / 'prod.MP4')
    sos = butter(4, [30, 180], 'band', fs=1000, output='sos')
    nb = np.sqrt(np.convolve((sosfiltfilt(sos, ws, axis=0) ** 2).sum(1), np.ones(100) / 100, 'same'))
    for r in worst:
        m = (ts >= r['t']) & (ts < r['t'] + 0.5); mp = (tp >= r['t']) & (tp < r['t'] + 0.5)
        r.update(noise=float(nb[m].max()), rate=float(np.percentile(np.linalg.norm(wp[mp], axis=1), 90)))
        out.append(r)
        mm, ss = divmod(r['t'], 60)
        print(f"{clip} {int(mm)}:{ss:04.1f}  render 1-8 {r['lo']:5.1f}  noise {r['noise']:6.1f}  rate p90 {r['rate']:6.0f} deg/s  inl {r['inliers']:4.0f}  {r['state']}")
json.dump(out, open(g.RV2 / 'outside_worst.json', 'w'), indent=0)
n = np.array([r['noise'] for r in out]); rt = np.array([r['rate'] for r in out])
print(f'\n{len(out)} bins: noise < 1.5 (clean) {np.sum(n < 1.5)}, 1.5-5 (light blend) {np.sum((n >= 1.5) & (n < 5))}, 5-8 {np.sum((n >= 5) & (n < 8))}, >= 8 {np.sum(n >= 8)}')
print(f'rate p90 > 300 deg/s: {np.sum(rt > 300)}, 150-300: {np.sum((rt > 150) & (rt <= 300))}, < 150: {np.sum(rt <= 150)}')
