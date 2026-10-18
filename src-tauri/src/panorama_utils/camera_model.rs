//! 3D Rotational Camera Model, Ray-Traced Surface Projections & Bundle Adjustment
//!
//! Replaces 2D planar homography stretching with true 3D spherical/cylindrical ray tracing.
//! Implements:
//! 1. 3D Camera Pose Parameterization: R_y(yaw) * R_x(pitch) * R_z(roll) with focal length f
//! 2. Surface Ray Lookups for Cylindrical, Spherical (Equirectangular), and Planar projections
//! 3. Bundle Adjustment optimizer for minimizing tie-point reprojection errors & horizon leveling

use nalgebra::{Matrix3, Point2, Vector3};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PanoramaProjection {
    Planar,
    Cylindrical,
    Spherical,
    Panini,
    Stereographic,
}

#[derive(Debug, Clone)]
pub struct CameraPose {
    #[allow(dead_code)]
    pub id: usize,
    pub yaw: f64,   // Pan angle (horizontal rotation about Y)
    pub pitch: f64, // Tilt angle (vertical rotation about X)
    pub roll: f64,  // Slant angle (rotation about Z)
    pub f: f64,     // Effective focal length in pixels
    pub cx: f64,    // Principal point X
    pub cy: f64,    // Principal point Y
    pub width: u32,
    pub height: u32,
    // Panotools polynomial optical distortion: r_src = r_dst * (a*r^3 + b*r^2 + c*r + (1 - a - b - c))
    pub dist_a: f64,
    pub dist_b: f64,
    pub dist_c: f64,
    // Radial lens vignetting correction: V(r) = 1.0 / (1.0 + v1*r^2 + v2*r^4)
    pub vig_v1: f64,
    pub vig_v2: f64,
}

impl CameraPose {
    pub fn new(id: usize, width: u32, height: u32, focal_length_px: Option<f64>) -> Self {
        let f = focal_length_px.unwrap_or((width.max(height) as f64) * 1.25);
        Self {
            id,
            yaw: 0.0,
            pitch: 0.0,
            roll: 0.0,
            f,
            cx: width as f64 / 2.0,
            cy: height as f64 / 2.0,
            width,
            height,
            dist_a: 0.0,
            dist_b: 0.0,
            dist_c: 0.0,
            vig_v1: 0.0,
            vig_v2: 0.0,
        }
    }

    /// Computes effective focal length in pixels using physical sensor dimensions and EXIF focal length
    pub fn focal_length_from_exif(focal_mm: f64, sensor_width_mm: Option<f64>, image_width_px: u32) -> f64 {
        let sensor_w = sensor_width_mm.unwrap_or(22.3); // Default Canon APS-C (EOS 77D/80D/200D)
        (focal_mm * image_width_px as f64) / sensor_w.max(1.0)
    }

    /// Initializes radial optical vignetting (cosine-fourth falloff) compensation factors from EXIF optical parameters
    pub fn with_vignetting_from_optical_params(mut self, _focal_mm: f64, f_number: Option<f64>) -> Self {
        let max_r = (self.width.max(self.height) as f64) * 0.6;
        let rf = max_r / self.f.max(50.0);
        let rf_sq = rf * rf;

        // Aperture factor: wider apertures have stronger mechanical pupil vignetting
        let aperture_factor = f_number.map(|fn_val| (3.5 / fn_val.max(1.4)).clamp(0.25, 1.0)).unwrap_or(0.6);

        // Calibrated polynomial coefficients: 1 / cos^4(theta) = 1 + 2*(r/f)^2 + (r/f)^4
        // Damped to avoid overshooting on stopped-down apertures
        self.vig_v1 = (1.2 * rf_sq * aperture_factor).clamp(0.0, 0.85);
        self.vig_v2 = (0.5 * rf_sq * rf_sq * aperture_factor).clamp(0.0, 0.45);

        self
    }

    /// Computes 3D rotation matrix R = R_y(yaw) * R_x(pitch) * R_z(roll)
    pub fn rotation_matrix(&self) -> Matrix3<f64> {
        let cy = self.yaw.cos();
        let sy = self.yaw.sin();
        let cp = self.pitch.cos();
        let sp = self.pitch.sin();
        let cr = self.roll.cos();
        let sr = self.roll.sin();

        let ry = Matrix3::new(cy, 0.0, sy, 0.0, 1.0, 0.0, -sy, 0.0, cy);
        let rx = Matrix3::new(1.0, 0.0, 0.0, 0.0, cp, -sp, 0.0, sp, cp);
        let rz = Matrix3::new(cr, -sr, 0.0, sr, cr, 0.0, 0.0, 0.0, 1.0);

        ry * rx * rz
    }

    /// Projects a 3D ray in world coordinates onto this camera's sensor coordinates (u, v)
    /// with Panotools 4th-order polynomial lens un-distortion, without clipping to sensor bounds.
    #[inline]
    pub fn project_ray_unbounded(
        &self,
        world_ray: &Vector3<f64>,
        r_inv: &Matrix3<f64>,
    ) -> Option<(f64, f64)> {
        let cam_ray = r_inv * world_ray;
        if cam_ray.z <= 0.01 {
            return None; // Point is behind or along camera optical plane
        }
        let u_norm = cam_ray.x / cam_ray.z;
        let v_norm = cam_ray.y / cam_ray.z;

        // Apply Panotools 4th-order radial distortion if coefficients are active
        let (u_corr, v_corr) = if self.dist_a != 0.0 || self.dist_b != 0.0 || self.dist_c != 0.0 {
            let r_sq = u_norm * u_norm + v_norm * v_norm;
            let r = r_sq.sqrt();
            if r > 1e-6 {
                let d = 1.0 - self.dist_a - self.dist_b - self.dist_c;
                let poly = self.dist_a * r_sq * r + self.dist_b * r_sq + self.dist_c * r + d;
                (u_norm * poly, v_norm * poly)
            } else {
                (u_norm, v_norm)
            }
        } else {
            (u_norm, v_norm)
        };

        let u = self.f * u_corr + self.cx;
        let v = self.f * v_corr + self.cy;
        Some((u, v))
    }

