"""regions-v2: out-of-sample check of the regions-v1 refinement cap gate
(pooled 1-8 Hz corr(probe, essential-matrix estimate) >= 0.75) on clips it was not fit on.

  gatecheck.py probe CLIP          warp probe (O4_REGIONS) on every capped burst window of cur_CLIP.MP4
  gatecheck.py render CLIP TAG     project + Gyroflow render + camera export of CLIP/TAG.MP4
  gatecheck.py measure CLIP TAG    render_residual_probe on the burst windows of TAG's render
  gatecheck.py label CLIP          label (probe~render on cur render) vs predictor (probe~ess)
  gatecheck.py adjust CLIP TAG all|gate   probe-derived correction on all capped / admitted bursts
  gatecheck.py repair CLIP TAG     feedback_repair cur + TAG-adjust.json -> CLIP/TAG.MP4
  gatecheck.py score CLIP TAG...   render 1-8 / 8-50 Hz per capped burst + control
Outputs under target/experiments/regions-v2/CLIP/."""
import json, os, re, subprocess, sys
from pathlib import Path
import numpy as np
from scipy.signal import butter, filtfilt
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'feedback-v1'))
import fbclip as F

ROOT = F.ROOT; RV2 = F.E / 'regions-v2'
SRC = {'0076': 'DJI_20260919130352_0076_D', '0057': 'DJI_20260808145055_0057_D',
       '0058': 'DJI_20260808145758_0058_D', '0059': 'DJI_20260808151054_0059_D',
       '0027': 'DJI_20260721141550_0027_D', '0071': 'DJI_20260829140442_0071_D', '0073': 'DJI_20260829141435_0073_D',
       '0005': 'DJI_20260613164205_0005_D'}
# CLIP ids ending in 'nr' = the same source repaired with --no-refine; every severe burst is then
# unrefined, so the probe~render label and the gate predictor are defined on all bursts.
SRC.update({k + 'nr': v for k, v in list(SRC.items())})
TEMPLATE = F.E / 'judder83/veto.gyroflow'
THRESH = 0.75
bb = butter(2, [1 / 50, 8 / 50], 'band')
ENV = dict(os.environ, PATH=os.environ['PATH'] + r';C:\opencv\build\x64\vc16\bin;C:\Program Files\LLVM\bin')


def d(clip):
    p = RV2 / clip; p.mkdir(parents=True, exist_ok=True); return p


def capped(clip):
    txt = open(RV2 / f'cur_{clip}_run.log').read()
    if clip.endswith('nr'):
        return [(float(m[1]), float(m[2]), float('nan')) for m in
                re.finditer(r'\[\s*([\d.]+),\s*([\d.]+)\]\s+optical drift over burst', txt)]
    return [(float(m[1]), float(m[2]), float(m[3])) for m in
            re.finditer(r'\[\s*([\d.]+),\s*([\d.]+)\]\s+not refined: correction ([\d.]+) deg over cap', txt)]


def windows(clip):
    dur = float(re.search(r'([\d.]+) s\n', open(RV2 / f'cur_{clip}_run.log').read())[1])
    ws = []
    for a, b, _ in capped(clip):
        s, e = max(0.05, a - 2), min(dur - 0.1, b + 2)
        if ws and s <= ws[-1][1]:
            ws[-1][1] = max(ws[-1][1], e)
        else:
            ws.append([s, e])
    return [(round(s, 2), round(e - s, 2)) for s, e in ws]


def probe(clip):
    for s, du in windows(clip):
        out = d(clip) / f'src-{s}.json'
        if not out.exists():
            subprocess.run([str(ROOT / 'target/release/examples/warp_residual_probe.exe'), str(RV2 / f'cur_{clip}.MP4'),
                            str(s), str(du), '5.092569', '0', '3', str(out)], check=True, cwd=ROOT,
                           env=dict(ENV, O4_REGIONS='1', O4_CROP='1.30,0.72'), stdout=subprocess.DEVNULL)


def mp4_of(clip, tag):
    return RV2 / f'cur_{clip}.MP4' if tag == 'cur' else d(clip) / f'{tag}.MP4'


def render(clip, tag):
    proj = d(clip) / f'{tag}.gyroflow'
    if not proj.exists():
        subprocess.run([sys.executable, str(ROOT / 'python/analysis/make_project.py'), str(TEMPLATE), str(mp4_of(clip, tag)),
                        str(proj), f'{tag}-render.mp4', str(d(clip))], check=True)
    F.render_and_export(proj, d(clip) / f'{tag}-render.mp4', d(clip) / f'{tag}-camera.json')


