//! High-Speed Downscaled Proxy 2D Graph-Cut Seam Finder for RapidRAW
//!
//! Solves 2D optimal seam cuts avoiding moving subjects and high-frequency edges
//! on a lightweight downsampled proxy grid (< 20ms, < 15MB RAM), then upsamples
//! the optimal cut mask with distance transform guidance for Laplacian pyramid blending.

use image::{GrayImage, Rgb32FImage};
use std::collections::VecDeque;

const MAX_GRAPH_CUT_PROXY_DIM: u32 = 320;

#[derive(Debug, Clone)]
pub(crate) struct Edge {
    pub(crate) to: usize,
    pub(crate) capacity: f32,
    pub(crate) flow: f32,
    pub(crate) rev: usize,
}

pub(crate) struct DinicGraph {
    pub(crate) adj: Vec<Vec<Edge>>,
    pub(crate) level: Vec<i32>,
    pub(crate) ptr: Vec<usize>,
}

impl DinicGraph {
    pub(crate) fn new(n: usize) -> Self {
        Self {
            adj: vec![Vec::new(); n],
            level: vec![-1; n],
            ptr: vec![0; n],
        }
    }

    pub(crate) fn add_edge(&mut self, from: usize, to: usize, cap: f32) {
        let from_len = self.adj[from].len();
        let to_len = self.adj[to].len();
        self.adj[from].push(Edge {
            to,
            capacity: cap,
            flow: 0.0,
            rev: to_len,
        });
        self.adj[to].push(Edge {
            to: from,
            capacity: cap,
            flow: 0.0,
            rev: from_len,
        });
    }

    pub(crate) fn add_terminal_edge(&mut self, from: usize, to: usize, cap: f32) {
        let from_len = self.adj[from].len();
        let to_len = self.adj[to].len();
        self.adj[from].push(Edge {
            to,
            capacity: cap,
            flow: 0.0,
            rev: to_len,
        });
        self.adj[to].push(Edge {
            to: from,
            capacity: 0.0,
            flow: 0.0,
            rev: from_len,
        });
    }

    pub(crate) fn bfs(&mut self, s: usize, t: usize) -> bool {
        self.level.fill(-1);
        self.level[s] = 0;
        let mut q = VecDeque::new();
        q.push_back(s);

        while let Some(v) = q.pop_front() {
            for edge in &self.adj[v] {
                if edge.capacity - edge.flow > 1e-4 && self.level[edge.to] == -1 {
                    self.level[edge.to] = self.level[v] + 1;
                    q.push_back(edge.to);
                }
            }
        }
        self.level[t] != -1
    }

    pub(crate) fn dfs(&mut self, v: usize, t: usize, pushed: f32) -> f32 {
        if pushed < 1e-4 || v == t {
            return pushed;
        }
        for cid in self.ptr[v]..self.adj[v].len() {
            self.ptr[v] = cid;
            let edge = self.adj[v][cid].clone();
            let tr = edge.to;
            if self.level[v] + 1 != self.level[tr] || edge.capacity - edge.flow < 1e-4 {
                continue;
            }
            let tr_pushed = self.dfs(tr, t, pushed.min(edge.capacity - edge.flow));
            if tr_pushed < 1e-4 {
                continue;
            }
            self.adj[v][cid].flow += tr_pushed;
            let rev_idx = self.adj[v][cid].rev;
            self.adj[tr][rev_idx].flow -= tr_pushed;
            return tr_pushed;
        }
        0.0
    }

    pub(crate) fn max_flow(&mut self, s: usize, t: usize) -> f32 {
        let mut flow = 0.0;
        while self.bfs(s, t) {
            self.ptr.fill(0);
            loop {
                let tr_flow = self.dfs(s, t, f32::INFINITY);
                if tr_flow < 1e-4 {
                    break;
                }
                flow += tr_flow;
            }
        }
        flow
    }

    /// Returns boolean reachable vector from source (nodes belonging to S-partition)
    pub(crate) fn get_source_reachable(&self, s: usize) -> Vec<bool> {
        let mut visited = vec![false; self.adj.len()];
        let mut q = VecDeque::new();
        visited[s] = true;
        q.push_back(s);

        while let Some(v) = q.pop_front() {
            for edge in &self.adj[v] {
                if edge.capacity - edge.flow > 1e-4 && !visited[edge.to] {
                    visited[edge.to] = true;
                    q.push_back(edge.to);
                }
            }
        }
        visited
    }
}