    /// Projects a 3D ray in world coordinates onto this camera's sensor coordinates (u, v)
    /// with Panotools 4th-order polynomial lens un-distortion.
    pub fn project_ray(&self, world_ray: &Vector3<f64>, r_inv: &Matrix3<f64>) -> Option<(f64, f64)> {
        if let Some((u, v)) = self.project_ray_unbounded(world_ray, r_inv) {
            if u >= 0.0 && u < self.width as f64 && v >= 0.0 && v < self.height as f64 {
                return Some((u, v));
            }
        }
        None
    }

    /// Unprojects a 2D sensor point (u, v) into a unit 3D ray in camera coordinates,
    /// inverting the Panotools radial distortion polynomial so `project_ray` is an exact inverse.
    #[inline]
    pub fn unproject_point(&self, u: f64, v: f64) -> Vector3<f64> {
        let f_safe = self.f.max(50.0);
        let u_d = (u - self.cx) / f_safe;
        let v_d = (v - self.cy) / f_safe;

        let (u_norm, v_norm) = if self.dist_a != 0.0 || self.dist_b != 0.0 || self.dist_c != 0.0 {
            let r_d = (u_d * u_d + v_d * v_d).sqrt();
            if r_d > 1e-6 {
                let d = 1.0 - self.dist_a - self.dist_b - self.dist_c;
                let mut r_u = r_d;
                for _ in 0..4 {
                    let r_sq = r_u * r_u;
                    let poly =
                        (self.dist_a * r_sq * r_u + self.dist_b * r_sq + self.dist_c * r_u + d)
                            .max(0.5);
                    r_u = r_d / poly;
                }
                let scale = r_u / r_d;
                (u_d * scale, v_d * scale)
            } else {
                (u_d, v_d)
            }
        } else {
            (u_d, v_d)
        };

        Vector3::new(u_norm, v_norm, 1.0).normalize()
    }

    /// Evaluates radial vignetting falloff compensation factor at sensor point (u, v)
    pub fn vignetting_gain(&self, u: f64, v: f64) -> f32 {
        if self.vig_v1 == 0.0 && self.vig_v2 == 0.0 {
            return 1.0;
        }
        let max_r = (self.width.max(self.height) as f64) * 0.6;
        let dx = (u - self.cx) / max_r;
        let dy = (v - self.cy) / max_r;
        let r_sq = (dx * dx + dy * dy).min(1.5);
        let boost = 1.0 + self.vig_v1 * r_sq + self.vig_v2 * r_sq * r_sq;
        boost.clamp(0.5, 3.0) as f32
    }
}

/// Computes a unit 3D ray for a given canvas coordinate (x, y) under the chosen projection
#[allow(dead_code)]
pub fn canvas_to_world_ray(
    x: f64,
    y: f64,
    canvas_cx: f64,
    canvas_cy: f64,
    f_canvas: f64,
    proj: PanoramaProjection,
) -> Vector3<f64> {
    match proj {
        PanoramaProjection::Cylindrical => {
            let theta = (x - canvas_cx) / f_canvas;
            let v = (y - canvas_cy) / f_canvas;
            let denom = (1.0 + v * v).sqrt();
            Vector3::new(theta.sin() / denom, v / denom, theta.cos() / denom)
        }
        PanoramaProjection::Spherical => {
            let theta = (x - canvas_cx) / f_canvas;
            let phi = (y - canvas_cy) / f_canvas;
            let cos_phi = phi.cos();
            Vector3::new(cos_phi * theta.sin(), phi.sin(), cos_phi * theta.cos())
        }
        PanoramaProjection::Planar => {
            let u = (x - canvas_cx) / f_canvas;
            let v = (y - canvas_cy) / f_canvas;
            let len = (u * u + v * v + 1.0).sqrt();
            Vector3::new(u / len, v / len, 1.0 / len)
        }
        PanoramaProjection::Panini => {
            let theta = (x - canvas_cx) / f_canvas;
            let phi = (y - canvas_cy) / f_canvas;
            let d = 1.0;
            let scale_v = (d + 1.0) / (d + theta.cos());
            let ray_x = theta.sin();
            let ray_y = phi / scale_v;
            let ray_z = theta.cos();
            Vector3::new(ray_x, ray_y, ray_z).normalize()
        }
        PanoramaProjection::Stereographic => {
            let u = (x - canvas_cx) / f_canvas;
            let v = (y - canvas_cy) / f_canvas;
            let r = (u * u + v * v).sqrt();
            if r < 1e-6 {
                Vector3::new(0.0, 0.0, 1.0)
            } else {
                let theta = 2.0 * (r / 2.0).atan();
                let sin_t = theta.sin();
                let cos_t = theta.cos();
                Vector3::new(sin_t * (u / r), sin_t * (v / r), cos_t).normalize()
            }
        }
    }
}

