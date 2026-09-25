"""Score residual (intended-path-subtracted, 1 Hz HP, body deg/s) per window/burst for render variants.
usage: score.py PREFIX:CAMERA_JSON [PREFIX:CAMERA_JSON ...]"""
import sys
from pathlib import Path
import numpy as np
sys.path.insert(0, str(Path(__file__).parent))
from fb import OUT, load
from fb2 import gate, Minv, filtfilt, butter2_hp, BURSTS
from fb3 import camera, design, pair_features

B = np.load(OUT / 'intended_B.npy')


def resid(prefix, nm, cam):
    t, r, n = load(OUT / f'{prefix}-{nm}.json')
    wS, wO, f = pair_features(t, cam)
    res = r[:, :3] - design(wS, f) @ B
    bad = ~np.isfinite(res).all(1)
    for k in range(3):
        res[bad, k] = np.interp(t[bad], t[~bad], res[~bad, k])
    e = np.degrees(res @ Minv.T)
    e = np.stack([filtfilt(*butter2_hp(1.0, 100.), e[:, k]) for k in range(3)], 1)
    return t, e, n


def band(e, lo, hi):
    from fb2 import butter2_hp as hp
    # crude band split via HP at lo minus HP at hi
    a = np.stack([filtfilt(*hp(lo, 100.), e[:, k]) for k in range(3)], 1)
    b = np.stack([filtfilt(*hp(hi, 100.), e[:, k]) for k in range(3)], 1)
    return a - b if hi < 50 else a


if __name__ == '__main__':
    variants = [a.split(':', 1) for a in sys.argv[1:]]
    cams = {p: camera(c) for p, c in variants}
    rows = []
    for nm in ['late', 'early', 'cal1']:
        segs = [(s, e) for s, e in BURSTS if (nm == 'late' and 131 < s < 147) or (nm == 'early' and 13 < s < 31)]
        labels = [f'{s:.1f}-{e:.1f}' for s, e in segs] + ['non-burst(n>=300)']
        out = {}
        for p, _ in variants:
            t, e, n = resid(p, nm, cams[p])
            w = gate(t)
            lo, hi = band(e, 1, 8), band(e, 8, 99)
            vals = []
            masks = [(t >= s) & (t <= e_) for s, e_ in segs] + [(w < 0.01) & (n >= 300)]
            for m in masks:
                vals.append((np.sqrt((lo[m] ** 2).sum(1).mean()), np.sqrt((hi[m] ** 2).sum(1).mean())))
            out[p] = vals
        print(f'== {nm}  (|3-axis| rms deg/s, 1-8 Hz / 8-50 Hz)')
        for i, lab in enumerate(labels):
            print(f'  {lab:18s}', '  '.join(f'{p}: {out[p][i][0]:5.2f}/{out[p][i][1]:5.2f}' for p, _ in variants))
