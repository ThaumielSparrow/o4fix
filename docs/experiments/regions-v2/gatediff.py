"""Compare two repaired MP4s in the body frame: per-sample body rates (deg/s) from their
quaternions, reported per region. Used to check that the cap gate changes only the bursts
it admits (outside them the gate-on and gate-off outputs must have identical body rates;
the refined bursts' world-frame carry leaves a constant offset, which rates ignore).

  gatediff.py A.MP4 B.MP4 RUN_LOG_OF_A"""
import re, subprocess, sys
from pathlib import Path
import numpy as np

ROOT = Path(__file__).resolve().parents[3]
EXE = ROOT / 'target/release/examples/dump_quats.exe'


def quats(p):
    out = subprocess.run([str(EXE), str(p)], capture_output=True, text=True, check=True).stdout
    a = np.array([[float(x) for x in l.split(',')] for l in out.splitlines()[1:] if l])
    t, keep = np.unique(a[:, 0], return_index=True)     # 2 kHz slots -> 1 kHz samples
    return t / 1000, a[keep, 1:]


def rates(q):
    q = q.copy()
    for i in range(1, len(q)):
        if (q[i] * q[i - 1]).sum() < 0: q[i] = -q[i]
    w1, x1, y1, z1 = (q[:-1] * [1, -1, -1, -1]).T; w2, x2, y2, z2 = q[1:].T
    v = np.stack([w1 * x2 + x1 * w2 + y1 * z2 - z1 * y2, w1 * y2 - x1 * z2 + y1 * w2 + z1 * x2,
                  w1 * z2 + x1 * y2 - y1 * x2 + z1 * w2], 1)
    return np.degrees(2 * v) * 1000


ta, qa = quats(sys.argv[1]); tb, qb = quats(sys.argv[2])
assert np.array_equal(ta, tb)
d = np.linalg.norm(rates(qa) - rates(qb), axis=1); tm = ta[1:]
adm = [(float(m[1]), float(m[2])) for m in re.finditer(
    r'\[\s*([\d.]+),\s*([\d.]+)\]\s+residual correction [\d.]+ deg \(over cap, admitted', open(sys.argv[3]).read())]
inside = np.zeros(len(tm), bool)
for a, b in adm:
    inside |= (tm > a - 1.5) & (tm < b + 1.5)          # window margin + gate pad/fade
print('admitted bursts', adm)
print(f'outside admitted windows: max body-rate diff {d[~inside].max():.2e} deg/s over {np.sum(~inside)} samples')
for a, b in adm:
    m = (tm > a) & (tm < b)
    print(f'  {a:7.2f}-{b:7.2f}: rms rate change {np.sqrt((d[m] ** 2).mean()):.2f} deg/s')
