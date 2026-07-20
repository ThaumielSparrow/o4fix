#!/usr/bin/env python3
"""Fix noisy DJI O4 Pro gyro data for Gyroflow.

Early-2026 O4 Pro air units ship a gyro that picks up strong broadband noise
(~10-180 Hz) during high-throttle maneuvers, which makes Gyroflow-stabilized
footage shudder.

Default mode writes VIDEO_fixed.MP4: a copy of the video whose embedded
orientation quaternions are repaired in place. Outside severe noise bursts the
telemetry is byte-identical to the original; inside bursts the orientation is
replaced by integrated optical-flow motion measured from the video itself
(slerp-pinned to the raw path at burst edges). Drop the fixed file into
Gyroflow exactly like a stock recording - no separate motion-data file, and
Gyroflow's native embedded-telemetry path (which measurably out-stabilizes
any external gcsv on this camera) is preserved.

Usage:
    python o4fix.py VIDEO.MP4 [VIDEO2.MP4 ...] [options]

--gcsv switches to the legacy rate-domain pipeline that writes a .gcsv
motion-data file next to the video instead (Hampel + noise-adaptive low-pass
blend + optical patching in the rate domain). In Gyroflow, drag the .gcsv
onto the loaded video; do NOT autosync (timestamps are already aligned).

Requires: pip install telemetry-parser numpy scipy opencv-python
(matplotlib for --plot)
"""
import argparse
import sys
from pathlib import Path

import numpy as np
from scipy.ndimage import median_filter, uniform_filter1d
from scipy.signal import butter, filtfilt


# ---------------------------------------------------------------- quaternions

def quat_mul(a, b):
    w1, x1, y1, z1 = a[..., 0], a[..., 1], a[..., 2], a[..., 3]
    w2, x2, y2, z2 = b[..., 0], b[..., 1], b[..., 2], b[..., 3]
    return np.stack([
        w1 * w2 - x1 * x2 - y1 * y2 - z1 * z2,
        w1 * x2 + x1 * w2 + y1 * z2 - z1 * y2,
        w1 * y2 - x1 * z2 + y1 * w2 + z1 * x2,
        w1 * z2 + x1 * y2 - y1 * x2 + z1 * w2], axis=-1)


def quat_conj(a):
    b = a.copy()
    b[..., 1:] *= -1
    return b


def extract_quats(video_path):
    """Return (t_seconds, quats_wxyz, meta) from a DJI O4 Pro video."""
    import telemetry_parser
    tp = telemetry_parser.Parser(str(video_path))
    meta = {"camera": tp.camera, "model": tp.model}
    ts, qs = [], []
    for entry in tp.telemetry():
        default = entry.get("Default", {}).get("Metadata", {})
        if default:
            meta.update(default)
        lens = entry.get("Lens", {}).get("Data", {})
        if lens:
            meta["frame_readout_time"] = lens.get("frame_readout_time")
            meta["fisheye_params"] = lens.get("fisheye_params", {})
            meta["calib_dimension"] = lens.get("calib_dimension")
        qd = entry.get("Quaternion", {}).get("Data")
        if qd:
            for s in qd:
                ts.append(s["t"])
                v = s["v"]
                qs.append((v["w"], v["x"], v["y"], v["z"]))
    if not ts:
        raise RuntimeError(f"No quaternion telemetry found in {video_path} "
                           f"(camera={meta.get('camera')}, model={meta.get('model')})")
    ts = np.asarray(ts, dtype=np.float64)  # milliseconds
    qs = np.asarray(qs, dtype=np.float64)

    # sort out timestamp jitter, drop repeated samples (2 kHz stream carries
    # each 1 kHz quat update twice)
    order = np.argsort(ts, kind="stable")
    ts, qs = ts[order], qs[order]
    change = np.any(qs[1:] != qs[:-1], axis=1)
    keep = np.r_[0, np.where(change)[0] + 1]
    ts, qs = ts[keep], qs[keep]

    # hemisphere continuity + normalize
    flips = np.cumsum(np.r_[0, (np.sum(qs[1:] * qs[:-1], axis=1) < 0)]) % 2
    qs[flips == 1] *= -1
    qs /= np.linalg.norm(qs, axis=1, keepdims=True)
    return ts / 1000.0, qs, meta


