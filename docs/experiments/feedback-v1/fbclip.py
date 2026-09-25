"""feedback-v1 generalized per-clip driver (fixed window ordering). Run from anywhere.

  fbclip.py measure CLIP PREFIX RENDER         measure all windows of RENDER -> DIR/PREFIX-<win>.json
  fbclip.py intended CLIP                      fit intended-motion model on base measurements
  fbclip.py correct CLIP TAG BASE_PREFIX CAMERA  write DIR/TAG-adjust.json (gain 1)
  fbclip.py repair CLIP TAG BASE_MP4           feedback_repair + project + render + camera export
  fbclip.py score CLIP PREFIX:CAMERA ...       banded residual table
  fbclip.py reviews CLIP TAG                   side-by-side exports (LEFT base render, RIGHT TAG)

Uses the 0073 perturbation calibration (same camera/lens/output settings) for M and lag."""
import json, math, subprocess, sys
from pathlib import Path
import numpy as np

ROOT = Path(__file__).resolve().parents[3]
E = ROOT / 'target/experiments'
FB = E / 'feedback-v1'
GF = ROOT / 'tools/gyroflow-portable/Gyroflow.exe'
FF, FP = 'C:/ffmpeg/bin/ffmpeg.exe', 'C:/ffmpeg/bin/ffprobe.exe'
READOUT_MID = 0.002546


def bursts_from(path, key):
    return [tuple(b['interval']) for b in json.loads(Path(path).read_text())['bursts']]


CLIPS = {
    '0073': dict(dir=FB, src='sample_vids/DJI_20260829141435_0073_D.MP4',
                 base_mp4=E / 'generalization-v1/0073/edgeoffset.MP4',
                 base_render=E / 'generalization-v1/0073/edgeoffset-render.mp4',
                 base_project=E / 'generalization-v1/0073/edgeoffset.gyroflow',
                 base_camera=E / 'bounce-localize-v1/camera.json', frames=37594,
                 bursts=E / 'residual-stage-v1/0073/stages.json',
                 windows={'early': (12, 32), 'late': (130, 148)}, fit_extra=['cal1', 'cal2'],
                 reviews=[('early', 14, 16), ('late', 132, 14), ('control', 55.9, 3)]),
    '0060': dict(dir=FB / '0060', src='sample_vids/DJI_20260808151831_0060_D.MP4',
                 base_mp4=E / 'gyro-trace-v1/edgeoffset.MP4',
                 base_render=E / 'gyro-trace-v1/edgeoffset-render.mp4',
                 base_project=E / 'gyro-trace-v1/edgeoffset.gyroflow',
                 base_camera=FB / '0060/base-camera.json', frames=38269,
                 bursts=E / 'gyro-trace-v1/edgeoffset-metrics.json',
                 windows={'control': (18, 23), 'w106': (96, 112), 'w225': (219, 231),
                          'w249': (244, 254), 'w308': (304, 313)}, fit_extra=[],
                 reviews=[('106', 102, 8), ('225', 221, 8), ('249', 245, 8), ('308', 304, 8), ('control', 19, 3)]),
    '0071': dict(dir=FB / '0071', src='sample_vids/DJI_20260829140442_0071_D.MP4',
                 base_mp4=E / 'generalization-v1/0071/edgeoffset.MP4',
                 base_render=E / 'generalization-v1/0071/edgeoffset-render.mp4',
                 base_project=E / 'generalization-v1/0071/edgeoffset.gyroflow',
                 base_camera=FB / '0071/base-camera.json', frames=None,
                 bursts=E / 'residual-stage-v1/0071/stages.json',
                 windows={'throttle': (11, 22), 'control': (73, 78), 'late': (251, 261)}, fit_extra=[],
                 reviews=[('throttle', 11, 10), ('late', 252, 8), ('control', 74, 3)]),
}

CAL = json.loads((FB / 'calibration.json').read_text())
M = np.array(CAL['M']); MINV = np.linalg.inv(M); LAG = CAL['lag']


# ---------- math ----------
def smooth_env(t, a, b, fade):
    x = np.clip(np.minimum(t - a, b - t) / fade, 0, 1)
    return x * x * (3 - 2 * x)


