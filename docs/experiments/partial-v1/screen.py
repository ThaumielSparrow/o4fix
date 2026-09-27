"""partial-v1: render-free offline screen of PARTIAL refinement corrections.

Refinement today is all-or-nothing per burst: subtract the rotation-only probe residual P
(`ser(src,'all')`) or nothing. This screen scores partial corrections built ONLY from P and the
translation-aware essential-matrix estimate S (`ser(src,'ess')`), both 1-8 Hz body deg/s. The
current (unrefined) render residual rr is used only as the referee:
    predicted outcome = rms(rr - c)/rms(rr)   (3-axis vector RMS, G.label's render mask)
< 1 = the correction is predicted to help. With c = P this is pool.py's `ratio`.

Variants: none (c=0), full (P), axis75/axis50 (per-axis corr(P_k,S_k) gate), local{0.3,0.4,0.6}
(sliding pooled corr -> weight clip((corr-0.3)/0.45,0,1), 0.2 s boxcar, c=w*P), proj (per-axis
projection of P onto S over the burst, negative clipped to 0), projloc (same in a 0.4 s sliding
window), ess (S), mean ((P+S)/2).

Data: target/experiments/regions-v2/<clip>nr (all severe bursts unrefined, 7 clips) and the
capped-only 0076/0058/0005 sets whose forced full-refinement renders give ground truth for 'full'.
Output: target/experiments/partial-v1/screen.json."""
import json, re, sys
from pathlib import Path
import numpy as np
from scipy.signal import filtfilt
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'regions-v2'))
import gatecheck as G

OUT = G.F.E / 'partial-v1'; OUT.mkdir(exist_ok=True)
J = G.F.E / 'judder83'
LOGS = {'0076nr': G.RV2 / 'cur_0076_run.log', '0058nr': G.RV2 / 'cur_0058_run.log', '0057nr': G.RV2 / 'cur_0057_run.log',
        '0059nr': G.RV2 / 'cur_0059_run.log', '0027nr': J / 'veto_0027_run.log', '0071nr': J / 'veto_0071_run.log',
        '0073nr': J / 'veto_0073_run.log'}
CAL = {'0076': 'score-cur-all-gate.json', '0058': 'score-cur-all.json', '0005': 'score-cur-all-gate.json'}
VARS = ['none', 'full', 'axis75', 'axis50', 'local0.3', 'local0.4', 'local0.6', 'proj', 'projloc', 'ess', 'mean']
FS = 100.0


def box(x, sec):
    n = max(1, int(round(sec * FS))); return np.apply_along_axis(lambda v: np.convolve(v, np.ones(n), 'same'), 0, x)


def weight(P, S, sec):
    """Sliding pooled 3-axis corr(P,S), centred, mapped to [0,1] and smoothed (0.2 s)."""
    ps = box((P * S).sum(1), sec); pp = box((P * P).sum(1), sec); ss = box((S * S).sum(1), sec)
    c = ps / np.sqrt(pp * ss + 1e-12)
    w = np.clip((c - G.FLOOR) / (G.THRESH - G.FLOOR), 0, 1)
    return box(w, 0.2) / box(np.ones_like(w), 0.2)


def variants(P, S, ms):
    """All correction series on the probe time base. Only P, S and the render-free mask ms."""
    ax = np.array([G.pc(P[ms, k], S[ms, k]) for k in range(3)])
    proj = S * np.maximum(0, (P[ms] * S[ms]).sum(0)) / ((S[ms] ** 2).sum(0) + 1e-12)
    projloc = S * np.maximum(0, box(P * S, 0.4)) / (box(S * S, 0.4) + 1e-9)
    c = {'none': 0 * P, 'full': P, 'axis75': P * (ax >= 0.75), 'axis50': P * (ax >= 0.5),
         'proj': proj, 'projloc': projloc, 'ess': S, 'mean': (P + S) / 2}
    for sec in (0.3, 0.4, 0.6):
        c[f'local{sec}'] = weight(P, S, sec)[:, None] * P
    return c, ax


def states(clip):
    if clip not in LOGS: return {}
    return {float(m[1]): ('refined' if m[4] else 'capped' if 'over cap' in m[5] else 'other')
            for m in re.finditer(r'\[\s*([\d.]+),\s*([\d.]+)\]\s+(residual correction ([\d.]+) deg|not refined: (.*))',
                                 open(LOGS[clip]).read())}


def pool_ratio(r):
    p = r['probe'] / r['render']; return np.sqrt(max(1 + p * p - 2 * r['label'] * p, 0))


