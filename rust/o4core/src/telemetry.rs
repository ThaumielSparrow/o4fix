//! extract_quats / flat stream via the telemetry-parser crate.
//! Ports o4fix.extract_quats (o4fix.py:55-94) and mp4patch._flat_reference.
use crate::error::O4Error;
use std::fs::File;
use std::path::Path;
use std::sync::{atomic::AtomicBool, Arc};
use telemetry_parser::tags_impl::*;
use telemetry_parser::Input;

#[derive(Clone, Debug, Default)]
pub struct Meta {
    pub camera: String,
    pub model: String,
    pub camera_matrix: Option<[[f64; 3]; 3]>,
    pub distortion: Option<[f64; 4]>,
    pub calib_w: Option<f64>,
    pub calib_h: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct Telemetry {
    pub t: Vec<f64>,      // seconds
    pub q: Vec<[f64; 4]>, // wxyz, sorted/deduped/continuous/normalized
    pub meta: Meta,
}

fn open_input(path: &Path) -> Result<Input, O4Error> {
    let mut f = File::open(path)?;
    let size = f.metadata()?.len() as usize;
    let input = Input::from_stream(
        &mut f,
        size,
        path,
        |_progress: f64| (),
        Arc::new(AtomicBool::new(false)),
    )
    .map_err(|e| O4Error::Telemetry(e.to_string()))?;
    Ok(input)
}

/// Emission-order stream: (t_ms, q wxyz). Exact mirror of the parser output
/// (verified byte-for-byte against Python in Task 3).
pub fn flat_quat_stream(path: &Path) -> Result<(Vec<f64>, Vec<[f64; 4]>), O4Error> {
    let input = open_input(path)?;
    let mut ts = Vec::new();
    let mut qs = Vec::new();
    if let Some(samples) = input.samples.as_ref() {
        for sample in samples {
            let Some(map) = sample.tag_map.as_ref() else {
                continue;
            };
            let Some(group) = map.get(&GroupId::Quaternion) else {
                continue;
            };
            let Some(desc) = group.get(&TagId::Data) else {
                continue;
            };
            let TagValue::Vec_TimeQuaternion_f64(vt) = &desc.value else {
                continue;
            };
            for q in vt.get() {
                ts.push(q.t);
                qs.push([q.v.w, q.v.x, q.v.y, q.v.z]);
            }
        }
    }
    Ok((ts, qs))
}

/// Camera/model + fisheye lens calibration (GroupId::Lens / TagId::Data JSON:
/// `fisheye_params { camera_matrix 3x3, distortion_coeffs [4] }` and
/// `calib_dimension {w,h}`), taken from the first sample that carries it
/// (the crate only attaches the Lens tag to `samples[0]`, see dji/mod.rs).
pub fn read_meta(path: &Path) -> Result<Meta, O4Error> {
    let input = open_input(path)?;
    let camera = input.camera_type();
    let model = input.camera_model().cloned().unwrap_or_default();

    let mut camera_matrix = None;
    let mut distortion = None;
    let mut calib_w = None;
    let mut calib_h = None;

    if let Some(samples) = input.samples.as_ref() {
        'outer: for sample in samples {
            let Some(map) = sample.tag_map.as_ref() else {
                continue;
            };
            let Some(group) = map.get(&GroupId::Lens) else {
                continue;
            };
            let Some(desc) = group.get(&TagId::Data) else {
                continue;
            };
            let TagValue::Json(vt) = &desc.value else {
                continue;
            };
            let v = vt.get();

            if let Some(fp) = v.get("fisheye_params") {
                if let Some(rows) = fp.get("camera_matrix").and_then(|x| x.as_array()) {
                    let mut m = [[0.0f64; 3]; 3];
                    let mut ok = rows.len() >= 3;
                    for (i, row) in rows.iter().enumerate().take(3) {
                        let Some(cols) = row.as_array() else {
                            ok = false;
                            break;
                        };
                        if cols.len() < 3 {
                            ok = false;
                            break;
                        }
                        for (j, val) in cols.iter().enumerate().take(3) {
                            match val.as_f64() {
                                Some(f) => m[i][j] = f,
                                None => {
                                    ok = false;
                                    break;
                                }
                            }
                        }
                    }
                    if ok {
                        camera_matrix = Some(m);
                    }
                }
                if let Some(arr) = fp.get("distortion_coeffs").and_then(|x| x.as_array()) {
                    if arr.len() >= 4 {
                        let mut d = [0.0f64; 4];
                        let mut ok = true;
                        for (i, val) in arr.iter().enumerate().take(4) {
                            match val.as_f64() {
                                Some(f) => d[i] = f,
                                None => {
                                    ok = false;
                                    break;
                                }
                            }
                        }
                        if ok {
                            distortion = Some(d);
                        }
                    }
                }
            }
            if let Some(cd) = v.get("calib_dimension") {
                calib_w = cd.get("w").and_then(|x| x.as_f64());
                calib_h = cd.get("h").and_then(|x| x.as_f64());
            }
            break 'outer; // first sample carrying Lens data is enough
        }
    }

    Ok(Meta {
        camera,
        model,
        camera_matrix,
        distortion,
        calib_w,
        calib_h,
    })
}

pub fn extract_quats(path: &Path) -> Result<Telemetry, O4Error> {
    let (ts_ms, qs) = flat_quat_stream(path)?;
    let meta = read_meta(path)?;
    if ts_ms.is_empty() {
        return Err(O4Error::NoTelemetry(format!(
            "camera={}, model={}",
            meta.camera, meta.model
        )));
    }
    // stable argsort by t
    let mut idx: Vec<usize> = (0..ts_ms.len()).collect();
    idx.sort_by(|&a, &b| ts_ms[a].total_cmp(&ts_ms[b])); // stable sort
    let ts: Vec<f64> = idx.iter().map(|&i| ts_ms[i]).collect();
    let qs_s: Vec<[f64; 4]> = idx.iter().map(|&i| qs[i]).collect();

    // dedupe consecutive equal quats (2 kHz stream duplicates each 1 kHz value)
    let mut t_out = vec![ts[0]];
    let mut q_raw = vec![qs_s[0]];
    for i in 1..qs_s.len() {
        if qs_s[i] != qs_s[i - 1] {
            t_out.push(ts[i]);
            q_raw.push(qs_s[i]);
        }
    }

    // Hemisphere continuity. Porting caution (session-1 trap): numpy computes
    // `flips = cumsum(dot(q[i], q[i-1]) < 0) % 2` on the deduped, UNFLIPPED
    // stream -- every dot product uses the ORIGINAL neighbor values, never a
    // previously-negated one. So: compute the dot-sign array on raw
    // neighbors first, cumulative-XOR into a flip flag per sample, THEN
    // negate flagged rows into a separate output buffer, THEN normalize.
    // (A sequential loop that compares against the running *flipped* q_out[i-1]
    // is a different, wrong recurrence and silently diverges from numpy.)
    let n = q_raw.len();
    let mut q_out = q_raw.clone();
    let mut flip = false;
    for i in 1..n {
        let dot: f64 = (0..4).map(|k| q_raw[i][k] * q_raw[i - 1][k]).sum();
        if dot < 0.0 {
            flip = !flip;
        }
        if flip {
            for c in q_out[i].iter_mut() {
                *c = -*c;
            }
        }
    }
    for q in q_out.iter_mut() {
        *q = crate::quat::qnorm(*q);
    }
    for t in t_out.iter_mut() {
        *t /= 1000.0;
    }
    Ok(Telemetry {
        t: t_out,
        q: q_out,
        meta,
    })
}