def butter2_hp(fc, fs):
    k = math.tan(math.pi * fc / fs); q = math.sqrt(2); n = 1 / (1 + q * k + k * k)
    return np.array([1, -2, 1]) * n, np.array([1, 2 * (k * k - 1) * n, (1 - q * k + k * k) * n])


def lfilter(b, a, x):
    y = np.zeros_like(x)
    for i in range(len(x)):
        y[i] = b[0] * x[i] + (b[1] * x[i-1] if i > 0 else 0) + (b[2] * x[i-2] if i > 1 else 0) \
            - (a[1] * y[i-1] if i > 0 else 0) - (a[2] * y[i-2] if i > 1 else 0)
    return y


def filtfilt(b, a, x, pad=150):
    xp = np.concatenate([2 * x[0] - x[pad:0:-1], x, 2 * x[-1] - x[-2:-pad-2:-1]])
    y = lfilter(b, a, xp); y = lfilter(b, a, y[::-1])[::-1]
    return y[pad:-pad]


def hp3(x, fc):
    return np.stack([filtfilt(*butter2_hp(fc, 100.), x[:, k]) for k in range(3)], 1)


def qmul(a, b):
    w1, x1, y1, z1 = a.T; w2, x2, y2, z2 = b.T
    return np.stack([w1*w2-x1*x2-y1*y2-z1*z2, w1*x2+x1*w2+y1*z2-z1*y2,
                     w1*y2-x1*z2+y1*w2+z1*x2, w1*z2+x1*y2-y1*x2+z1*w2], 1)


def qrates(q):
    q = q.copy()
    for i in range(1, len(q)):
        if (q[i] * q[i-1]).sum() < 0: q[i] = -q[i]
    d = qmul(q[:-1] * np.array([1, -1, -1, -1]), q[1:]); d[d[:, 0] < 0] *= -1
    v = d[:, 1:]; n = np.linalg.norm(v, axis=1)
    s = np.where(n > 1e-12, 2 * np.arcsin(np.clip(n, 0, 1)) / np.maximum(n, 1e-12), 2)
    return v * s[:, None] * 100.0


def camera(path):
    d = json.loads(Path(path).read_text())
    return qrates(np.array([f['stab_quat'] for f in d])), np.array([f['fov_scale'] for f in d])


def load(path):
    d = json.loads(Path(path).read_text())
    t = np.array([p['t'] for p in d['pairs']])
    r = np.array([p['rate'] if p['rate'] is not None else [np.nan] * 4 for p in d['pairs']], float)
    return t, r, np.array([p['n'] for p in d['pairs']])


def design(t, cam):
    wS, fov = cam
    k = np.round((t - 0.005) * 100).astype(int)
    f = 0.5 * (fov[k] + fov[k + 1])
    return np.concatenate([wS[k] / f[:, None], wS[k]], 1)


def gate(t, bursts, pad=0.25, fade=0.15):
    w = np.zeros_like(t)
    for a, b in bursts:
        w = np.maximum(w, smooth_env(t, a - pad, b + pad, fade))
    return w


def residual(c, prefix, win, cam, B):
    t, r, n = load(c['dir'] / f'{prefix}-{win}.json')
    res = r[:, :3] - design(t, cam) @ B
    bad = ~np.isfinite(res).all(1)
    for k in range(3):
        res[bad, k] = np.interp(t[bad], t[~bad], res[~bad, k])
    return t, hp3(res @ MINV.T, 1.0), n, bad


# ---------- commands ----------
def measure(c, prefix, render):
    exe = ROOT / 'target/release/examples/render_residual_probe.exe'
    for w, (a, b) in c['windows'].items():
        out = c['dir'] / f'{prefix}-{w}.json'
        if not out.exists():
            subprocess.run([str(exe), str(render), str(a), str(b - a), str(out)], check=True)


def intended(c):
    cam = camera(c['base_camera']); bursts = bursts_from(c['bursts'], 'bursts')
    X, Y = [], []
    names = list(c['windows']) + c['fit_extra']
    for w in names:
        t, r, n = load(c['dir'] / f'base-{w}.json')
        ok = np.isfinite(r).all(1) & (n >= 80) & (gate(t, bursts) < 0.01)
        X.append(design(t, cam)[ok]); Y.append(r[ok, :3])
    X = np.concatenate(X); Y = np.concatenate(Y); wt = np.ones(len(X))
    for _ in range(10):
        B = np.linalg.lstsq(X * wt[:, None], Y * wt[:, None], rcond=None)[0]
        res = Y - X @ B; s = np.median(np.abs(res), 0) * 1.48
        z = np.abs(res / s).max(1); wt = np.sqrt(np.where(z < 2, 1, 2 / z))
    r2 = 1 - (res ** 2).sum(0) / ((Y - Y.mean(0)) ** 2).sum(0)
    print('intended-motion fit r2', r2.round(4), 'samples', len(X))
    np.save(c['dir'] / 'intended_B.npy', B)