/// Perceptual compression curve mapping linear or sRGB values in `[0, inf)` to `[0, 1)`
/// with balanced sensitivity across shadows, midtones, and bright highlights.
#[inline]
fn perceptual_channel(v: f32) -> f32 {
    let x = v.max(0.0);
    (x / (x + 0.18)).sqrt()
}

/// Finds the optimal 2D Graph-Cut seam mask between two registered images using Dinic Max-Flow / Min-Cut
/// on a downscaled overlap ROI proxy grid.
///
/// Routes seams around moving objects (vehicles, pedestrians, swaying branches) and crosses linear
/// structures where gradients and colors agree.
/// Returns a full-resolution float weight mask in `[0.0, 1.0]` where `1.0` = `pano_canvas`, `0.0` = `img_to_add`.
pub fn compute_2d_graphcut_seam_mask(
    pano_canvas: &Rgb32FImage,
    pano_valid_mask: &GrayImage,
    img_to_add: &Rgb32FImage,
    add_valid_mask: &GrayImage,
) -> Vec<f32> {
    let (w, h) = pano_canvas.dimensions();
    let num_pixels = (w * h) as usize;

    // 1. Calculate bounding box of intersection
    let mut min_ox = w;
    let mut max_ox = 0;
    let mut min_oy = h;
    let mut max_oy = 0;
    let mut has_overlap = false;

    for y in 0..h {
        for x in 0..w {
            let on_pano = pano_valid_mask.get_pixel(x, y)[0] > 0;
            let on_add = add_valid_mask.get_pixel(x, y)[0] > 0;
            if on_pano && on_add {
                has_overlap = true;
                min_ox = min_ox.min(x);
                max_ox = max_ox.max(x);
                min_oy = min_oy.min(y);
                max_oy = max_oy.max(y);
            }
        }
    }

    if !has_overlap || min_ox >= max_ox || min_oy >= max_oy {
        let mut mask = vec![0.0f32; num_pixels];
        for (i, val) in mask.iter_mut().enumerate() {
            let x = (i % w as usize) as u32;
            let y = (i / w as usize) as u32;
            if pano_valid_mask.get_pixel(x, y)[0] > 0 {
                *val = 1.0;
            }
        }
        return mask;
    }

    // 2. Expand ROI slightly so outer boundary ownership (`only_A` vs `only_B`) is captured
    let pad = 8u32;
    let roi_min_x = min_ox.saturating_sub(pad);
    let roi_max_x = (max_ox + pad).min(w - 1);
    let roi_min_y = min_oy.saturating_sub(pad);
    let roi_max_y = (max_oy + pad).min(h - 1);

    let roi_w = (roi_max_x - roi_min_x + 1) as usize;
    let roi_h = (roi_max_y - roi_min_y + 1) as usize;

    let max_dim = roi_w.max(roi_h) as u32;
    let step = ((max_dim as f32 / MAX_GRAPH_CUT_PROXY_DIM as f32).ceil() as usize).max(1);
    let pw = roi_w.div_ceil(step).max(2);
    let ph = roi_h.div_ceil(step).max(2);
    let p_len = pw * ph;

    // 3. Build downsampled proxy grids of validity masks and perceptual RGB colors
    let mut p_in_a = vec![false; p_len];
    let mut p_in_b = vec![false; p_len];
    let mut p_col_a = vec![[0.0f32; 3]; p_len];
    let mut p_col_b = vec![[0.0f32; 3]; p_len];
    let mut p_lum_a = vec![0.0f32; p_len];
    let mut p_lum_b = vec![0.0f32; p_len];

    for py in 0..ph {
        let y0 = roi_min_y as usize + py * step;
        let y1 = (y0 + step).min(roi_max_y as usize + 1);
        for px in 0..pw {
            let x0 = roi_min_x as usize + px * step;
            let x1 = (x0 + step).min(roi_max_x as usize + 1);

            let mut cnt_a = 0usize;
            let mut cnt_b = 0usize;
            let mut cnt_both = 0usize;
            let mut tot = 0usize;
            let mut sum_a = [0.0f32; 3];
            let mut sum_b = [0.0f32; 3];

            for cy in y0..y1 {
                for cx in x0..x1 {
                    tot += 1;
                    let ia = pano_valid_mask.get_pixel(cx as u32, cy as u32)[0] > 0;
                    let ib = add_valid_mask.get_pixel(cx as u32, cy as u32)[0] > 0;
                    if ia {
                        cnt_a += 1;
                        let pa = pano_canvas.get_pixel(cx as u32, cy as u32);
                        sum_a[0] += perceptual_channel(pa[0]);
                        sum_a[1] += perceptual_channel(pa[1]);
                        sum_a[2] += perceptual_channel(pa[2]);
                    }
                    if ib {
                        cnt_b += 1;
                        let pb = img_to_add.get_pixel(cx as u32, cy as u32);
                        sum_b[0] += perceptual_channel(pb[0]);
                        sum_b[1] += perceptual_channel(pb[1]);
                        sum_b[2] += perceptual_channel(pb[2]);
                    }
                    if ia && ib {
                        cnt_both += 1;
                    }
                }
            }

            let pidx = py * pw + px;
            if cnt_both * 2 >= tot.max(1) {
                p_in_a[pidx] = true;
                p_in_b[pidx] = true;
            } else {
                p_in_a[pidx] = cnt_a * 2 >= tot.max(1);
                p_in_b[pidx] = cnt_b * 2 >= tot.max(1);
            }

            if cnt_a > 0 {
                let inv = 1.0 / cnt_a as f32;
                p_col_a[pidx] = [sum_a[0] * inv, sum_a[1] * inv, sum_a[2] * inv];
                p_lum_a[pidx] =
                    0.2126 * p_col_a[pidx][0] + 0.7152 * p_col_a[pidx][1] + 0.0722 * p_col_a[pidx][2];
            }
            if cnt_b > 0 {
                let inv = 1.0 / cnt_b as f32;
                p_col_b[pidx] = [sum_b[0] * inv, sum_b[1] * inv, sum_b[2] * inv];
                p_lum_b[pidx] =
                    0.2126 * p_col_b[pidx][0] + 0.7152 * p_col_b[pidx][1] + 0.0722 * p_col_b[pidx][2];
            }
        }
    }

    // 4. Compute Sobel edge gradients and Chamfer distance transforms on the proxy grid
    let mut grad_a = vec![0.0f32; p_len];
    let mut grad_b = vec![0.0f32; p_len];
    let mut dist_a = vec![1e5f32; p_len];
    let mut dist_b = vec![1e5f32; p_len];

    for py in 0..ph {
        for px in 0..pw {
            let idx = py * pw + px;
            if !p_in_a[idx] {
                dist_a[idx] = 0.0;
            }
            if !p_in_b[idx] {
                dist_b[idx] = 0.0;
            }
            if p_in_a[idx] && p_in_b[idx] {
                let xl = px.saturating_sub(1);
                let xr = (px + 1).min(pw - 1);
                let yu = py.saturating_sub(1);
                let yd = (py + 1).min(ph - 1);

                let gx_a = p_lum_a[py * pw + xr] - p_lum_a[py * pw + xl];
                let gy_a = p_lum_a[yd * pw + px] - p_lum_a[yu * pw + px];
                grad_a[idx] = (gx_a * gx_a + gy_a * gy_a).sqrt();

                let gx_b = p_lum_b[py * pw + xr] - p_lum_b[py * pw + xl];
                let gy_b = p_lum_b[yd * pw + px] - p_lum_b[yu * pw + px];
                grad_b[idx] = (gx_b * gx_b + gy_b * gy_b).sqrt();
            }
        }
    }

    // Forward & backward distance pass on proxy grid
    for py in 0..ph {
        for px in 0..pw {
            let idx = py * pw + px;
            if px > 0 {
                dist_a[idx] = dist_a[idx].min(dist_a[idx - 1] + 1.0);
                dist_b[idx] = dist_b[idx].min(dist_b[idx - 1] + 1.0);
            }
            if py > 0 {
                let up = (py - 1) * pw + px;
                dist_a[idx] = dist_a[idx].min(dist_a[up] + 1.0);
                dist_b[idx] = dist_b[idx].min(dist_b[up] + 1.0);
            }
        }
    }
    for py in (0..ph).rev() {
        for px in (0..pw).rev() {
            let idx = py * pw + px;
            if px + 1 < pw {
                dist_a[idx] = dist_a[idx].min(dist_a[idx + 1] + 1.0);
                dist_b[idx] = dist_b[idx].min(dist_b[idx + 1] + 1.0);
            }
            if py + 1 < ph {
                let dn = (py + 1) * pw + px;
                dist_a[idx] = dist_a[idx].min(dist_a[dn] + 1.0);
                dist_b[idx] = dist_b[idx].min(dist_b[dn] + 1.0);
            }
        }
    }

    // 5. Assign node IDs to overlap cells and construct Dinic Graph
    let mut node_id = vec![usize::MAX; p_len];
    let mut num_overlap_nodes = 0usize;
    for idx in 0..p_len {
        if p_in_a[idx] && p_in_b[idx] {
            node_id[idx] = num_overlap_nodes;
            num_overlap_nodes += 1;
        }
    }

    let mut proxy_mask = vec![0.0f32; p_len];
    for idx in 0..p_len {
        if p_in_a[idx] && !p_in_b[idx] {
            proxy_mask[idx] = 1.0;
        } else if !p_in_a[idx] && p_in_b[idx] {
            proxy_mask[idx] = 0.0;
        } else if p_in_a[idx] && p_in_b[idx] {
            let da = dist_a[idx];
            let db = dist_b[idx];
            proxy_mask[idx] = if da + db > 1e-4 { da / (da + db) } else { 0.5 };
        }
    }

    if num_overlap_nodes >= 4 {
        let source = num_overlap_nodes;
        let sink = num_overlap_nodes + 1;
        let mut graph = DinicGraph::new(num_overlap_nodes + 2);

        // Compute per-node seam penalty M(p)
        let mut node_cost = vec![0.002f32; p_len];
        for idx in 0..p_len {
            if node_id[idx] == usize::MAX {
                continue;
            }
            let ca = p_col_a[idx];
            let cb = p_col_b[idx];
            let dr = ca[0] - cb[0];
            let dg = ca[1] - cb[1];
            let db_c = ca[2] - cb[2];
            let raw_d_col = (dr * dr + dg * dg + db_c * db_c).sqrt();

            // Normalize color difference by local brightness so the min-cut cannot take
            // a cheap shortcut through dark shadow silhouettes near a panel border.
            let lum_mean = 0.5 * (p_lum_a[idx] + p_lum_b[idx]);
            let d_col = raw_d_col / (0.14 + 0.86 * lum_mean);

            let ga = grad_a[idx];
            let gb = grad_b[idx];
            let d_grad = (ga - gb).abs() + 0.55 * (ga + gb) * (d_col * 6.0).min(1.0);

            // Quadratic boost on large color differences routes seams cleanly around moving trucks/people
            let motion_boost = (d_col * 4.5).powi(2);

            // Penalize cuts near the extreme outer boundaries of the overlap (ratio -> 0 or 1)
            let da = dist_a[idx];
            let db = dist_b[idx];
            let ratio = da / (da + db + 1e-4);
            let border_penalty = 1.0 + 3.5 * (2.0 * ratio - 1.0).powi(2);

            node_cost[idx] = (0.002 + d_col + 2.0 * d_grad + motion_boost) * border_penalty;
        }

        let mut pinned = vec![false; p_len];

        for py in 0..ph {
            for px in 0..pw {
                let idx = py * pw + px;
                let u = node_id[idx];
                if u == usize::MAX {
                    continue;
                }

                let mut touches_only_a = false;
                let mut touches_only_b = false;

                let neighbors = [
                    (px.wrapping_sub(1), py),
                    (px + 1, py),
                    (px, py.wrapping_sub(1)),
                    (px, py + 1),
                ];

                for &(nx, ny) in &neighbors {
                    if nx < pw && ny < ph {
                        let nidx = ny * pw + nx;
                        if p_in_a[nidx] && !p_in_b[nidx] {
                            touches_only_a = true;
                        } else if !p_in_a[nidx] && p_in_b[nidx] {
                            touches_only_b = true;
                        }
                    }
                }

                let da = dist_a[idx];
                let db = dist_b[idx];
                let ratio = da / (da + db + 1e-4);

                if (touches_only_a && !touches_only_b)
                    || (db <= 3.0 && da > db + 3.0)
                    || (ratio > 0.86 && da > 6.0)
                {
                    graph.add_terminal_edge(source, u, 1e6);
                    pinned[idx] = true;
                } else if (touches_only_b && !touches_only_a)
                    || (da <= 3.0 && db > da + 3.0)
                    || (ratio < 0.14 && db > 6.0)
                {
                    graph.add_terminal_edge(u, sink, 1e6);
                    pinned[idx] = true;
                } else if ratio > 0.65 {
                    let t_cap = ((ratio - 0.65) / 0.21).powi(2) * 1.2;
                    graph.add_terminal_edge(source, u, t_cap);
                } else if ratio < 0.35 {
                    let t_cap = ((0.35 - ratio) / 0.21).powi(2) * 1.2;
                    graph.add_terminal_edge(u, sink, t_cap);
                }

                // Add undirected N-links to right and bottom neighbors
                if px + 1 < pw {
                    let r_idx = py * pw + (px + 1);
                    let v = node_id[r_idx];
                    if v != usize::MAX {
                        let cap = 0.5 * (node_cost[idx] + node_cost[r_idx]);
                        graph.add_edge(u, v, cap);
                    }
                }
                if py + 1 < ph {
                    let d_idx = (py + 1) * pw + px;
                    let v = node_id[d_idx];
                    if v != usize::MAX {
                        let cap = 0.5 * (node_cost[idx] + node_cost[d_idx]);
                        graph.add_edge(u, v, cap);
                    }
                }
            }
        }

        let _flow = graph.max_flow(source, sink);
        let reachable = graph.get_source_reachable(source);

        for idx in 0..p_len {
            let u = node_id[idx];
            if u != usize::MAX {
                proxy_mask[idx] = if reachable[u] { 1.0 } else { 0.0 };
            }
        }

        // 1 pass of 3x3 tent anti-aliasing on unpinned overlap cells so upsampled seam contour is smooth
        let prev_mask = proxy_mask.clone();
        for py in 1..(ph.saturating_sub(1)) {
            for px in 1..(pw.saturating_sub(1)) {
                let idx = py * pw + px;
                if node_id[idx] != usize::MAX && !pinned[idx] {
                    let sum = prev_mask[idx] * 4.0
                        + (prev_mask[idx - 1]
                            + prev_mask[idx + 1]
                            + prev_mask[idx - pw]
                            + prev_mask[idx + pw])
                            * 1.5
                        + (prev_mask[idx - pw - 1]
                            + prev_mask[idx - pw + 1]
                            + prev_mask[idx + pw - 1]
                            + prev_mask[idx + pw + 1])
                            * 0.5;
                    proxy_mask[idx] = (sum / 12.0).clamp(0.0, 1.0);
                }
            }
        }
    }

    // 6. Bilinearly upsample the optimal Graph-Cut proxy mask to full resolution
    let mut full_mask = vec![0.0f32; num_pixels];
    let inv_step = 1.0 / step as f32;

    for y in 0..h as usize {
        for x in 0..w as usize {
            let on_pano = pano_valid_mask.get_pixel(x as u32, y as u32)[0] > 0;
            let on_add = add_valid_mask.get_pixel(x as u32, y as u32)[0] > 0;
            let idx = y * w as usize + x;

            if on_pano && !on_add {
                full_mask[idx] = 1.0;
            } else if !on_pano && on_add {
                full_mask[idx] = 0.0;
            } else if on_pano && on_add {
                let rx = x.saturating_sub(roi_min_x as usize) as f32;
                let ry = y.saturating_sub(roi_min_y as usize) as f32;

                let gx = ((rx + 0.5) * inv_step - 0.5).clamp(0.0, (pw - 1) as f32);
                let gy = ((ry + 0.5) * inv_step - 0.5).clamp(0.0, (ph - 1) as f32);

                let x0 = gx.floor() as usize;
                let y0 = gy.floor() as usize;
                let x1 = (x0 + 1).min(pw - 1);
                let y1 = (y0 + 1).min(ph - 1);
                let fx = gx - x0 as f32;
                let fy = gy - y0 as f32;

                let v00 = proxy_mask[y0 * pw + x0];
                let v10 = proxy_mask[y0 * pw + x1];
                let v01 = proxy_mask[y1 * pw + x0];
                let v11 = proxy_mask[y1 * pw + x1];

                let top = v00 * (1.0 - fx) + v10 * fx;
                let bot = v01 * (1.0 - fx) + v11 * fx;
                full_mask[idx] = (top * (1.0 - fy) + bot * fy).clamp(0.0, 1.0);
            }
        }
    }

    full_mask
}
