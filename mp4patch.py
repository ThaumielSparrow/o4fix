#!/usr/bin/env python3
"""Locate, decode and rewrite DJI O4 Pro orientation quaternions inside the
MP4's dvtm (protobuf) metadata track, in place and byte-length-preserving.

Encoding (from telemetry-parser src/dji/): O4P samples carry no "oq101"/"WA530"
marker, so telemetry-parser decodes them with the wm169 protobuf:
  metadata track sample = wm169.ProductMeta protobuf
    frame_meta(3) . imu_frame_meta(3) . IMU_attitude_after_fusion(2)
      = DeviceAttitude{timestamp=1, vsync=2, attitude[]=3, offset=4(float)}
      attitude element = Quaternion{w=1,x=2,y=3,z=4}, float32 LE (wire type 5)
  frame timestamp (us) = frame_meta(3) . frame_meta_header(1) . frame_timestamp(2)

telemetry-parser output transform (src/dji/mod.rs):
  q_out = (0,0,1,0) * q_file * (0.5,-0.5,-0.5,0.5)   [Hamilton, wxyz]
  plus a norm-jump sign-continuity flip; timestamps from
  frame_meta_header.frame_timestamp(2) + index/len * vsync_duration.
"""
import argparse
import struct
import sys

import numpy as np

C_RIGHT = np.array([0.5, -0.5, -0.5, 0.5])
C_RIGHT_INV = np.array([0.5, 0.5, 0.5, -0.5])   # conj (unit quat)
Y180 = np.array([0.0, 0.0, 1.0, 0.0])
Y180_INV = np.array([0.0, 0.0, -1.0, 0.0])


def quat_mul(a, b):
    w1, x1, y1, z1 = a[..., 0], a[..., 1], a[..., 2], a[..., 3]
    w2, x2, y2, z2 = b[..., 0], b[..., 1], b[..., 2], b[..., 3]
    return np.stack([
        w1 * w2 - x1 * x2 - y1 * y2 - z1 * z2,
        w1 * x2 + x1 * w2 + y1 * z2 - z1 * y2,
        w1 * y2 - x1 * z2 + y1 * w2 + z1 * x2,
        w1 * z2 + x1 * y2 - y1 * x2 + z1 * w2], axis=-1)


def file_to_out(q_file):
    """Stored file quats -> telemetry-parser output frame (no sign continuity)."""
    return quat_mul(Y180[None, :], quat_mul(q_file, C_RIGHT[None, :]))


def out_to_file(q_out):
    """Inverse of file_to_out."""
    return quat_mul(Y180_INV[None, :], quat_mul(q_out, C_RIGHT_INV[None, :]))


# ------------------------------------------------------------------ MP4 boxes

def _walk_boxes(buf, start, end):
    """Yield (box_type, header_size, payload_start, payload_end)."""
    pos = start
    while pos + 8 <= end:
        size = int.from_bytes(buf[pos:pos + 4], "big")
        btype = buf[pos + 4:pos + 8]
        hdr = 8
        if size == 1:
            size = int.from_bytes(buf[pos + 8:pos + 16], "big")
            hdr = 16
        elif size == 0:
            size = end - pos
        if size < hdr:
            break
        yield btype, pos, pos + hdr, min(pos + size, end)
        pos += size


def _find_box(buf, start, end, path):
    """Return (payload_start, payload_end) of the first box at path, else None."""
    if not path:
        return start, end
    for btype, _, ps, pe in _walk_boxes(buf, start, end):
        if btype == path[0]:
            # 'meta' is a FullBox in some files; version/flags skip handled by caller
            return _find_box(buf, ps, pe, path[1:])
    return None