def correct(c, tag, prefix, cam_path):
    cam = camera(cam_path); B = np.load(c['dir'] / 'intended_B.npy'); bursts = bursts_from(c['bursts'], 'bursts')
    ts, angs, total = [], [], np.zeros(3)
    for w, (a, b) in sorted(c['windows'].items(), key=lambda kv: kv[1][0]):   # time order
        t, e, n, bad = residual(c, prefix, w, cam, B)
        conf = np.clip((n - 60) / 140, 0, 1); conf[bad] = 0
        conf = np.convolve(conf, np.ones(9) / 9, 'same')
        inside = [(s, e_) for s, e_ in bursts if s - 1 > a and e_ + 1 < b]
        g = gate(t, inside)
        ang = total + np.cumsum(-(g * conf)[:, None] * e, 0) * 0.01
        total = ang[-1]
        ts.append(t + READOUT_MID + LAG); angs.append(ang)
        print(w, 'bursts', [(round(s, 2), round(e_, 2)) for s, e_ in inside],
              'max window deg', np.degrees(np.abs(ang - ang[0]).max()).round(3))
    t = np.concatenate(ts); ang = np.concatenate(angs)
    assert np.all(np.diff(t) > 0)
    # angle is constant between windows (no ramp) and zero before the first correction
    with (c['dir'] / f'{tag}-adjust.json').open('x') as fh:
        json.dump({'t': t.tolist(), 'angle': ang.tolist()}, fh)


def response(c, prev, win, seg, B, lag_scan=np.arange(-0.008, 0.0041, 0.001)):
    """Fit the render's response to the previously applied correction inside one burst:
    best timing shift and scalar gain (measured residual change / applied correction rate)."""
    adj = json.loads((c['dir'] / f'{prev}-adjust.json').read_text())
    ta, aa = np.array(adj['t']), np.array(adj['angle'])
    t, eb, _, _ = residual(c, 'base', win, camera(c['base_camera']), B)
    _, en, _, _ = residual(c, prev, win, camera(c['dir'] / f'{prev}-camera.json'), B)
    d = en - eb; m = (t >= seg[0]) & (t <= seg[1]); best = None
    for L in lag_scan:
        tt = t + READOUT_MID + LAG + L
        a0 = np.stack([np.interp(tt - 0.005, ta, aa[:, k]) for k in range(3)], 1)
        a1 = np.stack([np.interp(tt + 0.005, ta, aa[:, k]) for k in range(3)], 1)
        cr = hp3((a1 - a0) / 0.01, 1.0)
        g = (d[m] * cr[m]).sum() / (cr[m] ** 2).sum()       # observed change ~ g * applied rate
        r2 = 1 - ((d[m] - g * cr[m]) ** 2).sum() / (d[m] ** 2).sum()
        if best is None or r2 > best[1]:
            best = (L, r2, g)
    return best


