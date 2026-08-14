"""Unit tests for the drift-rebase splice (spec 2026-08-13)."""
import sys
from pathlib import Path

import numpy as np
import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import o4fix


def make_case(drift_axis=(0.0, 0.0, 1.0), drift_deg=60.0):
    """1 kHz still-camera clip, one 1 s burst [2, 3] whose raw endpoint is
    drift_deg away from where zero patch rates integrate to."""
    fs = 1000
    t = np.arange(0, 10.0, 1.0 / fs)
    n = len(t)
    # raw quats rotate to the drifted attitude linearly across the burst
    ax = np.asarray(drift_axis, dtype=float)
    ax /= np.linalg.norm(ax)
    in_burst = (t >= 2.0) & (t <= 3.0)
    frac = np.clip((t - 2.0) / 1.0, 0.0, 1.0)
    q_raw = o4fix.quat_exp(np.radians(drift_deg) * frac[:, None] * ax[None, :])
    omega = np.zeros((n - 1, 3))  # optical says: camera did not move
    return t, q_raw, omega


def rates_deg(t, q):
    tm, om = o4fix.quats_to_rates(t, q)
    return tm, np.degrees(om)


def test_rebase_off_is_todays_behavior():
    t, q_raw, omega = make_case()
    q_out, stats = o4fix.splice_orientation(
        t, q_raw, omega, [(2.0, 3.0)], 0.3, rebase_above=0.0)
    # endpoints pinned to raw (1-ulp slerp wobble allowed: only samples
    # OUTSIDE intervals carry the byte guarantee), outside bit-identical
    assert stats[0][3] is False
    i1 = np.searchsorted(t, 3.0, "right") - 1
    dot = abs(float(np.dot(q_out[i1], q_raw[i1])))
    assert np.degrees(2 * np.arccos(min(dot, 1.0))) < 1e-7
    outside = (t < 2.0) | (t > 3.0)
    assert np.array_equal(q_out[outside], q_raw[outside])
    # the bridge injects fake rate inside the burst (that's the old floor)
    tm, om = rates_deg(t, q_out)
    mid = (tm > 2.2) & (tm < 2.8)
    assert np.linalg.norm(om[mid], axis=1).max() > 30.0


def test_rebase_kills_in_burst_fake_rate():
    t, q_raw, omega = make_case(drift_deg=60.0)  # implied 1.5*60/1 = 90 deg/s
    q_out, stats = o4fix.splice_orientation(
        t, q_raw, omega, [(2.0, 3.0)], 0.3,
        rebase_above=30.0, decay_rate=1.5)
    (a, b, drift, rebased) = stats[0]
    assert rebased is True
    assert drift == pytest.approx(60.0, abs=1.0)
    tm, om = rates_deg(t, q_out)
    # inside the burst: no bridge -> output rate ~ optical (zero), except
    # the 0.3 s edge ramps which blend real raw rate content
    mid = (tm > 2.35) & (tm < 2.65)
    assert np.linalg.norm(om[mid], axis=1).max() < 2.0
    # after the burst: offset decays at <= decay_rate (fake pan capped)
    post = (tm > 3.4) & (tm < 40.0)
    assert np.linalg.norm(om[post], axis=1).max() < 1.5 + 0.1


def test_offset_decays_to_bit_identical_raw():
    t, q_raw, omega = make_case(drift_deg=6.0)   # implied 9 deg/s
    q_out, stats = o4fix.splice_orientation(
        t, q_raw, omega, [(2.0, 3.0)], 0.3,
        rebase_above=5.0, decay_rate=1.5)
    assert stats[0][3] is True
    # 6 deg at 1.5 deg/s -> gone 4 s after the burst; far samples bit-exact
    far = t > 3.0 + 6.0 / 1.5 + 0.5
    assert np.array_equal(q_out[far], q_raw[far])


def test_offsets_compose_across_bursts():
    fs = 1000
    t = np.arange(0, 20.0, 1.0 / fs)
    n = len(t)
    ax = np.array([0.0, 0.0, 1.0])
    # two bursts, each drifting raw 40 deg further about z
    frac = (np.clip((t - 2.0), 0, 1) + np.clip((t - 8.0), 0, 1))
    q_raw = o4fix.quat_exp(np.radians(40.0) * frac[:, None] * ax[None, :])
    omega = np.zeros((n - 1, 3))
    q_out, stats = o4fix.splice_orientation(
        t, q_raw, omega, [(2.0, 3.0), (8.0, 9.0)], 0.3,
        rebase_above=30.0, decay_rate=0.0)   # carry forever
    assert [s[3] for s in stats] == [True, True]
    # with zero decay the offset after burst 2 is the composed 80 deg:
    # q_out stays at identity attitude (optical said "no motion") forever
    tm, om = rates_deg(t, q_out)
    quiet = (tm > 9.4)
    assert np.linalg.norm(om[quiet], axis=1).max() < 0.5
    end_err = o4fix.quat_log(q_out[-1:])  # attitude still ~identity
    assert np.degrees(np.linalg.norm(end_err)) < 1.0


def test_identity_offset_spans_bit_identical():
    t, q_raw, omega = make_case()
    q_out, _ = o4fix.splice_orientation(
        t, q_raw, omega, [(2.0, 3.0)], 0.3,
        rebase_above=1000.0, decay_rate=1.5)  # gate never trips
    outside = (t < 2.0) | (t > 3.0)
    assert np.array_equal(q_out[outside], q_raw[outside])
