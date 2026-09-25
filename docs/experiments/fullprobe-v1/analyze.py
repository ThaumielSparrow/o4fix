"""fullprobe-v1: where does measurable telemetry residual remain in a production output?

  analyze.py CLIP_SRC.MP4 PROBE_GLOB RUN_LOG [--noise-cache X.npz]

Joins the whole-clip Gyroflow-free probe (warp_residual_probe chunks on the
production _fixed.MP4) with the source clip's 30-180 Hz gyro-noise envelope and
the production burst list (from the repair log), and reports high-passed (1 Hz)
residual RMS by region and band. If the residual outside bursts grows with the
source gyro noise level, it is telemetry error (the tracker does not see gyro
noise); if it is flat, it is the tracker floor.
"""
import glob, json, re, sys
from pathlib import Path
import numpy as np
from scipy.signal import butter, filtfilt
from scipy.ndimage import uniform_filter1d

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / 'python'))


def noise_env(src, cache):
    if cache and Path(cache).exists():
        d = np.load(cache)
        return d['t'], d['noise']
    import o4fix
    t, q, _ = o4fix.extract_quats(src)
    tm, om = o4fix.quats_to_rates(t, q)
    fs = 1.0 / np.median(np.diff(tm))
    b, a = butter(2, [30 / (fs / 2), 180 / (fs / 2)], 'band')
    hf = filtfilt(b, a, np.degrees(om), axis=0)
    noise = np.sqrt(uniform_filter1d(hf ** 2, size=int(0.05 * fs), axis=0)).max(1)
    if cache:
        np.savez(cache, t=tm, noise=noise)
    return tm, noise


def probe(pattern):
    t, r, n = [], [], []
    for f in sorted(glob.glob(pattern)):
        for p in json.load(open(f))['pairs']:
            t.append(p['t']); n.append(p['n'])
            r.append(p['rate'] if p['rate'] else [np.nan] * 3)
    t, r, n = np.array(t), np.array(r, float), np.array(n)
    o = np.argsort(t); t, r, n = t[o], r[o], n[o]
    keep = np.r_[True, np.diff(t) > 1e-4]
    return t[keep], r[keep], n[keep]


def bursts(log):
    out = []
    for m in re.finditer(r'\[\s*([\d.]+),\s*([\d.]+)\]\s+(residual correction ([\d.]+) deg|not refined: (.*))', open(log).read()):
        out.append((float(m[1]), float(m[2]), m[4] is not None))
    return out


def band(x, lo, hi, fs=100.0):
    b, a = butter(2, [lo / (fs / 2), hi / (fs / 2)], 'band') if hi else butter(2, lo / (fs / 2), 'high')
    return filtfilt(b, a, x, axis=0)


def main():
    src, pat, log = sys.argv[1:4]
    cache = sys.argv[sys.argv.index('--noise-cache') + 1] if '--noise-cache' in sys.argv else None
    tn, noise = noise_env(src, cache)
    t, r, n = probe(pat)
    bad = ~np.isfinite(r).all(1)
    for k in range(3):
        r[bad, k] = np.interp(t[bad], t[~bad], r[~bad, k])
    r = np.degrees(r)
    lo, hi = band(r, 1, 8), band(r, 8, 45)
    nz = np.interp(t, tn, uniform_filter1d(noise, 10))
    B = bursts(log)
    inb = np.zeros(len(t), bool); near = np.zeros(len(t), bool); refd = np.zeros(len(t), bool)
    for a, b, applied in B:
        m = (t >= a) & (t <= b); inb |= m; refd |= m & applied
        near |= (t >= a - 1) & (t <= b + 1)
    ok = (n >= 150) & ~bad
    rms = lambda x, m: np.sqrt((x[m] ** 2).sum(1).mean()) if m.sum() > 20 else np.nan
    print(f'pairs {len(t)}  bad {bad.sum()}  t {t[0]:.1f}-{t[-1]:.1f}')
    print(f'{"region":34s} {"n":>6s} {"1-8Hz":>7s} {"8-45Hz":>7s}   (deg/s, |vec| RMS, HP residual)')
    rows = [('refined bursts', refd & ok), ('unrefined bursts', inb & ~refd & ok),
            ('near bursts (<=1s, outside)', near & ~inb & ok)]
    far = ~near & ok
    for a_, b_ in [(0, 2), (2, 3), (3, 4), (4, 6), (6, 8), (8, 1e9)]:
        rows.append((f'far, src noise {a_}-{b_} deg/s', far & (nz >= a_) & (nz < b_)))
    for name, m in rows:
        print(f'{name:34s} {m.sum():6d} {rms(lo, m):7.2f} {rms(hi, m):7.2f}')
    # per-second worst far-from-burst spots
    sec = np.floor(t).astype(int)
    worst = []
    for s in np.unique(sec[far]):
        m = far & (sec == s)
        if m.sum() > 50:
            worst.append((rms(lo, m), rms(hi, m), s, nz[m].mean()))
    worst.sort(reverse=True)
    print('worst far seconds (1-8, 8-45, t, src noise):')
    for w in worst[:15]:
        print(f'  {w[2]:4d}s  {w[0]:6.2f} {w[1]:6.2f}  noise {w[3]:5.1f}')
    np.savez(Path(pat).parent / 'joined.npz', t=t, r=r, n=n, nz=nz, inb=inb, refd=refd, near=near)


if __name__ == '__main__':
    main()
