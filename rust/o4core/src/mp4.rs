//! In-place O4P quat patching. Ports mp4patch.py (read-only reference).
//! Layout notes at mp4patch.py:1-17; wire format: protobuf inside 'meta'
//! handler track samples; quats = float32 LE fixed32 fields 1..4.
use crate::error::O4Error;
use crate::quat::{qmul, qnorm};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

pub const C_RIGHT: [f64; 4] = [0.5, -0.5, -0.5, 0.5];
pub const C_RIGHT_INV: [f64; 4] = [0.5, 0.5, 0.5, -0.5];
pub const Y180: [f64; 4] = [0.0, 0.0, 1.0, 0.0];
pub const Y180_INV: [f64; 4] = [0.0, 0.0, -1.0, 0.0];

/// Stored file quat -> telemetry-parser output frame (no sign continuity).
pub fn file_to_out(q: [f64; 4]) -> [f64; 4] {
    qmul(Y180, qmul(q, C_RIGHT))
}
pub fn out_to_file(q: [f64; 4]) -> [f64; 4] {
    qmul(Y180_INV, qmul(q, C_RIGHT_INV))
}

fn be32(b: &[u8], p: usize) -> u64 {
    u32::from_be_bytes(b[p..p + 4].try_into().unwrap()) as u64
}
fn be64(b: &[u8], p: usize) -> u64 {
    u64::from_be_bytes(b[p..p + 8].try_into().unwrap())
}

/// Yield (type, payload_start, payload_end) of boxes in buf[start..end].
fn walk_boxes(buf: &[u8], start: usize, end: usize) -> Vec<([u8; 4], usize, usize)> {
    let mut out = Vec::new();
    let mut pos = start;
    while pos + 8 <= end {
        let mut size = be32(buf, pos) as usize;
        let btype: [u8; 4] = buf[pos + 4..pos + 8].try_into().unwrap();
        let mut hdr = 8;
        if size == 1 {
            size = be64(buf, pos + 8) as usize;
            hdr = 16;
        } else if size == 0 {
            size = end - pos;
        }
        if size < hdr {
            break;
        }
        out.push((btype, pos + hdr, (pos + size).min(end)));
        pos += size;
    }
    out
}

fn find_box(buf: &[u8], start: usize, end: usize, path: &[&[u8; 4]]) -> Option<(usize, usize)> {
    if path.is_empty() {
        return Some((start, end));
    }
    for (t, ps, pe) in walk_boxes(buf, start, end) {
        if &t == path[0] {
            return find_box(buf, ps, pe, &path[1..]);
        }
    }
    None
}

