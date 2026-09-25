"""feedback-v1 helpers: perturbation spec, Gyroflow project creation, calibration and correction."""
import json, sys, math
from pathlib import Path
import numpy as np

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / 'target/experiments/feedback-v1'
BASE_PROJECT = ROOT / 'target/experiments/generalization-v1/0073/edgeoffset.gyroflow'
PERTURB = [(56.2, 58.6), (100.2, 102.6)]
FREQ = [3.0, 4.5, 6.0]
AMP = math.radians(0.4)


def smooth_env(t, a, b, fade=0.3):
    x = np.clip(np.minimum(t - a, b - t) / fade, 0, 1)
    return x * x * (3 - 2 * x)


def perturbation(t):
    env = sum(smooth_env(t, a, b) for a, b in PERTURB)
    return np.stack([env * AMP * np.sin(2 * math.pi * f * t) for f in FREQ], 1)


def write_perturb():
    t = np.arange(50.0, 110.0, 0.001)
    ang = perturbation(t)
    (OUT / 'perturb-adjust.json').write_text(json.dumps({'t': t.tolist(), 'angle': ang.tolist()}))


def project(name):
    p = json.loads(BASE_PROJECT.read_text())
    assert p['video_info']['num_frames'] == 37594
    assert p['stabilization']['horizon_lock_amount'] == 0
    assert p['offsets'] == {} and p['synchronization']['auto_sync_points'] is False
    p['videofile'] = (OUT / f'{name}.MP4').as_uri()
    p['gyro_source']['filepath'] = p['videofile']
    p['gyro_source'].pop('file_metadata', None)
    p['output']['output_filename'] = f'{name}-render.mp4'
    p['output']['output_folder'] = OUT.as_uri() + '/'
    with (OUT / f'{name}.gyroflow').open('x') as f:
        json.dump(p, f, indent=2)


READOUT_MID = 0.002546  # Gyroflow export: frame k timestamp = k*10 ms + half readout


def load(path):
    d = json.loads(Path(path).read_text())
    t = np.array([p['t'] for p in d['pairs']])
    r = np.array([p['rate'] if p['rate'] is not None else [np.nan] * 4 for p in d['pairs']], float)
    n = np.array([p['n'] for p in d['pairs']])
    return t, r, n


def pair_rate(fn, t, lag=0.0):
    """Body-rate implied by an angle function across each frame pair (finite difference)."""
    a = fn(t - 0.005 + READOUT_MID + lag)
    b = fn(t + 0.005 + READOUT_MID + lag)
    return (b - a) / 0.01


def calibrate():
    rows = []
    for w in ['cal1', 'cal2']:
        tb, rb, _ = load(OUT / f'base-{w}.json')
        tp, rp, _ = load(OUT / f'perturb-{w}.json')
        assert np.allclose(tb, tp)
        rows.append((tb, rp[:, :3] - rb[:, :3]))
    res = {}
    for lag in np.arange(-0.02, 0.0201, 0.002):
        X = np.concatenate([pair_rate(perturbation, t, lag) for t, _ in rows])
        Y = np.concatenate([y for _, y in rows])
        ok = np.isfinite(Y).all(1) & (np.abs(X).sum(1) > 0)
        M, *_ = np.linalg.lstsq(X[ok], Y[ok], rcond=None)
        r2 = 1 - ((Y[ok] - X[ok] @ M) ** 2).sum(0) / ((Y[ok] - Y[ok].mean(0)) ** 2).sum(0)
        res[round(lag, 3)] = (M.T, r2)
    best = max(res, key=lambda k: res[k][1].mean())
    M, r2 = res[best]
    print('lag', best, 'r2', r2)
    print('M (rows image dx,dy,roll; cols body x,y,z)\n', M)
    # per-window consistency
    for (t, y), w in zip(rows, ['cal1', 'cal2']):
        X = pair_rate(perturbation, t, best)
        ok = np.isfinite(y).all(1) & (np.abs(X).sum(1) > 0)
        Mw, *_ = np.linalg.lstsq(X[ok], y[ok], rcond=None)
        print(w, '\n', Mw.T)
    (OUT / 'calibration.json').write_text(json.dumps({'lag': best, 'M': M.tolist(), 'r2': r2.tolist()}))


if __name__ == '__main__':
    cmd = sys.argv[1]
    if cmd == 'perturb':
        write_perturb()
    elif cmd == "calibrate":
        calibrate()
    elif cmd == 'project':
        project(sys.argv[2])
