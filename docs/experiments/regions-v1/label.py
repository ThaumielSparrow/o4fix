"""regions-v1: label each burst "probe right/wrong" with the render measurement, and test
render-free predictors (agreement of the rotation-only probe with a translation-aware
essential-matrix estimate on the same features)."""
import glob, json, re, sys
from pathlib import Path
import numpy as np
from scipy.signal import butter, filtfilt
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'feedback-v1'))
import fbclip as F
E = F.E; RV = E / 'regions-v1'
bb = butter(2, [1 / 50, 8 / 50], 'band')

def bursts(log):
    out = []
    for m in re.finditer(r'\[\s*([\d.]+),\s*([\d.]+)\]\s+(residual correction ([\d.]+) deg|not refined: (?:correction ([\d.]+) deg over cap|(.*)))', open(log).read()):
        out.append((float(m[1]), float(m[2]), float(m[4] or m[5] or 'nan')))
    return out

def load_src(files):
    rows = [p for f in files for p in json.load(open(f))['pairs']]
    rows.sort(key=lambda p: p['t'])
    t = np.array([p['t'] for p in rows])
    keep = np.r_[True, np.diff(t) > 1e-4]; rows = [r for r, k in zip(rows, keep) if k]; t = t[keep]
    def ser(key):
        r = np.full((len(t), 3), np.nan)
        for i, p in enumerate(rows):
            v = p.get('rate') if key == 'all' else (p.get(key) or {}).get('rate')
            if v: r[i] = v
        ok = np.isfinite(r).all(1)
        for k in range(3): r[~ok, k] = np.interp(t[~ok], t[ok], r[ok, k])
        return np.degrees(filtfilt(*bb, r, axis=0)), ok
    return t, ser('all'), ser('ess')

def load_ren(files, cam_json):
    cam = F.camera(cam_json); B = np.load(F.FB / 'intended_B.npy'); T = []; X = []; N = []
    for f in sorted(files):
        t, r, n = F.load(Path(f))
        res = r[:, :3] - F.design(t, cam) @ B
        bad = ~np.isfinite(res).all(1)
        for k in range(3): res[bad, k] = np.interp(t[bad], t[~bad], res[~bad, k])
        T.append(t); X.append(filtfilt(*bb, res @ F.MINV.T, axis=0)); N.append(n)
    t = np.concatenate(T); o = np.argsort(t)
    return t[o], np.degrees(np.concatenate(X)[o]), np.concatenate(N)[o]

SETS = [
    ('0021', sorted(glob.glob(str(RV / '0021/src-*.json'))), sorted(glob.glob(str(RV / '0021/ren-*.json'))),
     E / 'release-v013/0021-norefine-camera.json', E / 'release-v013/0021_v2_run.log'),
    ('0083', sorted(glob.glob(str(RV / '0083/src-*.json'))), sorted(glob.glob(str(RV / '0083/ren-*.json'))),
     E / 'judder83/veto-camera.json', E / 'judder83/veto_run.log'),
    ('0060', [str(RV / 'c60_48.json'), str(RV / 'c60_191.json')],
     [str(E / 'fullprobe-v1/0060/render-45.json'), str(E / 'fullprobe-v1/0060/render-188.json')],
     E / 'release-v013/0060-release-camera.json', E / 'release-v013/0060_run.log'),
]
pc = lambda a, b: float((a * b).sum() / np.sqrt((a * a).sum() * (b * b).sum()))
rows = []
for clip, src, ren, cam, log in SETS:
    ts, (full, okf), (ess, oke) = load_src(src)
    tr, rr, nr = load_ren(ren, cam)
    for a, b, cap in bursts(log):
        m = (tr > a) & (tr < b) & (nr >= 60) & (tr > ts[0] + 0.5) & (tr < ts[-1] - 0.5)
        if m.sum() < 20:
            continue
        P = np.stack([np.interp(tr[m], ts, full[:, k]) for k in range(3)], 1)
        S = np.stack([np.interp(tr[m], ts, ess[:, k]) for k in range(3)], 1)
        R = rr[m]
        rows.append(dict(clip=clip, a=a, b=b, cap=cap, n=int(m.sum()),
                         label=pc(P, R), probe=np.sqrt((P**2).sum(1).mean()), render=np.sqrt((R**2).sum(1).mean()),
                         pred=pc(P, S), ess=np.sqrt((S**2).sum(1).mean()), ess_vs_render=pc(S, R)))
rows.sort(key=lambda r: r['pred'])
print(f'{"burst":20s} {"cap":>5s} {"probe~render":>12s} {"probe~ess":>9s} {"ess~render":>10s} {"probe":>6s} {"ess":>6s} {"render":>6s}')
for r in rows:
    print(f'{r["clip"]} {r["a"]:6.1f}-{r["b"]:6.1f} {r["cap"]:5.1f} {r["label"]:12.2f} {r["pred"]:9.2f} {r["ess_vs_render"]:10.2f} {r["probe"]:6.1f} {r["ess"]:6.1f} {r["render"]:6.1f}')
L = np.array([r['label'] for r in rows]); Pp = np.array([r['pred'] for r in rows]); C = np.array([r['cap'] for r in rows])
print('\ncorr(label, probe~ess) =', np.corrcoef(L, Pp)[0, 1].round(2), ' corr(label, -cap) =', np.corrcoef(L, -np.nan_to_num(C, nan=0))[0, 1].round(2), ' n =', len(rows))
json.dump(rows, open(RV / 'labels.json', 'w'), indent=1)