def quat_exp(v):
    """Rotation vector (N,3) -> unit quat (N,4) wxyz."""
    theta = np.linalg.norm(v, axis=-1, keepdims=True)
    # sin(t/2)/t -> 0.5 as t -> 0, so the small-angle limit falls out of where()
    k = np.where(theta > 1e-12, np.sin(theta / 2) / np.maximum(theta, 1e-12), 0.5)
    q = np.concatenate([np.cos(theta / 2), v * k], axis=-1)
    return q / np.linalg.norm(q, axis=-1, keepdims=True)


def quat_log(q):
    """Unit quat (N,4) -> rotation vector (N,3)."""
    q = np.where(q[..., :1] < 0, -q, q)
    vecn = np.linalg.norm(q[..., 1:], axis=-1, keepdims=True)
    theta = 2 * np.arcsin(np.clip(vecn, 0.0, 1.0))
    k = np.where(vecn > 1e-12, theta / np.maximum(vecn, 1e-12), 2.0)
    return q[..., 1:] * k


def slerp(qa, qb, w):
    """Element-wise slerp between quat arrays, w in [0,1] shape (N,)."""
    dot = np.sum(qa * qb, axis=-1, keepdims=True)
    qb = np.where(dot < 0, -qb, qb)
    dot = np.abs(dot).clip(-1.0, 1.0)
    theta = np.arccos(dot)
    sin_t = np.sin(theta)
    lin = sin_t[..., 0] < 1e-6
    w = w[:, None]
    wa = np.where(lin[:, None], 1 - w,
                  np.sin((1 - w) * theta) / np.maximum(sin_t, 1e-12))
    wb = np.where(lin[:, None], w,
                  np.sin(w * theta) / np.maximum(sin_t, 1e-12))
    out = wa * qa + wb * qb
    return out / np.linalg.norm(out, axis=-1, keepdims=True)


def smoothstep(x):
    x = np.clip(x, 0.0, 1.0)
    return x * x * (3 - 2 * x)


def splice_orientation(t, q_raw, omega_patch_rad, intervals, ramp_s):
    """Replace q_raw inside each interval with integrated omega_patch, pinned
    to raw at both edges. omega_patch_rad[k] applies to step t[k]->t[k+1].

    The accumulated drift of the integrated path over each interval is spread
    across the whole interval as a slow rotation-vector correction
    (smoothstep), so both endpoints match raw exactly with no fast fake
    motion; a ramp_s slerp cross-fade at each edge blends rate content
    smoothly. Samples outside intervals are returned bit-identical.
    """
    q_out = q_raw.copy()
    stats = []
    for (a, b) in intervals:
        i0 = max(int(np.searchsorted(t, a, "left")), 0)
        i1 = min(int(np.searchsorted(t, b, "right")) - 1, len(t) - 1)
        if i1 - i0 < 8:
            continue
        n = i1 - i0
        dt = np.diff(t[i0:i1 + 1])
        dq = quat_exp(omega_patch_rad[i0:i1] * dt[:, None])
        qs = np.empty((n + 1, 4))
        qs[0] = q_raw[i0]
        for k in range(n):
            qs[k + 1] = quat_mul(qs[k], dq[k])
        qs /= np.linalg.norm(qs, axis=1, keepdims=True)

        e = quat_log(quat_mul(quat_conj(qs[-1:]), q_raw[i1:i1 + 1]))[0]
        drift_deg = np.degrees(np.linalg.norm(e))
        s = smoothstep((t[i0:i1 + 1] - t[i0]) / max(t[i1] - t[i0], 1e-9))
        qs = quat_mul(qs, quat_exp(s[:, None] * e[None, :]))

        tt = t[i0:i1 + 1]
        r = np.minimum(smoothstep((tt - tt[0]) / ramp_s),
                       smoothstep((tt[-1] - tt) / ramp_s))
        q_out[i0:i1 + 1] = slerp(q_raw[i0:i1 + 1], qs, r)
        stats.append((a, b, drift_deg))
    return q_out, stats


def quats_to_rates(t, q):
    """Body angular rate (rad/s) between consecutive quats, small-angle safe.

    Returns (t_mid, omega) with len(t)-1 samples.
    """
    dq = quat_mul(quat_conj(q[:-1]), q[1:])
    dq[dq[:, 0] < 0] *= -1
    vecn = np.linalg.norm(dq[:, 1:], axis=1)
    theta = 2 * np.arcsin(np.clip(vecn, 0.0, 1.0))
    scale = np.where(vecn > 1e-12, theta / np.maximum(vecn, 1e-12), 2.0)
    dt = np.diff(t)
    omega = dq[:, 1:] * (scale / np.maximum(dt, 1e-9))[:, None]
    return t[:-1] + dt / 2, omega


# -------------------------------------------------------------------- filters

