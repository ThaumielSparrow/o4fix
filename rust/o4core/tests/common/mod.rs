//! Shared test helpers. Lives in a subdirectory so Cargo does NOT compile
//! it as its own test binary; each tests/*.rs declares `mod common;`.
#![allow(dead_code)] // each test binary compiles its own copy and uses a subset
use ndarray::{Array1, Array2};
use ndarray_npy::NpzReader;
use std::fs::File;

pub fn repo(p: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../")
        .join(p)
}
pub fn npz(name: &str) -> NpzReader<File> {
    NpzReader::new(File::open(repo(&format!("goldens/{name}"))).unwrap()).unwrap()
}
pub fn npz_extract() -> (Vec<f64>, Vec<[f64; 4]>) {
    let mut z = npz("extract.npz");
    let t: Array1<f64> = z.by_name("t").unwrap();
    let q: Array2<f64> = z.by_name("q").unwrap();
    let t_vec = t.to_vec();
    let q_vec: Vec<[f64; 4]> = (0..q.nrows())
        .map(|i| [q[[i, 0]], q[[i, 1]], q[[i, 2]], q[[i, 3]]])
        .collect();
    (t_vec, q_vec)
}
pub fn fx(name: &str) -> serde_json::Value {
    let p = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}
pub fn col(v: &serde_json::Value) -> Vec<f64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap())
        .collect()
}
pub fn rows3(v: &serde_json::Value) -> Vec<[f64; 3]> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|r| core::array::from_fn(|i| r[i].as_f64().unwrap()))
        .collect()
}
pub fn rows4(v: &serde_json::Value) -> Vec<[f64; 4]> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|r| core::array::from_fn(|i| r[i].as_f64().unwrap()))
        .collect()
}
pub fn close(a: &[f64], b: &[f64], tol: f64, what: &str) {
    assert_eq!(a.len(), b.len(), "{what} len");
    for i in 0..a.len() {
        assert!(
            (a[i] - b[i]).abs() <= tol * b[i].abs().max(1.0),
            "{what}[{i}]: {} vs {}",
            a[i],
            b[i]
        );
    }
}
pub fn assert_close<const N: usize>(a: &[[f64; N]], b: &[[f64; N]], tol: f64, what: &str) {
    assert_eq!(a.len(), b.len(), "{what} len");
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        for k in 0..N {
            assert!(
                (x[k] - y[k]).abs() <= tol,
                "{what}[{i}][{k}]: {} vs {}",
                x[k],
                y[k]
            );
        }
    }
}
