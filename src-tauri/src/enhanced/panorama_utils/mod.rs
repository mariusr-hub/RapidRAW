pub mod camera_model;
pub mod gpu_blend;
pub mod gpu_warp;
pub mod graph_cut;
pub mod photometric;
pub mod processing;
pub mod stitching;
pub mod tile_runtime;

use crate::app::settings::AppSettings;
use crate::decode::formats::is_raw_file;
use crate::decode::image_loader::load_base_image_from_bytes;
use crate::enhanced::hdr_alignment::Feature;
use image::{GrayImage, Rgb32FImage};
use std::borrow::Cow;
use std::fs;

#[derive(Clone)]
pub struct MatchInfo {
    pub homography: nalgebra::Matrix3<f64>,
    pub inliers: usize,
    pub inlier_matches: Vec<crate::enhanced::hdr_alignment::Match>,
    pub is_subpixel: bool,
}

#[derive(Clone)]
pub struct ImageInfo {
    pub id: usize,
    pub filename: String,
    pub full_width: u32,
    pub full_height: u32,
    pub proxy_image: Rgb32FImage,
    pub full_image: Option<Rgb32FImage>,
    #[allow(dead_code)]
    pub low_detail_mask: GrayImage,
    pub scale_factor: f64,
    pub features: Vec<Feature>,
    pub exposure_gain: f32,
}

impl ImageInfo {
    pub fn get_panel_image<'a>(
        &'a self,
        settings: &AppSettings,
    ) -> Result<Cow<'a, Rgb32FImage>, String> {
        if let Some(ref img) = self.full_image {
            Ok(Cow::Borrowed(img))
        } else {
            let file_bytes = fs::read(&self.filename)
                .map_err(|e| format!("Failed to read image {}: {}", self.filename, e))?;
            let mut load_settings = settings.clone();
            if is_raw_file(&self.filename) {
                load_settings.raw_preprocessing_sharpening = Some(0.0);
            }
            let dynamic_image = load_base_image_from_bytes(
                &file_bytes,
                &self.filename,
                false,
                &load_settings,
                None,
            )
            .map_err(|e| format!("Failed to load image {}: {}", self.filename, e))?;
            Ok(Cow::Owned(dynamic_image.to_rgb32f()))
        }
    }
}

struct Dsu {
    parent: Vec<usize>,
}

impl Dsu {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
        }
    }

    fn find(&mut self, i: usize) -> usize {
        if self.parent[i] == i {
            i
        } else {
            self.parent[i] = self.find(self.parent[i]);
            self.parent[i]
        }
    }

    fn union(&mut self, i: usize, j: usize) {
        let root_i = self.find(i);
        let root_j = self.find(j);
        if root_i != root_j {
            self.parent[root_i] = root_j;
        }
    }
}

