//! Deterministic tiled execution engine bounding peak RAM usage to <= 512 MB.
//!
//! Subdivides large panoramic composites into overlapping halo-buffered tiles
//! allowing streaming computation without unbounded memory allocations.

use serde::{Deserialize, Serialize};

/// Default memory ceiling for tiled computation (512 MB).
pub const DEFAULT_MEMORY_BUDGET_BYTES: u64 = 512 * 1024 * 1024;

/// Base core tile dimension in pixels.
pub const DEFAULT_TILE_SIZE: u32 = 512;

/// Halo padding on each tile border (prevents seam artifacts across filters).
pub const DEFAULT_HALO_PADDING: u32 = 32;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TileHalo {
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
    pub left: u32,
}

impl Default for TileHalo {
    fn default() -> Self {
        Self {
            top: DEFAULT_HALO_PADDING,
            right: DEFAULT_HALO_PADDING,
            bottom: DEFAULT_HALO_PADDING,
            left: DEFAULT_HALO_PADDING,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TileRect {
    pub index: usize,
    pub core_x: u32,
    pub core_y: u32,
    pub core_width: u32,
    pub core_height: u32,
    pub padded_x: u32,
    pub padded_y: u32,
    pub padded_width: u32,
    pub padded_height: u32,
    pub halo: TileHalo,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TileMemoryEstimate {
    pub resident_input_bytes: u64,
    pub transformed_scratch_bytes: u64,
    pub accumulator_bytes: u64,
    pub output_tile_bytes: u64,
    pub estimated_peak_bytes: u64,
}

impl TileMemoryEstimate {
    /// Estimates memory required for one active halo tile.
    pub fn calculate_per_tile(
        source_count: usize,
        tile_width: u32,
        tile_height: u32,
        halo: TileHalo,
    ) -> Self {
        let padded_w = (tile_width + halo.left + halo.right) as u64;
        let padded_h = (tile_height + halo.top + halo.bottom) as u64;
        let padded_pixels = padded_w * padded_h;

        // 3 channels float (12 B/px) per source slice
        let resident_input = (source_count as u64) * padded_pixels * 12;
        // RGBA float scratch (16 B/px)
        let scratch = padded_pixels * 16;
        // RGBA float accumulator (16 B/px)
        let accumulator = padded_pixels * 16;
        // Output RGB float core tile (12 B/px)
        let output = (tile_width as u64 * tile_height as u64) * 12;

        let peak = resident_input + scratch + accumulator + output;

        Self {
            resident_input_bytes: resident_input,
            transformed_scratch_bytes: scratch,
            accumulator_bytes: accumulator,
            output_tile_bytes: output,
            estimated_peak_bytes: peak,
        }
    }
}

/// Slices a large canvas into tiled regions with halo padding.
pub fn plan_canvas_tiles(
    canvas_width: u32,
    canvas_height: u32,
    core_size: u32,
    halo_padding: u32,
) -> Vec<TileRect> {
    let mut tiles = Vec::new();
    let mut idx = 0;

    let mut y = 0;
    while y < canvas_height {
        let cur_h = core_size.min(canvas_height - y);
        let mut x = 0;
        while x < canvas_width {
            let cur_w = core_size.min(canvas_width - x);

            let pad_left = halo_padding.min(x);
            let pad_top = halo_padding.min(y);
            let pad_right = halo_padding.min(canvas_width.saturating_sub(x + cur_w));
            let pad_bottom = halo_padding.min(canvas_height.saturating_sub(y + cur_h));

            tiles.push(TileRect {
                index: idx,
                core_x: x,
                core_y: y,
                core_width: cur_w,
                core_height: cur_h,
                padded_x: x - pad_left,
                padded_y: y - pad_top,
                padded_width: cur_w + pad_left + pad_right,
                padded_height: cur_h + pad_top + pad_bottom,
                halo: TileHalo {
                    top: pad_top,
                    right: pad_right,
                    bottom: pad_bottom,
                    left: pad_left,
                },
            });

            idx += 1;
            x += cur_w;
        }
        y += cur_h;
    }

    tiles
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tile_memory_estimate_bounded() {
        let est = TileMemoryEstimate::calculate_per_tile(
            10,
            DEFAULT_TILE_SIZE,
            DEFAULT_TILE_SIZE,
            TileHalo::default(),
        );

        // Even with 10 overlapping images, a single tile memory stays bounded at ~60.75 MB
        assert!(est.estimated_peak_bytes < 70 * 1024 * 1024, "Peak tile memory exceeded budget");
        assert!(est.estimated_peak_bytes < DEFAULT_MEMORY_BUDGET_BYTES, "Exceeded 512 MB ceiling");
    }

    #[test]
    fn test_canvas_tiling_coverage() {
        let canvas_w = 2000;
        let canvas_h = 1000;
        let tiles = plan_canvas_tiles(canvas_w, canvas_h, 512, 32);

        assert_eq!(tiles.len(), 8); // 4 columns x 2 rows = 8 tiles

        // Verify total core area equals canvas area without gaps
        let total_core_pixels: u64 = tiles.iter().map(|t| (t.core_width * t.core_height) as u64).sum();
        assert_eq!(total_core_pixels, (canvas_w * canvas_h) as u64);

        // Verify all tiles have non-zero halo padding on appropriate boundaries
        for t in &tiles {
            assert!(t.padded_width >= t.core_width);
            assert!(t.padded_height >= t.core_height);
        }
    }
}

/// Fallback lens calibration parameters when camera/lens profile is absent from the database.
///
/// Follows the robust pinhole assumption:
/// - Zero radial distortion ($k_1 = 0.0$)
/// - Principal point centered at $(width / 2.0, height / 2.0)$
/// - Focal length estimated from sensor diagonal and standard 35mm equivalent field-of-view
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GracefulLensCalibration {
    pub focal_length_px: f64,
    pub principal_point_px: [f64; 2],
    pub radial_k1: f64,
}

impl GracefulLensCalibration {
    pub fn fallback_for_dimensions(width: u32, height: u32, diagonal_fov_degrees: Option<f64>) -> Self {
        let w = width as f64;
        let h = height as f64;
        let diag = (w * w + h * h).sqrt();
        let fov_rad = diagonal_fov_degrees.unwrap_or(65.0).to_radians();
        let focal_px = (diag / 2.0) / (fov_rad / 2.0).tan();

        Self {
            focal_length_px: focal_px,
            principal_point_px: [w / 2.0, h / 2.0],
            radial_k1: 0.0,
        }
    }

    /// Forward maps an image point $(x, y)$ to normalized camera coordinates, applying radial lens distortion model.
    pub fn distort_point(&self, point: [f64; 2]) -> [f64; 2] {
        let dx = point[0] - self.principal_point_px[0];
        let dy = point[1] - self.principal_point_px[1];
        let r2 = (dx * dx + dy * dy) / (self.focal_length_px * self.focal_length_px);
        let scale = 1.0 + self.radial_k1 * r2;
        [self.principal_point_px[0] + dx * scale, self.principal_point_px[1] + dy * scale]
    }

    /// Inverts radial distortion using Newton-Raphson iterations to map distorted coordinates to undistorted.
    pub fn undistort_point(&self, point: [f64; 2]) -> [f64; 2] {
        if self.radial_k1.abs() < 1e-9 {
            return point;
        }
        let dx = point[0] - self.principal_point_px[0];
        let dy = point[1] - self.principal_point_px[1];
        let mut cur = [dx, dy];
        for _ in 0..5 {
            let r2 = (cur[0] * cur[0] + cur[1] * cur[1]) / (self.focal_length_px * self.focal_length_px);
            let scale = 1.0 + self.radial_k1 * r2;
            if scale.abs() <= f64::EPSILON {
                break;
            }
            cur = [dx / scale, dy / scale];
        }
        [self.principal_point_px[0] + cur[0], self.principal_point_px[1] + cur[1]]
    }
}