def read_meta_track_samples(path):
    """Return list of (abs_offset, size) for the 'meta' handler track samples."""
    with open(path, "rb") as f:
        head = f.read(16)
        # find moov by walking top-level boxes on disk
        f.seek(0, 2)
        file_size = f.tell()
        pos = 0
        moov = None
        while pos + 8 <= file_size:
            f.seek(pos)
            hdr = f.read(16)
            size = int.from_bytes(hdr[0:4], "big")
            btype = hdr[4:8]
            hsz = 8
            if size == 1:
                size = int.from_bytes(hdr[8:16], "big")
                hsz = 16
            elif size == 0:
                size = file_size - pos
            if btype == b"moov":
                f.seek(pos)
                moov = f.read(size)
                moov_payload = (hsz, size)
                break
            pos += size
        if moov is None:
            raise RuntimeError("no moov box found")

    tracks = []
    ps, pe = moov_payload
    for btype, _, tps, tpe in _walk_boxes(moov, ps, pe):
        if btype == b"trak":
            tracks.append((tps, tpe))

    for tps, tpe in tracks:
        mdia = _find_box(moov, tps, tpe, [b"mdia"])
        if not mdia:
            continue
        hdlr = _find_box(moov, mdia[0], mdia[1], [b"hdlr"])
        if not hdlr:
            continue
        handler = moov[hdlr[0] + 8:hdlr[0] + 12]  # version/flags(4) + predefined(4)
        if handler != b"meta":
            continue
        stbl = _find_box(moov, mdia[0], mdia[1], [b"minf", b"stbl"])
        if not stbl:
            continue
        boxes = {}
        for bt, _, bps, bpe in _walk_boxes(moov, stbl[0], stbl[1]):
            boxes[bt] = (bps, bpe)

        # stsz
        sps, spe = boxes[b"stsz"]
        sample_size, sample_count = struct.unpack_from(">II", moov, sps + 4)
        if sample_size:
            sizes = [sample_size] * sample_count
        else:
            sizes = list(struct.unpack_from(f">{sample_count}I", moov, sps + 12))

        # stco / co64
        if b"stco" in boxes:
            cps, _ = boxes[b"stco"]
            n = struct.unpack_from(">I", moov, cps + 4)[0]
            chunk_offsets = list(struct.unpack_from(f">{n}I", moov, cps + 8))
        else:
            cps, _ = boxes[b"co64"]
            n = struct.unpack_from(">I", moov, cps + 4)[0]
            chunk_offsets = list(struct.unpack_from(f">{n}Q", moov, cps + 8))

        # stsc
        cps, _ = boxes[b"stsc"]
        n = struct.unpack_from(">I", moov, cps + 4)[0]
        stsc = [struct.unpack_from(">III", moov, cps + 8 + 12 * i) for i in range(n)]

        samples = []
        si = 0
        for i, (first_chunk, per_chunk, _desc) in enumerate(stsc):
            last_chunk = stsc[i + 1][0] - 1 if i + 1 < len(stsc) else len(chunk_offsets)
            for c in range(first_chunk - 1, last_chunk):
                off = chunk_offsets[c]
                for _ in range(per_chunk):
                    if si >= sample_count:
                        break
                    samples.append((off, sizes[si]))
                    off += sizes[si]
                    si += 1
        if len(samples) != sample_count:
            raise RuntimeError(f"stsc walk mismatch: {len(samples)} != {sample_count}")
        return samples

    raise RuntimeError("no 'meta' handler track found")


# ------------------------------------------------------------------ protobuf

def _read_varint(buf, pos):
    val = 0
    shift = 0
    while True:
        b = buf[pos]
        val |= (b & 0x7F) << shift
        pos += 1
        if not b & 0x80:
            return val, pos
        shift += 7


def _fields(buf, start, end):
    """Yield (field_no, wire_type, payload_start, payload_end) over a message."""
    pos = start
    while pos < end:
        key, pos = _read_varint(buf, pos)
        fno, wt = key >> 3, key & 7
        if wt == 0:
            _, npos = _read_varint(buf, pos)
            yield fno, wt, pos, npos
            pos = npos
        elif wt == 1:
            yield fno, wt, pos, pos + 8
            pos += 8
        elif wt == 2:
            ln, pos = _read_varint(buf, pos)
            yield fno, wt, pos, pos + ln
            pos += ln
        elif wt == 5:
            yield fno, wt, pos, pos + 4
            pos += 4
        else:
            raise ValueError(f"unsupported wire type {wt} at {pos}")


