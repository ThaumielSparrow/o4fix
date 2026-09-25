//! Research-only six-frame inverse-depth bundle fit. No temporal motion prior.
use nalgebra::{DMatrix, DVector, Matrix3, Rotation3, SMatrix, Vector2, Vector3};
use opencv::{
    calib3d,
    core::{self, no_array, Mat, Point2f, Vector},
    prelude::*,
};
use serde_json::{json, Value};
const FRAMES: usize = 6;
const PARAMS: usize = 6 * (FRAMES - 1);
const HUBER: f64 = 0.002;
#[derive(Clone)]
struct Pose {
    r: Matrix3<f64>,
    t: Vector3<f64>,
}
#[derive(Clone)]
struct Track {
    id: usize,
    p: Vec<Vector2<f64>>,
}
struct Fit {
    poses: Vec<Pose>,
    cost: f64,
    initial_cost: f64,
    iterations: usize,
    converged: bool,
    depths: Vec<f64>,
}
fn skew(v: Vector3<f64>) -> Matrix3<f64> {
    Matrix3::new(0., -v.z, v.y, v.z, 0., -v.x, -v.y, v.x, 0.)
}
fn ray(p: Vector2<f64>) -> Vector3<f64> {
    Vector3::new(p.x, p.y, 1.)
}
fn angle(r: Matrix3<f64>) -> f64 {
    ((r.trace() - 1.) / 2.).clamp(-1., 1.).acos()
}
fn omega(poses: &[Pose]) -> [f64; 3] {
    let r = poses[3].r * poses[2].r.transpose();
    let v = nalgebra::UnitQuaternion::from_rotation_matrix(&Rotation3::from_matrix_unchecked(r))
        .scaled_axis()
        * 100.;
    [v.x, v.y, v.z]
}
fn median(v: &[f64]) -> Option<f64> {
    if v.is_empty() {
        None
    } else {
        let mut s = v.to_vec();
        s.sort_by(f64::total_cmp);
        Some(s[s.len() / 2])
    }
}
fn depth(track: &Track, poses: &[Pose]) -> Option<f64> {
    let a = ray(track.p[0]);
    let (mut num, mut den) = (0., 0.);
    for (k, pose) in poses.iter().enumerate().skip(1) {
        let b = ray(track.p[k]);
        let x = b.cross(&(pose.r * a));
        let y = b.cross(&pose.t);
        num -= x.dot(&y);
        den += y.norm_squared();
    }
    if den < 1e-14 {
        return None;
    }
    let d = num / den;
    if d.is_finite() && d > 1e-8 {
        Some(d)
    } else {
        None
    }
}
fn project(
    track: &Track,
    pose: &Pose,
    k: usize,
    rho: f64,
) -> Option<(Vector2<f64>, SMatrix<f64, 2, 6>, Vector2<f64>)> {
    let rotated = pose.r * ray(track.p[0]);
    let q = rotated + pose.t * rho;
    if q.z <= 1e-8 || !q.iter().all(|x| x.is_finite()) {
        return None;
    }
    let pred = Vector2::new(q.x / q.z, q.y / q.z);
    let d = SMatrix::<f64, 2, 3>::new(
        1. / q.z,
        0.,
        -q.x / q.z.powi(2),
        0.,
        1. / q.z,
        -q.y / q.z.powi(2),
    );
    let mut j = SMatrix::<f64, 2, 6>::zeros();
    j.fixed_view_mut::<2, 3>(0, 0)
        .copy_from(&(d * (-skew(rotated))));
    j.fixed_view_mut::<2, 3>(0, 3).copy_from(&(d * rho));
    Some((pred - track.p[k], j, d * pose.t))
}
fn cost(tracks: &[Track], poses: &[Pose], rho: &[f64]) -> Option<f64> {
    let mut sum = 0.;
    for (i, tr) in tracks.iter().enumerate() {
        for (k, p) in poses.iter().enumerate().skip(1) {
            let e = project(tr, p, k, rho[i])?.0.norm();
            sum += if e <= HUBER {
                e * e / 2.
            } else {
                HUBER * (e - HUBER / 2.)
            };
        }
    }
    Some(sum)
}
fn optimize(
    tracks: &[Track],
    mut poses: Vec<Pose>,
    mut rho: Vec<f64>,
    max_iterations: usize,
) -> Option<Fit> {
    if tracks.len() < 40 || rho.len() != tracks.len() {
        return None;
    }
    let initial_cost = cost(tracks, &poses, &rho)?;
    let mut current = initial_cost;
    let mut lambda = 1e-4;
    let mut converged = false;
    let mut iterations = 0;
    for iteration in 0..max_iterations {
        iterations = iteration + 1;
        if current < 1e-15 {
            converged = true;
            break;
        }
        let mut h = DMatrix::<f64>::zeros(PARAMS, PARAMS);
        let mut g = DVector::<f64>::zeros(PARAMS);
        let mut cross = vec![];
        let mut dh = vec![];
        let mut dg = vec![];
        for (i, tr) in tracks.iter().enumerate() {
            let mut v = DVector::<f64>::zeros(PARAMS);
            let (mut hh, mut gg) = (0., 0.);
            for (k, p) in poses.iter().enumerate().skip(1) {
                let (e, j, mut d) = project(tr, p, k, rho[i])?;
                d *= rho[i]; // Optimize log inverse-depth to preserve positivity.
                let weight = (HUBER / e.norm().max(1e-15)).min(1.);
                let hhpose = j.transpose() * j * weight;
                let ggpose = j.transpose() * e * weight;
                let cd = j.transpose() * d * weight;
                let off = (k - 1) * 6;
                for r in 0..6 {
                    g[off + r] += ggpose[r];
                    v[off + r] += cd[r];
                    for c in 0..6 {
                        h[(off + r, off + c)] += hhpose[(r, c)];
                    }
                }
                hh += weight * d.norm_squared();
                gg += weight * d.dot(&e);
            }
            cross.push(v);
            dh.push(hh);
            dg.push(gg);
        }
        for i in 0..PARAMS {
            h[(i, i)] += lambda * (h[(i, i)] + 1e-8);
        }
        for i in 0..tracks.len() {
            dh[i] += lambda * (dh[i] + 1e-8);
            h -= (&cross[i] * cross[i].transpose()) / dh[i];
            g -= &cross[i] * (dg[i] / dh[i]);
        }
        let step = h.lu().solve(&(-g))?;
        if !step.iter().all(|v| v.is_finite()) {
            return None;
        }
        let mut np = poses.clone();
        for (k, p) in np.iter_mut().enumerate().skip(1) {
            let off = (k - 1) * 6;
            let dr = Vector3::new(step[off], step[off + 1], step[off + 2]);
            p.r = Rotation3::new(dr).matrix() * p.r;
            p.t += Vector3::new(step[off + 3], step[off + 4], step[off + 5]);
        }
        let mut nr: Vec<_> = (0..rho.len())
            .map(|i| rho[i] * (-(dg[i] + cross[i].dot(&step)) / dh[i]).exp())
            .collect();
        let scale = np.last()?.t.norm();
        let valid = scale > 1e-8 && nr.iter().all(|d| d.is_finite() && *d > 1e-8);
        if valid {
            for p in &mut np {
                p.t /= scale;
            }
            for d in &mut nr {
                *d *= scale;
            }
        }
        let nc = if valid { cost(tracks, &np, &nr) } else { None };
        if nc.is_some_and(|v| v < current) {
            let next = nc?;
            let relative = (current - next) / current.max(1e-14);
            poses = np;
            rho = nr;
            current = next;
            lambda = (lambda * 0.3).max(1e-10);
            if relative < 1e-7 || step.norm() < 1e-7 || current < 1e-15 {
                converged = true;
                break;
            }
        } else {
            lambda *= 10.;
            if lambda > 1e8 {
                break;
            }
        }
    }
    Some(Fit {
        poses,
        cost: current,
        initial_cost,
        iterations,
        converged,
        depths: rho,
    })
}
type Initialization = (Vec<Pose>, Vec<Track>, Vec<f64>);
fn initialize(tracks: &[Track]) -> opencv::Result<Option<Initialization>> {
    if tracks.len() < 60 {
        return Ok(None);
    }
    let a: Vector<Point2f> = tracks
        .iter()
        .map(|t| Point2f::new(t.p[0].x as f32, t.p[0].y as f32))
        .collect();
    let b: Vector<Point2f> = tracks
        .iter()
        .map(|t| Point2f::new(t.p[FRAMES - 1].x as f32, t.p[FRAMES - 1].y as f32))
        .collect();
    let eye = Mat::eye(3, 3, core::CV_64F)?.to_mat()?;
    let mut mask = Mat::default();
    let e =
        calib3d::find_essential_mat(&a, &b, &eye, calib3d::RANSAC, 0.999, 0.002, 1000, &mut mask)?;
    if e.rows() != 3 || e.cols() != 3 {
        return Ok(None);
    }
    let (mut r, mut t) = (Mat::default(), Mat::default());
    let n = calib3d::recover_pose_triangulated(
        &e,
        &a,
        &b,
        &eye,
        &mut r,
        &mut t,
        1e6,
        &mut mask,
        &mut no_array(),
    )?;
    if n < 40 {
        return Ok(None);
    }
    let rot = Matrix3::from_fn(|r0, c| *r.at_2d::<f64>(r0 as i32, c as i32).unwrap());
    let trans = Vector3::new(*t.at::<f64>(0)?, *t.at::<f64>(1)?, *t.at::<f64>(2)?);
    let axis =
        nalgebra::UnitQuaternion::from_rotation_matrix(&Rotation3::from_matrix_unchecked(rot))
            .scaled_axis();
    let poses: Vec<_> = (0..FRAMES)
        .map(|k| {
            let u = k as f64 / (FRAMES - 1) as f64;
            Pose {
                r: *Rotation3::new(axis * u).matrix(),
                t: trans * u,
            }
        })
        .collect();
    // Interpolation initializes only; all non-anchor poses are independent optimization variables.
    let mut accepted = vec![];
    let mut depths = vec![];
    for tr in tracks {
        if let Some(d) = depth(tr, &poses) {
            if (1..FRAMES).all(|k| project(tr, &poses[k], k, d).is_some()) {
                accepted.push(tr.clone());
                depths.push(d);
            }
        }
    }
    if accepted.len() < 40 {
        return Ok(None);
    }
    Ok(Some((poses, accepted, depths)))
}
fn solve(tracks: &[Track], seed: i32) -> opencv::Result<Option<(Fit, usize)>> {
    core::set_rng_seed(seed)?;
    let Some((poses, accepted, depths)) = initialize(tracks)? else {
        return Ok(None);
    };
    let n = accepted.len();
    Ok(optimize(&accepted, poses, depths, 60).map(|f| (f, n)))
}
fn heldout(tracks: &[Track], poses: &[Pose]) -> Value {
    let mut errors = vec![];
    let mut grid: Vec<Vec<f64>> = vec![vec![]; 9];
    for tr in tracks {
        if let Some(d) = depth(tr, poses) {
            let e: Option<Vec<f64>> = (1..FRAMES)
                .map(|k| project(tr, &poses[k], k, d).map(|x| x.0.norm()))
                .collect();
            if let Some(e) = e {
                let m = median(&e).unwrap();
                errors.push(m);
                let x = tr.p[0];
                let col = ((x.x + 1.) * 1.5).floor().clamp(0., 2.) as usize;
                let row = ((x.y + 0.75) * 2.).floor().clamp(0., 2.) as usize;
                grid[row * 3 + col].push(m);
            }
        }
    }
    json!({"valid_tracks":errors.len(),"total_tracks":tracks.len(),"median_normalized_error":median(&errors),"normalized_grid":grid.iter().map(|v|if v.len()<8{Value::Null}else{json!({"count":v.len(),"median":median(v)})}).collect::<Vec<_>>()})
}
fn assemble(rows: &[Value]) -> Option<Vec<Track>> {
    if rows.len() != FRAMES - 1 {
        return None;
    }
    for w in rows.windows(2) {
        if (w[1]["t"].as_f64()? - w[0]["t"].as_f64()? - 0.01).abs() > 1e-6 {
            return None;
        }
    }
    let maps: Vec<std::collections::HashMap<usize, usize>> = rows
        .iter()
        .map(|r| {
            r["ids"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .map(|(i, id)| (id.as_u64().unwrap() as usize, i))
                .collect()
        })
        .collect();
    let mut tracks = vec![];
    for id in rows[0]["ids"].as_array()? {
        let id = id.as_u64()? as usize;
        let mut p = vec![];
        let mut valid = true;
        for (k, row) in rows.iter().enumerate() {
            let Some(&i) = maps[k].get(&id) else {
                valid = false;
                break;
            };
            let cv = |v: &Value| Vector2::new(v[0].as_f64().unwrap(), v[1].as_f64().unwrap());
            let a = cv(&row["normalized_a"][i]);
            let b = cv(&row["normalized_b"][i]);
            if k == 0 {
                p.push(a);
            } else if (p[k] - a).norm() > 1e-6 {
                valid = false;
                break;
            }
            p.push(b);
        }
        if valid {
            tracks.push(Track { id, p });
        }
    }
    Some(tracks)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 5 {
        return Err(
            "usage: multiframe_pose_probe SOURCE_OBSERVATIONS START END OUTPUT_JSON".into(),
        );
    }
    let data: Value = serde_json::from_str(&std::fs::read_to_string(&args[1])?)?;
    let rows = data["rows"].as_array().ok_or("missing rows")?;
    let start: f64 = args[2].parse()?;
    let end: f64 = args[3].parse()?;
    let mut out = vec![];
    for i in 2..rows.len() - 2 {
        let time = rows[i]["t"].as_f64().unwrap();
        if time < start || time > end {
            continue;
        }
        let Some(tracks) = assemble(&rows[i - 2..=i + 2]) else {
            out.push(json!({"t":time,"fit":null,"reason":"noncontiguous"}));
            continue;
        };
        // Deterministic feature subsampling, capped independently in each parity group.
        // A fixed stride would alias ID parity and could leave a split empty.
        let mut tracks = tracks;
        tracks.sort_by_key(|t| (t.id as u64).wrapping_mul(11400714819323198485));
        let mut selected = [0; 2];
        let tracks: Vec<_> = tracks
            .into_iter()
            .filter(|t| {
                let g = t.id % 2;
                if selected[g] >= 90 {
                    false
                } else {
                    selected[g] += 1;
                    true
                }
            })
            .collect();
        let fit = solve(&tracks, 2_000_000 + i as i32)?;
        let value=fit.as_ref().map(|(f,n)|json!({"omega":omega(&f.poses),"cost":f.cost,"initial_cost":f.initial_cost,"iterations":f.iterations,"converged":f.converged,"depth_tracks":n,"median_inverse_depth":median(&f.depths)}));
        let mut independent = vec![];
        let mut splits = vec![];
        for parity in 0..2 {
            let tr: Vec<_> = tracks
                .iter()
                .filter(|t| t.id % 2 == parity)
                .cloned()
                .collect();
            let ev: Vec<_> = tracks
                .iter()
                .filter(|t| t.id % 2 != parity)
                .cloned()
                .collect();
            let f = solve(&tr, 3_000_000 + i as i32 * 2 + parity as i32)?;
            independent.push(f.as_ref().map(|(f,n)|json!({"omega":omega(&f.poses),"converged":f.converged,"depth_tracks":n,"heldout":heldout(&ev,&f.poses)})));
            splits.push(f);
        }
        let disagreement = splits[0]
            .as_ref()
            .zip(splits[1].as_ref())
            .map(|((a, _), (b, _))| {
                let ra = a.poses[3].r * a.poses[2].r.transpose();
                let rb = b.poses[3].r * b.poses[2].r.transpose();
                angle(ra * rb.transpose()) * 100.
            });
        out.push(json!({"t":time,"tracks":tracks.len(),"fit":value,"independent":independent,"split_disagreement_rad_s":disagreement,"reference_models":rows[i]["models"]}));
        if out.len() % 50 == 0 {
            println!("{} pairs", out.len());
        }
    }
    std::fs::write(
        &args[4],
        serde_json::to_vec(
            &json!({"window":[start,end],"rows":out,"frames":FRAMES,"temporal_prior":false}),
        )?,
    )?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn synthetic() -> (Vec<Track>, Vec<Pose>, Vec<f64>) {
        let poses: Vec<_> = (0..FRAMES)
            .map(|k| {
                let u = k as f64 / 5.;
                Pose {
                    r: *Rotation3::from_euler_angles(0.015 * u * u, 0.04 * u, -0.02 * u * u)
                        .matrix(),
                    t: Vector3::new(u, 0.08 * u * u, 0.02 * u),
                }
            })
            .collect();
        let mut tracks = vec![];
        let mut rho = vec![];
        for id in 0..120 {
            let a = Vector3::new(
                -0.6 + (id % 12) as f64 * 0.11,
                -0.4 + (id / 12) as f64 * 0.09,
                1.,
            );
            let d = 1. / (5. + (id * 7 % 19) as f64);
            let p = poses
                .iter()
                .map(|pose| {
                    let q = pose.r * a + pose.t * d;
                    Vector2::new(q.x / q.z, q.y / q.z)
                })
                .collect();
            tracks.push(Track { id, p });
            rho.push(d);
        }
        (tracks, poses, rho)
    }
    #[test]
    fn fast_turn_with_measurement_noise_and_outliers() {
        let (mut tr, mut truth, depths) = synthetic();
        for (k, p) in truth.iter_mut().enumerate() {
            let u = k as f64 / 5.;
            p.r = *Rotation3::from_euler_angles(0.2 * u * u, 0.4 * u, -0.1 * u * u).matrix();
        }
        for (i, t) in tr.iter_mut().enumerate() {
            let a = ray(t.p[0]);
            for (k, p) in truth.iter().enumerate() {
                let q = p.r * a + p.t * depths[i];
                t.p[k] = Vector2::new(q.x / q.z, q.y / q.z);
                if k > 0 {
                    t.p[k].x += 0.0001 * ((i * 3 + k * 7) % 11) as f64 / 11. - 0.00005;
                    t.p[k].y += 0.0001 * ((i * 7 + k * 3) % 13) as f64 / 13. - 0.00005;
                    if i % 17 == 0 {
                        t.p[k].x += 0.02;
                        t.p[k].y -= 0.015;
                    }
                }
            }
        }
        let (f, _) = solve(&tr, 54321).unwrap().unwrap();
        let expected = truth[3].r * truth[2].r.transpose();
        let error = angle(f.poses[3].r * f.poses[2].r.transpose() * expected.transpose());
        assert!(error < 0.001, "rotation error {error}");
        assert!(f.cost < f.initial_cost);
    }
    #[test]
    fn exact_solution_and_scale_gauge_are_preserved() {
        let (tr, truth, depths) = synthetic();
        let mut scaled = truth.clone();
        for p in &mut scaled {
            p.t *= 7.;
        }
        let f = optimize(&tr, scaled, depths.iter().map(|d| d / 7.).collect(), 5).unwrap();
        assert!(f.converged);
        assert!(f.cost < 1e-20);
    }
    #[test]
    fn cached_time_gaps_are_not_bridged() {
        let rows: Vec<_> = (0..5)
            .map(|i| json!({"t":if i<2{i as f64*0.01}else{1.+i as f64*0.01}}))
            .collect();
        assert!(assemble(&rows).is_none());
    }
    #[test]
    fn image_initialized_joint_fit_recovers_known_motion() {
        let (tr, truth, _) = synthetic();
        let (f, n) = solve(&tr, 12345).unwrap().unwrap();
        assert!(n >= 100);
        assert!(f.cost < 1e-9, "cost {}", f.cost);
        assert!(
            angle(
                (f.poses[3].r * f.poses[2].r.transpose())
                    * (truth[3].r * truth[2].r.transpose()).transpose()
            ) < 1e-4
        );
    }
    #[test]
    fn joint_fit_recovers_nonconstant_motion() {
        let (tr, truth, depths) = synthetic();
        let mut init = truth.clone();
        for (k, p) in init.iter_mut().enumerate().skip(1) {
            p.r = Rotation3::new(Vector3::new(0.002, -0.001, 0.001) * k as f64 / 5.).matrix() * p.r;
            p.t += Vector3::new(0.01, 0.005, -0.002);
        }
        let f = optimize(&tr, init, depths.iter().map(|d| d * 1.05).collect(), 60).unwrap();
        assert!(
            f.cost < f.initial_cost * 1e-5,
            "{} -> {}",
            f.initial_cost,
            f.cost
        );
        for k in 1..FRAMES {
            assert!(angle(f.poses[k].r * truth[k].r.transpose()) < 1e-4);
        }
    }
    #[test]
    fn pose_and_depth_jacobians_match_finite_differences() {
        let (tr, poses, rho) = synthetic();
        let (e, j, d) = project(&tr[3], &poses[2], 2, rho[3]).unwrap();
        for c in 0..6 {
            let mut p = poses[2].clone();
            if c < 3 {
                let mut v = Vector3::zeros();
                v[c] = 1e-7;
                p.r = Rotation3::new(v).matrix() * p.r;
            } else {
                p.t[c - 3] += 1e-7;
            }
            let fd = (project(&tr[3], &p, 2, rho[3]).unwrap().0 - e) / 1e-7;
            assert!((fd - j.column(c)).norm() < 1e-6);
        }
        let fd = (project(&tr[3], &poses[2], 2, rho[3] + 1e-7).unwrap().0 - e) / 1e-7;
        assert!((fd - d).norm() < 1e-6);
    }
    #[test]
    fn pure_rotation_has_no_observable_depth() {
        let (tr, mut poses, _) = synthetic();
        for p in &mut poses {
            p.t = Vector3::zeros();
        }
        assert!(depth(&tr[0], &poses).is_none());
    }
}
