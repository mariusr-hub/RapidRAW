use super::camera::{Camera, Lens};
use nalgebra::{Vector2, Vector3};
use serde::{Deserialize, Serialize};
use std::f64::consts::{FRAC_PI_2, PI, TAU};

const BORDER_SAMPLES: usize = 24;
const LONGITUDE_BINS: usize = 1440;
const PERSPECTIVE_MAX_ANGLE: f64 = 80.0 * PI / 180.0;
const PERSPECTIVE_AUTO_MAX_FOV: f64 = 100.0 * PI / 180.0;
const CYLINDRICAL_MAX_LATITUDE: f64 = 75.0 * PI / 180.0;
const CYLINDRICAL_AUTO_MAX_VFOV: f64 = 70.0 * PI / 180.0;
const PANINI_MAX_FOV: f64 = 155.0 * PI / 180.0;
const STEREOGRAPHIC_MAX_FOV: f64 = 150.0 * PI / 180.0;
const MAX_CANVAS_SIDE: f64 = 65_000.0;
const MAX_CANVAS_PIXELS: f64 = 1.0e9;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Projection {
    Perspective,
    Cylindrical,
    Spherical,
    Panini,
    Stereographic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ProjectionMode {
    #[default]
    Auto,
    Perspective,
    Cylindrical,
    Spherical,
    Panini,
    Stereographic,
}

impl Projection {
    #[inline]
    pub fn project(self, ray: &Vector3<f64>) -> Option<Vector2<f64>> {
        match self {
            Projection::Perspective => {
                (ray.z > 1e-9).then(|| Vector2::new(ray.x / ray.z, ray.y / ray.z))
            }
            Projection::Cylindrical => {
                let rho = ray.x.hypot(ray.z);
                (rho > 1e-9).then(|| Vector2::new(ray.x.atan2(ray.z), ray.y / rho))
            }
            Projection::Spherical => {
                let rho = ray.x.hypot(ray.z);
                Some(Vector2::new(ray.x.atan2(ray.z), ray.y.atan2(rho)))
            }
            Projection::Panini => {
                let rho = ray.x.hypot(ray.z);
                if rho <= 1e-9 {
                    return None;
                }
                let denom = 1.0 + ray.z / rho;
                (denom > 1e-4).then(|| {
                    let s = 2.0 / denom;
                    Vector2::new(s * (ray.x / rho), s * (ray.y / rho))
                })
            }
            Projection::Stereographic => {
                let r = ray.normalize();
                let denom = 1.0 + r.z;
                (denom > 1e-4).then(|| {
                    let k = 2.0 / denom;
                    Vector2::new(k * r.x, k * r.y)
                })
            }
        }
    }

    #[inline]
    pub fn unproject(self, u: f64, v: f64) -> Option<Vector3<f64>> {
        match self {
            Projection::Perspective => Some(Vector3::new(u, v, 1.0).normalize()),
            Projection::Cylindrical => {
                let (s, c) = u.sin_cos();
                Some(Vector3::new(s, v, c).normalize())
            }
            Projection::Spherical => {
                if v.abs() > FRAC_PI_2 {
                    return None;
                }
                let (su, cu) = u.sin_cos();
                let (sv, cv) = v.sin_cos();
                Some(Vector3::new(su * cv, sv, cu * cv))
            }
            Projection::Panini => {
                let lambda = 2.0 * (u * 0.5).atan();
                let (sin_l, cos_l) = lambda.sin_cos();
                let denom = 1.0 + cos_l;
                if denom <= 1e-4 {
                    return None;
                }
                let s = 2.0 / denom;
                let vy = v / s;
                Some(Vector3::new(sin_l, vy, cos_l).normalize())
            }
            Projection::Stereographic => {
                let r2 = u * u + v * v;
                let denom = 4.0 + r2;
                if denom <= 1e-9 {
                    return None;
                }
                Some(Vector3::new(4.0 * u / denom, 4.0 * v / denom, (4.0 - r2) / denom).normalize())
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Canvas {
    pub projection: Projection,
    pub scale: f64,
    pub origin: Vector2<f64>,
    pub width: f64,
    pub height: f64,
    pub wraps: bool,
}

impl Canvas {
    #[inline]
    pub fn ray_at(&self, x: f64, y: f64) -> Option<Vector3<f64>> {
        self.projection.unproject(
            self.origin.x + x / self.scale,
            self.origin.y + y / self.scale,
        )
    }
}

struct FieldOfView {
    longitude_start: f64,
    longitude_span: f64,
    latitude_min: f64,
    latitude_max: f64,
    full_circle: bool,
    pole_visible: bool,
    max_axis_angle: f64,
}

fn border_points(camera: &Camera) -> Vec<Vector2<f64>> {
    let (hw, hh) = (0.5 * camera.width, 0.5 * camera.height);
    let mut points = Vec::with_capacity(BORDER_SAMPLES * 4);
    for k in 0..BORDER_SAMPLES {
        let t = k as f64 / BORDER_SAMPLES as f64;
        points.push(Vector2::new(-hw + t * 2.0 * hw, -hh));
        points.push(Vector2::new(hw, -hh + t * 2.0 * hh));
        points.push(Vector2::new(hw - t * 2.0 * hw, hh));
        points.push(Vector2::new(-hw, hh - t * 2.0 * hh));
    }
    points
}

fn wrap_angle(a: f64) -> f64 {
    (a + PI).rem_euclid(TAU) - PI
}

fn field_of_view(cameras: &[Camera], lens: &Lens) -> FieldOfView {
    let mut covered = vec![false; LONGITUDE_BINS];
    let mut latitude_min = f64::MAX;
    let mut latitude_max = f64::MIN;
    let mut pole_visible = false;
    let mut max_axis_angle: f64 = 0.0;

    for camera in cameras {
        let mut pole_here = false;
        for pole in [Vector3::new(0.0, -1.0, 0.0), Vector3::new(0.0, 1.0, 0.0)] {
            if let Some(p) = camera.project(lens, &pole)
                && camera.contains(&p)
            {
                pole_here = true;
                if pole.y < 0.0 {
                    latitude_min = -FRAC_PI_2;
                } else {
                    latitude_max = FRAC_PI_2;
                }
            }
        }
        pole_visible |= pole_here;

        let center = camera.ray(lens, Vector2::zeros());
        let center_longitude = center.x.atan2(center.z);
        let mut lo = 0.0f64;
        let mut hi = 0.0f64;
        for p in border_points(camera) {
            let ray = camera.ray(lens, p);
            let longitude = wrap_angle(ray.x.atan2(ray.z) - center_longitude);
            let latitude = ray.y.atan2(ray.x.hypot(ray.z));
            lo = lo.min(longitude);
            hi = hi.max(longitude);
            latitude_min = latitude_min.min(latitude);
            latitude_max = latitude_max.max(latitude);
            max_axis_angle = max_axis_angle.max(ray.z.clamp(-1.0, 1.0).acos());
        }
        if pole_here {
            covered.iter_mut().for_each(|c| *c = true);
            continue;
        }
        let first = ((center_longitude + lo + PI) / TAU * LONGITUDE_BINS as f64).floor() as isize;
        let last = ((center_longitude + hi + PI) / TAU * LONGITUDE_BINS as f64).ceil() as isize;
        for bin in first..=last {
            covered[bin.rem_euclid(LONGITUDE_BINS as isize) as usize] = true;
        }
    }

    let mut best_gap = (0usize, 0usize);
    let mut run_start = None;
    for k in 0..2 * LONGITUDE_BINS {
        let bin = k % LONGITUDE_BINS;
        if !covered[bin] {
            run_start.get_or_insert(k);
            if let Some(start) = run_start {
                let length = (k - start + 1).min(LONGITUDE_BINS);
                if length > best_gap.1 {
                    best_gap = (start, length);
                }
            }
        } else {
            run_start = None;
        }
    }

    let bin_width = TAU / LONGITUDE_BINS as f64;
    let full_circle = best_gap.1 == 0;
    let (longitude_start, longitude_span) = if full_circle {
        (-PI, TAU)
    } else {
        let gap_end = (best_gap.0 + best_gap.1) % LONGITUDE_BINS;
        (
            gap_end as f64 * bin_width - PI,
            (LONGITUDE_BINS - best_gap.1) as f64 * bin_width,
        )
    };

    FieldOfView {
        longitude_start,
        longitude_span,
        latitude_min,
        latitude_max,
        full_circle,
        pole_visible,
        max_axis_angle,
    }
}

fn resolve_projection_with_info(mode: ProjectionMode, fov: &FieldOfView) -> (Projection, f32, Option<String>) {
    let fov_deg = (fov.longitude_span.to_degrees() as f32).max(1.0);
    let perspective_ok = !fov.full_circle && fov.max_axis_angle < PERSPECTIVE_MAX_ANGLE;
    let cylindrical_ok = !fov.pole_visible
        && fov.latitude_min > -CYLINDRICAL_MAX_LATITUDE
        && fov.latitude_max < CYLINDRICAL_MAX_LATITUDE;
    let panini_ok =
        !fov.full_circle && fov.longitude_span <= PANINI_MAX_FOV && !fov.pole_visible;
    let stereographic_ok =
        !fov.full_circle && fov.longitude_span <= STEREOGRAPHIC_MAX_FOV && fov.max_axis_angle < 130.0 * PI / 180.0;
    let vfov = fov.latitude_max - fov.latitude_min;
    let fallback = if cylindrical_ok && vfov <= CYLINDRICAL_AUTO_MAX_VFOV {
        Projection::Cylindrical
    } else {
        Projection::Spherical
    };
    let (projection, mut note) = match mode {
        ProjectionMode::Perspective if perspective_ok => (Projection::Perspective, None),
        ProjectionMode::Perspective => (
            fallback,
            Some(format!(
                "Horizontal FOV (~{:.0}°) exceeds rectilinear Perspective limit (~100°); switched to {}.",
                fov_deg,
                if fallback == Projection::Cylindrical { "Cylindrical" } else { "Spherical" }
            )),
        ),
        ProjectionMode::Cylindrical if cylindrical_ok => (Projection::Cylindrical, None),
        ProjectionMode::Cylindrical => (
            Projection::Spherical,
            Some("Vertical pitch approaches zenith/nadir pole; switched to Spherical.".to_string()),
        ),
        ProjectionMode::Spherical => (Projection::Spherical, None),
        ProjectionMode::Panini if panini_ok => (Projection::Panini, None),
        ProjectionMode::Panini => (
            Projection::Spherical,
            Some(format!(
                "Horizontal FOV (~{:.0}°) exceeds Panini 155° span; clamped to Spherical.",
                fov_deg
            )),
        ),
        ProjectionMode::Stereographic if stereographic_ok => (Projection::Stereographic, None),
        ProjectionMode::Stereographic => (
            Projection::Spherical,
            Some(format!(
                "Horizontal FOV (~{:.0}°) exceeds Stereographic 150° span; clamped to Spherical.",
                fov_deg
            )),
        ),
        ProjectionMode::Auto
            if perspective_ok
                && fov.longitude_span <= PERSPECTIVE_AUTO_MAX_FOV
                && vfov <= PERSPECTIVE_AUTO_MAX_FOV =>
        {
            (Projection::Perspective, None)
        }
        ProjectionMode::Auto => (fallback, None),
    };
    if note.is_none() && fov_deg < 45.0 && mode != ProjectionMode::Auto {
        note = Some(format!(
            "Narrow horizontal FOV ({:.0}°): projections converge closely when span is under 45°.",
            fov_deg
        ));
    }
    (projection, fov_deg, note)
}

pub fn plan_canvas_with_info(
    cameras: &[Camera],
    lens: &Lens,
    mode: ProjectionMode,
) -> (Canvas, f32, Option<String>) {
    let fov = field_of_view(cameras, lens);
    let (projection, fov_deg, note) = resolve_projection_with_info(mode, &fov);

    let mut focals: Vec<f64> = cameras.iter().map(|c| c.focal).collect();
    focals.sort_by(|a, b| a.total_cmp(b));
    let mut scale = focals[focals.len() / 2];

    let (min, max) = match projection {
        Projection::Perspective | Projection::Panini | Projection::Stereographic => {
            let mut min = Vector2::new(f64::MAX, f64::MAX);
            let mut max = Vector2::new(f64::MIN, f64::MIN);
            for camera in cameras {
                for p in border_points(camera) {
                    if let Some(q) = projection.project(&camera.ray(lens, p)) {
                        min = min.inf(&q);
                        max = max.sup(&q);
                    }
                }
            }
            (min, max)
        }
        Projection::Cylindrical | Projection::Spherical => {
            let to_v = |latitude: f64| match projection {
                Projection::Cylindrical => latitude.tan(),
                _ => latitude,
            };
            (
                Vector2::new(fov.longitude_start, to_v(fov.latitude_min)),
                Vector2::new(
                    fov.longitude_start + fov.longitude_span,
                    to_v(fov.latitude_max),
                ),
            )
        }
    };

    let extent = max - min;
    let side_limit = MAX_CANVAS_SIDE / (extent.x.max(extent.y) * scale);
    let area_limit = (MAX_CANVAS_PIXELS / (extent.x * extent.y * scale * scale)).sqrt();
    scale *= side_limit.min(area_limit).min(1.0);

    (
        Canvas {
            projection,
            scale,
            origin: min,
            width: (extent.x * scale).ceil().max(1.0),
            height: (extent.y * scale).ceil().max(1.0),
            wraps: fov.full_circle,
        },
        fov_deg,
        note,
    )
}

pub fn plan_canvas(cameras: &[Camera], lens: &Lens, mode: ProjectionMode) -> Canvas {
    plan_canvas_with_info(cameras, lens, mode).0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_panini_and_stereographic_roundtrip() {
        let ray = Vector3::new(0.45, -0.2, 0.87).normalize();
        for proj in [Projection::Panini, Projection::Stereographic] {
            let uv = proj.project(&ray).expect("should project front-hemisphere ray");
            let unproj = proj.unproject(uv.x, uv.y).expect("should unproject uv");
            assert!(
                (unproj - ray).norm() < 1e-6,
                "Round-trip mismatch: {:?} vs {:?}",
                unproj,
                ray
            );
        }
    }

    #[test]
    fn test_projection_fov_clamp_and_narrow_notes() {
        let wide_fov = FieldOfView {
            longitude_start: -1.6,
            longitude_span: 170.0_f64.to_radians(),
            latitude_min: -0.3,
            latitude_max: 0.3,
            full_circle: false,
            pole_visible: false,
            max_axis_angle: 1.5,
        };
        let (p_panini, _, note_panini) = resolve_projection_with_info(ProjectionMode::Panini, &wide_fov);
        assert_eq!(p_panini, Projection::Spherical);
        assert!(note_panini.as_deref().unwrap_or("").contains("155°"));

        let (p_stereo, _, note_stereo) =
            resolve_projection_with_info(ProjectionMode::Stereographic, &wide_fov);
        assert_eq!(p_stereo, Projection::Spherical);
        assert!(note_stereo.as_deref().unwrap_or("").contains("150°"));

        let narrow_fov = FieldOfView {
            longitude_start: -0.25,
            longitude_span: 32.0_f64.to_radians(),
            latitude_min: -0.2,
            latitude_max: 0.2,
            full_circle: false,
            pole_visible: false,
            max_axis_angle: 0.35,
        };
        let (p_narrow, fov_deg, note_narrow) =
            resolve_projection_with_info(ProjectionMode::Panini, &narrow_fov);
        assert_eq!(p_narrow, Projection::Panini);
        assert!(fov_deg < 45.0);
        assert!(note_narrow.as_deref().unwrap_or("").contains("45°"));
    }
}