pub fn build_stitching_order(
    images: &[ImageInfo],
    matches: &std::collections::HashMap<(usize, usize), MatchInfo>,
) -> (Vec<usize>, std::collections::HashMap<usize, nalgebra::Matrix3<f64>>) {
    use nalgebra::Matrix3;
    use std::collections::{HashMap, HashSet, VecDeque};

    if images.is_empty() {
        return (vec![], HashMap::new());
    }
    let n = images.len();
    if n < 2 {
        let mut homographies = HashMap::new();
        if n == 1 {
            homographies.insert(0, Matrix3::identity());
        }
        return ((0..n).collect(), homographies);
    }

    let mut edges = Vec::new();
    for (&(i, j), m) in matches {
        edges.push((m.inliers, i, j));
    }
    edges.sort_by_key(|&(inliers, _, _)| std::cmp::Reverse(inliers));

    let mut mst_adj: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut dsu = Dsu::new(n);
    let mut num_edges = 0;

    for &(_, i, j) in &edges {
        if dsu.find(i) != dsu.find(j) {
            dsu.union(i, j);
            mst_adj.entry(i).or_default().push(j);
            mst_adj.entry(j).or_default().push(i);
            num_edges += 1;
            if num_edges == n - 1 {
                break;
            }
        }
    }

    if mst_adj.is_empty() {
        return (vec![], HashMap::new());
    }

    let mut component_sizes: HashMap<usize, usize> = HashMap::new();
    for i in 0..n {
        let root = dsu.find(i);
        *component_sizes.entry(root).or_default() += 1;
    }
    let largest_root = component_sizes
        .into_iter()
        .max_by_key(|&(_, count)| count)
        .map(|(root, _)| root)
        .unwrap_or(0);

    let candidate_nodes: Vec<usize> = mst_adj
        .keys()
        .copied()
        .filter(|&node| dsu.find(node) == largest_root)
        .collect();

    if candidate_nodes.is_empty() {
        return (vec![], HashMap::new());
    }

    let mut best_node = candidate_nodes[0];
    let mut min_max_dist = usize::MAX;

    for &candidate in &candidate_nodes {
        let mut dist_map: HashMap<usize, usize> = HashMap::new();
        let mut bfs_q = VecDeque::new();
        dist_map.insert(candidate, 0);
        bfs_q.push_back(candidate);

        let mut max_d = 0;
        while let Some(curr) = bfs_q.pop_front() {
            let d = dist_map[&curr];
            max_d = max_d.max(d);
            if let Some(neighbors) = mst_adj.get(&curr) {
                for &nxt in neighbors {
                    if !dist_map.contains_key(&nxt) {
                        dist_map.insert(nxt, d + 1);
                        bfs_q.push_back(nxt);
                    }
                }
            }
        }

        if max_d < min_max_dist {
            min_max_dist = max_d;
            best_node = candidate;
        }
    }

    let mut ordered_indices = Vec::new();
    let mut global_homographies = HashMap::new();
    let mut q = VecDeque::new();
    let mut visited = HashSet::new();

    q.push_back((best_node, Matrix3::identity()));
    visited.insert(best_node);

    while let Some((u, h_u_global)) = q.pop_front() {
        ordered_indices.push(u);
        global_homographies.insert(u, h_u_global);

        if let Some(neighbors) = mst_adj.get(&u) {
            for &v in neighbors {
                if !visited.contains(&v) {
                    visited.insert(v);
                    let h_vu = if let Some(m) = matches.get(&(v, u)) {
                        m.homography
                    } else if let Some(m) = matches.get(&(u, v)) {
                        m.homography.try_inverse().unwrap_or_else(Matrix3::identity)
                    } else {
                        Matrix3::identity()
                    };

                    let mut h_v_global: Matrix3<f64> = h_u_global * h_vu;
                    let h33 = h_v_global[(2, 2)];
                    if h33.abs() > 1e-6 {
                        h_v_global /= h33;
                    }
                    h_v_global[(2, 0)] = h_v_global[(2, 0)].clamp(-0.0003, 0.0003);
                    h_v_global[(2, 1)] = h_v_global[(2, 1)].clamp(-0.0003, 0.0003);
                    q.push_back((v, h_v_global));
                }
            }
        }
    }

    (ordered_indices, global_homographies)
}

pub fn stitch_images_headless(
    image_paths: Vec<String>,
    _projection: camera_model::PanoramaProjection,
    _boundary_warp: f32,
) -> Result<image::DynamicImage, String> {
    if image_paths.len() < 2 {
        return Err("Panorama stitching requires at least 2 images".to_string());
    }
    for p in &image_paths {
        if !std::path::Path::new(p).exists() {
            return Err(format!("Panorama input file does not exist: {}", p));
        }
    }
    Err("Headless panorama stitcher requires Tauri runtime context".to_string())
}

pub fn stitch_images_native_with_radiance(
    image_paths: Vec<String>,
    projection: camera_model::PanoramaProjection,
    boundary_warp: f32,
    _half_size: bool,
    _app_handle: Option<&tauri::AppHandle>,
    _cancel_token: Option<&std::sync::Arc<std::sync::atomic::AtomicBool>>,
) -> Result<(image::DynamicImage, image::Rgb32FImage), String> {
    let disp = stitch_images_headless(image_paths, projection, boundary_warp)?;
    let rgb32f = disp.to_rgb32f();
    Ok((disp, rgb32f))
}
