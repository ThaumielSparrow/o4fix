"""Compare Gyroflow-free warp residual with render-measured residual (0073 base)."""
import sys, json
import numpy as np
from pathlib import Path
sys.path.insert(0, str(Path(__file__).parent))
import fbclip as F


def warp(path):
    d = json.load(open(path))
    t = np.array([p['t'] for p in d['pairs']]); n = np.array([p['n'] for p in d['pairs']])
    r = np.array([p['rate'] if p['rate'] else [np.nan] * 3 for p in d['pairs']], float)
    bad = ~np.isfinite(r).all(1)
    for k in range(3):
        r[bad, k] = np.interp(t[bad], t[~bad], r[~bad, k])
    return t, F.hp3(r, 1.0), n


if __name__ == '__main__':
    c = F.CLIPS['0073']; B = np.load(c['dir'] / 'intended_B.npy'); cam = F.camera(c['base_camera'])
    bursts = F.bursts_from(c['bursts'], 'b')
    for nm in ['late', 'early', 'cal1']:
        tr, er, nr, _ = F.residual(c, 'base', nm, cam, B)
        tw, ew, nw = warp(F.FB / f'warp/base-{nm}.json')
        g = F.gate(tr, bursts)
        for sh in [-0.01, -0.005, 0.0, 0.005]:
            ew2 = np.stack([np.interp(tr, tw + sh, ew[:, k]) for k in range(3)], 1)
            for lab, m in [('burst', g > 0.99), ('nonburst', (g < 0.01) & (nr >= 300))]:
                if m.sum() < 50: continue
                out = []
                for k in range(3):
                    a = er[m, k]; b = ew2[m, k]
                    out.append(f"ax{k} corr {np.corrcoef(a, b)[0, 1]:+.2f} slope {(a * b).sum() / (b * b).sum():+.2f} rms {np.degrees(a.std()):.1f}/{np.degrees(b.std()):.1f}")
                print(f"{nm:5s} sh{sh*1000:+.0f}ms {lab:8s} n{m.sum():5d} " + ' | '.join(out))


def correct_from_warp(clip, tag, gain=1.0, prefix='base'):
    """Correction angles from the Gyroflow-free probe (warp/<clip-prefix>base-<win>.json)."""
    c = F.CLIPS[clip]; bursts = F.bursts_from(c['bursts'], 'b')
    wdir = F.FB / 'warp' / ('' if clip == '0073' else clip)
    ts, angs, total = [], [], np.zeros(3)
    for w, (a, b) in sorted(c['windows'].items(), key=lambda kv: kv[1][0]):
        p = wdir / f'{prefix}-{w}.json'
        if not p.exists():
            continue
        t, e, n = warp(p)
        conf = np.convolve(np.clip((n - 60) / 140, 0, 1), np.ones(9) / 9, 'same')
        inside = [(s, e_) for s, e_ in bursts if s - 1 > a and e_ + 1 < b]
        g = F.gate(t, inside)
        ang = total + np.cumsum(-gain * (g * conf)[:, None] * e, 0) * 0.01
        total = ang[-1]; ts.append(t); angs.append(ang)
        print(w, 'bursts', len(inside), 'max deg', np.degrees(np.abs(ang - ang[0]).max()).round(3))
    t = np.concatenate(ts); ang = np.concatenate(angs); assert np.all(np.diff(t) > 0)
    with (c['dir'] / f'{tag}-adjust.json').open('x') as fh:
        json.dump({'t': t.tolist(), 'angle': ang.tolist()}, fh)
