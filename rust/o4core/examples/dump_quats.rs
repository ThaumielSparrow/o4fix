//! Dump telemetry-parser's flat quaternion stream (GroupId::Quaternion /
//! TagId::Data) as CSV: `t_ms,w,x,y,z`, one row per raw emission-order slot
//! (no sorting, no deduplication — this must match the Python twin
//! `tools/dump_quats_py.py`, which reads the same stream via
//! `mp4patch._flat_reference()` / the `telemetry_parser` Python bindings).
//!
//! Usage: `cargo run --example dump_quats -- VIDEO.MP4 > out.csv`

use std::fs::File;
use std::io::{BufWriter, Write};
use telemetry_parser::tags_impl::*;
use telemetry_parser::Input;

fn main() {
    let path = std::env::args().nth(1).expect("usage: dump_quats VIDEO.MP4");
    let mut f = File::open(&path).expect("open video");
    let size = f.metadata().expect("stat video").len() as usize;

    let input = Input::from_stream(
        &mut f,
        size,
        &path,
        |_| (),
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )
    .expect("parse video");

    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());

    for sample in input.samples.as_ref().expect("no samples parsed") {
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
            writeln!(
                out,
                "{:.17e},{:.17e},{:.17e},{:.17e},{:.17e}",
                q.t, q.v.w, q.v.x, q.v.y, q.v.z
            )
            .expect("write stdout");
        }
    }
}