def _sub(buf, span, field_no):
    """First length-delimited subfield span, or None."""
    for fno, wt, ps, pe in _fields(buf, span[0], span[1]):
        if fno == field_no and wt == 2:
            return ps, pe
    return None


def scan_sample(data, base_off):
    """Scan one ProductMeta sample.

    Returns (frame_timestamp_us or None, atts, att_offset) where each att is
    (offs, vals): absolute file offsets of the 4 float32 payloads (None if the
    field was omitted) and their wxyz values.
    """
    fm = _sub(data, (0, len(data)), 3)          # frame_meta
    if not fm:
        return None, [], 0.0
    frame_ts = None
    hdr = _sub(data, fm, 1)                     # frame_meta_header
    if hdr:
        for fno, wt, ps, pe in _fields(data, hdr[0], hdr[1]):
            if fno == 2 and wt == 0:
                frame_ts, _ = _read_varint(data, ps)
    imu = _sub(data, fm, 3)                     # imu_frame_meta
    if not imu:
        return frame_ts, [], 0.0
    fusion = _sub(data, imu, 2)                 # IMU_attitude_after_fusion = DeviceAttitude
    if not fusion:
        return frame_ts, [], 0.0

    atts = []
    att_offset = 0.0
    for fno, wt, ps, pe in _fields(data, fusion[0], fusion[1]):
        if fno == 4 and wt == 5:                # DeviceAttitude.offset
            att_offset = struct.unpack_from("<f", data, ps)[0]
        if fno == 3 and wt == 2:                # attitude element
            offs = [None] * 4
            vals = [0.0] * 4
            for qf, qwt, qps, qpe in _fields(data, ps, pe):
                if 1 <= qf <= 4 and qwt == 5:
                    offs[qf - 1] = base_off + qps
                    vals[qf - 1] = struct.unpack_from("<f", data, qps)[0]
            atts.append((offs, vals))
    return frame_ts, atts, att_offset


def scan_file(path, progress=True):
    """Decode every metadata sample. Returns list of per-sample dicts."""
    samples = read_meta_track_samples(path)
    result = []
    with open(path, "rb") as f:
        for i, (off, size) in enumerate(samples):
            f.seek(off)
            data = f.read(size)
            frame_ts, atts, att_offset = scan_sample(data, off)
            result.append({"index": i, "offset": off, "size": size,
                           "frame_ts": frame_ts, "atts": atts,
                           "att_offset": att_offset})
            if progress and i % 2000 == 0:
                print(f"\r  scanning sample {i}/{len(samples)}", end="", file=sys.stderr)
    if progress:
        print(f"\r  scanned {len(samples)} samples          ", file=sys.stderr)
    return result


# ------------------------------------------------------------------ commands