def hampel(x, k, nsig):
    """Rolling-median spike rejection. x shape (N, 3)."""
    med = median_filter(x, size=2 * k + 1, mode="nearest", axes=0)
    sigma = 1.4826 * median_filter(np.abs(x - med), size=2 * k + 1,
                                   mode="nearest", axes=0) + 1e-9
    bad = np.abs(x - med) > nsig * sigma
    out = x.copy()
    out[bad] = med[bad]
    return out, bad


def zero_phase_lp(x, fc, fs, order=2):
    b, a = butter(order, fc / (fs / 2), "low")
    return filtfilt(b, a, x, axis=0)


def adaptive_clean(omega, fs, args):
    """Hampel pre-pass + noise-adaptive blend of light/strong zero-phase LPFs.

    omega in rad/s, shape (N, 3). Returns (cleaned, diagnostics).
    """
    deg = np.degrees(omega)
    x, spikes = hampel(deg, args.hampel_window, args.hampel_sigma)

    if args.lpf:  # plain fallback mode
        out = zero_phase_lp(x, args.lpf, fs)
        return np.radians(out), {"spikes": spikes, "alpha": np.zeros(len(x))}

    # local noise estimate: rolling RMS of the 30-180 Hz band (pure noise for
    # this camera; real motion lives below ~10 Hz). Max across axes, since
    # noise bursts are physical vibration hitting the whole IMU.
    b, a = butter(2, [args.noise_band[0] / (fs / 2),
                      min(args.noise_band[1], 0.95 * fs / 2) / (fs / 2)], "band")
    hf = filtfilt(b, a, x, axis=0)
    win = max(3, int(round(args.noise_window * fs / 1000)))
    noise = np.sqrt(uniform_filter1d(hf ** 2, size=win, axis=0,
                                     mode="nearest")).max(axis=1)

    # blend factor: 0 below noise-low, 1 above noise-high (deg/s RMS),
    # smoothed so crossfades take ~200 ms
    alpha = np.clip((noise - args.noise_low) /
                    (args.noise_high - args.noise_low), 0.0, 1.0)
    alpha = uniform_filter1d(alpha, size=max(3, int(0.2 * fs)), mode="nearest")

    light = zero_phase_lp(x, args.light_cutoff, fs)
    strong = zero_phase_lp(x, args.strong_cutoff, fs)
    out = light * (1 - alpha[:, None]) + strong * alpha[:, None]
    return np.radians(out), {"spikes": spikes, "alpha": alpha, "noise": noise,
                             "light": light, "strong": strong}


# ------------------------------------------------------- optical motion patch
#
# During noise bursts the O4 Pro quaternions carry phantom motion at ALL
# frequencies (verified against optical flow ground truth), so no temporal
# filter can recover the true motion there. Instead we measure the camera's
# actual rotation from the video frames and splice it in. The video->gyro
# axis alignment and time offset are calibrated per clip on clean sections
# where the gyro is trustworthy.

def find_intervals(mask, t, pad_s, merge_s, min_s):
    """Time intervals [t0, t1] where mask is True, padded/merged/pruned."""
    edges = np.diff(mask.astype(np.int8))
    starts = list(np.where(edges == 1)[0] + 1)
    ends = list(np.where(edges == -1)[0] + 1)
    if mask[0]:
        starts.insert(0, 0)
    if mask[-1]:
        ends.append(len(mask))
    iv = [[t[s] - pad_s, t[min(e, len(t) - 1)] + pad_s]
          for s, e in zip(starts, ends)]
    merged = []
    for a, b in iv:
        if merged and a - merged[-1][1] < merge_s:
            merged[-1][1] = b
        else:
            merged.append([a, b])
    return [(a, b) for a, b in merged if b - a >= min_s]


def video_rates(video_path, intervals, meta):
    """Camera rotation rate measured by optical flow over given time intervals.

    Returns (t, omega_cam_rad_s, quality) sampled per frame pair; quality in
    [0, 1] from RANSAC inlier support (0 where estimation failed).
    """
    import cv2
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
                rvec, q = _pair_rotation(prev, gray, K, D, feat, lk)
                ts.append((fidx - 0.5) / fps)
                oms.append(rvec * fps)
                qs.append(q)
            prev = gray
    cap.release()
    return np.array(ts), np.array(oms), np.array(qs)