def correct2(c, tag, prev):
    """Second feedback step on top of PREV: residual of PREV's render, divided by the
    per-burst response gain measured base->PREV, applied at the per-burst best timing."""
    B = np.load(c['dir'] / 'intended_B.npy'); bursts = bursts_from(c['bursts'], 'bursts')
    cam = camera(c['dir'] / f'{prev}-camera.json')
    ts, angs, total, report = [], [], np.zeros(3), []
    for w, (a, b) in sorted(c['windows'].items(), key=lambda kv: kv[1][0]):
        t, e, n, bad = residual(c, prev, w, cam, B)
        conf = np.clip((n - 60) / 140, 0, 1); conf[bad] = 0
        conf = np.convolve(conf, np.ones(9) / 9, 'same')
        inside = [(s, e_) for s, e_ in bursts if s - 1 > a and e_ + 1 < b]
        rate = np.zeros_like(e)
        for s, e_ in inside:
            L, r2, g = response(c, prev, w, (s, e_), B)
            g = float(np.clip(g, 0.8, 2.0)) if r2 > 0.3 else 1.0
            L = L if r2 > 0.3 else 0.0
            gi = gate(t, [(s, e_)])
            # shift this burst's correction by L: evaluate the measured residual at t - L
            es = np.stack([np.interp(t - L, t, e[:, k]) for k in range(3)], 1)
            rate += -(gi * conf)[:, None] * es / g
            report.append({'burst': [s, e_], 'lag_ms': L * 1000, 'r2': r2, 'gain': g})
        ang = total + np.cumsum(rate, 0) * 0.01
        total = ang[-1]
        ts.append(t + READOUT_MID + LAG); angs.append(ang)
    for r in report:
        print(f"  burst {r['burst'][0]:.2f}-{r['burst'][1]:.2f} lag {r['lag_ms']:+.0f} ms r2 {r['r2']:.2f} gain {r['gain']:.2f}")
    t = np.concatenate(ts); ang = np.concatenate(angs)
    assert np.all(np.diff(t) > 0)
    (c['dir'] / f'{tag}-response.json').write_text(json.dumps(report, indent=1))
    with (c['dir'] / f'{tag}-adjust.json').open('x') as fh:
        json.dump({'t': t.tolist(), 'angle': ang.tolist()}, fh)


def gyroflow(args, log):
    with open(str(log) + '.out', 'w') as o, open(str(log) + '.err', 'w') as e:
        subprocess.run([str(GF)] + args, stdout=o, stderr=e, check=False)


def frames_of(p):
    return int(subprocess.check_output([FP, '-v', 'error', '-select_streams', 'v:0', '-count_packets',
        '-show_entries', 'stream=nb_read_packets', '-of', 'csv=p=0', str(p)], text=True).strip())


def repair(c, tag, base_mp4):
    d = c['dir']
    subprocess.run([str(ROOT / 'target/release/examples/feedback_repair.exe'), str(ROOT / c['src']), str(base_mp4),
                    str(d / f'{tag}-adjust.json'), str(d / f'{tag}.MP4')], check=True)
    p = json.loads(Path(c['base_project']).read_text())
    if c['frames']:
        assert p['video_info']['num_frames'] == c['frames']
    assert p['stabilization']['horizon_lock_amount'] == 0 and p['offsets'] == {} and p['synchronization']['auto_sync_points'] is False
    p['videofile'] = (d / f'{tag}.MP4').as_uri(); p['gyro_source']['filepath'] = p['videofile']
    p['gyro_source'].pop('file_metadata', None)
    p['output']['output_filename'] = f'{tag}-render.mp4'; p['output']['output_folder'] = d.as_uri() + '/'
    proj = d / f'{tag}.gyroflow'
    with proj.open('x') as fh:
        json.dump(p, fh, indent=2)
    render_and_export(proj, d / f'{tag}-render.mp4', d / f'{tag}-camera.json')


def render_and_export(proj, render, cam):
    if not render.exists():
        gyroflow([str(proj), '-f', '--stdout-progress'], render.with_suffix('.log'))
    if not cam.exists() or cam.stat().st_size == 0:
        cam.touch()
        gyroflow([str(proj), '--export-metadata', f'3:{cam}'], cam.with_suffix('.log'))
    print(render.name, frames_of(render), 'frames; camera bytes', cam.stat().st_size)


def score(c, variants):
    bursts = bursts_from(c['bursts'], 'bursts'); B = np.load(c['dir'] / 'intended_B.npy')
    for w, (a, b) in c['windows'].items():
        segs = [(s, e_) for s, e_ in bursts if s - 1 > a and e_ + 1 < b]
        labels = [f'{s:.1f}-{e_:.1f}' for s, e_ in segs] + ['non-burst(n>=300)']
        rows = {}
        for p, cp in variants:
            t, e, n, _ = residual(c, p, w, camera(cp), B)
            e = np.degrees(e)
            lo = hp3(e, 1.0) - hp3(e, 8.0); hi = hp3(e, 8.0)
            masks = [(t >= s) & (t <= e_) for s, e_ in segs] + [(gate(t, bursts) < 0.01) & (n >= 300)]
            rows[p] = [(np.sqrt((lo[m] ** 2).sum(1).mean()), np.sqrt((hi[m] ** 2).sum(1).mean())) if m.any() else (np.nan, np.nan) for m in masks]
        print(f'== {w}  (|3-axis| body deg/s, 1-8 Hz / 8-50 Hz)')
        for i, lab in enumerate(labels):
            print(f'  {lab:18s}', '  '.join(f'{p}: {rows[p][i][0]:5.2f}/{rows[p][i][1]:5.2f}' for p, _ in variants))


