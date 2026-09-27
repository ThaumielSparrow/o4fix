"""regions-v2 step 2: is the judder OUTSIDE bursts a telemetry error the source-frame probe can
see (and fix)? For the worst outside bins (outside_worst.json) plus calm control bins, run
warp_residual_probe (O4_REGIONS=1) on the production output and compare, per 0.5 s bin:
  label = 1-8 Hz corr(probe, render residual)      (probe right?)
  pred  = 1-8 Hz corr(probe, essential estimate)    (render-free agreement)
  proxy = predicted rms(render - probe)/rms(render) (would subtracting the probe help?)

  outside_probe.py probe      run the probe on merged windows around the bins
  outside_probe.py report     tables"""
import json, sys
from pathlib import Path
import numpy as np
from scipy.signal import butter, filtfilt
sys.path.insert(0, str(Path(__file__).resolve().parent))
import gatecheck as g

F = g.F; RV2 = g.RV2
bb = butter(2, [1 / 50, 8 / 50], 'band')


def bins():
    worst = [dict(r, kind='worst') for r in json.load(open(RV2 / 'outside_worst.json'))]
    R = json.load(open(RV2 / 'remap_classified.json'))
    rng = np.random.default_rng(0); ctl = []
    for clip in sorted(set(r['clip'] for r in worst)):
        calm = [r for r in R if r['clip'] == clip and r['state'] == 'outside, far from bursts'
                and r['inliers'] >= 150 and r['lo'] < 4]
        for i in rng.choice(len(calm), min(8, len(calm)), replace=False):
            ctl.append(dict(calm[i], kind='control'))
    return worst + ctl


def windows(clip, bs):
    ws = []
    for t in sorted(r['t'] for r in bs if r['clip'] == clip):
        s, e = max(0.1, t - 1.5), t + 2.0
        if ws and s <= ws[-1][1]: ws[-1][1] = max(ws[-1][1], e)
        else: ws.append([s, e])
    return [(round(s, 2), round(e - s, 2)) for s, e in ws]


def probe():
    bs = bins()
    for clip in sorted(set(r['clip'] for r in bs)):
        for s, du in windows(clip, bs):
            out = g.d(clip + 'p') / f'osrc-{s}.json'
            if not out.exists():
                F.run_low([str(g.ROOT / 'target/release/examples/warp_residual_probe.exe'), str(g.d(clip + 'p') / 'prod.MP4'),
                           str(s), str(du), '5.092569', '0', '3', str(out)], cwd=g.ROOT,
                          env=dict(g.ENV, O4_REGIONS='1', O4_CROP='1.30,0.72'), stdout=g.subprocess.DEVNULL)
        print(clip, 'probed', flush=True)


def series(rows, key):
    t = np.array([p['t'] for p in rows]); r = np.full((len(t), 3), np.nan)
    for i, p in enumerate(rows):
        v = p.get('rate') if key == 'all' else (p.get(key) or {}).get('rate')
        if v: r[i] = v
    ok = np.isfinite(r).all(1)
    if ok.sum() < 20: return t, None
    for k in range(3): r[~ok, k] = np.interp(t[~ok], t[ok], r[ok, k])
    return t, np.degrees(filtfilt(*bb, r, axis=0))


def render(clip):
    cam = F.camera(g.d(clip + 'p') / 'prod-camera.json'); B = np.load(F.FB / 'intended_B.npy')
    t, r, n = F.load(g.d(clip + 'p') / 'ren-prod-0.json') if (g.d(clip + 'p') / 'ren-prod-0.json').exists() else (None, None, None)
    T, X, N = [], [], []
    for p in sorted(g.d(clip + 'p').glob('ren-prod-*.json'), key=lambda p: float(p.stem.split('-')[-1])):
        t, r, n = F.load(p)
        res = r[:, :3] - F.design(t, cam) @ B
        bad = ~np.isfinite(res).all(1)
        for k in range(3): res[bad, k] = np.interp(t[bad], t[~bad], res[~bad, k])
        T.append(t); X.append(np.degrees(res @ F.MINV.T)); N.append(n)
    t = np.concatenate(T); o = np.argsort(t)
    return t[o], filtfilt(*bb, np.concatenate(X)[o], axis=0), np.concatenate(N)[o]


def report():
    bs = bins(); out = []
    pc = lambda a, b: float((a * b).sum() / np.sqrt((a * a).sum() * (b * b).sum()))
    for clip in sorted(set(r['clip'] for r in bs)):
        tr, rr, nr = render(clip)
        for s, du in windows(clip, bs):
            src = json.load(open(g.d(clip + 'p') / f'osrc-{s}.json'))['pairs']
            ts, P = series(src, 'all'); _, S = series(src, 'ess')
            if P is None or S is None: continue
            for r in (x for x in bs if x['clip'] == clip and s <= x['t'] and x['t'] + 0.5 <= s + du):
                m = (tr >= r['t']) & (tr < r['t'] + 0.5) & (nr >= 60)
                ms = (ts >= r['t']) & (ts < r['t'] + 0.5)
                if m.sum() < 20 or ms.sum() < 20: continue
                Pi = np.stack([np.interp(tr[m], ts, P[:, k]) for k in range(3)], 1)
                pr = float(np.sqrt((Pi ** 2).sum(1).mean())); re = float(np.sqrt((rr[m] ** 2).sum(1).mean()))
                lab = pc(Pi, rr[m]); p_ = pr / re
                out.append(dict(r, label=lab, pred=pc(P[ms], S[ms]), probe=pr, render=re,
                                proxy=float(np.sqrt(max(1 + p_ * p_ - 2 * lab * p_, 0)))))
    json.dump(out, open(RV2 / 'outside_probe.json', 'w'), indent=0)
    for kind in ['worst', 'control']:
        x = [r for r in out if r['kind'] == kind]
        L = np.array([r['label'] for r in x]); Pd = np.array([r['pred'] for r in x]); Q = np.array([r['proxy'] for r in x])
        print(f"\n{kind}: n={len(x)}  label median {np.median(L):.2f}  pred median {np.median(Pd):.2f}  proxy median {np.median(Q):.2f}"
              f"  helps(<0.9) {np.sum(Q < 0.9)}  hurts(>1.1) {np.sum(Q > 1.1)}  corr(label,pred) {np.corrcoef(L, Pd)[0, 1]:.2f}")
        if kind == 'worst':
            for lo, hi in [(-1, 0.3), (0.3, 0.5), (0.5, 0.75), (0.75, 1.01)]:
                m = (Pd >= lo) & (Pd < hi)
                if m.any():
                    print(f'  pred [{lo:.2f},{hi:.2f}) n{m.sum():3d} label median {np.median(L[m]):.2f} proxy median {np.median(Q[m]):.2f}'
                          f' helps {np.sum(Q[m] < 0.9)} hurts {np.sum(Q[m] > 1.1)}')
            n = np.array([r['noise'] for r in x])
            for lo, hi, nm in [(0, 1.5, 'clean'), (1.5, 5, 'light'), (5, 8, 'near-severe'), (8, 1e9, 'severe-level')]:
                m = (n >= lo) & (n < hi)
                if m.any():
                    print(f'  noise {nm:12s} n{m.sum():3d} label median {np.median(L[m]):.2f} pred median {np.median(Pd[m]):.2f} proxy median {np.median(Q[m]):.2f}')


if __name__ == '__main__':
    {'probe': probe, 'report': report}[sys.argv[1]]()
