//! Experimental inversion on the first monotone branch of the tested fisheye polynomial.
use opencv::{
    core::{Mat, Point2f},
    prelude::*,
};
pub struct Inverse {
    pub coeff: [f64; 4],
    pub theta_max: f64,
    pub radius_max: f64,
    fx: f64,
    fy: f64,
    cx: f64,
    cy: f64,
}
impl Inverse {
    pub fn radial(&self, t: f64) -> f64 {
        let t2 = t * t;
        t * (1.
            + t2 * (self.coeff[0]
                + t2 * (self.coeff[1] + t2 * (self.coeff[2] + t2 * self.coeff[3]))))
    }
    pub fn derivative(&self, t: f64) -> f64 {
        let t2 = t * t;
        1. + t2
            * (3. * self.coeff[0]
                + t2 * (5. * self.coeff[1] + t2 * (7. * self.coeff[2] + t2 * 9. * self.coeff[3])))
    }
    pub fn new(k: &Mat, d: &Mat) -> opencv::Result<Self> {
        let mut out = Self {
            coeff: [
                *d.at::<f64>(0)?,
                *d.at::<f64>(1)?,
                *d.at::<f64>(2)?,
                *d.at::<f64>(3)?,
            ],
            theta_max: std::f64::consts::FRAC_PI_2 - 1e-7,
            radius_max: 0.,
            fx: *k.at_2d::<f64>(0, 0)?,
            fy: *k.at_2d::<f64>(1, 1)?,
            cx: *k.at_2d::<f64>(0, 2)?,
            cy: *k.at_2d::<f64>(1, 2)?,
        };
        // This scan is a bounded research implementation validated for the O4 profile.
        let limit = out.theta_max;
        let mut prev = 0.;
        for step in 1..=512 {
            let cur = limit * step as f64 / 512.;
            if out.derivative(cur) <= 0. {
                let (mut lo, mut hi) = (prev, cur);
                for _ in 0..60 {
                    let mid = (lo + hi) / 2.;
                    if out.derivative(mid) > 0. {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                out.theta_max = (lo + hi) / 2.;
                break;
            }
            prev = cur;
        }
        out.radius_max = out.radial(out.theta_max);
        Ok(out)
    }
    pub fn point(&self, p: Point2f) -> Option<Point2f> {
        let x = (p.x as f64 - self.cx) / self.fx;
        let y = (p.y as f64 - self.cy) / self.fy;
        let r = x.hypot(y);
        if !r.is_finite() || r > self.radius_max || self.fx <= 0. || self.fy <= 0. {
            return None;
        }
        if r < 1e-12 {
            return Some(Point2f::new(x as f32, y as f32));
        }
        let (mut lo, mut hi) = (0., self.theta_max);
        for _ in 0..50 {
            let t = (lo + hi) / 2.;
            if self.radial(t) < r {
                lo = t;
            } else {
                hi = t;
            }
        }
        let scale = ((lo + hi) / 2.).tan() / r;
        let p = Point2f::new((x * scale) as f32, (y * scale) as f32);
        if p.x.is_finite() && p.y.is_finite() {
            Some(p)
        } else {
            None
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use opencv::{calib3d, core::Vector};
    fn setup() -> (Mat, Mat) {
        (
            Mat::from_slice_2d(&[[546.40271, 0., 720.], [0., 546.40271, 540.], [0., 0., 1.]])
                .unwrap(),
            Mat::from_slice(&[0.1551311, 0.1371409, -0.0938614, 0.0041704])
                .unwrap()
                .try_clone()
                .unwrap(),
        )
    }
    #[test]
    fn tested_profile_roundtrips_all_source_grid_points() {
        let (k, d) = setup();
        let inv = Inverse::new(&k, &d).unwrap();
        let pts: Vec<_> = (0..19)
            .flat_map(|r| (0..25).map(move |c| Point2f::new(c as f32 * 60., r as f32 * 60.)))
            .collect();
        let u: Vector<Point2f> = pts.iter().map(|&p| inv.point(p).unwrap()).collect();
        let mut back = Vector::<Point2f>::new();
        calib3d::fisheye_distort_points_def(&u, &mut back, &k, &d).unwrap();
        for (a, b) in pts.iter().zip(back) {
            assert!((a.x - b.x).hypot(a.y - b.y) < 0.001);
        }
    }
    #[test]
    fn rejects_beyond_monotone_domain_and_preserves_center() {
        let (k, d) = setup();
        let inv = Inverse::new(&k, &d).unwrap();
        assert!(inv.point(Point2f::new(3000., 3000.)).is_none());
        assert_eq!(
            inv.point(Point2f::new(720., 540.)).unwrap(),
            Point2f::new(0., 0.)
        );
        assert!(inv.derivative(inv.theta_max).abs() < 1e-10);
    }
}
