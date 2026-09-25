"""Synthesize the feedback correction (angle JSON for feedback_repair) from measured windows.
usage: fb4.py TAG GAIN MEAS_PREFIX [CAMERA_JSON]   (MEAS_PREFIX e.g. base -> base-late.json/base-early.json)"""
import json, sys
from pathlib import Path
import numpy as np
sys.path.insert(0, str(Path(__file__).parent))
from fb import OUT, READOUT_MID, load
from fb2 import gate, Minv, filtfilt, butter2_hp, LAG, BURSTS
from fb3 import camera, CAM, design, pair_features

WIN = {'late': (130, 148), 'early': (12, 32)}


def correction(prefix, B, cam, gain):
    ts, angs = [], []
    total = np.zeros(3)
    for nm, (a, b) in WIN.items():
        t, r, n = load(OUT / f'{prefix}-{nm}.json')
        wS, wO, f = pair_features(t, cam)
        res = r[:, :3] - design(wS, f) @ B
        bad = ~np.isfinite(res).all(1)
        for k in range(3):
            res[bad, k] = np.interp(t[bad], t[~bad], res[~bad, k])
        e = res @ Minv.T
        e = np.stack([filtfilt(*butter2_hp(1.0, 100.), e[:, k]) for k in range(3)], 1)
        conf = np.clip((n - 60) / 140, 0, 1); conf[bad] = 0
        conf = np.convolve(conf, np.ones(9) / 9, 'same')
        # only bursts wholly inside the measured window with 1 s margin
        inside = [(s, e_) for s, e_ in BURSTS if s - 1 > a and e_ + 1 < b]
        w = np.zeros_like(t)
        from fb import smooth_env
        for s, e_ in inside:
            w = np.maximum(w, smooth_env(t, s - 0.25, e_ + 0.25, 0.15))
        c = -gain * (w * conf)[:, None] * e
        tau = t + READOUT_MID + LAG
        ang = total + np.cumsum(c, 0) * 0.01
        total = ang[-1]
        ts.append(tau); angs.append(ang)
        print(nm, 'bursts', [(round(s, 2), round(e_, 2)) for s, e_ in inside], 'max deg', np.degrees(np.abs(ang - ang[0]).max()).round(3))
    order = np.argsort([x[0] for x in ts])
    t = np.concatenate([ts[i] for i in order]); ang = np.concatenate([angs[i] for i in order])
    # windows are disjoint; carried offset of the earlier window must precede the later one
    return t, ang


if __name__ == '__main__':
    tag, gain, prefix = sys.argv[1], float(sys.argv[2]), sys.argv[3]
    cam = camera(sys.argv[4] if len(sys.argv) > 4 else CAM)
    B = np.load(OUT / 'intended_B.npy')
    t, ang = correction(prefix, B, cam, gain)
    with (OUT / f'{tag}-adjust.json').open('x') as fh:
        json.dump({'t': t.tolist(), 'angle': ang.tolist()}, fh)