def cmd_scan(args):
    samples = read_meta_track_samples(args.video)
    print(f"meta track: {len(samples)} samples, "
          f"sizes {min(s for _, s in samples)}..{max(s for _, s in samples)}, "
          f"first offset {samples[0][0]:#x}")
    with open(args.video, "rb") as f:
        f.seek(samples[0][0])
        data = f.read(samples[0][1])
    marker = ("oq101" if b"oq101" in data[:64]
              else "wa530" if b"WA530" in data[:64] or b"wa530" in data[:64]
              else "wm169 (fallback)")
    print(f"proto selection: {marker}; head: {data[:32]!r}")
    for idx in (0, 1, 2, len(samples) // 2):
        off, size = samples[idx]
        with open(args.video, "rb") as f:
            f.seek(off)
            data = f.read(size)
        frame_ts, atts, att_off = scan_sample(data, off)
        print(f"sample {idx}: frame_ts={frame_ts} us, "
              f"{len(atts)} quats, offset field={att_off}")
        for offs, vals in atts[:3]:
            q_out = file_to_out(np.array([vals]))[0]
            print(f"    @{[hex(o) if o else None for o in offs]} "
                  f"file wxyz={np.round(vals, 6)} -> out {np.round(q_out, 6)}")


def cmd_verify(args):
    sys.path.insert(0, str(__import__("pathlib").Path(__file__).parent))
    from o4fix import extract_quats

    print("decoding MP4 dvtm stream ...")
    scanned = scan_file(args.video)
    n_missing = 0
    rows = []          # (ts_ms, w, x, y, z) mirroring telemetry-parser
    first_ts = None
    for s in scanned:
        if s["frame_ts"] is None or not s["atts"]:
            continue
        if first_ts is None:
            first_ts = s["frame_ts"]
        n = len(s["atts"])
        for j, (offs, vals) in enumerate(s["atts"]):
            if any(o is None for o in offs):
                n_missing += 1
            rows.append((s["frame_ts"] - first_ts, n, j, s["att_offset"], *vals))
    print(f"decoded {len(rows)} quat slots ({n_missing} with omitted fields)")

    # replicate telemetry-parser timestamp + transform
    arr = np.array(rows)
    frame_rel_ms = arr[:, 0] / 1000.0
    n_arr, j_arr, off_arr = arr[:, 1], arr[:, 2], arr[:, 3]
    q_file = arr[:, 4:8]
    # sensor_fps/fps from clip meta: both present in stream; assume equal until
    # verified (fps_ratio == 1 for O4P: fps=100, sensor_fps=100)
    sensor_fps = args.sensor_fps
    vsync_duration = 1000.0 / sensor_fps
    ts = frame_rel_ms + ((j_arr - off_arr) / n_arr) * vsync_duration

    q_out = file_to_out(q_file)
    # sign continuity like telemetry-parser (norm jump > 1.5)
    d = np.linalg.norm(np.diff(q_out, axis=0), axis=1)
    flips = np.cumsum(np.r_[0, d > 1.5]) % 2
    q_out[flips == 1] *= -1

    print("extracting reference via telemetry-parser ...")
    t_ref, q_ref, _meta = extract_quats(args.video)   # sorted, deduped, 1 kHz, normalized

    # apply extract_quats' own sort/dedupe/continuity/normalize to our stream
    order = np.argsort(ts, kind="stable")
    ts_s, qs = ts[order], q_out[order]
    change = np.any(qs[1:] != qs[:-1], axis=1)
    keep = np.r_[0, np.where(change)[0] + 1]
    ts_s, qs = ts_s[keep], qs[keep]
    fl = np.cumsum(np.r_[0, (np.sum(qs[1:] * qs[:-1], axis=1) < 0)]) % 2
    qs[fl == 1] *= -1
    qs /= np.linalg.norm(qs, axis=1, keepdims=True)

    print(f"ours: {len(ts_s)} deduped quats, ref: {len(t_ref)}")
    n = min(len(ts_s), len(t_ref))
    dt = ts_s[:n] / 1000.0 - t_ref[:n]
    dq = np.abs(qs[:n] - q_ref[:n]).max()
    dq_neg = np.abs(qs[:n] + q_ref[:n]).max()
    print(f"timestamp diff: max {np.abs(dt).max():.6f} s")
    print(f"quat value diff: max {min(dq, dq_neg):.3e} (sign-folded)")
    if len(ts_s) == len(t_ref) and np.abs(dt).max() < 1e-6 and min(dq, dq_neg) < 1e-9:
        print("VERIFY OK: decode matches telemetry-parser bit-for-bit")
    else:
        print("VERIFY MISMATCH — inspect before patching")


def _flat_reference(video):
    """telemetry-parser's flat quat stream in emission order: (ts_ms, q_out)."""
    import telemetry_parser
    tp = telemetry_parser.Parser(str(video))
    ts, qs = [], []
    for entry in tp.telemetry():
        qd = entry.get("Quaternion", {}).get("Data")
        if qd:
            for s in qd:
                ts.append(s["t"])
                v = s["v"]
                qs.append((v["w"], v["x"], v["y"], v["z"]))
    return np.asarray(ts, np.float64), np.asarray(qs, np.float64)


def _aligned_slots(video, scanned=None):
    """File-order quat slots aligned 1:1 with telemetry-parser's flat stream.

    Applies the parser's skip rules (NaN, all-zero after transform) and asserts
    the surviving values equal the parser output up to sign. Returns
    (slot_offsets [N,4], q_file [N,4], ts_ms [N], q_out_ref [N,4]).
    """
    if scanned is None:
        scanned = scan_file(video)
    offs_l, vals_l = [], []
    for s in scanned:
        for offs, vals in s["atts"]:
            v = np.array(vals)
            if np.any(np.isnan(v)):
                continue
            q_out = file_to_out(v[None, :])[0]
            if not np.any(q_out):
                continue
            if any(o is None for o in offs):
                raise RuntimeError(f"quat at {offs} has omitted fields; "
                                   "in-place patch cannot write it")
            offs_l.append(offs)
            vals_l.append(vals)
    slot_offs = np.array(offs_l, np.int64)
    q_file = np.array(vals_l, np.float64)

    ts_ref, q_ref = _flat_reference(video)
    if len(q_ref) != len(q_file):
        raise RuntimeError(f"slot count {len(q_file)} != parser stream {len(q_ref)}")
    q_out = file_to_out(q_file)
    err = np.minimum(np.abs(q_out - q_ref).max(axis=1),
                     np.abs(q_out + q_ref).max(axis=1))
    if err.max() > 1e-12:
        raise RuntimeError(f"slot/parser alignment broken: max err {err.max():g}")
    return slot_offs, q_file, ts_ref, q_ref


def _dedup_index(ts_ms, q_ref):
    """Map each flat-stream slot to its index in the sorted+deduped 1 kHz
    stream (mirrors o4fix.extract_quats). Returns (d_file [N], n_dedup)."""
    order = np.argsort(ts_ms, kind="stable")
    qs = q_ref[order]
    change = np.any(qs[1:] != qs[:-1], axis=1)
    d_sorted = np.cumsum(np.r_[0, change.astype(np.int64)])
    d_file = np.empty(len(ts_ms), np.int64)
    d_file[order] = d_sorted
    return d_file, int(d_sorted[-1]) + 1


def patch_video(video, out_path, q_target=None, verify_only=False):
    """Copy video to out_path, rewriting quat slots in place.

    q_target: (n_dedup, 4) wxyz array in telemetry-parser's output frame
    (same convention as o4fix.extract_quats), one row per deduped 1 kHz
    sample, or None for a null patch (rewrite original values; output must
    be byte-identical).
    """
    import shutil
    slot_offs, q_file, ts_ms, q_ref = _aligned_slots(video)
    d_file, n_dedup = _dedup_index(ts_ms, q_ref)

    if q_target is None:
        new_file = q_file
    else:
        if len(q_target) != n_dedup:
            raise RuntimeError(f"target has {len(q_target)} rows, "
                               f"file has {n_dedup} deduped samples")
        qt = np.asarray(q_target, np.float64)

        # deduped reference stream, processed exactly like o4fix.extract_quats:
        # rows of q_target that equal it are unchanged -> keep the original
        # file bytes for those slots (bit-exact raw base)
        order = np.argsort(ts_ms, kind="stable")
        qs = q_ref[order]
        change = np.any(qs[1:] != qs[:-1], axis=1)
        keep = np.r_[0, np.where(change)[0] + 1]
        qd_ref = qs[keep]
        fl = np.cumsum(np.r_[0, (np.sum(qd_ref[1:] * qd_ref[:-1], axis=1) < 0)]) % 2
        qd_ref[fl == 1] *= -1
        qd_ref /= np.linalg.norm(qd_ref, axis=1, keepdims=True)
        unchanged = np.all(qt == qd_ref, axis=1)
        print(f"  {int(unchanged.sum())}/{n_dedup} samples unchanged "
              f"(original bytes kept)")

        # original file value per deduped row (first slot of each row)
        qf_orig = np.empty((n_dedup, 4))
        qf_orig[d_file[::-1]] = q_file[::-1]   # first occurrence wins

        qtn = qt / np.linalg.norm(qt, axis=1, keepdims=True)
        qf = out_to_file(qtn)
        # per-row sign pinned to the previous WRITTEN value so the parser's
        # norm-jump inversion logic sees a continuous stream
        merged = np.where(unchanged[:, None], qf_orig, qf)
        prev = merged[0]
        for d in range(1, n_dedup):
            if unchanged[d]:
                prev = merged[d]
                continue
            if np.dot(merged[d], prev) < 0:
                merged[d] = -merged[d]
            prev = merged[d]
        new_file = merged[d_file]

    new_f32 = new_file.astype(np.float32)
    if np.any(np.isnan(new_f32)):
        raise RuntimeError("NaN in injected values")
    if np.any(~np.any(new_f32, axis=1)):
        raise RuntimeError("all-zero quat in injected values")

    print(f"copying {video} -> {out_path}")
    shutil.copyfile(video, out_path)
    print(f"writing {len(slot_offs)} slots ({n_dedup} unique samples) ...")
    with open(out_path, "r+b") as f:
        for offs, vals in zip(slot_offs, new_f32):
            for o, v in zip(offs, vals):
                f.seek(o)
                f.write(struct.pack("<f", v))
    return slot_offs, new_f32, ts_ms


def cmd_nullpatch(args):
    import hashlib
    patch_video(args.video, args.out, q_target=None)
    h1 = hashlib.sha256(open(args.video, "rb").read()).hexdigest()
    h2 = hashlib.sha256(open(args.out, "rb").read()).hexdigest()
    print(f"src sha256: {h1}\nout sha256: {h2}")
    print("NULLPATCH OK: byte-identical" if h1 == h2 else "NULLPATCH FAILED")


def inject_and_check(video, q_target, out):
    """Patch `video` -> `out` with q_target and gate: telemetry-parser must
    return exactly the injected values. Returns True on success."""
    slot_offs, new_f32, ts_orig = patch_video(video, out, q_target)

    print("round-trip: re-parsing patched file ...")
    ts2, q2 = _flat_reference(out)
    if len(ts2) != len(new_f32):
        print(f"ROUND-TRIP FAILED: slot count changed {len(new_f32)} -> {len(ts2)}")
        return False
    dt = np.abs(ts2 - ts_orig).max()
    q_expect = file_to_out(new_f32.astype(np.float64))
    err = np.minimum(np.abs(q_expect - q2).max(axis=1),
                     np.abs(q_expect + q2).max(axis=1)).max()
    print(f"timestamps: max diff {dt:g} ms; values: max diff {err:g} (sign-folded)")
    if dt == 0.0 and err == 0.0:
        print("ROUND-TRIP OK: parser returns exactly the injected values")
        return True
    print("ROUND-TRIP FAILED")
    return False


def cmd_inject(args):
    data = np.load(args.data)
    inject_and_check(args.video, data["q"], args.out)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    sub = p.add_subparsers(dest="cmd", required=True)
    ps = sub.add_parser("scan", help="probe structure of first metadata sample")
    ps.add_argument("video")
    ps.set_defaults(func=cmd_scan)
    pv = sub.add_parser("verify", help="full decode, compare with telemetry-parser")
    pv.add_argument("video")
    pv.add_argument("--sensor-fps", type=float, default=100.0)
    pv.set_defaults(func=cmd_verify)
    pn = sub.add_parser("nullpatch", help="rewrite original values; must be byte-identical")
    pn.add_argument("video")
    pn.add_argument("out")
    pn.set_defaults(func=cmd_nullpatch)
    pi = sub.add_parser("inject", help="write target quats from .npz (key 'q', wxyz, 1 kHz deduped)")
    pi.add_argument("video")
    pi.add_argument("data")
    pi.add_argument("out")
    pi.set_defaults(func=cmd_inject)
    args = p.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