def _pair_rotation(prev, gray, K, D, feat, lk):
    """Rotation (rad, camera frame) between two half-res frames; quality 0-1."""
    import cv2
    p0 = cv2.goodFeaturesToTrack(prev, **feat)
    if p0 is None or len(p0) < 40:
        return np.zeros(3), 0.0
    p1, st, _ = cv2.calcOpticalFlowPyrLK(prev, gray, p0, None, **lk)
    good = st.ravel() == 1
    if good.sum() < 40:
        return np.zeros(3), 0.0
    u0 = cv2.fisheye.undistortPoints(
        (p0[good].reshape(-1, 2) * 2).reshape(-1, 1, 2), K, D).reshape(-1, 2)
    u1 = cv2.fisheye.undistortPoints(
        (p1[good].reshape(-1, 2) * 2).reshape(-1, 1, 2), K, D).reshape(-1, 2)
    # essential matrix separates true rotation from translation parallax
    # (a rotation-only homography fit absorbs parallax into pan/tilt).
    # recoverPose's cheirality test degenerates for low-parallax frame pairs,
    # so decompose E ourselves and take the small-angle rotation candidate
    # (frame-to-frame rotation is always far below 90 degrees).
    E, inl = cv2.findEssentialMat(u0, u1, np.eye(3), method=cv2.RANSAC,
                                  prob=0.999, threshold=0.002)
    if E is None or E.shape != (3, 3) or inl is None or inl.sum() < 60:
        return np.zeros(3), 0.0
    R1, R2, _ = cv2.decomposeEssentialMat(E)
    r1, _ = cv2.Rodrigues(R1)
    r2, _ = cv2.Rodrigues(R2)
    rvec = r1 if np.linalg.norm(r1) <= np.linalg.norm(r2) else r2
    quality = min(1.0, (int(inl.sum()) - 60) / 150.0)
    return rvec.ravel(), max(0.0, quality)


def fit_video_alignment(tv, ov, qv, tm, gyro_deg, fs):
    """Fit time shift + 3x3 matrix mapping video rates -> gyro frame (deg/s).

    Calibrated on samples where optical quality is good. Returns
    (shift_s, N, r2) or None if the fit is unusable.
    """
    good = qv > 0.5
    if good.sum() < 200:
        return None
    ovd = np.degrees(ov)

    def gyro_at(tq):
        sm = uniform_filter1d(gyro_deg, size=max(1, int(fs / 100)), axis=0,
                              mode="nearest")
        return np.stack([np.interp(tq, tm, sm[:, k]) for k in range(3)], axis=1)

    b, a = butter(2, 5.0 / 50.0, "low")  # <5 Hz on ~100 Hz samples

    def lowf(x):
        return filtfilt(b, a, x, axis=0)

    def procrustes(B, A):
        # best orthogonal N (video -> gyro): unit scale by construction, so
        # integrated orientation cannot drift from a rate scale error.
        # Reflections are allowed: the optical estimate measures apparent
        # scene motion (inverse of camera motion), so the true mapping has
        # det -1.
        U, _, Vt = np.linalg.svd(B.T @ A)
        return U @ Vt

    shift, N = 0.0, None
    for _ in range(3):
        g = gyro_at(tv[good] + shift)
        A = lowf(g)
        B = lowf(ovd[good])
        N = procrustes(B, A)
        # refine shift: scan +-60 ms
        best = (-np.inf, shift)
        for sh in np.arange(shift - 0.06, shift + 0.061, 0.002):
            g = lowf(gyro_at(tv[good] + sh))
            p = B @ N
            r = np.mean([np.corrcoef(g[:, k], p[:, k])[0, 1] for k in range(3)])
            if r > best[0]:
                best = (r, sh)
        shift = best[1]
    g = lowf(gyro_at(tv[good] + shift))
    p = lowf(ovd[good]) @ N
    r2 = 1 - ((g - p) ** 2).sum() / ((g - g.mean(0)) ** 2).sum()
    return shift, N, r2