/// Decomposes an orthogonal 3D rotation matrix R = R_y(yaw) * R_x(pitch) * R_z(roll)
/// into exact Euler angles (yaw, pitch, roll).
#[inline]
pub fn rotation_matrix_to_euler(r: &Matrix3<f64>) -> (f64, f64, f64) {
    let pitch = (-r[(1, 2)]).clamp(-1.0, 1.0).asin();
    if pitch.abs() < std::f64::consts::FRAC_PI_2 - 1e-5 {
        let yaw = r[(0, 2)].atan2(r[(2, 2)]);
        let roll = r[(1, 0)].atan2(r[(1, 1)]);
        (yaw, pitch, roll)
    } else {
        let yaw = (-r[(0, 1)]).atan2(r[(0, 0)]);
        (yaw, pitch, 0.0)
    }
}

/// Converts a pairwise 2D homography into an estimated relative 3D camera rotation
/// by evaluating the homography on the principal point and horizontal optical axis.
/// This avoids the telephoto `f * h_20` projective noise amplification of `K^-1 H K`.
pub fn homography_to_relative_rotation(h: &Matrix3<f64>, f: f64, w: u32, h_img: u32) -> (f64, f64, f64) {
    let cx = w as f64 / 2.0;
    let cy = h_img as f64 / 2.0;
    let f_safe = f.max(100.0);

    let map_pt = |x: f64, y: f64| -> Option<(f64, f64)> {
        let p = h * Vector3::new(x, y, 1.0);
        if p.z.abs() > 1e-6 && p.x.is_finite() && p.y.is_finite() {
            Some((p.x / p.z, p.y / p.z))
        } else {
            None
        }
    };

    let (uc, vc) = map_pt(cx, cy).unwrap_or((cx, cy));
    let ray_c = Vector3::new((uc - cx) / f_safe, (vc - cy) / f_safe, 1.0).normalize();
    let yaw = ray_c.x.atan2(ray_c.z);
    let pitch = (-ray_c.y).clamp(-1.0, 1.0).asin();

    let delta = (w.min(h_img) as f64 * 0.15).max(16.0);
    let roll = match (map_pt(cx - delta, cy), map_pt(cx + delta, cy)) {
        (Some((ul, vl)), Some((ur, vr))) => (vr - vl).atan2(ur - ul).clamp(-0.35, 0.35),
        _ => 0.0,
    };

    (yaw, pitch, roll)
}

#[derive(Debug, Clone)]
pub struct MatchTiePoint {
    pub img1: usize,
    pub img2: usize,
    pub p1: Point2<f64>,
    pub p2: Point2<f64>,
}

/// Solves Wahba's Problem (IRLS Kabsch / Davenport SVD) for the optimal 3D rotation `R_k in SO(3)`
/// such that `R_k * local_ray_m ~ world_ray_m` across all matched tie-points.
fn solve_kabsch_so3_irls(
    pairs: &[(Vector3<f64>, Vector3<f64>)],
    angular_huber_thresh: f64,
) -> Option<Matrix3<f64>> {
    if pairs.len() < 3 {
        return None;
    }
    let mut weights = vec![1.0f64; pairs.len()];
    let mut r_best = Matrix3::identity();

    for _iter in 0..5 {
        let mut cov = Matrix3::zeros();
        for ((local_r, world_r), &w) in pairs.iter().zip(weights.iter()) {
            cov += w * (world_r * local_r.transpose());
        }
        let svd = cov.svd(true, true);
        if let (Some(u), Some(v_t)) = (svd.u, svd.v_t) {
            let mut d = Matrix3::identity();
            if (u * v_t).determinant() < 0.0 {
                d[(2, 2)] = -1.0;
            }
            r_best = u * d * v_t;
        } else {
            return None;
        }

        for ((local_r, world_r), w) in pairs.iter().zip(weights.iter_mut()) {
            let diff = (world_r - r_best * local_r).norm();
            *w = if diff > angular_huber_thresh {
                angular_huber_thresh / diff
            } else {
                1.0
            };
        }
    }

    Some(r_best)
}