/// (abs_offset, size) of every sample in the 'meta'-handler track.
/// Ports mp4patch.read_meta_track_samples (mp4patch.py:81-172).
pub fn read_meta_track_samples(path: &Path) -> Result<Vec<(u64, u32)>, O4Error> {
    let mut f = std::fs::File::open(path)?;
    let flen = f.metadata()?.len();
    // find + load moov
    let mut pos = 0u64;
    let mut moov: Option<Vec<u8>> = None;
    let mut moov_payload = (0usize, 0usize);
    let mut hdr = [0u8; 16];
    while pos + 8 <= flen {
        f.seek(SeekFrom::Start(pos))?;
        f.read_exact(&mut hdr[..8])?;
        let mut size = u32::from_be_bytes(hdr[0..4].try_into().unwrap()) as u64;
        let is_moov = &hdr[4..8] == b"moov";
        let mut hsz = 8usize;
        if size == 1 {
            f.read_exact(&mut hdr[8..16])?;
            size = u64::from_be_bytes(hdr[8..16].try_into().unwrap());
            hsz = 16;
        } else if size == 0 {
            size = flen - pos;
        }
        if is_moov {
            f.seek(SeekFrom::Start(pos))?;
            let mut buf = vec![0u8; size as usize];
            f.read_exact(&mut buf)?;
            moov_payload = (hsz, size as usize);
            moov = Some(buf);
            break;
        }
        pos += size;
    }
    let moov = moov.ok_or_else(|| O4Error::Mp4("no moov box".into()))?;
    let (ps, pe) = moov_payload;

    for (t, tps, tpe) in walk_boxes(&moov, ps, pe) {
        if &t != b"trak" {
            continue;
        }
        let Some(mdia) = find_box(&moov, tps, tpe, &[b"mdia"]) else {
            continue;
        };
        let Some(hdlr) = find_box(&moov, mdia.0, mdia.1, &[b"hdlr"]) else {
            continue;
        };
        let Some(h) = moov.get(hdlr.0 + 8..hdlr.0 + 12) else {
            continue;
        };
        if h != b"meta" {
            continue;
        }
        let Some(stbl) = find_box(&moov, mdia.0, mdia.1, &[b"minf", b"stbl"]) else {
            continue;
        };
        let mut boxes = std::collections::HashMap::new();
        for (bt, bps, bpe) in walk_boxes(&moov, stbl.0, stbl.1) {
            boxes.insert(bt, (bps, bpe));
        }
        // stsz
        let (sps, _) = *boxes
            .get(b"stsz")
            .ok_or_else(|| O4Error::Mp4("meta track missing stsz".into()))?;
        let sample_size = be32(&moov, sps + 4) as u32;
        let count = be32(&moov, sps + 8) as usize;
        let sizes: Vec<u32> = if sample_size != 0 {
            vec![sample_size; count]
        } else {
            (0..count)
                .map(|i| be32(&moov, sps + 12 + 4 * i) as u32)
                .collect()
        };
        // stco / co64
        let chunk_offsets: Vec<u64> = if let Some(&(cps, _)) = boxes.get(b"stco") {
            let n = be32(&moov, cps + 4) as usize;
            (0..n).map(|i| be32(&moov, cps + 8 + 4 * i)).collect()
        } else {
            let (cps, _) = *boxes
                .get(b"co64")
                .ok_or_else(|| O4Error::Mp4("meta track missing stco/co64".into()))?;
            let n = be32(&moov, cps + 4) as usize;
            (0..n).map(|i| be64(&moov, cps + 8 + 8 * i)).collect()
        };
        // stsc
        let (cps, _) = *boxes
            .get(b"stsc")
            .ok_or_else(|| O4Error::Mp4("meta track missing stsc".into()))?;
        let n = be32(&moov, cps + 4) as usize;
        let stsc: Vec<(u64, u64)> = (0..n)
            .map(|i| {
                (
                    be32(&moov, cps + 8 + 12 * i),
                    be32(&moov, cps + 12 + 12 * i),
                )
            })
            .collect();
        let mut samples = Vec::with_capacity(count);
        let mut si = 0usize;
        for (i, &(first_chunk, per_chunk)) in stsc.iter().enumerate() {
            let last_chunk = if i + 1 < stsc.len() {
                stsc[i + 1].0 - 1
            } else {
                chunk_offsets.len() as u64
            };
            for c in (first_chunk - 1)..last_chunk {
                let mut off = chunk_offsets[c as usize];
                for _ in 0..per_chunk {
                    if si >= count {
                        break;
                    }
                    samples.push((off, sizes[si]));
                    off += sizes[si] as u64;
                    si += 1;
                }
            }
        }
        if samples.len() != count {
            return Err(O4Error::Mp4(format!(
                "stsc walk mismatch: {} != {count}",
                samples.len()
            )));
        }
        return Ok(samples);
    }
    Err(O4Error::Mp4("no 'meta' handler track".into()))
}

// ---------------- protobuf field scan (wire types 0/1/2/5 only)

fn read_varint(buf: &[u8], mut pos: usize) -> (u64, usize) {
    let (mut val, mut shift) = (0u64, 0u32);
    loop {
        let b = buf[pos];
        val |= ((b & 0x7f) as u64) << shift;
        pos += 1;
        if b & 0x80 == 0 {
            return (val, pos);
        }
        shift += 7;
    }
}

/// (field_no, wire_type, payload_start, payload_end) over buf[start..end].
fn fields(buf: &[u8], start: usize, end: usize) -> Vec<(u64, u8, usize, usize)> {
    let mut out = Vec::new();
    let mut pos = start;
    while pos < end {
        let (key, np) = read_varint(buf, pos);
        pos = np;
        let (fno, wt) = (key >> 3, (key & 7) as u8);
        match wt {
            0 => {
                let (_, np2) = read_varint(buf, pos);
                out.push((fno, 0, pos, np2));
                pos = np2;
            }
            1 => {
                out.push((fno, 1, pos, pos + 8));
                pos += 8;
            }
            2 => {
                let (ln, np2) = read_varint(buf, pos);
                out.push((fno, 2, np2, np2 + ln as usize));
                pos = np2 + ln as usize;
            }
            5 => {
                out.push((fno, 5, pos, pos + 4));
                pos += 4;
            }
            w => panic!("unsupported wire type {w} at {pos}"),
        }
    }
    out
}

