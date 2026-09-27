"""partial-v1 step 2: render the two refinement policies on the --no-refine clips.

  drive.py adjust CLIP     write pv1prod-adjust.json and pv1local-adjust.json (+ pv1local-w.json)
  drive.py states CLIP     print the production state per labels.json burst

pv1prod : full-weight correction (same as gatecheck.adjust) on exactly the bursts production
          refines today (cap gate 0.75 + agreement floor 0.3), taken from <clip>p/prod_run.log.
pv1local: every labels.json burst, weight g*conf*w(t), w = screen.weight(P, S, 0.4) on each
          probe window's src pairs (P = ser(src,'all'), S = ser(src,'ess')).
Repair / render / measure / score then use gatecheck.py unchanged (tags pv1prod, pv1local)."""
import json, re, sys
from pathlib import Path
import numpy as np
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'regions-v2'))
sys.path.insert(0, str(Path(__file__).resolve().parent))
import gatecheck as G
import screen

F = G.F
TOL = 0.02


def prod_log(clip):
    return G.RV2 / (clip.replace('nr', '') + 'p') / 'prod_run.log'


def prod_states(clip):
    """{burst start: (end, state, agreement or None)} from the production repair log."""
    txt = open(prod_log(clip)).read()
    st = {}
    for m in re.finditer(r'\[\s*([\d.]+),\s*([\d.]+)\]\s+(residual correction [\d.]+ deg( \(over cap, admitted: agreement ([-\d.]+)\))?|not refined: (.*))', txt):
        a, b = float(m[1]), float(m[2])
        ag = re.search(r'agreement ([-\d.]+)', m[3])
        ag = float(ag[1]) if ag else None
        if m[6] is None: s = 'admitted' if m[4] else 'refined'
        elif 'over cap' in m[6]: s = 'capped-rejected'
        elif 'disagree' in m[6]: s = 'floor-skipped'
        else: s = 'unrefined-other'
        st[a] = (b, s, ag)
    return st


def match(clip):
    """labels.json rows -> production state; also returns log bursts with no label row."""
    rows = json.load(open(G.d(clip) / 'labels.json'))
    st = prod_states(clip); used = set(); out = []
    for r in rows:
        k = [a for a in st if abs(a - r['a']) <= TOL]
        if k:
            a = k[0]; used.add(a); out.append((r, st[a][1], st[a][2], a))
        else:
            out.append((r, 'unmatched', None, None))
    extra = [(a, st[a]) for a in st if a not in used]
    return out, extra


def build(clip, tag, sel, weighted):
    """gatecheck.adjust's loop, with an optional extra per-sample weight w(t) (screen.weight 0.4)."""
    ts, angs, total, wrec = [], [], np.zeros(3), {}
    for s, du in G.windows(clip):
        src = json.load(open(G.d(clip) / f'src-{s}.json'))['pairs']
        t = np.array([p['t'] for p in src]); n = np.array([p['n'] for p in src])
        r = np.array([p['rate'] if p['rate'] else [np.nan] * 3 for p in src], float)
        bad = ~np.isfinite(r).all(1)
        for k in range(3): r[bad, k] = np.interp(t[bad], t[~bad], r[~bad, k])
        e = F.hp3(r, 1.0)
        conf = np.convolve(np.clip((n - 60) / 140, 0, 1), np.ones(9) / 9, 'same')
        mine = [(a, b) for a, b in sel if s < a and b < s + du]
        g = F.gate(t, mine)
        if weighted:
            tp, P = G.ser(src, 'all'); tS, S = G.ser(src, 'ess')
            assert np.array_equal(tp, t) and np.array_equal(tS, t) and len(P) == len(t)
            assert np.allclose(np.diff(t), 0.01, atol=2e-3), 'screen.weight assumes 100 Hz pairs'
            w = screen.weight(P, S, 0.4)
            for a, b in mine:
                m = (t > a) & (t < b)
                wrec[a] = dict(b=b, w_mean=float(w[m].mean()), w_min=float(w[m].min()), w_max=float(w[m].max()),
                               gconfw=float((g * conf * w)[m].mean()), gconf=float((g * conf)[m].mean()))
            g = g * w
        ang = total + np.cumsum(-(g * conf)[:, None] * e, 0) * 0.01
        total = ang[-1]; ts.append(t); angs.append(ang)
        print(clip, tag, 'window', s, 'max deg', np.degrees(np.abs(ang - ang[0]).max()).round(2))
    t = np.concatenate(ts); ang = np.concatenate(angs); assert np.all(np.diff(t) > 0)
    with (G.d(clip) / f'{tag}-adjust.json').open('x') as fh:
        json.dump({'t': t.tolist(), 'angle': ang.tolist(), 'bursts': sel}, fh)
    if weighted:
        with (G.d(clip) / f'{tag}-w.json').open('x') as fh:
            json.dump(wrec, fh, indent=1)
    return wrec


def adjust(clip):
    out, extra = match(clip)
    print(f'{clip}: {len(out)} label rows; unmatched labels:',
          [r['a'] for r, s, _, _ in out if s == 'unmatched'] or 'none',
          '; log bursts without label row:', [(a, v[1]) for a, v in extra] or 'none')
    prod = [(r['a'], r['b']) for r, s, _, _ in out if s in ('refined', 'admitted')]
    allb = [(r['a'], r['b']) for r, _, _, _ in out]
    print('pv1prod bursts', prod)
    build(clip, 'pv1prod', prod, False)
    wrec = build(clip, 'pv1local', allb, True)
    print(f"\n{'a':>7s} {'b':>7s} {'state':16s} {'agree(lab)':>10s} {'agree(log)':>10s} {'w mean':>6s} {'min':>5s} {'max':>5s}")
    for r, s, ag, _ in out:
        x = wrec.get(r['a'])
        print(f"{r['a']:7.2f} {r['b']:7.2f} {s:16s} {r['pred_free']:10.2f} {'' if ag is None else f'{ag:.2f}':>10s} "
              + (f"{x['w_mean']:6.2f} {x['w_min']:5.2f} {x['w_max']:5.2f}" if x else '  (no window)'))
    fl = G.d(clip) / 'floor-adjust.json'
    if fl.exists():
        fb = {tuple(x) for x in json.load(open(fl))['bursts']}; pb = set(prod)
        print('\npv1prod vs floor: only in pv1prod', sorted(pb - fb), ' only in floor', sorted(fb - pb))


if __name__ == '__main__':
    cmd, clip = sys.argv[1], sys.argv[2]
    if cmd == 'adjust': adjust(clip)
    elif cmd == 'states':
        out, extra = match(clip)
        for r, s, ag, _ in out: print(r['a'], r['b'], s, ag)
        print('extra', extra)