def optical_patch(video, tm, cleaned_rad, diag, fs, args, meta):
    """Replace gyro data with video-measured motion inside noise bursts."""
    alpha = diag["alpha"]
    if args.optical_noise:  # separate (higher) trigger for optical substitution
        lo_n, hi_n = args.optical_noise
        alpha_opt = np.clip((diag["noise"] - lo_n) / (hi_n - lo_n), 0.0, 1.0)
        alpha_opt = uniform_filter1d(alpha_opt, size=max(3, int(0.2 * fs)),
                                     mode="nearest")
    else:
        alpha_opt = alpha
    noisy = find_intervals(alpha_opt > 0.15, tm, pad_s=args.patch_pad,
                           merge_s=args.patch_merge, min_s=0.2)
    if not noisy:
        print("   optical patch: no noisy sections detected, skipping")
        return cleaned_rad

    # calibration sections: clean flight with decent motion, spread over clip
    clean_mask = alpha < 0.02
    calib_all = find_intervals(clean_mask, tm, pad_s=-0.2, merge_s=0.0, min_s=3.0)
    motion = np.degrees(np.linalg.norm(cleaned_rad, axis=1))
    scored = []
    for (a, b) in calib_all:
        m = (tm >= a) & (tm <= b)
        scored.append((motion[m].std(), a, min(b, a + 4.0)))
    scored.sort(reverse=True)
    calib = [(a, b) for _, a, b in scored[:6]]
    if not calib:
        print("   optical patch: no clean calibration sections, skipping")
        return cleaned_rad

    total = sum(b - a for a, b in noisy) + sum(b - a for a, b in calib)
    print(f"   optical patch: analyzing {len(noisy)} noisy + {len(calib)} "
          f"calibration sections ({total:.0f} s of video)...")
    tvc, ovc, qvc = video_rates(video, calib, meta)
    fit = fit_video_alignment(tvc, ovc, qvc, tm, np.degrees(cleaned_rad), fs)
    if fit is None:
        print("   optical patch: calibration failed, keeping filtered gyro")
        return cleaned_rad
    shift, N, r2 = fit
    print(f"   optical patch: video/gyro alignment R2={r2:.3f}, "
          f"time offset {shift * 1000:.0f} ms")
    if r2 < 0.8:
        print("   optical patch: alignment too poor (<0.8), keeping filtered gyro")
        return cleaned_rad

    tv, ov, qv = video_rates(video, noisy, meta)
    if len(tv) == 0:
        return cleaned_rad
    patch_deg = np.degrees(ov) @ N          # gyro frame, deg/s, ~frame rate
    tv = tv + shift

    out = np.degrees(cleaned_rad).copy()
    light = diag["light"]

    # rate-aware source selection: optical flow degrades during fast motion
    # (motion blur, rolling-shutter skew scale with rate) while the gyro's
    # phantom noise becomes fractionally tiny, so hand fast movements back
    # to the band-limited gyro. Both sources share the same bandwidth.
    medium = zero_phase_lp(light, args.handback_cutoff or args.optical_cutoff,
                           fs)
    rate_mag = uniform_filter1d(np.linalg.norm(medium, axis=1),
                                size=max(3, int(0.1 * fs)), mode="nearest")
    lo_r, hi_r = args.fast_handback
    w_fast = np.clip((rate_mag - lo_r) / max(hi_r - lo_r, 1e-6), 0.0, 1.0)
    w_fast = uniform_filter1d(w_fast, size=max(3, int(0.15 * fs)),
                              mode="nearest")

    # very fast motion (>~150 deg/s): the gyro's 8-16 Hz content is
    # real-motion dominated (verified: embedded renders correct 8-15 Hz well
    # there, while at 50-150 deg/s the same band is phantom-dominated), so
    # hand back a wider band as rate climbs to keep sharp turns/flips crisp
    if args.fast_wide_cutoff:
        wide = zero_phase_lp(light, args.fast_wide_cutoff, fs)
        lo_w, hi_w = args.fast_wide_ramp
        w_wide = np.clip((rate_mag - lo_w) / max(hi_w - lo_w, 1e-6), 0.0, 1.0)
        if args.fast_wide_accel:
            # snap transitions (flicks/180s) punch the throttle and corrupt
            # the mid-band gyro; only sustained fast rotation gets the wider
            # handback
            acc = uniform_filter1d(np.abs(np.gradient(rate_mag) * fs),
                                   size=max(3, int(0.1 * fs)), mode="nearest")
            w_wide *= np.clip(1.0 - acc / args.fast_wide_accel, 0.0, 1.0)
        w_wide = uniform_filter1d(w_wide, size=max(3, int(0.15 * fs)),
                                  mode="nearest")
        medium = (1 - w_wide[:, None]) * medium + w_wide[:, None] * wide

    vfps = 1.0 / np.median(np.diff(tv[:100])) if len(tv) > 1 else 100.0
    bq, aq = butter(2, min(args.optical_cutoff, 0.45 * vfps) / (vfps / 2), "low")
    strong = diag["strong"]
    for (a, b) in noisy:
        m = (tv >= a - 0.3) & (tv <= b + 0.3)
        if m.sum() < 30:
            continue
        seg_t, seg_o, seg_q = tv[m], patch_deg[m], qv[m]
        frac_bad = (seg_q < 0.3).mean()
        if frac_bad > 0.3:
            print(f"   optical patch: {a:.1f}-{b:.1f}s skipped "
                  f"({frac_bad * 100:.0f}% low-quality flow), keeping filtered gyro")
            continue
        # fill low-quality flow samples by interpolation, then smooth
        badq = seg_q < 0.3
        if badq.any() and not badq.all():
            for k in range(3):
                seg_o[badq, k] = np.interp(seg_t[badq], seg_t[~badq],
                                           seg_o[~badq, k])
        seg_o = filtfilt(bq, aq, seg_o, axis=0)
        gm = (tm >= a) & (tm <= b)
        video_1k = np.stack([np.interp(tm[gm], seg_t, seg_o[:, k])
                             for k in range(3)], axis=1)
        wf = w_fast[gm, None]
        if args.anchor_mode:
            # optical is only a low-frequency drift anchor on the band-limited
            # gyro: mid/high frequencies keep the gyro's crisp real motion
            # (strong-cutoff band), optical replaces its slow phantom wander.
            # The anchor fades out at high rates where optical LF degrades.
            g = strong[gm]
            corr = video_1k - g
            nseg = len(corr)
            ba, aa = butter(2, args.anchor_cutoff / (fs / 2), "low")
            if nseg > 3 * max(len(ba), len(aa)) * 10:
                corr = filtfilt(ba, aa, corr, axis=0,
                                padlen=min(nseg - 1, int(2 * fs)))
            burst = g + (1 - wf) * corr
        else:
            burst = (1 - wf) * video_1k + wf * medium[gm]
        # steep ramp: reach full burst weight quickly so phantom-laden gyro
        # doesn't leak through at burst edges via the (1-w) term
        w = np.clip(alpha_opt[gm, None] / 0.35, 0.0, 1.0)
        # with a separate optical trigger, the non-burst partner is the
        # mild-zone light/strong blend (out); classic mode keeps light
        partner = out[gm] if args.optical_noise else light[gm]
        out[gm] = (1 - w) * partner + w * burst
    return np.radians(out)