def reviews(c, tag):
    d = c['dir']; left, right = c['base_render'], d / f'{tag}-render.mp4'
    rows = []
    for label, start, dur in c['reviews']:
        for suffix, panel in [('', 'scale=720:405,pad=720:406:0:0'), ('_full', 'null'), ('_detail', 'crop=720:405:720:0')]:
            dest = d / (f'compare_{label}{suffix}.mp4' if tag == 'fb1' else f'compare_{tag}_{label}{suffix}.mp4')
            if not dest.exists():
                subprocess.run([FF, '-v', 'error', '-n', '-ss', str(start), '-i', str(left), '-ss', str(start), '-i', str(right),
                    '-t', str(dur), '-filter_complex', f'[0:v]{panel}[a];[1:v]{panel}[b];[a][b]hstack[v]',
                    '-map', '[v]', '-an', '-c:v', 'libx264', '-crf', '16', '-preset', 'fast', '-pix_fmt', 'yuv420p', str(dest)], check=True)
            n = frames_of(dest); assert n == round(dur * 100), (dest, n)
            rows.append({'video': dest.name, 'start': start, 'duration': dur, 'frames': n, 'left': str(left), 'right': str(right)})
    (d / f'review-validation-{tag}.json').write_text(json.dumps(rows, indent=2))
    print(len(rows), 'review videos verified')


def reviews_pair(c, left_tag, right_tag):
    """Side-by-side exports LEFT <left_tag>-render.mp4 vs RIGHT <right_tag>-render.mp4."""
    d = c['dir']; left, right = d / f'{left_tag}-render.mp4', d / f'{right_tag}-render.mp4'
    rows = []
    for label, start, dur in c['reviews']:
        for suffix, panel in [('', 'scale=720:405,pad=720:406:0:0'), ('_full', 'null'), ('_detail', 'crop=720:405:720:0')]:
            dest = d / f'compare_{left_tag}_vs_{right_tag}_{label}{suffix}.mp4'
            if not dest.exists():
                subprocess.run([FF, '-v', 'error', '-n', '-ss', str(start), '-i', str(left), '-ss', str(start), '-i', str(right),
                    '-t', str(dur), '-filter_complex', f'[0:v]{panel}[a];[1:v]{panel}[b];[a][b]hstack[v]',
                    '-map', '[v]', '-an', '-c:v', 'libx264', '-crf', '16', '-preset', 'fast', '-pix_fmt', 'yuv420p', str(dest)], check=True)
            n = frames_of(dest); assert n == round(dur * 100), (dest, n)
            rows.append({'video': dest.name, 'start': start, 'duration': dur, 'frames': n})
    (d / f'review-validation-{left_tag}-vs-{right_tag}.json').write_text(json.dumps(rows, indent=2))
    print(len(rows), 'review videos verified')


if __name__ == '__main__':
    cmd, clip = sys.argv[1], sys.argv[2]
    c = CLIPS[clip]; c['dir'].mkdir(parents=True, exist_ok=True)
    if cmd == 'measure': measure(c, sys.argv[3], sys.argv[4])
    elif cmd == 'basecam': render_and_export(Path(c['base_project']), Path(c['base_render']), Path(c['base_camera']))
    elif cmd == 'intended': intended(c)
    elif cmd == 'correct': correct(c, sys.argv[3], sys.argv[4], sys.argv[5])
    elif cmd == 'correct2': correct2(c, sys.argv[3], sys.argv[4])
    elif cmd == 'repair': repair(c, sys.argv[3], sys.argv[4])
    elif cmd == 'score': score(c, [a.split(':', 1) for a in sys.argv[3:]])
    elif cmd == 'reviews': reviews(c, sys.argv[3])
    elif cmd == 'reviews_pair': reviews_pair(c, sys.argv[3], sys.argv[4])