fn sub(buf: &[u8], span: (usize, usize), field_no: u64) -> Option<(usize, usize)> {
    fields(buf, span.0, span.1)
        .into_iter()
        .find(|&(f, wt, _, _)| f == field_no && wt == 2)
        .map(|(_, _, ps, pe)| (ps, pe))
}

/// (absolute file offsets of the 4 float payloads, wxyz f32 values)
type AttEntry = ([Option<u64>; 4], [f32; 4]);

pub struct ScanSample {
    pub offset: u64,
    pub size: u32,
    pub frame_ts: Option<u64>,
    pub atts: Vec<AttEntry>,
    pub att_offset: f32,
}

/// Ports mp4patch.scan_sample (mp4patch.py:221-257).
fn scan_sample(data: &[u8], base_off: u64) -> (Option<u64>, Vec<AttEntry>, f32) {
    let Some(fm) = sub(data, (0, data.len()), 3) else {
        return (None, vec![], 0.0);
    };
    let mut frame_ts = None;
    if let Some(hdr) = sub(data, fm, 1) {
        for (fno, wt, ps, _) in fields(data, hdr.0, hdr.1) {
            if fno == 2 && wt == 0 {
                frame_ts = Some(read_varint(data, ps).0);
            }
        }
    }
    let Some(imu) = sub(data, fm, 3) else {
        return (frame_ts, vec![], 0.0);
    };
    let Some(fusion) = sub(data, imu, 2) else {
        return (frame_ts, vec![], 0.0);
    };
    let mut atts = Vec::new();
    let mut att_offset = 0.0f32;
    for (fno, wt, ps, pe) in fields(data, fusion.0, fusion.1) {
        if fno == 4 && wt == 5 {
            att_offset = f32::from_le_bytes(data[ps..ps + 4].try_into().unwrap());
        }
        if fno == 3 && wt == 2 {
            let mut offs = [None; 4];
            let mut vals = [0.0f32; 4];
            for (qf, qwt, qps, _) in fields(data, ps, pe) {
                if (1..=4).contains(&qf) && qwt == 5 {
                    offs[(qf - 1) as usize] = Some(base_off + qps as u64);
                    vals[(qf - 1) as usize] =
                        f32::from_le_bytes(data[qps..qps + 4].try_into().unwrap());
                }
            }
            atts.push((offs, vals));
        }
    }
    (frame_ts, atts, att_offset)
}

pub fn scan_file(path: &Path) -> Result<Vec<ScanSample>, O4Error> {
    let samples = read_meta_track_samples(path)?;
    let mut f = std::fs::File::open(path)?;
    let mut out = Vec::with_capacity(samples.len());
    for (off, size) in samples {
        f.seek(SeekFrom::Start(off))?;
        let mut data = vec![0u8; size as usize];
        f.read_exact(&mut data)?;
        let (frame_ts, atts, att_offset) = scan_sample(&data, off);
        out.push(ScanSample {
            offset: off,
            size,
            frame_ts,
            atts,
            att_offset,
        });
    }
    Ok(out)
}

pub struct SlotTable {
    pub offs: Vec<[u64; 4]>,
    pub q_file: Vec<[f64; 4]>,
    pub ts_ms: Vec<f64>,
    pub q_ref: Vec<[f64; 4]>,
    pub d_file: Vec<usize>,
    pub n_dedup: usize,
}