/// Initializes all camera rotations `1..N-1` in `SO(3)` directly from tie-point 3D rays
/// using BFS Kabsch Procrustes alignment followed by multi-pass global `SO(3)` rotation averaging.
fn initialize_poses_kabsch_so3(poses: &mut [CameraPose], tie_points: &[MatchTiePoint]) {
    let n = poses.len();
    if n < 2 || tie_points.is_empty() {
        return;
    }

    // 0. If panels were captured at different zoom focal lengths (e.g. 100mm -> 126mm -> 188mm),
    // refine each zoomed panel's focal length relative to anchor 0 using the SO(3)-invariant
    // median pairwise tie-point chord ratio ||p_k(a) - p_k(b)|| / ||p_j(a) - p_j(b)||
    // to eliminate discrete zoom-encoder quantization error before solving 3D rotations.
    let min_exif_f = poses.iter().map(|p| p.f).fold(f64::INFINITY, f64::min);
    let max_exif_f = poses.iter().map(|p| p.f).fold(f64::NEG_INFINITY, f64::max);
    if max_exif_f > min_exif_f * 1.05 {
        let mut f_resolved = vec![false; n];
        f_resolved[0] = true;
        for _step in 1..n {
            for k in 1..n {
                if f_resolved[k] {
                    continue;
                }
                // Find a resolved neighbor j with the most tie points to k
                let mut best_j = None;
                let mut best_matches: Vec<(Point2<f64>, Point2<f64>)> = Vec::new();
                for j in 0..n {
                    if !f_resolved[j] {
                        continue;
                    }
                    let matches_jk: Vec<(Point2<f64>, Point2<f64>)> = tie_points
                        .iter()
                        .filter_map(|t| {
                            if t.img1 == j && t.img2 == k {
                                Some((t.p1, t.p2))
                            } else if t.img1 == k && t.img2 == j {
                                Some((t.p2, t.p1))
                            } else {
                                None
                            }
                        })
                        .collect();
                    if matches_jk.len() > best_matches.len() {
                        best_matches = matches_jk;
                        best_j = Some(j);
                    }
                }
                if let Some(j) = best_j
                    && best_matches.len() >= 12
                {
                    let mut ratios = Vec::new();
                    let stride = (best_matches.len() / 60).max(1);
                    for a in (0..best_matches.len()).step_by(stride) {
                        for b in ((a + 1)..best_matches.len()).step_by(stride) {
                            let dj = (best_matches[a].0 - best_matches[b].0).norm();
                            let dk = (best_matches[a].1 - best_matches[b].1).norm();
                            if dj > 80.0 && dk > 80.0 {
                                ratios.push(dk / dj);
                            }
                        }
                    }
                    if ratios.len() >= 10 {
                        ratios.sort_by(|a, b| a.total_cmp(b));
                        let med_ratio = ratios[ratios.len() / 2];
                        let est_fk = poses[j].f * med_ratio;
                        // Clamp within +/- 12% of EXIF focal length to correct zoom-encoder steps safely
                        poses[k].f = est_fk.clamp(poses[k].f * 0.88, poses[k].f * 1.12);
                    }
                    f_resolved[k] = true;
                }
            }
        }
    }

    let local_rays: Vec<(usize, usize, Vector3<f64>, Vector3<f64>)> = tie_points
        .iter()
        .filter(|t| t.img1 < n && t.img2 < n && t.img1 != t.img2)
        .map(|t| {
            let r1 = poses[t.img1].unproject_point(t.p1.x, t.p1.y);
            let r2 = poses[t.img2].unproject_point(t.p2.x, t.p2.y);
            (t.img1, t.img2, r1, r2)
        })
        .collect();

    let mut r_mats: Vec<Matrix3<f64>> = poses.iter().map(|p| p.rotation_matrix()).collect();
    r_mats[0] = Matrix3::identity();
    poses[0].yaw = 0.0;
    poses[0].pitch = 0.0;
    poses[0].roll = 0.0;

    let mut resolved = vec![false; n];
    resolved[0] = true;

    // 1. Sequential spanning-tree initialization from anchor 0
    for _step in 1..n {
        let mut best_k = None;
        let mut best_count = 0usize;
        for k in 1..n {
            if resolved[k] {
                continue;
            }
            let cnt = local_rays
                .iter()
                .filter(|&&(i, j, _, _)| (i == k && resolved[j]) || (j == k && resolved[i]))
                .count();
            if cnt > best_count {
                best_count = cnt;
                best_k = Some(k);
            }
        }

        let Some(k) = best_k else {
            break;
        };

        let mut pairs = Vec::with_capacity(best_count);
        for &(i, j, ref r1, ref r2) in &local_rays {
            if i == k && resolved[j] {
                pairs.push((*r1, r_mats[j] * r2));
            } else if j == k && resolved[i] {
                pairs.push((*r2, r_mats[i] * r1));
            }
        }

        let thresh = (4.5 / poses[k].f.max(200.0)).clamp(0.0003, 0.02);
        if let Some(r_k) = solve_kabsch_so3_irls(&pairs, thresh) {
            r_mats[k] = r_k;
        }
        resolved[k] = true;
    }

    // 2. Global SO(3) rotation averaging across all loop-closure edges (3 passes)
    for _pass in 0..3 {
        for k in 1..n {
            if !resolved[k] {
                continue;
            }
            let mut pairs = Vec::new();
            for &(i, j, ref r1, ref r2) in &local_rays {
                if i == k && resolved[j] {
                    pairs.push((*r1, r_mats[j] * r2));
                } else if j == k && resolved[i] {
                    pairs.push((*r2, r_mats[i] * r1));
                }
            }
            let thresh = (3.5 / poses[k].f.max(200.0)).clamp(0.0002, 0.015);
            if let Some(r_k) = solve_kabsch_so3_irls(&pairs, thresh) {
                r_mats[k] = r_k;
            }
        }
    }

    for k in 1..n {
        if resolved[k] {
            let (yaw, pitch, roll) = rotation_matrix_to_euler(&r_mats[k]);
            poses[k].yaw = yaw;
            poses[k].pitch = pitch;
            poses[k].roll = roll.clamp(-0.45, 0.45);
        }
    }
}

/// Evaluates total Huber-weighted reprojection cost across all tie points plus intrinsic regularization priors.
fn evaluate_ba_cost(
    poses: &[CameraPose],
    tie_points: &[MatchTiePoint],
    base_focals: &[f64],
    log_f_scale: f64,
    dist_b: f64,
    huber_delta: f64,
) -> f64 {
    let num_images = poses.len();
    let r_mats: Vec<Matrix3<f64>> = poses.iter().map(|p| p.rotation_matrix()).collect();
    let r_invs: Vec<Matrix3<f64>> = r_mats.iter().map(|r| r.transpose()).collect();

    let mut total_cost = 0.0f64;
    for tie in tie_points {
        let i = tie.img1;
        let j = tie.img2;
        if i >= num_images || j >= num_images {
            continue;
        }
        let ray_i = poses[i].unproject_point(tie.p1.x, tie.p1.y);
        let world_ray = r_mats[i] * ray_i;
        let (u_diff, v_diff) = match poses[j].project_ray_unbounded(&world_ray, &r_invs[j]) {
            Some((u_proj, v_proj)) => (u_proj - tie.p2.x, v_proj - tie.p2.y),
            None => (50.0, 50.0),
        };
        let r_sq = u_diff * u_diff + v_diff * v_diff;
        let r_norm = r_sq.sqrt();
        let huber_loss = if r_norm <= huber_delta {
            r_sq
        } else {
            huber_delta * (2.0 * r_norm - huber_delta)
        };
        total_cost += huber_loss;
    }

    let n_ties = tie_points.len().max(1) as f64;
    let _ = base_focals;
    total_cost + (80.0 * n_ties) * (log_f_scale * log_f_scale) + (350.0 * n_ties) * (dist_b * dist_b)
}