# --------------------------------------------------------------------- output

def write_gcsv(path, t, omega_rad, meta, video_name, orientation):
    tscale = 0.001  # timestamps written in ms
    lines = [
        "GYROFLOW IMU LOG",
        "version,1.3",
        "id,o4fix",
        f"orientation,{orientation}",
        "note,DJI O4 Pro gyro cleaned by o4fix",
        f"vendor,{meta.get('product_name', 'DJI')}",
        f"videofilename,{video_name}",
        f"tscale,{tscale}",
        "gscale,1.0",
        "ascale,1.0",
    ]
    frt = meta.get("frame_readout_time")
    if frt:
        lines.append(f"frame_readout_time,{frt:.6f}")
    lines.append("t,gx,gy,gz")
    body = "\n".join(f"{tm:.3f},{gx:.6f},{gy:.6f},{gz:.6f}"
                     for tm, (gx, gy, gz) in zip(t * 1000.0, omega_rad))
    Path(path).write_text("\n".join(lines) + "\n" + body + "\n",
                          encoding="ascii", newline="\n")


def save_plot(path, t, raw_deg, clean_deg, alpha):
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    fig, axes = plt.subplots(2, 1, figsize=(14, 8), sharex=True)
    for i, (axn, c) in enumerate(zip("xyz", "rgb")):
        axes[0].plot(t, raw_deg[:, i], c, lw=0.3, alpha=0.35)
        axes[0].plot(t, clean_deg[:, i], c, lw=0.8, label=axn)
    axes[0].set_ylabel("deg/s")
    axes[0].legend()
    axes[0].set_title("raw (faint) vs cleaned gyro")
    axes[1].plot(t, alpha, "k", lw=0.5)
    axes[1].set_ylabel("blend alpha")
    axes[1].set_xlabel("time (s)")
    axes[1].set_title("0 = light filtering, 1 = strong filtering")
    fig.tight_layout()
    fig.savefig(path, dpi=100)
    plt.close(fig)


# ----------------------------------------------------------------------- main

