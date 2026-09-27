"""regions-v2 remap: where does judder remain after the cap gate + agreement floor?

  remap.py run CLIP            final-build repair -> Gyroflow render -> whole-clip render measurement
  remap.py analyze CLIP...     rank 0.5 s bins by render 1-8 Hz residual, classify by production state
Outputs under target/experiments/regions-v2/<CLIP>p/ (tag 'prod')."""
import json, re, subprocess, sys
from pathlib import Path
import numpy as np
sys.path.insert(0, str(Path(__file__).resolve().parent))
import gatecheck as g

F = g.F; ROOT = g.ROOT


def d(clip):
    return g.d(clip + 'p')


def duration(clip):
    txt = open(d(clip) / 'prod_run.log').read()
    return float(re.search(r'quat samples @ 1000 Hz, ([\d.]+) s', txt)[1])


def repair(clip):
    out = d(clip) / 'prod.MP4'
    if not out.exists():
        with open(d(clip) / 'prod_run.log', 'w') as fh:
            F.run_low([str(ROOT / 'target/release/o4fix.exe'), str(ROOT / f'sample_vids/{g.SRC[clip]}.MP4'),
                            '-o', str(out)], stdout=fh, stderr=subprocess.STDOUT, env=g.ENV)


def measure(clip):
    render = d(clip) / 'prod-render.mp4'
    if not list(d(clip).glob('ren-prod-*.json')):
        g.probe_render(render, [(0.0, (F.frames_of(render) - 1) / 100, d(clip) / 'ren-prod-0.json')])


def run(clip):
    repair(clip); g.render(clip + 'p', 'prod'); measure(clip)
    print(clip, 'done')


def runall(clips):
    """Two lanes: one CPU-heavy job at a time (repair or measurement, each already capped at
    half the cores) and one Gyroflow render (GPU) alongside it, so clip N renders while clip
    N+1 repairs or clip N-1 is measured. Never more than that, so the machine stays usable."""
    import threading
    cpu, gpu = threading.Semaphore(1), threading.Semaphore(1)
    def lane(clip):
        try:
            with cpu: repair(clip)
            with gpu: g.render(clip + 'p', 'prod')
            with cpu: measure(clip)
            print('OK', clip, flush=True)
        except Exception as e:
            print('FAIL', clip, e, flush=True)
    ts = []
    for c in clips:               # start in order; each thread blocks on its lane
        t = threading.Thread(target=lane, args=(c,)); t.start(); ts.append(t)
        import time; time.sleep(0.5)
    for t in ts: t.join()


def states(clip):
    """Production state per severe burst from the repair log."""
    txt = open(d(clip) / 'prod_run.log').read()
    bursts = {float(m[1]): float(m[2]) for m in re.finditer(r'\[\s*([\d.]+),\s*([\d.]+)\]\s+optical drift over burst', txt)}
    st = {a: 'unmeasured' for a in bursts}
    for m in re.finditer(r'\[\s*([\d.]+),\s*([\d.]+)\]\s+(residual correction [\d.]+ deg( \(over cap)?|not refined: (.*))', txt):
        a = float(m[1])
        if m[4]: st[a] = 'refined past cap'
        elif m[5] is None: st[a] = 'refined'
        elif 'over cap' in m[5]: st[a] = 'over cap, rejected'
        elif 'disagree' in m[5]: st[a] = 'floor-skipped'
        else: st[a] = 'not refined (edge)'
    return [(a, bursts[a], st[a]) for a in sorted(bursts)]


def load(clip):
    cam = F.camera(d(clip) / 'prod-camera.json'); B = np.load(F.FB / 'intended_B.npy')
    T, X, N = [], [], []
    for p in sorted(d(clip).glob('ren-prod-*.json'), key=lambda p: float(p.stem.split('-')[-1])):
        t, r, n = F.load(p)
        res = r[:, :3] - F.design(t, cam) @ B
        bad = ~np.isfinite(res).all(1)
        if bad.all(): continue
        for k in range(3): res[bad, k] = np.interp(t[bad], t[~bad], res[~bad, k])
        e = F.hp3(res @ F.MINV.T, 1.0)
        T.append(t); X.append(np.degrees(np.stack([e - F.hp3(e, 8.0), F.hp3(e, 8.0)], 0))); N.append(n)
    return np.concatenate(T), np.concatenate(X, axis=1), np.concatenate(N)


def analyze(clips):
    rows = []
    for clip in clips:
        t, X, n = load(clip); sts = states(clip)
        for b0 in np.arange(np.floor(t[0]), t[-1] - 0.5, 0.5):
            m = (t >= b0) & (t < b0 + 0.5)
            if m.sum() < 30: continue
            lo = np.sqrt((X[0][m] ** 2).sum(1).mean()); hi = np.sqrt((X[1][m] ** 2).sum(1).mean())
            inl = float(np.median(n[m]))
            where = 'outside bursts'
            for a, b, s in sts:
                if b0 + 0.5 > a - 0.2 and b0 < b + 0.2:
                    where = s; break
            rows.append(dict(clip=clip, t=float(b0), lo=float(lo), hi=float(hi), inliers=inl, state=where))
    json.dump(rows, open(g.RV2 / 'remap.json', 'w'), indent=0)
    R = rows
    lo = np.array([r['lo'] for r in R]); S = np.array([r['state'] for r in R]); inl = np.array([r['inliers'] for r in R])
    print(f'{len(R)} half-second bins over {len(clips)} clips; low-inlier bins (<150) are tracker-limited\n')
    print(f'{"state":22s} {"bins":>5s} {"median":>7s} {"p90":>6s} {">20":>5s} {">40":>5s}   (render 1-8 Hz deg/s)')
    for s in ['outside bursts', 'refined', 'refined past cap', 'floor-skipped', 'over cap, rejected', 'not refined (edge)', 'unmeasured']:
        m = S == s
        if m.any():
            print(f'{s:22s} {m.sum():5d} {np.median(lo[m]):7.1f} {np.percentile(lo[m], 90):6.1f} {np.sum(lo[m] > 20):5d} {np.sum(lo[m] > 40):5d}')
    print('\nworst 40 bins:')
    for r in sorted(R, key=lambda r: -r['lo'])[:40]:
        mm, ss = divmod(r['t'], 60)
        print(f"  {r['clip']} {int(mm)}:{ss:04.1f}  1-8 {r['lo']:6.1f}  8-50 {r['hi']:6.1f}  inliers {r['inliers']:5.0f}  {r['state']}")


if __name__ == '__main__':
    if sys.argv[1] == 'run':
        run(sys.argv[2])
    elif sys.argv[1] == 'runall':
        runall(sys.argv[2:])
    else:
        analyze(sys.argv[2:])
