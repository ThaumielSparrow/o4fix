"""regions-v1: do image halves agree on the probe residual inside a burst?
A telemetry error rotates every region alike; translation parallax does not."""
import json, sys
import numpy as np
from scipy.signal import butter, filtfilt
O = 'target/experiments/regions-v1/'
CASES = [  # file, burst, label (known from render / user review)
    ('c83', (113.09, 114.68), 'GOOD 0083 1:53 burst (B fixed it)'),
    ('c73_141', (141.25, 144.45), 'GOOD 0073 refined, user-approved'),
    ('c21_35', (35.52, 46.28), 'GOOD 0021 refined'),
    ('c21pl', (21.71, 26.11), 'BAD 0021 powerloop (probe wrong)'),
    ('c21_109', (109.59, 110.51), 'BAD 0021 109.6 (probe wrong)'),
    ('c60_48', (48.69, 49.48), 'BAD 0060 48.7 (probe wrong)'),
    ('c60_191', (191.71, 192.22), 'MIXED 0060 191.7 (axis 0 real)'),
]
bb = butter(2, [1 / 50, 8 / 50], 'band')
def series(pairs, key):
    t = np.array([p['t'] for p in pairs]); r = np.full((len(t), 3), np.nan)
    for i, p in enumerate(pairs):
        v = p.get('rate') if key == 'all' else (p.get(key) or {}).get('rate')
        if v: r[i] = v
    ok = np.isfinite(r).all(1)
    for k in range(3): r[~ok, k] = np.interp(t[~ok], t[ok], r[ok, k])
    return t, np.degrees(filtfilt(*bb, r, axis=0)), ok
out = {}
print(f'{"case":38s} {"full":>6s} {"T-B":>6s} {"L-R":>6s} {"T-B/full":>8s} {"L-R/full":>8s}  per-axis T-B/full')
for f, (a, b), lab in CASES:
    pairs = json.load(open(O + f + '.json'))['pairs']
    t, full, ok = series(pairs, 'all')
    S = {k: series(pairs, k)[1] for k in ['top', 'bottom', 'left', 'right']}
    m = (t > a) & (t < b) & ok
    rms = lambda x: np.sqrt((x[m] ** 2).sum(1).mean())
    tb, lr, fu = rms(S['top'] - S['bottom']), rms(S['left'] - S['right']), rms(full)
    ax = [np.sqrt(((S['top'] - S['bottom'])[m, k] ** 2).mean()) / np.sqrt((full[m, k] ** 2).mean()) for k in range(3)]
    print(f'{lab:38s} {fu:6.1f} {tb:6.1f} {lr:6.1f} {tb/fu:8.2f} {lr/fu:8.2f}  {np.round(ax,2)}')


def ess_report():
    """Rotation-only probe vs translation-aware (essential-matrix) estimate of the same residual."""
    print(f'\n{"case":38s} {"full":>6s} {"ess":>6s} {"full-ess":>8s} {"ratio":>6s} {"corr/axis":>18s} {"calm ratio":>10s}')
    for f, (a, b), lab in CASES:
        pairs = json.load(open(O + f + '.json'))['pairs']
        t, full, ok = series(pairs, 'all')
        _, ess, oke = series(pairs, 'ess')
        m = (t > a) & (t < b) & ok & oke
        c = ((t < a - 0.5) | (t > b + 0.5)) & ok & oke
        rms = lambda x, mm: np.sqrt((x[mm] ** 2).sum(1).mean())
        cr = [np.corrcoef(full[m, k], ess[m, k])[0, 1] for k in range(3)]
        print(f'{lab:38s} {rms(full,m):6.1f} {rms(ess,m):6.1f} {rms(full-ess,m):8.1f} {rms(full-ess,m)/rms(full,m):6.2f} {str(np.round(cr,2)):>18s} {rms(full-ess,c)/max(rms(full,c),1e-9):10.2f}')


if __name__ == '__main__':
    ess_report()
