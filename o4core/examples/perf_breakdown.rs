//! Research-only: per-step cost of the refine measurement loop on one thread.
//! Usage: perf_breakdown VIDEO START_FRAME N_FRAMES
use opencv::core::{self, Mat, Point2f, Size, TermCriteria, Vector};
use opencv::prelude::*;
use opencv::{imgproc, video, videoio};
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().collect();
    let (f0, n): (i64, i64) = (a[2].parse()?, a[3].parse()?);
    core::set_num_threads(1)?;
    let params = Vector::<i32>::from(vec![videoio::CAP_PROP_N_THREADS, 1]);
    let mut cap = videoio::VideoCapture::from_file_with_params(&a[1], videoio::CAP_ANY, &params)?;
    let t = Instant::now();
    cap.set(videoio::CAP_PROP_POS_FRAMES, f0 as f64)?;
    let seek = t.elapsed();
    let (mut dec, mut gray, mut gftt, mut pyr, mut lk) = [Duration::ZERO; 5].into();
    let crit = TermCriteria::new(core::TermCriteria_COUNT + core::TermCriteria_EPS, 30, 0.01)?;
    let (mut frame, mut prev) = (Mat::default(), None::<(Mat, Vector<Mat>)>);
    for _ in 0..n {
        let t = Instant::now();
        if !cap.read(&mut frame)? {
            break;
        }
        dec += t.elapsed();
        let t = Instant::now();
        let mut g = Mat::default();
        imgproc::cvt_color_def(&frame, &mut g, imgproc::COLOR_BGR2GRAY)?;
        gray += t.elapsed();
        let t = Instant::now();
        let mut gp = Vector::<Mat>::new();
        video::build_optical_flow_pyramid(
            &g,
            &mut gp,
            Size::new(21, 21),
            4,
            true,
            core::BORDER_REFLECT_101,
            core::BORDER_CONSTANT,
            true,
        )?;
        pyr += t.elapsed();
        if let Some((p, pp)) = &prev {
            let t = Instant::now();
            let mut pts = Vector::<Point2f>::new();
            imgproc::good_features_to_track(
                p,
                &mut pts,
                1200,
                0.005,
                18.0,
                &core::no_array(),
                7,
                false,
                0.04,
            )?;
            gftt += t.elapsed();
            let t = Instant::now();
            let (mut b, mut st, mut err) = (
                Vector::<Point2f>::new(),
                Vector::<u8>::new(),
                Vector::<f32>::new(),
            );
            video::calc_optical_flow_pyr_lk(
                pp,
                &gp,
                &pts,
                &mut b,
                &mut st,
                &mut err,
                Size::new(21, 21),
                4,
                crit,
                0,
                1e-4,
            )?;
            let (mut r, mut sr, mut er) = (
                Vector::<Point2f>::new(),
                Vector::<u8>::new(),
                Vector::<f32>::new(),
            );
            video::calc_optical_flow_pyr_lk(
                &gp,
                pp,
                &b,
                &mut r,
                &mut sr,
                &mut er,
                Size::new(21, 21),
                4,
                crit,
                0,
                1e-4,
            )?;
            lk += t.elapsed();
        }
        prev = Some((g, gp));
    }
    let ms = |d: Duration| d.as_secs_f64() * 1000.0 / n as f64;
    println!(
        "per frame (ms): seek-once {:.0} total | decode {:.2} | bgr->gray {:.2} | pyramid {:.2} | gftt {:.2} | lk fwd+back {:.2}",
        seek.as_secs_f64() * 1000.0,
        ms(dec),
        ms(gray),
        ms(pyr),
        ms(gftt),
        ms(lk)
    );
    Ok(())
}
