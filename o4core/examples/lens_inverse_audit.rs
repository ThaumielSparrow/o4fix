//! Read-only audit of default fisheye inverse branches across the image.
#[path = "support/lens_inverse.rs"]
mod lens_inverse;
use opencv::{
    calib3d,
    core::{Mat, Point2f, Vector},
    prelude::*,
};
use serde_json::json;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<_> = std::env::args().collect();
    if a.len() != 3 {
        return Err("usage: lens_inverse_audit VIDEO OUTPUT_JSON".into());
    }
    let meta = o4core::telemetry::extract_quats(std::path::Path::new(&a[1]))?.meta;
    let k = Mat::from_slice_2d(&meta.camera_matrix.ok_or("missing matrix")?)?;
    let dc = meta.distortion.ok_or("missing distortion")?;
    let d = Mat::from_slice(&dc)?.try_clone()?;
    let inverse = lens_inverse::Inverse::new(&k, &d)?;
    let p: Vector<Point2f> = (0..55)
        .flat_map(|r| {
            (0..73).map(move |c| Point2f::new(c as f32 * 1439. / 72., r as f32 * 1079. / 54.))
        })
        .collect();
    let mut old = Vector::<Point2f>::new();
    calib3d::fisheye_undistort_points_def(&p, &mut old, &k, &d)?;
    let mut back = Vector::<Point2f>::new();
    calib3d::fisheye_distort_points_def(&old, &mut back, &k, &d)?;
    let mut out = vec![];
    for i in 0..p.len() {
        let pixel = p.get(i)?;
        let old = old.get(i)?;
        let new = inverse.point(pixel);
        let reconstructed = back.get(i)?;
        let roundtrip = (pixel.x - reconstructed.x).hypot(pixel.y - reconstructed.y);
        let theta = (old.x as f64).hypot(old.y as f64).atan();
        out.push(json!({"pixel":[pixel.x,pixel.y],"opencv_normalized":[old.x,old.y],"principal_normalized":new.map(|p|[p.x,p.y]),"opencv_roundtrip_px":roundtrip,"opencv_beyond_first_branch":theta>inverse.theta_max,"opencv_sentinel":old.x.abs()>1e5||old.y.abs()>1e5}));
    }
    std::fs::write(
        &a[2],
        serde_json::to_vec(
            &json!({"theta_limit":inverse.theta_max,"distorted_radius_limit":inverse.radius_max,"rows":out}),
        )?,
    )?;
    Ok(())
}