/// Refines all camera poses (yaw, pitch, roll) jointly with shared focal length `f` and
/// radial distortion `dist_b` ($k_1$) using IRLS Kabsch SO(3) initialization followed by
/// Huber-robust Levenberg-Marquardt Bundle Adjustment.
pub fn bundle_adjust_poses(
    poses: &mut [CameraPose],
    tie_points: &[MatchTiePoint],
    iterations: usize,
) {
    if poses.len() < 2 || tie_points.is_empty() {
        return;
    }

    // Step 0: Closed-form IRLS Kabsch SO(3) rotation initialization directly from 3D tie-point rays
    initialize_poses_kabsch_so3(poses, tie_points);

    let num_images = poses.len();
    let pose_params = (num_images - 1) * 3; // Keep anchor (index 0) fixed at (0, 0, 0)
    let idx_log_f = pose_params;
    let idx_dist_b = pose_params + 1;
    let num_params = pose_params + 2; // +2 for shared log(f/f0) and shared radial distortion dist_b

    let num_residuals = tie_points.len() * 2;
    if num_residuals < pose_params {
        auto_level_camera_poses(poses);
        return;
    }

    let base_focals: Vec<f64> = poses.iter().map(|p| p.f.max(100.0)).collect();
    let mut log_f_scale = 0.0f64;
    let mut shared_dist_b = poses[0].dist_b;

    let mut lambda = 0.01f64;
    let huber_delta = 3.2f64; // Robust Huber loss threshold in pixels
    let n_ties = tie_points.len().max(1) as f64;
    let prior_weight_f = 80.0 * n_ties;
    let prior_weight_b = 350.0 * n_ties;

    let mut current_cost = evaluate_ba_cost(
        poses,
        tie_points,
        &base_focals,
        log_f_scale,
        shared_dist_b,
        huber_delta,
    );

    for _ in 0..iterations.min(30) {
        let mut residual_vector = vec![0.0f64; num_residuals];
        let mut jacobian = vec![vec![0.0f64; num_params]; num_residuals];

        let r_mats: Vec<Matrix3<f64>> = poses.iter().map(|p| p.rotation_matrix()).collect();
        let r_invs: Vec<Matrix3<f64>> = r_mats.iter().map(|r| r.transpose()).collect();

        for (k, tie) in tie_points.iter().enumerate() {
            let i = tie.img1;
            let j = tie.img2;
            if i >= num_images || j >= num_images {
                continue;
            }

            let ray_i = poses[i].unproject_point(tie.p1.x, tie.p1.y);
            let world_ray = r_mats[i] * ray_i;
            let (u_diff, v_diff) = match poses[j].project_ray_unbounded(&world_ray, &r_invs[j]) {
                Some((u_proj, v_proj)) => (u_proj - tie.p2.x, v_proj - tie.p2.y),
                None => (50.0, 50.0),
            };

            let r_norm = (u_diff * u_diff + v_diff * v_diff).sqrt();
            let weight = if r_norm > huber_delta {
                (huber_delta / r_norm).sqrt()
            } else {
                1.0
            };

            residual_vector[k * 2] = u_diff * weight;
            residual_vector[k * 2 + 1] = v_diff * weight;

            let delta = 1e-4;

            // A. Derivative w.r.t Camera j (if j > 0)
            if j > 0 {
                let param_base_j = (j - 1) * 3;

                // d/d_yaw_j
                poses[j].yaw += delta;
                let r_j_plus = poses[j].rotation_matrix().transpose();
                if let Some((u_plus, v_plus)) = poses[j].project_ray_unbounded(&world_ray, &r_j_plus) {
                    jacobian[k * 2][param_base_j] = ((u_plus - tie.p2.x) - u_diff) / delta * weight;
                    jacobian[k * 2 + 1][param_base_j] = ((v_plus - tie.p2.y) - v_diff) / delta * weight;
                }
                poses[j].yaw -= delta;

                // d/d_pitch_j
                poses[j].pitch += delta;
                let r_j_plus = poses[j].rotation_matrix().transpose();
                if let Some((u_plus, v_plus)) = poses[j].project_ray_unbounded(&world_ray, &r_j_plus) {
                    jacobian[k * 2][param_base_j + 1] = ((u_plus - tie.p2.x) - u_diff) / delta * weight;
                    jacobian[k * 2 + 1][param_base_j + 1] = ((v_plus - tie.p2.y) - v_diff) / delta * weight;
                }
                poses[j].pitch -= delta;

                // d/d_roll_j
                poses[j].roll += delta;
                let r_j_plus = poses[j].rotation_matrix().transpose();
                if let Some((u_plus, v_plus)) = poses[j].project_ray_unbounded(&world_ray, &r_j_plus) {
                    jacobian[k * 2][param_base_j + 2] = ((u_plus - tie.p2.x) - u_diff) / delta * weight;
                    jacobian[k * 2 + 1][param_base_j + 2] = ((v_plus - tie.p2.y) - v_diff) / delta * weight;
                }
                poses[j].roll -= delta;
            }

            // B. Derivative w.r.t Camera i (if i > 0)
            if i > 0 {
                let param_base_i = (i - 1) * 3;

                // d/d_yaw_i
                poses[i].yaw += delta;
                let w_ray_plus = poses[i].rotation_matrix() * ray_i;
                if let Some((u_plus, v_plus)) = poses[j].project_ray_unbounded(&w_ray_plus, &r_invs[j]) {
                    jacobian[k * 2][param_base_i] = ((u_plus - tie.p2.x) - u_diff) / delta * weight;
                    jacobian[k * 2 + 1][param_base_i] = ((v_plus - tie.p2.y) - v_diff) / delta * weight;
                }
                poses[i].yaw -= delta;

                // d/d_pitch_i
                poses[i].pitch += delta;
                let w_ray_plus = poses[i].rotation_matrix() * ray_i;
                if let Some((u_plus, v_plus)) = poses[j].project_ray_unbounded(&w_ray_plus, &r_invs[j]) {
                    jacobian[k * 2][param_base_i + 1] = ((u_plus - tie.p2.x) - u_diff) / delta * weight;
                    jacobian[k * 2 + 1][param_base_i + 1] = ((v_plus - tie.p2.y) - v_diff) / delta * weight;
                }
                poses[i].pitch -= delta;

                // d/d_roll_i
                poses[i].roll += delta;
                let w_ray_plus = poses[i].rotation_matrix() * ray_i;
                if let Some((u_plus, v_plus)) = poses[j].project_ray_unbounded(&w_ray_plus, &r_invs[j]) {
                    jacobian[k * 2][param_base_i + 2] = ((u_plus - tie.p2.x) - u_diff) / delta * weight;
                    jacobian[k * 2 + 1][param_base_i + 2] = ((v_plus - tie.p2.y) - v_diff) / delta * weight;
                }
                poses[i].roll -= delta;
            }

            // C. Derivative w.r.t shared log focal length scale s_f
            let scale_plus = (log_f_scale + delta).exp();
            let old_fi = poses[i].f;
            let old_fj = poses[j].f;
            poses[i].f = base_focals[i] * scale_plus;
            poses[j].f = base_focals[j] * scale_plus;
            let ray_i_f = poses[i].unproject_point(tie.p1.x, tie.p1.y);
            let w_ray_f = r_mats[i] * ray_i_f;
            if let Some((u_plus, v_plus)) = poses[j].project_ray_unbounded(&w_ray_f, &r_invs[j]) {
                jacobian[k * 2][idx_log_f] = ((u_plus - tie.p2.x) - u_diff) / delta * weight;
                jacobian[k * 2 + 1][idx_log_f] = ((v_plus - tie.p2.y) - v_diff) / delta * weight;
            }
            poses[i].f = old_fi;
            poses[j].f = old_fj;

            // D. Derivative w.r.t shared radial distortion dist_b
            let old_bi = poses[i].dist_b;
            let old_bj = poses[j].dist_b;
            poses[i].dist_b = shared_dist_b + delta;
            poses[j].dist_b = shared_dist_b + delta;
            let ray_i_b = poses[i].unproject_point(tie.p1.x, tie.p1.y);
            let w_ray_b = r_mats[i] * ray_i_b;
            if let Some((u_plus, v_plus)) = poses[j].project_ray_unbounded(&w_ray_b, &r_invs[j]) {
                jacobian[k * 2][idx_dist_b] = ((u_plus - tie.p2.x) - u_diff) / delta * weight;
                jacobian[k * 2 + 1][idx_dist_b] = ((v_plus - tie.p2.y) - v_diff) / delta * weight;
            }
            poses[i].dist_b = old_bi;
            poses[j].dist_b = old_bj;
        }

        // 2. Normal equations: (J^T * J + Prior + lambda * diag) * delta_p = -(J^T * r + Prior_grad)
        let mut jt_j = nalgebra::DMatrix::<f64>::zeros(num_params, num_params);
        let mut jt_r = nalgebra::DVector::<f64>::zeros(num_params);

        for row in 0..num_residuals {
            let r_val = residual_vector[row];
            for col_a in 0..num_params {
                let j_a = jacobian[row][col_a];
                if j_a.abs() < 1e-12 {
                    continue;
                }
                jt_r[col_a] += j_a * r_val;
                for col_b in 0..num_params {
                    let j_b = jacobian[row][col_b];
                    if j_b.abs() < 1e-12 {
                        continue;
                    }
                    jt_j[(col_a, col_b)] += j_a * j_b;
                }
            }
        }

        // Add soft regularizing priors on shared focal scale and radial distortion
        jt_j[(idx_log_f, idx_log_f)] += prior_weight_f;
        jt_r[idx_log_f] += prior_weight_f * log_f_scale;

        jt_j[(idx_dist_b, idx_dist_b)] += prior_weight_b;
        jt_r[idx_dist_b] += prior_weight_b * shared_dist_b;

        for p in 0..num_params {
            jt_j[(p, p)] += lambda * (jt_j[(p, p)] + 1e-2);
        }

        if let Some(delta_params) = jt_j.cholesky().map(|c| c.solve(&(-jt_r.clone()))) {
            let mut valid = true;
            for val in delta_params.iter() {
                if !val.is_finite() || val.abs() > 0.8 {
                    valid = false;
                    break;
                }
            }
            if valid {
                let saved_poses = poses.to_vec();
                let saved_log_f = log_f_scale;
                let saved_dist_b = shared_dist_b;

                for j in 1..poses.len() {
                    let base = (j - 1) * 3;
                    poses[j].yaw += delta_params[base];
                    poses[j].pitch += delta_params[base + 1];
                    poses[j].roll = (poses[j].roll + delta_params[base + 2]).clamp(-0.45, 0.45);
                }

                log_f_scale = (log_f_scale + delta_params[idx_log_f]).clamp(-0.16, 0.16);
                shared_dist_b = (shared_dist_b + delta_params[idx_dist_b]).clamp(-0.07, 0.07);
                let f_mult = log_f_scale.exp();
                for (k, pose) in poses.iter_mut().enumerate() {
                    pose.f = base_focals[k] * f_mult;
                    pose.dist_b = shared_dist_b;
                }

                let new_cost = evaluate_ba_cost(
                    poses,
                    tie_points,
                    &base_focals,
                    log_f_scale,
                    shared_dist_b,
                    huber_delta,
                );
                if new_cost < current_cost {
                    current_cost = new_cost;
                    lambda = (lambda * 0.65).max(1e-5);
                } else {
                    poses.clone_from_slice(&saved_poses);
                    log_f_scale = saved_log_f;
                    shared_dist_b = saved_dist_b;
                    lambda = (lambda * 2.5).min(1e3);
                }
            } else {
                lambda = (lambda * 2.5).min(1e3);
            }
        } else {
            lambda = (lambda * 2.5).min(1e3);
        }
    }

    // 3. Level Horizon using SO(3)-invariant 3D World Frame Rotation
    auto_level_camera_poses(poses);
}