def measure(clip, tag):
    for s, du in windows(clip):
        out = d(clip) / f'ren-{tag}-{s}.json'
        if not out.exists():
            subprocess.run([str(ROOT / 'target/release/examples/render_residual_probe.exe'), str(d(clip) / f'{tag}-render.mp4'),
                            str(s), str(du), str(out)], check=True, env=ENV, stdout=subprocess.DEVNULL)


def ser(rows, key):
    t = np.array([p['t'] for p in rows]); r = np.full((len(t), 3), np.nan)
    for i, p in enumerate(rows):
        v = p.get('rate') if key == 'all' else (p.get(key) or {}).get('rate')
        if v: r[i] = v
    ok = np.isfinite(r).all(1)
    for k in range(3): r[~ok, k] = np.interp(t[~ok], t[ok], r[ok, k])
    return t, np.degrees(filtfilt(*bb, r, axis=0))


def ren_residual(clip, tag, s):
    """Render residual in body deg/s (intended path removed), unfiltered; t, res, n."""
    cam = F.camera(d(clip) / f'{tag}-camera.json'); B = np.load(F.FB / 'intended_B.npy')
    t, r, n = F.load(d(clip) / f'ren-{tag}-{s}.json')
    res = r[:, :3] - F.design(t, cam) @ B
    bad = ~np.isfinite(res).all(1)
    for k in range(3): res[bad, k] = np.interp(t[bad], t[~bad], res[~bad, k])
    return t, np.degrees(res @ F.MINV.T), n


pc = lambda a, b: float((a * b).sum() / np.sqrt((a * a).sum() * (b * b).sum()))


def label(clip):
    rows = []
    for (a, b, cap) in capped(clip):
        w = next(((s, du) for s, du in windows(clip) if s <= a and b <= s + du + 1e-6), None)
        if w is None:   # burst runs into the last 0.1 s of the clip: no window covers it
            continue
        s, du = w
        src = json.load(open(d(clip) / f'src-{s}.json'))['pairs']
        ts, full = ser(src, 'all'); _, ess = ser(src, 'ess')
        tr, res, nr = ren_residual(clip, 'cur', s)
        rr = filtfilt(*bb, res, axis=0)
        m = (tr > a) & (tr < b) & (nr >= 60) & (tr > ts[0] + 0.5) & (tr < ts[-1] - 0.5)
        ms = (ts > a) & (ts < b) & (ts > ts[0] + 0.5) & (ts < ts[-1] - 0.5)   # render-free version
        row = dict(clip=clip, a=a, b=b, cap=cap, n=int(m.sum()), pred_free=pc(full[ms], ess[ms]))
        if m.sum() >= 20:
            P = np.stack([np.interp(tr[m], ts, full[:, k]) for k in range(3)], 1)
            S = np.stack([np.interp(tr[m], ts, ess[:, k]) for k in range(3)], 1)
            row.update(label=pc(P, rr[m]), pred=pc(P, S), probe=float(np.sqrt((P**2).sum(1).mean())),
                       render=float(np.sqrt((rr[m]**2).sum(1).mean())))
        rows.append(row)
        print(f"{clip} {a:7.2f}-{b:7.2f} cap {cap:5.2f}  pred {row.get('pred', np.nan):5.2f} (free {row['pred_free']:5.2f})"
              f"  label {row.get('label', np.nan):5.2f}  n {row['n']}  {'ADMIT' if row['pred_free'] >= THRESH else ''}")
    json.dump(rows, open(d(clip) / 'labels.json', 'w'), indent=1)


