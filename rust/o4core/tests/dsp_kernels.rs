mod common;
use self::common as helpers;
use helpers::*;
use o4core::dsp::*;

#[test]
fn kernels_match_scipy() {
    let f = fx("kernels.json");
    let x300 = col(&f["x300"]);
    for (s, key) in [
        (3usize, "3"),
        (4, "4"),
        (10, "10"),
        (15, "15"),
        (100, "100"),
        (200, "200"),
    ] {
        close(
            &uniform_filter1d(&x300, s),
            &col(&f["uniform"][key]),
            1e-12,
            key,
        );
    }
    close(
        &median_filter(&x300, 15),
        &col(&f["median"]["15"]),
        0.0,
        "median15",
    );
    let hin = rows3(&f["hampel_in"]);
    let (hout, frac) = hampel(&hin, 7, 6.0);
    assert_close(&hout, &rows3(&f["hampel_out"]), 1e-12, "hampel");
    assert!((frac - f["hampel_frac"].as_f64().unwrap()).abs() < 1e-15);
    close(
        &interp(
            &col(&f["interp_xq"]),
            &col(&f["interp_xp"]),
            &col(&f["interp_fp"]),
        ),
        &col(&f["interp_y"]),
        1e-14,
        "interp",
    );
    close(
        &gradient(&col(&f["gradient_in"])),
        &col(&f["gradient_out"]),
        1e-14,
        "gradient",
    );
    let srt = col(&f["sorted"]);
    for (i, q) in col(&f["queries"]).iter().enumerate() {
        assert_eq!(
            searchsorted_left(&srt, *q),
            f["ss_left"][i].as_u64().unwrap() as usize
        );
        assert_eq!(
            searchsorted_right(&srt, *q),
            f["ss_right"][i].as_u64().unwrap() as usize
        );
    }
}

#[test]
fn uniform3_matches_per_column() {
    let f = fx("kernels.json");
    let input = rows3(&f["hampel_in"]);
    for size in [4, 15] {
        let result = uniform_filter3(&input, size);
        for k in 0..3 {
            let col_in: Vec<f64> = input.iter().map(|r| r[k]).collect();
            let col_expected = uniform_filter1d(&col_in, size);
            let col_result: Vec<f64> = result.iter().map(|r| r[k]).collect();
            close(
                &col_result,
                &col_expected,
                0.0,
                &format!("uniform3 size={} col={}", size, k),
            );
        }
    }
}
