#[path = "golden_telemetry.rs"] mod gt;
use ndarray::Array2;

#[test]
#[ignore]
fn slot_scan_matches_python() {
    let video = gt::repo("sample_vids/DJI_20260711124046_0021_D.MP4");
    let scanned = o4core::mp4::scan_file(&video).unwrap();
    // flatten to slots exactly like mp4patch._aligned_slots' pre-filter form
    let mut offs = Vec::new();
    let mut vals = Vec::new();
    for s in &scanned {
        for (o, v) in &s.atts {
            if v.iter().any(|x| x.is_nan()) { continue; }
            let q64: [f64; 4] = core::array::from_fn(|i| v[i] as f64);
            let qo = o4core::mp4::file_to_out(q64);
            if qo == [0.0; 4] { continue; }
            assert!(o.iter().all(|x| x.is_some()), "omitted field in scanned quat");
            offs.push([o[0].unwrap(), o[1].unwrap(), o[2].unwrap(), o[3].unwrap()]);
            vals.push(q64);
        }
    }
    let mut z = gt::npz("slots.npz");
    let offs_g: Array2<i64> = z.by_name("offs").unwrap();
    let qf_g: Array2<f64> = z.by_name("q_file").unwrap();
    assert_eq!(offs.len(), offs_g.nrows(), "slot count");
    for i in 0..offs.len() {
        for k in 0..4 {
            assert_eq!(offs[i][k], offs_g[[i, k]] as u64, "off[{i}][{k}]");
            assert_eq!(vals[i][k], qf_g[[i, k]], "val[{i}][{k}]"); // exact f32->f64
        }
    }
}
