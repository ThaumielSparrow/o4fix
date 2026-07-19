#!/usr/bin/env python3
"""Dump per-stage goldens of the Python pipeline (M2 defaults, seeded optical)
for the Rust port's stage tests. Writes goldens/*.npz + goldens/meta.json.

The ONLY deviation from o4fix defaults: cv2.setRNGSeed(1_000_000 + frame_idx)
before each frame pair's essential-matrix estimation, so RANSAC is
reproducible cross-language. o4fix.py itself is never modified; video_rates
is monkeypatched with a seeded copy of its loop.

Usage: python tools/dump_goldens.py            (~10-20 min: optical runs twice)
       python tools/dump_goldens.py --m4-only   (~8-12 min: optical runs once;
       only (re)generates goldens/ref_fixed_m4.MP4, skips the M2 stage dumps)
"""
import argparse, json, sys
from pathlib import Path
import cv2
import numpy as np

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
import mp4patch
import o4fix

SEED_BASE = 1_000_000
VIDEO = ROOT / "sample_vids/DJI_20260711124046_0021_D.MP4"
GOLD = ROOT / "goldens"


def build_args(fast_wide_cutoff=0.0):
    return argparse.Namespace(
        output=None, gcsv=False, severe=8.0, severe_pad=0.2, severe_merge=0.2,
        ramp=0.3, plot=False, orientation="XYZ", light_cutoff=25.0,
        strong_cutoff=2.5, noise_low=1.5, noise_high=5.0,
        noise_band=[30.0, 180.0], noise_window=100.0, hampel_window=7,
        hampel_sigma=6.0, lpf=None, no_optical=False, optical_cutoff=8.0,
        fast_handback=[100.0, 250.0], patch_pad=0.5, patch_merge=1.0,
        optical_noise=None, handback_cutoff=None,
        fast_wide_cutoff=fast_wide_cutoff,
        fast_wide_ramp=[150.0, 300.0], fast_wide_accel=1500.0,
        anchor_mode=False, anchor_cutoff=1.5)