def process(video, args):
    video = Path(video)
    print(f"== {video.name}")
    t, q, meta = extract_quats(video)
    fs = 1.0 / np.median(np.diff(t))
    print(f"   {meta.get('camera', '?')} {meta.get('model', '?')}, "
          f"{len(q)} quat samples @ {fs:.0f} Hz, "
          f"{t[-1] - t[0]:.1f} s")

    tm, omega = quats_to_rates(t, q)
    clean, diag = adaptive_clean(omega, fs, args)

    frac = float(np.mean(diag["alpha"] > 0.5))
    print(f"   spikes replaced: {diag['spikes'].mean() * 100:.1f}% of samples, "
          f"noise bursts cover: {frac * 100:.1f}% of flight")

    if not args.gcsv:
        return process_mp4(video, args, t, q, meta, tm, omega, clean, diag, fs)

    if not args.lpf and not args.no_optical:
        try:
            import cv2  # noqa: F401
            clean = optical_patch(video, tm, clean, diag, fs, args, meta)
        except ImportError:
            print("   optical patch: opencv-python not installed, "
                  "falling back to filtered gyro only")

    out = Path(args.output) if args.output else video.with_suffix(".gcsv")
    write_gcsv(out, tm, clean, meta, video.name, args.orientation)
    print(f"   wrote {out}")

    if args.plot:
        png = out.with_suffix(".png")
        save_plot(png, tm, np.degrees(omega), np.degrees(clean), diag["alpha"])
        print(f"   wrote {png}")


def process_mp4(video, args, t, q_raw, meta, tm, omega, clean, diag, fs):
    """Default mode: write a copy of the MP4 with repaired embedded quats."""
    try:
        import cv2  # noqa: F401
    except ImportError:
        print("   ERROR: opencv-python is required for MP4 repair "
              "(pip install opencv-python), or use --gcsv for the "
              "filter-only fallback")
        return

    patched = optical_patch(video, tm, clean, diag, fs, args, meta)
    if patched is clean:
        print("   optical patch unavailable (see above) - severe bursts "
              "cannot be repaired; not writing an output file")
        return

    severe_mask = diag["noise"] > args.severe
    intervals = find_intervals(severe_mask, tm, pad_s=args.severe_pad,
                               merge_s=args.severe_merge, min_s=0.2)
    if not intervals:
        print(f"   no severe bursts (> {args.severe} deg/s band-RMS) found - "
              "telemetry looks healthy, nothing to repair")
        return
    tot = sum(b - a for a, b in intervals)
    print(f"   replacing orientation in {len(intervals)} severe bursts "
          f"({tot:.1f} s)")

    q_out, stats = splice_orientation(t, q_raw, patched, intervals, args.ramp)
    for a, b, drift in stats:
        print(f"     [{a:7.2f}, {b:7.2f}] optical drift over burst: "
              f"{drift:5.2f} deg")

    import mp4patch
    out = Path(args.output) if args.output else \
        video.with_name(video.stem + "_fixed" + video.suffix)
    if mp4patch.inject_and_check(str(video), q_out, str(out)):
        print(f"   wrote {out} - load it in Gyroflow like a stock recording")
    else:
        out.unlink(missing_ok=True)
        print("   ERROR: round-trip verification failed, output deleted")


