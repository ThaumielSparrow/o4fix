#!/usr/bin/env python3
"""Generate JSON unit fixtures for the Rust port from numpy/scipy/o4fix.

Writes rust/o4core/tests/fixtures/*.json. Values come from the exact
reference code the port must match. Commit the outputs.
"""
import json, sys
from pathlib import Path
import numpy as np
from scipy.ndimage import median_filter, uniform_filter1d
from scipy.signal import butter, filtfilt, lfilter, lfilter_zi

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
import o4fix

OUT = ROOT / "rust/o4core/tests/fixtures"

def j(x):
    return np.asarray(x, dtype=np.float64).tolist()

def signal(n, seed):
    rng = np.random.default_rng(seed)
    t = np.arange(n) / 1000.0
    return (np.sin(2 * np.pi * 3 * t) + 0.5 * np.sin(2 * np.pi * 40 * t + 1.0)
            + 0.2 * rng.standard_normal(n))

def main():
    OUT.mkdir(parents=True, exist_ok=True)

    # ---------------- quat_ops.json (via o4fix itself)
    rng = np.random.default_rng(7)
    q = rng.standard_normal((32, 4))
    q /= np.linalg.norm(q, axis=1, keepdims=True)
    qa, qb = q[:16].copy(), q[16:].copy()
    v = 0.5 * rng.standard_normal((16, 3))
    v[0] = 0.0                              # exact small-angle limit
    v[1] = [1e-13, 0.0, 0.0]                # near-limit branch
    w = np.linspace(0.0, 1.0, 16)
    t = np.sort(rng.uniform(0.0, 1.0, 33))
    qs = o4fix.quat_exp(np.cumsum(0.01 * rng.standard_normal((33, 3)), axis=0))
    tm, om = o4fix.quats_to_rates(t, qs)
    ss_x = np.linspace(-0.5, 1.5, 21)
    json.dump({
        "qa": j(qa), "qb": j(qb),
        "mul": j(o4fix.quat_mul(qa, qb)),
        "conj": j(o4fix.quat_conj(qa)),
        "v": j(v), "exp": j(o4fix.quat_exp(v)),
        "log": j(o4fix.quat_log(qa)),
        "w": j(w), "slerp": j(o4fix.slerp(qa, qb, w)),
        "smoothstep_x": j(ss_x), "smoothstep": j(o4fix.smoothstep(ss_x)),
        "rates_t": j(t), "rates_q": j(qs), "rates_tm": j(tm), "rates_om": j(om),
    }, open(OUT / "quat_ops.json", "w"))

    # ---------------- filters.json (butter/lfilter_zi/lfilter/filtfilt)
    x = signal(256, 1)
    cases = []
    for kind, arg in [("low", [0.05]), ("low", [0.005]), ("low", [0.016]),
                      ("low", [0.032]), ("low", [0.1]),
                      ("band", [0.06, 0.36])]:
        b, a = butter(2, arg[0] if kind == "low" else arg,
                      "low" if kind == "low" else "band")
        zi = lfilter_zi(b, a)
        y, zf = lfilter(b, a, x, zi=zi * x[0])
        cases.append({"kind": kind, "arg": arg, "b": j(b), "a": j(a),
                      "zi": j(zi), "lfilter_y": j(y), "lfilter_zf": j(zf),
                      "filtfilt": j(filtfilt(b, a, x))})
    json.dump({"x": j(x), "cases": cases}, open(OUT / "filters.json", "w"))

    # ---------------- kernels.json
    x64 = signal(64, 2)
    x300 = signal(300, 3)
    uni = {str(s): j(uniform_filter1d(x300, size=s, mode="nearest"))
           for s in (3, 4, 10, 15, 100, 200)}
    med = {"15": j(median_filter(x300, size=15, mode="nearest"))}
    h_in = np.stack([signal(300, 5), signal(300, 6), signal(300, 8)], axis=1)
    h_in[50, 0] += 30.0
    h_in[200, 2] -= 25.0                    # injected spikes
    h_out, h_bad = o4fix.hampel(h_in, 7, 6.0)
    xq = np.array([-0.5, 0.0, 0.01, 0.5, 0.999, 1.0, 1.5])
    xp = np.linspace(0.0, 1.0, 32)
    fp = signal(32, 9)
    srt = np.sort(signal(32, 10))
    queries = [srt[0] - 1, srt[0], srt[10], (srt[10] + srt[11]) / 2,
               srt[-1], srt[-1] + 1]
    json.dump({
        "x64": j(x64), "x300": j(x300),
        "uniform": uni, "median": med,
        "hampel_in": j(h_in), "hampel_out": j(h_out),
        "hampel_frac": float(h_bad.mean()),
        "interp_xq": j(xq), "interp_xp": j(xp), "interp_fp": j(fp),
        "interp_y": j(np.interp(xq, xp, fp)),
        "gradient_in": j(x64), "gradient_out": j(np.gradient(x64)),
        "sorted": j(srt), "queries": j(queries),
        "ss_left": [int(np.searchsorted(srt, v, "left")) for v in queries],
        "ss_right": [int(np.searchsorted(srt, v, "right")) for v in queries],
    }, open(OUT / "kernels.json", "w"))
    print("fixtures written to", OUT)

if __name__ == "__main__":
    main()