def seeded_video_rates(video_path, intervals, meta):
    """o4fix.video_rates (o4fix.py:273-318) with per-frame-pair RNG seed."""
    cap = cv2.VideoCapture(str(video_path))
    fps = cap.get(cv2.CAP_PROP_FPS)
    W = int(cap.get(cv2.CAP_PROP_FRAME_WIDTH))
    H = int(cap.get(cv2.CAP_PROP_FRAME_HEIGHT))
    fp = meta.get("fisheye_params", {})
    K = np.array(fp.get("camera_matrix",
                        [[546.4027, 0, W / 2], [0, 546.4027, H / 2], [0, 0, 1]]),
                 dtype=np.float64)
    D = np.array(fp.get("distortion_coeffs",
                        [0.1551311, 0.1371409, -0.0938614, 0.0041704]),
                 dtype=np.float64)
    cd = meta.get("calib_dimension", {"w": W, "h": H})
    K = K.copy()
    K[0] *= W / cd["w"]
    K[1] *= H / cd["h"]
    feat = dict(maxCorners=600, qualityLevel=0.01, minDistance=12, blockSize=7)
    lk = dict(winSize=(21, 21), maxLevel=3,
              criteria=(cv2.TERM_CRITERIA_EPS | cv2.TERM_CRITERIA_COUNT, 30, 0.01))
    ts, oms, qs = [], [], []
    for (a, b) in intervals:
        f0 = max(0, int(a * fps))
        f1 = int(b * fps) + 1
        cap.set(cv2.CAP_PROP_POS_FRAMES, f0)
        prev = None
        for fidx in range(f0, f1 + 1):
            ok, frame = cap.read()
            if not ok:
                break
            gray = cv2.resize(cv2.cvtColor(frame, cv2.COLOR_BGR2GRAY),
                              (W // 2, H // 2))
            if prev is not None:
                cv2.setRNGSeed(SEED_BASE + fidx)        # <-- the one change
                rvec, q = o4fix._pair_rotation(prev, gray, K, D, feat, lk)
                ts.append((fidx - 0.5) / fps)
                oms.append(rvec * fps)
                qs.append(q)
            prev = gray
    cap.release()
    return np.array(ts), np.array(oms), np.array(qs)


def main():
    GOLD.mkdir(exist_ok=True)
    ap = argparse.ArgumentParser()
    ap.add_argument("--m4-only", action="store_true",
                    help="only (re)generate goldens/ref_fixed_m4.MP4 (~10 min); "
                         "skips the M2 stage dumps")
    opts = ap.parse_args()

    args = build_args()

    t, q, meta = o4fix.extract_quats(VIDEO)
    fs = 1.0 / np.median(np.diff(t))
    tm, omega = o4fix.quats_to_rates(t, q)
    clean, diag = o4fix.adaptive_clean(omega, fs, args)
    severe = o4fix.find_intervals(diag["noise"] > args.severe, tm,
                                  args.severe_pad, args.severe_merge, 0.2)
    o4fix.video_rates = seeded_video_rates      # monkeypatch once, both modes

    if not opts.m4_only:
        np.savez(GOLD / "extract.npz", t=t, q=q)
        json.dump({k: meta.get(k) for k in
                   ("camera", "model", "frame_readout_time", "calib_dimension",
                    "fisheye_params")},
                  open(GOLD / "meta.json", "w"), default=str, indent=1)

        np.savez(GOLD / "clean.npz", tm=tm, omega=omega, cleaned=clean,
                 alpha=diag["alpha"], noise=diag["noise"], light=diag["light"],
                 strong=diag["strong"], spike_frac=diag["spikes"].mean())

        noisy = o4fix.find_intervals(diag["alpha"] > 0.15, tm,
                                     pad_s=args.patch_pad,
                                     merge_s=args.patch_merge, min_s=0.2)
        calib_all = o4fix.find_intervals(diag["alpha"] < 0.02, tm, -0.2, 0.0, 3.0)
        motion = np.degrees(np.linalg.norm(clean, axis=1))
        scored = []
        for (a, b) in calib_all:
            m = (tm >= a) & (tm <= b)
            scored.append((motion[m].std(), a, min(b, a + 4.0)))
        scored.sort(reverse=True)
        calib = [(a, b) for _, a, b in scored[:6]]
        np.savez(GOLD / "intervals.npz", noisy=np.array(noisy),
                 calib=np.array(calib), severe=np.array(severe))

        tvc, ovc, qvc = seeded_video_rates(VIDEO, calib, meta)
        np.savez(GOLD / "optical_calib.npz", t=tvc, omega=ovc, quality=qvc)
        fit = o4fix.fit_video_alignment(tvc, ovc, qvc, tm, np.degrees(clean), fs)
        shift, N, r2 = fit
        np.savez(GOLD / "fit.npz", shift=shift, n=N, r2=r2)
        tvn, ovn, qvn = seeded_video_rates(VIDEO, noisy, meta)
        np.savez(GOLD / "optical_noisy.npz", t=tvn, omega=ovn, quality=qvn)

        patched = o4fix.optical_patch(VIDEO, tm, clean, diag, fs, args, meta)
        np.savez(GOLD / "patched.npz", rates=patched)

        q_out, stats = o4fix.splice_orientation(t, q, patched, severe, args.ramp)
        np.savez(GOLD / "splice.npz", q_out=q_out,
                 drifts=np.array([(a, b, d) for a, b, d in stats]))

        # SEEDED python-reference MP4 for the Task 15 e2e gate. The user's
        # sample_vids/..._fixed.MP4 was made with unseeded RANSAC and is NOT
        # bit-comparable; this one shares the rust seed scheme.
        ok = mp4patch.inject_and_check(str(VIDEO), q_out, str(GOLD / "ref_fixed.MP4"))
        assert ok, "python reference round-trip failed"

        so, qf, ts_ms, q_ref = mp4patch._aligned_slots(str(VIDEO))
        np.savez(GOLD / "slots.npz", offs=so, q_file=qf, ts_ms=ts_ms, q_ref=q_ref)

    # SEEDED M4 reference (Plan 2 Task 3): same clip, fast_wide_cutoff=16
    args_m4 = build_args(fast_wide_cutoff=16.0)
    patched_m4 = o4fix.optical_patch(VIDEO, tm, clean, diag, fs, args_m4, meta)
    q_out_m4, _ = o4fix.splice_orientation(t, q, patched_m4, severe, args_m4.ramp)
    ok = mp4patch.inject_and_check(str(VIDEO), q_out_m4,
                                   str(GOLD / "ref_fixed_m4.MP4"))
    assert ok, "python M4 reference round-trip failed"
    print("goldens written to", GOLD)


if __name__ == "__main__":
    main()