/// Ports mp4patch._aligned_slots + _dedup_index (mp4patch.py:386-432).
pub fn aligned_slots(video: &Path) -> Result<SlotTable, O4Error> {
    let scanned = scan_file(video)?;
    let mut offs = Vec::new();
    let mut q_file = Vec::new();
    for s in &scanned {
        for (o, v) in &s.atts {
            if v.iter().any(|x| x.is_nan()) {
                continue;
            }
            let q64: [f64; 4] = core::array::from_fn(|i| v[i] as f64);
            if file_to_out(q64) == [0.0; 4] {
                continue;
            }
            if o.iter().any(|x| x.is_none()) {
                return Err(O4Error::Mp4(
                    "quat with omitted fields; cannot patch in place".into(),
                ));
            }
            offs.push([o[0].unwrap(), o[1].unwrap(), o[2].unwrap(), o[3].unwrap()]);
            q_file.push(q64);
        }
    }
    let (ts_ms, q_ref) = crate::telemetry::flat_quat_stream(video)?;
    if q_ref.len() != q_file.len() {
        return Err(O4Error::Mp4(format!(
            "slot count {} != parser stream {}",
            q_file.len(),
            q_ref.len()
        )));
    }
    for i in 0..q_file.len() {
        let qo = file_to_out(q_file[i]);
        let e1: f64 = (0..4)
            .map(|k| (qo[k] - q_ref[i][k]).abs())
            .fold(0.0, f64::max);
        let e2: f64 = (0..4)
            .map(|k| (qo[k] + q_ref[i][k]).abs())
            .fold(0.0, f64::max);
        if e1.min(e2) > 1e-12 {
            return Err(O4Error::Mp4(format!("slot/parser alignment broken at {i}")));
        }
    }
    // dedup index: map each flat slot to its sorted+deduped 1 kHz row
    let mut order: Vec<usize> = (0..ts_ms.len()).collect();
    order.sort_by(|&a, &b| ts_ms[a].total_cmp(&ts_ms[b]));
    let mut d_sorted = vec![0usize; order.len()];
    for i in 1..order.len() {
        let changed = q_ref[order[i]] != q_ref[order[i - 1]];
        d_sorted[i] = d_sorted[i - 1] + usize::from(changed);
    }
    let mut d_file = vec![0usize; order.len()];
    for (i, &oi) in order.iter().enumerate() {
        d_file[oi] = d_sorted[i];
    }
    let n_dedup = d_sorted[order.len() - 1] + 1;
    Ok(SlotTable {
        offs,
        q_file,
        ts_ms,
        q_ref,
        d_file,
        n_dedup,
    })
}

/// Deduped reference stream processed exactly like o4fix.extract_quats
/// (sorted, deduped, sign-continuous, normalized) — the patching base.
///
/// LOCKSTEP: this must stay in exact agreement with
/// `telemetry::extract_quats`'s sort/dedupe/flip/normalize contract — the
/// e2e test's clean-zone EXACT-0.0 gate (`tests/e2e.rs`) enforces that
/// agreement at runtime. Any edit to one function's sort/dedupe/sign-flip/
/// normalize logic must be mirrored in the other.
pub fn deduped_reference(st: &SlotTable) -> Vec<[f64; 4]> {
    let mut order: Vec<usize> = (0..st.ts_ms.len()).collect();
    order.sort_by(|&a, &b| st.ts_ms[a].total_cmp(&st.ts_ms[b]));
    let mut qd: Vec<[f64; 4]> = Vec::with_capacity(st.n_dedup);
    for (i, &oi) in order.iter().enumerate() {
        if i == 0 || st.q_ref[oi] != st.q_ref[order[i - 1]] {
            qd.push(st.q_ref[oi]);
        }
    }
    // hemisphere continuity on RAW neighbors, then flips, then normalize
    let mut flips = vec![false; qd.len()];
    let mut cum = false;
    for i in 1..qd.len() {
        let dot: f64 = (0..4).map(|k| qd[i][k] * qd[i - 1][k]).sum();
        if dot < 0.0 {
            cum = !cum;
        }
        flips[i] = cum;
    }
    for i in 0..qd.len() {
        if flips[i] {
            for c in qd[i].iter_mut() {
                *c = -*c;
            }
        }
        qd[i] = qnorm(qd[i]);
    }
    qd
}

pub struct PatchReport {
    pub slots: usize,
    pub unchanged: usize,
    pub new_f32: Vec<[f32; 4]>,
    pub ts_ms: Vec<f64>,
}