def main():
    p = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("videos", nargs="+", help="DJI O4 Pro .MP4 file(s)")
    p.add_argument("-o", "--output",
                   help="output path (single video only); default "
                        "VIDEO_fixed.MP4, or VIDEO.gcsv with --gcsv")
    p.add_argument("--gcsv", action="store_true",
                   help="write a .gcsv motion-data file (legacy rate-domain "
                        "pipeline) instead of repairing the MP4's embedded "
                        "telemetry in place")
    m = p.add_argument_group("MP4 repair tuning (default mode)")
    m.add_argument("--severe", type=float, default=8.0,
                   help="deg/s 30-180 Hz band-RMS above which orientation is "
                        "replaced with integrated optical motion (default 8)")
    m.add_argument("--severe-pad", type=float, default=0.2,
                   help="s, padding around each severe burst (default 0.2)")
    m.add_argument("--severe-merge", type=float, default=0.2,
                   help="s, gap below which severe bursts merge (default 0.2)")
    m.add_argument("--ramp", type=float, default=0.3,
                   help="s, slerp cross-fade to the raw path at burst edges "
                        "(default 0.3)")
    p.add_argument("--plot", action="store_true",
                   help="save a before/after diagnostic .png next to the .gcsv")
    p.add_argument("--orientation", default="XYZ",
                   help="IMU orientation string written to the gcsv header "
                        "(default XYZ; change if horizon comes out wrong)")
    f = p.add_argument_group("filter tuning (defaults tuned on O4 Pro test flight)")
    f.add_argument("--light-cutoff", type=float, default=25.0,
                   help="Hz, zero-phase low-pass applied in clean sections "
                        "(default 25; real motion above 20 Hz is negligible "
                        "per video ground truth)")
    f.add_argument("--strong-cutoff", type=float, default=2.5,
                   help="Hz, low-pass blended in during noise bursts (default 2.5; "
                        "video ground-truth analysis showed phantom gyro motion "
                        "extends down to ~3 Hz while real motion above 3 Hz is tiny)")
    f.add_argument("--noise-low", type=float, default=1.5,
                   help="deg/s band-RMS where patching starts blending in "
                        "(default 1.5; calm-flight floor is ~1)")
    f.add_argument("--noise-high", type=float, default=5.0,
                   help="deg/s band-RMS where patching is fully active (default 5)")
    f.add_argument("--noise-band", type=float, nargs=2, default=[30.0, 180.0],
                   metavar=("LO", "HI"),
                   help="Hz band used to measure noise level (default 30 180)")
    f.add_argument("--noise-window", type=float, default=100.0,
                   help="ms rolling window for the noise estimate (default 100)")
    f.add_argument("--hampel-window", type=int, default=7,
                   help="Hampel half-window in samples (default 7)")
    f.add_argument("--hampel-sigma", type=float, default=6.0,
                   help="Hampel outlier threshold in sigmas (default 6)")
    f.add_argument("--lpf", type=float, metavar="HZ",
                   help="bypass adaptive mode: plain zero-phase low-pass at HZ")
    f.add_argument("--no-optical", action="store_true",
                   help="disable optical motion patching (filtered gyro only)")
    f.add_argument("--optical-cutoff", type=float, default=8.0,
                   help="Hz, bandwidth of patched sections: applied to both "
                        "video-measured motion and handed-back gyro (default 8)")
    f.add_argument("--fast-handback", type=float, nargs=2, default=[100.0, 250.0],
                   metavar=("LO", "HI"),
                   help="deg/s ramp over which patched sections switch from "
                        "optical data back to band-limited gyro during fast "
                        "motion, where gyro SNR is high and optical flow "
                        "degrades (default 100 250)")
    f.add_argument("--patch-pad", type=float, default=0.5,
                   help="s, padding added around each optical-patch interval "
                        "(default 0.5)")
    f.add_argument("--patch-merge", type=float, default=1.0,
                   help="s, gap below which adjacent patch intervals merge "
                        "(default 1.0)")
    f.add_argument("--optical-noise", type=float, nargs=2, metavar=("LO", "HI"),
                   help="deg/s band-RMS ramp that triggers OPTICAL substitution "
                        "(separate, higher threshold than --noise-low/high; "
                        "mild zones then keep --strong-cutoff filtered gyro "
                        "instead of optical). Default: same as noise-low/high")
    f.add_argument("--handback-cutoff", type=float, metavar="HZ",
                   help="Hz, bandwidth of gyro handed back during fast motion "
                        "inside patches (default: --optical-cutoff)")
    f.add_argument("--fast-wide-cutoff", type=float, default=0.0, metavar="HZ",
                   help="Hz, wider handback bandwidth blended in during very "
                        "fast motion where mid-band gyro is real-dominated. "
                        "OFF by default; 16 recovers more sharp-turn/flip "
                        "crispness (2-8/8-15 Hz) at the cost of ~1.5 deg/s "
                        "extra 15-30 Hz flick shake - a measured trade-off, "
                        "eyeball eval_M4 vs eval_M2 to choose")
    f.add_argument("--fast-wide-ramp", type=float, nargs=2,
                   default=[150.0, 300.0], metavar=("LO", "HI"),
                   help="deg/s ramp over which the wider handback engages "
                        "(default 150 300)")
    f.add_argument("--fast-wide-accel", type=float, default=1500.0,
                   metavar="DEG_S2",
                   help="deg/s^2 above which the wider handback fades back "
                        "out (snap transitions corrupt the mid-band gyro; "
                        "default 1500, 0 disables the gate)")
    f.add_argument("--anchor-mode", action="store_true",
                   help="in noise bursts keep --strong-cutoff band-limited "
                        "gyro and use optical only as a low-frequency drift "
                        "anchor (see --anchor-cutoff), instead of replacing "
                        "the motion with optical rates")
    f.add_argument("--anchor-cutoff", type=float, default=1.5,
                   help="Hz, bandwidth of the optical drift anchor in "
                        "--anchor-mode (default 1.5)")
    args = p.parse_args()

    if args.output and len(args.videos) > 1:
        p.error("-o/--output only works with a single video")
    for v in args.videos:
        process(v, args)


if __name__ == "__main__":
    main()