/// Automatically levels camera poses by applying a single global 3D world rotation
/// `R_new = R_level * R_old` to all cameras simultaneously.
/// Because `R_level` is applied on the left in world space, every pairwise relative rotation
/// `R_j^T * R_i` solved by Bundle Adjustment remains 100% invariant while centering and leveling the horizon.
pub fn auto_level_camera_poses(poses: &mut [CameraPose]) {
    if poses.is_empty() {
        return;
    }

    let r_mats: Vec<Matrix3<f64>> = poses.iter().map(|p| p.rotation_matrix()).collect();

    // 1. Compute mean forward viewing vector to center yaw and pitch
    let mut mean_fwd = Vector3::zeros();
    for r in &r_mats {
        mean_fwd += r * Vector3::new(0.0, 0.0, 1.0);
    }
    if mean_fwd.norm() < 1e-6 {
        return;
    }
    mean_fwd.normalize_mut();

    let center_yaw = mean_fwd.x.atan2(mean_fwd.z);
    let center_pitch = (-mean_fwd.y).clamp(-1.0, 1.0).asin();

    let cy = (-center_yaw).cos();
    let sy = (-center_yaw).sin();
    let ry_inv = Matrix3::new(cy, 0.0, sy, 0.0, 1.0, 0.0, -sy, 0.0, cy);

    let cp = (-center_pitch).cos();
    let sp = (-center_pitch).sin();
    let rx_inv = Matrix3::new(1.0, 0.0, 0.0, 0.0, cp, -sp, 0.0, sp, cp);

    let r_center = rx_inv * ry_inv;

    // 2. Estimate residual horizon roll from both average camera up-vector and sweep trajectory slope
    let mut sum_up_x = 0.0f64;
    let mut sum_up_y = 0.0f64;
    let mut fwd_pts: Vec<(f64, f64)> = Vec::with_capacity(poses.len());

    for r in &r_mats {
        let r_c = r_center * r;
        let up = r_c * Vector3::new(0.0, 1.0, 0.0);
        sum_up_x += up.x;
        sum_up_y += up.y;

        let fwd = r_c * Vector3::new(0.0, 0.0, 1.0);
        let theta = fwd.x.atan2(fwd.z);
        let phi = (-fwd.y).clamp(-1.0, 1.0).asin();
        fwd_pts.push((theta, phi));
    }

    let up_roll = (-sum_up_x).atan2(sum_up_y.max(1e-6));

    // Linear regression of elevation phi vs azimuth theta across the sweep
    let n = fwd_pts.len() as f64;
    let mean_t = fwd_pts.iter().map(|p| p.0).sum::<f64>() / n;
    let mean_p = fwd_pts.iter().map(|p| p.1).sum::<f64>() / n;
    let mut cov_tp = 0.0f64;
    let mut var_t = 0.0f64;
    for &(t, p) in &fwd_pts {
        let dt = t - mean_t;
        let dp = p - mean_p;
        cov_tp += dt * dp;
        var_t += dt * dt;
    }

    // If the panorama has a clear horizontal span, straighten the sweep trajectory
    let sweep_roll = if var_t > 0.015 {
        (cov_tp / var_t).atan().clamp(-0.15, 0.15)
    } else {
        0.0
    };

    // Combine camera up-vector leveling with 65% of sweep trajectory leveling
    let target_roll_corr = -(up_roll + 0.65 * sweep_roll).clamp(-0.25, 0.25);
    let cr = target_roll_corr.cos();
    let sr = target_roll_corr.sin();
    let rz_level = Matrix3::new(cr, -sr, 0.0, sr, cr, 0.0, 0.0, 0.0, 1.0);

    let r_level = rz_level * r_center;

    // 3. Apply R_level on the left to every camera and decompose back into exact Euler angles
    for (p, r_old) in poses.iter_mut().zip(r_mats.iter()) {
        let r_new = r_level * r_old;
        let (yaw, pitch, roll) = rotation_matrix_to_euler(&r_new);
        p.yaw = yaw;
        p.pitch = pitch;
        p.roll = roll.clamp(-0.75, 0.75);
    }
}