def adjust(clip, tag, which):
    rows = json.load(open(d(clip) / 'labels.json'))
    if which in ('refined', 'floor'):
        # bursts production refines today (under the cap, per the refined build's log);
        # 'floor' additionally skips those whose agreement is below FLOOR
        st = refined_today(clip)
        sel = [(r['a'], r['b']) for r in rows if r['a'] in st and (which == 'refined' or r['pred_free'] >= FLOOR)]
    else:
        sel = [(r['a'], r['b']) for r in rows if which == 'all' or r['pred_free'] >= THRESH]
    ts, angs, total = [], [], np.zeros(3)
    for s, du in windows(clip):
        src = json.load(open(d(clip) / f'src-{s}.json'))['pairs']
        t = np.array([p['t'] for p in src]); n = np.array([p['n'] for p in src])
        r = np.array([p['rate'] if p['rate'] else [np.nan] * 3 for p in src], float)
        bad = ~np.isfinite(r).all(1)
        for k in range(3): r[bad, k] = np.interp(t[bad], t[~bad], r[~bad, k])
        e = F.hp3(r, 1.0)
        conf = np.convolve(np.clip((n - 60) / 140, 0, 1), np.ones(9) / 9, 'same')
        g = F.gate(t, [(a, b) for a, b in sel if s < a and b < s + du])
        ang = total + np.cumsum(-(g * conf)[:, None] * e, 0) * 0.01
        total = ang[-1]; ts.append(t); angs.append(ang)
        print(clip, tag, 'window', s, 'max deg', np.degrees(np.abs(ang - ang[0]).max()).round(2))
    t = np.concatenate(ts); ang = np.concatenate(angs); assert np.all(np.diff(t) > 0)
    print('bursts', sel)
    with (d(clip) / f'{tag}-adjust.json').open('x') as fh:
        json.dump({'t': t.tolist(), 'angle': ang.tolist(), 'bursts': sel}, fh)


FLOOR = 0.3
REFINED_LOGS = {'0058nr': RV2 / 'cur_0058_run.log', '0057nr': RV2 / 'cur_0057_run.log',
                '0073nr': F.E / 'judder83/veto_0073_run.log'}


def refined_today(clip):
    txt = open(REFINED_LOGS[clip]).read()
    return {float(m[1]) for m in re.finditer(r'\[\s*([\d.]+),\s*([\d.]+)\]\s+residual correction', txt)}


def repair(clip, tag):
    subprocess.run([str(ROOT / 'target/release/examples/feedback_repair.exe'), str(ROOT / f'sample_vids/{SRC[clip]}.MP4'),
                    str(RV2 / f'cur_{clip}.MP4'), str(d(clip) / f'{tag}-adjust.json'), str(d(clip) / f'{tag}.MP4')],
                   check=True, env=ENV)


def score(clip, tags):
    rows = {r['a']: r for r in json.load(open(d(clip) / 'labels.json'))}
    out = []
    for s, du in windows(clip):
        caps = [(a, b) for a, b, _ in capped(clip) if s <= a and b <= s + du + 1e-6]
        vals = {}
        for tag in tags:
            t, res, n = ren_residual(clip, tag, s)
            e = F.hp3(np.radians(res), 1.0)
            lo = np.degrees(e - F.hp3(e, 8.0)); hi = np.degrees(F.hp3(e, 8.0))
            g = F.gate(t, [(a, b) for a, b, _ in capped(clip)])
            masks = [(f'{a:.2f}-{b:.2f}', (t >= a) & (t <= b)) for a, b in caps] + [('control', (g < 0.01) & (n >= 300))]
            vals[tag] = {k: (float(np.sqrt((lo[m]**2).sum(1).mean())), float(np.sqrt((hi[m]**2).sum(1).mean())), int(m.sum()))
                         if m.sum() else None for k, m in masks}
        for k in vals[tags[0]]:
            r = rows.get(float(k.split('-')[0])) if k != 'control' else None
            tagtxt = '' if r is None else f"pred {r['pred_free']:5.2f} {'ADMIT ' if r['pred_free'] >= THRESH else 'reject'}"
            print(f'{clip} {k:16s} {tagtxt:14s}', '  '.join(
                f"{tg}: {v[0]:6.1f}/{v[1]:5.1f}" if (v := vals[tg][k]) else f'{tg}: n/a' for tg in tags))
            out.append({'clip': clip, 'window': s, 'segment': k, **{tg: vals[tg][k] for tg in tags}})
    json.dump(out, open(d(clip) / f'score-{"-".join(tags)}.json', 'w'), indent=1)


if __name__ == '__main__':
    cmd, clip = sys.argv[1], sys.argv[2]
    {'windows': lambda: print(windows(clip), capped(clip)), 'probe': lambda: probe(clip), 'render': lambda: render(clip, sys.argv[3]), 'measure': lambda: measure(clip, sys.argv[3]),
     'label': lambda: label(clip), 'adjust': lambda: adjust(clip, sys.argv[3], sys.argv[4]),
     'repair': lambda: repair(clip, sys.argv[3]), 'score': lambda: score(clip, sys.argv[3:])}[cmd]()
