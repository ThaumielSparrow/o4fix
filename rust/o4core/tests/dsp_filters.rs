#[path = "helpers.rs"] mod helpers;
use helpers::*;
use o4core::dsp::*;

#[test]
fn filters_match_scipy() {
    let f = fx("filters.json");
    let x = col(&f["x"]);
    for case in f["cases"].as_array().unwrap() {
        let arg = col(&case["arg"]);
        let ba = if case["kind"] == "low" { butter_low(2, arg[0]) }
                 else { butter_band(2, arg[0], arg[1]) };
        let what = format!("{}{:?}", case["kind"], arg);
        close(&ba.b, &col(&case["b"]), 1e-12, &format!("{what} b"));
        close(&ba.a, &col(&case["a"]), 1e-12, &format!("{what} a"));
        let zi = lfilter_zi(&ba);
        close(&zi, &col(&case["zi"]), 1e-10, &format!("{what} zi"));
        let zi0: Vec<f64> = zi.iter().map(|z| z * x[0]).collect();
        let (y, zf) = lfilter(&ba, &x, &zi0);
        close(&y, &col(&case["lfilter_y"]), 1e-10, &format!("{what} lfilter"));
        close(&zf, &col(&case["lfilter_zf"]), 1e-9, &format!("{what} zf"));
        close(&filtfilt(&ba, &x), &col(&case["filtfilt"]), 1e-9,
              &format!("{what} filtfilt"));
    }
}