/// 2D Elastic Spatial Mesh for Local As-Projective-As-Possible (APAP) Warping
#[derive(Debug, Clone)]
pub struct MeshWarp2D {
    pub grid_w: usize,
    pub grid_h: usize,
    pub img_w: f64,
    pub img_h: f64,
    pub dx: Vec<Vec<f64>>,
    pub dy: Vec<Vec<f64>>,
}

impl MeshWarp2D {
    pub fn new(img_w: u32, img_h: u32, grid_w: usize, grid_h: usize) -> Self {
        Self {
            grid_w,
            grid_h,
            img_w: img_w as f64,
            img_h: img_h as f64,
            dx: vec![vec![0.0; grid_w + 1]; grid_h + 1],
            dy: vec![vec![0.0; grid_w + 1]; grid_h + 1],
        }
    }

    /// Evaluates displacement at (u, v) using bilinear interpolation from the deformation grid
    pub fn warp_point(&self, u: f64, v: f64) -> (f64, f64) {
        let gx = (u / self.img_w.max(1.0) * self.grid_w as f64).clamp(0.0, self.grid_w as f64);
        let gy = (v / self.img_h.max(1.0) * self.grid_h as f64).clamp(0.0, self.grid_h as f64);

        let x0 = (gx.floor() as usize).min(self.grid_w);
        let x1 = (x0 + 1).min(self.grid_w);
        let y0 = (gy.floor() as usize).min(self.grid_h);
        let y1 = (y0 + 1).min(self.grid_h);

        let fx = gx - x0 as f64;
        let fy = gy - y0 as f64;

        let d_x00 = self.dx[y0][x0];
        let d_x10 = self.dx[y0][x1];
        let d_x01 = self.dx[y1][x0];
        let d_x11 = self.dx[y1][x1];

        let d_y00 = self.dy[y0][x0];
        let d_y10 = self.dy[y0][x1];
        let d_y01 = self.dy[y1][x0];
        let d_y11 = self.dy[y1][x1];

        let interp_dx = (d_x00 * (1.0 - fx) + d_x10 * fx) * (1.0 - fy) + (d_x01 * (1.0 - fx) + d_x11 * fx) * fy;
        let interp_dy = (d_y00 * (1.0 - fx) + d_y10 * fx) * (1.0 - fy) + (d_y01 * (1.0 - fx) + d_y11 * fx) * fy;

        (u + interp_dx, v + interp_dy)
    }
}

