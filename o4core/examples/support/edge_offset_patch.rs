//! Research splice: fixed offset base at entry/exit, blend offset only where optical weight is one.
use o4core::patch::BurstStat;
use o4core::{dsp, quat};
/// Write q_out[k0..k1] = O(t) (x) q_raw with the offset O decaying toward
/// identity at decay_rate deg/s (0 = carry forever). Samples reached after
/// the decay completes are left untouched (bit-identical raw). Returns the
/// offset remaining at index k1. Ports o4fix.py's `_apply_offset_span`.
fn apply_offset_span(
    q_out: &mut [[f64; 4]],
    q_raw: &[[f64; 4]],
    t: &[f64],
    off: [f64; 4],
    k0: usize,
    k1: usize,
    decay_rate: f64,
) -> [f64; 4] {
    use quat::{qexp, qlog, qmul, qnorm};
    let v = qlog(off);
    let ang = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if ang < 1e-12 || k1 <= k0 {
        return if ang < 1e-12 {
            [1.0, 0.0, 0.0, 0.0]
        } else {
            off
        };
    }
    let axis = [v[0] / ang, v[1] / ang, v[2] / ang];
    let rate = decay_rate.to_radians();
    let mut rem = ang;
    for k in k0..k1 {
        let a = if decay_rate > 0.0 {
            (ang - rate * (t[k] - t[k0])).max(0.0)
        } else {
            ang
        };
        if a > 0.0 {
            let o = qexp([a * axis[0], a * axis[1], a * axis[2]]);
            q_out[k] = qmul(o, q_raw[k]);
        }
    }
    if decay_rate > 0.0 {
        rem = (ang - rate * (t[k1.min(t.len() - 1)] - t[k0])).max(0.0);
    }
    qnorm(qexp([rem * axis[0], rem * axis[1], rem * axis[2]]))
}

/// Replace q_raw inside each interval with integrated omega_patch, pinned to
/// raw at both edges (o4fix.py:137-178, `_apply_offset_span` +
/// `splice_orientation`). Accumulated drift is spread across the interval as
/// a smoothstep-over-time rotation-vector correction; ramp_s slerp
/// cross-fades at the edges. Samples outside intervals are returned
/// bit-identical (clean-zone guarantee — feeds mp4::patch_video's
/// unchanged-row original-bytes path). (Measured 2026-07-21: rate-weighted
/// spreading of the correction is WORSE on both test clips; keep uniform.)
///
/// Drift rebase (spec 2026-08-13): bursts whose implied bridge rate
/// 1.5*drift/duration exceeds rebase_above (deg/s; 0 disables) skip the
/// smoothstep endpoint correction entirely — the path lands on the optical
/// endpoint and the drift is carried forward as a constant orientation
/// offset on the following samples (invisible to stabilization, which only
/// sees rate of orientation error). The offset decays toward identity at
/// decay_rate deg/s (0 = carry forever) and composes across bursts. Samples
/// under an identity offset keep their original bit patterns.
pub fn splice_orientation(
    t: &[f64],
    q_raw: &[[f64; 4]],
    omega_patch: &[[f64; 3]],
    intervals: &[(f64, f64)],
    ramp_s: f64,
    rebase_above: f64,
    decay_rate: f64,
) -> (Vec<[f64; 4]>, Vec<BurstStat>) {
    use quat::{qconj, qexp, qlog, qmul, qnorm, slerp, smoothstep};
    let mut q_out = q_raw.to_vec();
    let mut stats = Vec::new();
    let mut off = [1.0, 0.0, 0.0, 0.0];
    let mut prev_end = 0usize;
    for &(a, b) in intervals {
        let i0 = dsp::searchsorted_left(t, a);
        let i1 = dsp::searchsorted_right(t, b)
            .saturating_sub(1)
            .min(t.len() - 1);
        if i1 < i0 + 8 {
            continue;
        } // python: if i1 - i0 < 8

        off = apply_offset_span(&mut q_out, q_raw, t, off, prev_end, i0, decay_rate);
        let off_pre = off;
        let n = i1 - i0;

        // sequential integration: qs[k+1] = qs[k] * qexp(omega*dt), seeded
        // from the pre-burst offset frame
        let mut qs: Vec<[f64; 4]> = Vec::with_capacity(n + 1);
        qs.push(qmul(off_pre, q_raw[i0]));
        for k in 0..n {
            let dt = t[i0 + k + 1] - t[i0 + k];
            let o = omega_patch[i0 + k];
            qs.push(qmul(qs[k], qexp([o[0] * dt, o[1] * dt, o[2] * dt])));
        }
        for q in qs.iter_mut() {
            *q = qnorm(*q);
        } // python normalizes ONCE, after the loop

        // endpoint drift vs the raw endpoint (in the same offset frame)
        let e = qlog(qmul(qconj(qs[n]), qmul(off_pre, q_raw[i1])));
        let drift_deg = (e[0] * e[0] + e[1] * e[1] + e[2] * e[2])
            .sqrt()
            .to_degrees();
        let dur = (t[i1] - t[i0]).max(1e-9);
        let rebased = rebase_above > 0.0 && 1.5 * drift_deg / dur > rebase_above;
        if rebased {
            // land on the optical endpoint; carry the drift forward as a
            // constant offset instead of bridging it inside the burst
            off = qnorm(qmul(qs[n], qconj(q_raw[i1])));
        } else {
            for k in 0..=n {
                let s = smoothstep((t[i0 + k] - t[i0]) / dur);
                qs[k] = qmul(qs[k], qexp([s * e[0], s * e[1], s * e[2]]));
                // NOTE: python does NOT renormalize after this multiply — neither do we
            }
        }

        // base path the edge ramps blend toward: pre-offset frame at entry,
        // post-offset frame at exit (identical unless rebased); smoothstep
        // interpolation keeps it continuous and flat at both edges
        let d_off = qlog(qmul(off, qconj(off_pre)));
        let off_pre_ang = {
            let v = qlog(off_pre);
            (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
        };
        for k in 0..=n {
            let tt = t[i0 + k];
            // Experimental: keep entry/exit offsets fixed throughout the edge blends.
            // Interpolate only in the fully replaced interior. Fall back for overlapping ramps.
            let s = if rebased && dur > 2.0 * ramp_s {
                smoothstep((tt - t[i0] - ramp_s) / (dur - 2.0 * ramp_s))
            } else {
                smoothstep((tt - t[i0]) / dur)
            };
            let base = if !rebased && off_pre_ang < 1e-12 {
                q_raw[i0 + k] // exact original path (bit-identical branch)
            } else {
                let o = qmul(qexp([s * d_off[0], s * d_off[1], s * d_off[2]]), off_pre);
                qmul(o, q_raw[i0 + k])
            };
            let r = smoothstep((tt - t[i0]) / ramp_s).min(smoothstep((t[i1] - tt) / ramp_s));
            q_out[i0 + k] = slerp(base, qs[k], r);
        }
        stats.push(BurstStat {
            start: a,
            end: b,
            drift_deg,
            rebased,
        });
        prev_end = i1 + 1;
    }
    apply_offset_span(&mut q_out, q_raw, t, off, prev_end, t.len(), decay_rate);
    (q_out, stats)
}