def run(clip):
    lab = {r['a']: r for r in json.load(open(G.d(clip) / 'labels.json'))}
    st = states(clip); rows, skipped, cache = [], [], {}
    for a, b, cap in G.capped(clip):
        w = next(((s, du) for s, du in G.windows(clip) if s <= a and b <= s + du + 1e-6), None)
        if w is None: skipped.append((clip, a, 'no window')); continue
        s = w[0]
        if s not in cache:
            src = json.load(open(G.d(clip) / f'src-{s}.json'))['pairs']
            ts, P = G.ser(src, 'all'); _, S = G.ser(src, 'ess')
            tr, res, nr = G.ren_residual(clip, 'cur', s)
            cache[s] = ts, P, S, tr, filtfilt(*G.bb, res, axis=0), nr
        ts, P, S, tr, rr, nr = cache[s]
        m = (tr > a) & (tr < b) & (nr >= 60) & (tr > ts[0] + 0.5) & (tr < ts[-1] - 0.5)
        ms = (ts > a) & (ts < b) & (ts > ts[0] + 0.5) & (ts < ts[-1] - 0.5)
        if m.sum() < 20 or ms.sum() < 20: skipped.append((clip, a, f'n render {m.sum()} / probe {ms.sum()} < 20')); continue
        cs, ax = variants(P, S, ms)
        R = rr[m]; den = np.sqrt((R ** 2).sum(1).mean())
        ratio = {}
        for k, c in cs.items():
            ci = np.stack([np.interp(tr[m], ts, c[:, j]) for j in range(3)], 1)
            ratio[k] = float(np.sqrt(((R - ci) ** 2).sum(1).mean()) / den)
        L = lab.get(a, {})
        rows.append(dict(clip=clip, a=a, b=b, state=st.get(a, 'capped' if clip in CAL else 'other'),
                         agreement=G.pc(P[ms], S[ms]), agree_axes=ax.tolist(), render=float(den), n=int(m.sum()),
                         pool_ratio=pool_ratio(L) if 'label' in L else None, pred_free=L.get('pred_free'), ratio=ratio))
    return rows, skipped


def group(r):
    ag = r['agreement']
    if ag >= 0.75: return 'admitted/high (>=0.75)'
    if r['state'] == 'capped': return 'over-cap rejected (<0.75)'
    if r['state'] == 'refined': return 'floor-skipped (<0.3)' if ag < 0.3 else 'refined 0.3-0.75'
    return None


if __name__ == '__main__':
    rows, skipped = [], []
    for c in list(LOGS) + list(CAL):
        r, sk = run(c); rows += r; skipped += sk
    nr = [r for r in rows if r['clip'] in LOGS]; cal = [r for r in rows if r['clip'] in CAL]

    print('(a) plumbing: full vs pool.py ratio')
    dd = np.array([abs(r['ratio']['full'] - r['pool_ratio']) for r in rows if r['pool_ratio'] is not None])
    da = np.array([abs(r['agreement'] - r['pred_free']) for r in rows if r['pred_free'] is not None])
    print(f'  n {len(dd)}  max |ratio diff| {dd.max():.2e}   agreement vs labels.json pred_free: n {len(da)} max |diff| {da.max():.2e}')
    print('  skipped:', skipped or 'none')

    print('\n(b) calibration, capped bursts with forced full-refinement renders (proxy full vs actual all/cur, 1-8 Hz)')
    for c, fn in CAL.items():
        sc = {float(s['segment'].split('-')[0]): s for s in json.load(open(G.d(c) / fn)) if s['segment'] != 'control'}
        for r in [r for r in cal if r['clip'] == c]:
            s = sc.get(r['a'])
            act = s['all'][0] / s['cur'][0] if s else np.nan
            print(f"  {c} {r['a']:7.2f}-{r['b']:7.2f} agree {r['agreement']:5.2f}  proxy full {r['ratio']['full']:5.2f}  actual {act:5.2f}"
                  f"   | best other: " + ' '.join(f"{k} {r['ratio'][k]:.2f}" for k in ['axis75', 'local0.4', 'proj', 'projloc', 'mean']))
            r['actual_full'] = None if s is None else act

    print('\n(c) groups (7 --no-refine clips): median ratio  helps<0.9/neutral/hurts>1.1')
    G_ = {}
    for r in nr: G_.setdefault(group(r), []).append(r)
    for g in ['over-cap rejected (<0.75)', 'floor-skipped (<0.3)', 'refined 0.3-0.75', 'admitted/high (>=0.75)', None]:
        rs = G_.get(g, [])
        if not rs: continue
        print(f'  {str(g)} n={len(rs)}')
        for k in VARS:
            q = np.array([r['ratio'][k] for r in rs])
            print(f'    {k:9s} median {np.median(q):5.2f}  {np.sum(q < 0.9):3d} / {np.sum((q >= 0.9) & (q <= 1.1)):3d} / {np.sum(q > 1.1):3d}')

    print('\n(d) target bursts (over-cap rejected + floor-skipped)')
    print(f"  {'clip':7s} {'a':>7s} {'b':>7s} grp  agree  ax0   ax1   ax2   render " + ' '.join(f'{k[:7]:>7s}' for k in VARS[1:]))
    for g, tag in [('over-cap rejected (<0.75)', 'cap'), ('floor-skipped (<0.3)', 'flr')]:
        for r in sorted(G_.get(g, []), key=lambda r: (r['clip'], r['a'])):
            print(f"  {r['clip']:7s} {r['a']:7.2f} {r['b']:7.2f} {tag}  {r['agreement']:5.2f} " + ' '.join(f'{x:5.2f}' for x in r['agree_axes'])
                  + f"  {r['render']:6.1f} " + ' '.join(f"{r['ratio'][k]:7.2f}" for k in VARS[1:]))
    for r in rows: r['group'] = group(r) if r['clip'] in LOGS else 'calibration (capped)'
    json.dump(rows, open(OUT / 'screen.json', 'w'), indent=1)
    print('\nwrote', OUT / 'screen.json', len(rows), 'rows')