/// Ports mp4patch.patch_video (mp4patch.py:435-503). q_target rows are in
/// telemetry-parser output frame, one per deduped 1 kHz sample; None = null
/// patch (must produce a byte-identical file).
pub fn patch_video(
    video: &Path,
    out: &Path,
    q_target: Option<&[[f64; 4]]>,
) -> Result<PatchReport, O4Error> {
    let st = aligned_slots(video)?;
    let mut unchanged_count = st.n_dedup;
    let new_file: Vec<[f64; 4]> = match q_target {
        None => st.q_file.clone(),
        Some(qt) => {
            if qt.len() != st.n_dedup {
                return Err(O4Error::Mp4(format!(
                    "target has {} rows, file has {} deduped samples",
                    qt.len(),
                    st.n_dedup
                )));
            }
            let qd_ref = deduped_reference(&st);
            let unchanged: Vec<bool> = (0..st.n_dedup).map(|d| qt[d] == qd_ref[d]).collect();
            unchanged_count = unchanged.iter().filter(|&&u| u).count();
            // original file value per deduped row: first slot occurrence wins
            let mut qf_orig = vec![[0.0f64; 4]; st.n_dedup];
            for i in (0..st.q_file.len()).rev() {
                qf_orig[st.d_file[i]] = st.q_file[i];
            }
            let mut merged = vec![[0.0f64; 4]; st.n_dedup];
            for d in 0..st.n_dedup {
                merged[d] = if unchanged[d] {
                    qf_orig[d]
                } else {
                    out_to_file(qnorm(qt[d]))
                };
            }
            // per-row sign pinned to previous WRITTEN value (mp4patch.py:478-486)
            let mut prev = merged[0];
            for d in 1..st.n_dedup {
                if unchanged[d] {
                    prev = merged[d];
                    continue;
                }
                let dot: f64 = (0..4).map(|k| merged[d][k] * prev[k]).sum();
                if dot < 0.0 {
                    for c in merged[d].iter_mut() {
                        *c = -*c;
                    }
                }
                prev = merged[d];
            }
            st.d_file.iter().map(|&d| merged[d]).collect()
        }
    };
    let new_f32: Vec<[f32; 4]> = new_file
        .iter()
        .map(|q| core::array::from_fn(|k| q[k] as f32))
        .collect();
    for q in &new_f32 {
        if q.iter().any(|x| x.is_nan()) {
            return Err(O4Error::Mp4("NaN in injected values".into()));
        }
        if *q == [0.0f32; 4] {
            return Err(O4Error::Mp4("all-zero quat in injected values".into()));
        }
    }
    std::fs::copy(video, out)?;
    let mut f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(out)?;
    // NOTE: slots are written in file order; unchanged rows re-write their
    // original f32 bytes (f64 came from f32, so the cast is lossless).
    let mut ordered: Vec<(u64, f32)> = Vec::with_capacity(st.offs.len() * 4);
    for (o4, v4) in st.offs.iter().zip(&new_f32) {
        for k in 0..4 {
            ordered.push((o4[k], v4[k]));
        }
    }
    ordered.sort_by_key(|&(o, _)| o);
    for (o, v) in ordered {
        f.seek(SeekFrom::Start(o))?;
        f.write_all(&v.to_le_bytes())?;
    }
    f.flush()?;
    Ok(PatchReport {
        slots: st.offs.len(),
        unchanged: unchanged_count,
        new_f32,
        ts_ms: st.ts_ms,
    })
}

/// Ports mp4patch.inject_and_check (mp4patch.py:515-534): the shipping gate.
pub fn inject_and_check(
    video: &Path,
    out: &Path,
    q_target: &[[f64; 4]],
    log: &dyn Fn(&str),
) -> Result<bool, O4Error> {
    let rep = patch_video(video, out, Some(q_target))?;
    log(&format!(
        "  {}/{} samples unchanged (original bytes kept)",
        rep.unchanged,
        q_target.len()
    ));
    let (ts2, q2) = crate::telemetry::flat_quat_stream(out)?;
    if ts2.len() != rep.new_f32.len() {
        log("ROUND-TRIP FAILED: slot count changed");
        return Ok(false);
    }
    let mut dt_max = 0.0f64;
    let mut err_max = 0.0f64;
    for i in 0..ts2.len() {
        dt_max = dt_max.max((ts2[i] - rep.ts_ms[i]).abs());
        let qe = file_to_out(core::array::from_fn(|k| rep.new_f32[i][k] as f64));
        let e1: f64 = (0..4).map(|k| (qe[k] - q2[i][k]).abs()).fold(0.0, f64::max);
        let e2: f64 = (0..4).map(|k| (qe[k] + q2[i][k]).abs()).fold(0.0, f64::max);
        err_max = err_max.max(e1.min(e2));
    }
    log(&format!(
        "timestamps: max diff {dt_max} ms; values: max diff {err_max} (sign-folded)"
    ));
    Ok(dt_max == 0.0 && err_max == 0.0)
}
