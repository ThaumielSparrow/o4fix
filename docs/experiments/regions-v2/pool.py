"""regions-v2: pool the all-burst (--no-refine) labels and check the refinement-outcome proxy.
proxy = predicted rms(render - probe)/rms(render) if the probe correction is subtracted,
from the probe/render 1-8 Hz magnitudes and their correlation (label)."""
import json, re, numpy as np
R = 'target/experiments/regions-v2/'
LOGS = {'0076nr': R + 'cur_0076_run.log', '0058nr': R + 'cur_0058_run.log', '0057nr': R + 'cur_0057_run.log',
        '0059nr': R + 'cur_0059_run.log', '0027nr': 'target/experiments/judder83/veto_0027_run.log',
        '0071nr': 'target/experiments/judder83/veto_0071_run.log', '0073nr': 'target/experiments/judder83/veto_0073_run.log'}


def ratio(r):
    p = r['probe'] / r['render']; return np.sqrt(max(1 + p * p - 2 * r['label'] * p, 0))


print('proxy vs actual forced-refinement render (1-8 Hz, forced/cur):')
for c in ['0076', '0058', '0005']:
    lab = {r['a']: r for r in json.load(open(R + c + '/labels.json'))}
    for s in json.load(open(R + c + f'/score-cur-all{"-gate" if c != "0058" else ""}.json')):
        if s['segment'] != 'control':
            r = lab[float(s['segment'].split('-')[0])]
            print(f"  {c} {s['segment']:14s} agreement {r['pred_free']:5.2f} proxy {ratio(r):.2f} actual {s['all'][0] / s['cur'][0]:.2f}")
rows = []
for c, lg in LOGS.items():
    st = {float(m[1]): ('refined' if m[4] else 'capped' if 'over cap' in m[5] else 'other')
          for m in re.finditer(r'\[\s*([\d.]+),\s*([\d.]+)\]\s+(residual correction ([\d.]+) deg|not refined: (.*))', open(lg).read())}
    rows += [dict(r, clip=c, ratio=ratio(r), state=st.get(r['a'], 'other')) for r in json.load(open(R + c + '/labels.json')) if 'label' in r]
P = np.array([r['pred_free'] for r in rows]); Q = np.array([r['ratio'] for r in rows]); S = np.array([r['state'] for r in rows])
print(f'\nall unseen bursts n={len(rows)}')
for name, sel in [('admitted (>=0.75)', P >= 0.75), ('refined today, agreement <0.3', (S == 'refined') & (P < 0.3)),
                  ('refined today, 0.3-0.5', (S == 'refined') & (P >= 0.3) & (P < 0.5)),
                  ('refined today, 0.5-0.75', (S == 'refined') & (P >= 0.5) & (P < 0.75))]:
    q = Q[sel]; print(f'  {name:32s} n{sel.sum():3d} median proxy {np.median(q):.2f}  helps(<0.9) {np.sum(q < 0.9)}  neutral {np.sum((q >= 0.9) & (q <= 1.1))}  hurts(>1.1) {np.sum(q > 1.1)}')
