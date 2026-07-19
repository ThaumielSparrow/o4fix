#![allow(dead_code)]
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
