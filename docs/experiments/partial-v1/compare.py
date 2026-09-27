"""partial-v1 step 2 comparison: cur (no refinement) vs pv1prod (production's refine set) vs
pv1local (every burst, local0.4 weight). Reads score-cur-pv1prod-pv1local.json, labels.json,
pv1local-w.json and the step-1 screen.json. Render values are 1-8 / 8-50 Hz deg/s.

A burst is a control-by-construction (ctl) when both variants apply the same correction:
production leaves it unrefined and pv1local's weight is ~0 there (w max < 0.05), or production
refines it and the weight is ~1 (w mean >= 0.95 and w min >= 0.85). Their pv1local - pv1prod
difference is render / measurement noise."""
import json, sys
from pathlib import Path
import numpy as np
sys.path.insert(0, str(Path(__file__).resolve().parent))
import drive
G = drive.G

CLIPS = ['0058nr', '0076nr', '0073nr']
TAGS = ['cur', 'pv1prod', 'pv1local']
SAME = 2.0
ABBR = {'refined': 'refined', 'admitted': 'admitted', 'capped-rejected': 'capped-rej', 'floor-skipped': 'floor-skip',
        'unrefined-other': 'unref-other', 'unmatched': 'unmatched'}


def main():
    scr = {(r['clip'], round(r['a'], 2)): r for r in json.load(open(G.F.E / 'partial-v1/screen.json'))}
    ctl_d, diff_d, allrows = [], [], []
    for clip in CLIPS:
        sc = json.load(open(G.d(clip) / 'score-cur-pv1prod-pv1local.json'))
        W = {float(k): v for k, v in json.load(open(G.d(clip) / 'pv1local-w.json')).items()}
        st = {r['a']: (s, ag) for r, s, ag, _ in drive.match(clip)[0]}
        lab = {r['a']: r for r in json.load(open(G.d(clip) / 'labels.json'))}
        print(f'\n=== {clip}  (render 1-8/8-50 deg/s; ratios are 1-8 Hz vs cur; pred = step-1 screen proxy)')
        print(f"{'burst':15s} {'agr':>5s} {'log':>5s} {'prod state':11s} {'w':>4s} {'wmin':>4s} "
              f"{'cur':>10s} {'pv1prod':>10s} {'pv1local':>10s} {'L-P':>6s} "
              f"{'predL':>5s} {'L/cur':>5s} {'predP':>5s} {'P/cur':>5s}  ctl")
        controls = []
        for e in sc:
            if e['segment'] == 'control':
                controls.append(e); continue
            a = float(e['segment'].split('-')[0])
            if any(e[t] is None for t in TAGS):
                print(f"{e['segment']:15s} (no render samples)"); continue
            s, ag = st.get(a, ('unmatched', None)); w = W.get(a, {})
            refined = s in ('refined', 'admitted')
            ctl = (not refined and w.get('w_max', 1) < 0.05) or (refined and w.get('w_mean', 0) >= 0.95 and w.get('w_min', 0) >= 0.85)
            cur, P, L = (e[t] for t in TAGS)
            d = L[0] - P[0]
            r = scr.get((clip, round(a, 2)))
            predL = r['ratio']['local0.4'] if r else np.nan
            predP = (r['ratio']['full'] if refined else 1.0) if r else np.nan
            print(f"{e['segment']:15s} {lab[a]['pred_free']:5.2f} {'' if ag is None else f'{ag:5.2f}':>5s} {ABBR[s]:11s} "
                  f"{w.get('w_mean', np.nan):4.2f} {w.get('w_min', np.nan):4.2f} "
                  + ' '.join(f"{v[0]:5.1f}/{v[1]:4.1f}" for v in (cur, P, L))
                  + f" {d:+6.1f} {predL:5.2f} {L[0] / cur[0]:5.2f} {predP:5.2f} {P[0] / cur[0]:5.2f}  {'CTL' if ctl else ''}")
            (ctl_d if ctl else diff_d).append(d)
            allrows.append(dict(clip=clip, a=a, state=s, ctl=ctl, d=d, cur=cur[0], P=P[0], L=L[0], predL=predL, predP=predP))
        print('control (non-burst) rows:')
        for e in controls:
            print(f"  window {e['window']:7.2f}  " + '  '.join(f"{t}: {e[t][0]:4.1f}/{e[t][1]:4.1f}" if e[t] else f'{t}: n/a' for t in TAGS)
                  + f"   L-P {e['pv1local'][0] - e['pv1prod'][0]:+5.1f}" if all(e[t] for t in TAGS) else '')

    print('\n=== summary (1-8 Hz, delta = pv1local - pv1prod, deg/s)')
    c = np.array(ctl_d); x = np.array(diff_d)
    print(f'identical-correction bursts: n {len(c)}  deltas ' + ' '.join(f'{v:+.1f}' for v in c)
          + f'\n  median |delta| {np.median(np.abs(c)):.2f}  rms {np.sqrt((c ** 2).mean()):.2f}  max |delta| {np.abs(c).max():.1f}')
    print(f'bursts where the variants differ: n {len(x)}  local better {np.sum(x <= -SAME)} / same {np.sum(np.abs(x) < SAME)}'
          f' / worse {np.sum(x >= SAME)}  (|delta| < {SAME})  median delta {np.median(x):+.2f}  mean {x.mean():+.2f}')
    for grp in ['refined', 'admitted', 'capped-rejected', 'floor-skipped']:
        y = np.array([r['d'] for r in allrows if not r['ctl'] and r['state'] == grp])
        if len(y):
            print(f'  {grp:16s} n {len(y):2d}  better {np.sum(y <= -SAME)} same {np.sum(np.abs(y) < SAME)} worse {np.sum(y >= SAME)}'
                  f'  median {np.median(y):+.2f}  sum {y.sum():+.1f}')
    z = np.array([r['P'] - r['cur'] for r in allrows if r['state'] not in ('refined', 'admitted')])
    print(f'second noise estimate, pv1prod - cur on bursts production leaves unrefined (zero correction in both, '
          f'separate renders): n {len(z)}  median |delta| {np.median(np.abs(z)):.2f}  rms {np.sqrt((z ** 2).mean()):.2f}'
          f'  max |delta| {np.abs(z).max():.1f}')
    for nm, k, pk in [('pv1local', 'L', 'predL'), ('pv1prod', 'P', 'predP')]:
        q = [(r[pk], r[k] / r['cur']) for r in allrows if np.isfinite(r[pk])]
        p, a = np.array(q).T
        print(f'step-1 proxy vs actual {nm}/cur: n {len(p)}  corr {np.corrcoef(p, a)[0, 1]:.2f}  median pred {np.median(p):.2f}'
              f'  median actual {np.median(a):.2f}  same side of 1 (+-0.1 neutral band) {np.mean(np.sign(np.round((p - 1) / 0.1).clip(-1, 1)) == np.sign(np.round((a - 1) / 0.1).clip(-1, 1))):.2f}')
    sums = {k: sum(r[k] for r in allrows) for k in ('cur', 'P', 'L')}
    print(f"sum over all scored bursts of 1-8 Hz: cur {sums['cur']:.1f}  pv1prod {sums['P']:.1f}  pv1local {sums['L']:.1f}")


if __name__ == '__main__':
    main()
