#[path = "helpers.rs"] mod helpers;
use helpers::*;
use o4core::quat::*;

#[test]
fn quat_ops_match_python() {
    let f = fx("quat_ops.json");
    let (qa, qb) = (rows4(&f["qa"]), rows4(&f["qb"]));
    let mul: Vec<[f64; 4]> = qa.iter().zip(&qb).map(|(a, b)| qmul(*a, *b)).collect();
    assert_close(&mul, &rows4(&f["mul"]), 1e-14, "mul");
    let conj: Vec<[f64; 4]> = qa.iter().map(|a| qconj(*a)).collect();
    assert_close(&conj, &rows4(&f["conj"]), 0.0, "conj");
    let ex: Vec<[f64; 4]> = rows3(&f["v"]).iter().map(|v| qexp(*v)).collect();
    assert_close(&ex, &rows4(&f["exp"]), 1e-14, "exp");
    let lg: Vec<[f64; 3]> = qa.iter().map(|a| qlog(*a)).collect();
    assert_close(&lg, &rows3(&f["log"]), 1e-13, "log");
    let w = col(&f["w"]);
    let sl: Vec<[f64; 4]> = (0..qa.len()).map(|i| slerp(qa[i], qb[i], w[i])).collect();
    assert_close(&sl, &rows4(&f["slerp"]), 1e-13, "slerp");
    let sx = col(&f["smoothstep_x"]);
    let ss: Vec<f64> = sx.iter().map(|x| smoothstep(*x)).collect();
    close(&ss, &col(&f["smoothstep"]), 1e-15, "smoothstep");
    let (tm, om) = quats_to_rates(&col(&f["rates_t"]), &rows4(&f["rates_q"]));
    close(&tm, &col(&f["rates_tm"]), 1e-15, "rates_tm");
    assert_close(&om, &rows3(&f["rates_om"]), 1e-11, "rates");
}