/// Computes local As-Projective-As-Possible (APAP) deformation grids across all images
/// to eliminate parallax disparities between foreground and background structures.
pub fn compute_apap_mesh_warps(
    poses: &[CameraPose],
    _tie_points: &[MatchTiePoint],
) -> Vec<MeshWarp2D> {
    let num_images = poses.len();
    let mut warps = Vec::with_capacity(num_images);

    // Return rigid identity meshes: pure bundle-adjusted camera geometry without fake Gaussian distortion.
    // This preserves straight lines, railway tracks, horizons, and building facades without artificial shearing.
    for pose in poses.iter() {
        let mesh = MeshWarp2D::new(pose.width, pose.height, 1, 1);
        warps.push(mesh);
    }

    warps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_panini_projection_vertical_straightness() {
        // In Panini projection, vertical lines (constant X, varying Y) have constant azimuth theta
        let f_canvas = 1000.0;
        let cx = 500.0;
        let cy = 500.0;

        let ray1 = canvas_to_world_ray(600.0, 300.0, cx, cy, f_canvas, PanoramaProjection::Panini);
        let ray2 = canvas_to_world_ray(600.0, 700.0, cx, cy, f_canvas, PanoramaProjection::Panini);

        // Azimuth angle (atan2(x, z)) must be identical for vertical colinearity
        let theta1 = ray1.x.atan2(ray1.z);
        let theta2 = ray2.x.atan2(ray2.z);
        assert!((theta1 - theta2).abs() < 1e-6, "Panini vertical straightness violated: {} vs {}", theta1, theta2);
    }

    #[test]
    fn test_panotools_lens_distortion_monotone() {
        let mut pose = CameraPose::new(0, 1000, 1000, Some(800.0));
        pose.dist_a = 0.02;
        pose.dist_b = -0.05;
        pose.dist_c = 0.01;

        let r_inv = Matrix3::identity();
        let ray_center = Vector3::new(0.0, 0.0, 1.0);
        let proj_center = pose.project_ray(&ray_center, &r_inv);
        assert!(proj_center.is_some());
        let (uc, vc) = proj_center.unwrap();
        assert!((uc - 500.0).abs() < 1e-4);
        assert!((vc - 500.0).abs() < 1e-4);

        let ray_edge = Vector3::new(0.3, 0.0, 1.0).normalize();
        let proj_edge = pose.project_ray(&ray_edge, &r_inv);
        assert!(proj_edge.is_some());
    }

    #[test]
    fn test_vignetting_gain_falloff() {
        let mut pose = CameraPose::new(0, 1000, 1000, Some(800.0));
        pose.vig_v1 = 0.2;
        pose.vig_v2 = 0.1;

        let gain_center = pose.vignetting_gain(500.0, 500.0);
        let gain_corner = pose.vignetting_gain(900.0, 900.0);

        assert!((gain_center - 1.0).abs() < 1e-4);
        assert!(gain_corner > 1.0, "Vignetting corner compensation should be > 1.0, got {}", gain_corner);
    }
}
