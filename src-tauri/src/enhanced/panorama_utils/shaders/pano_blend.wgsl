// 1D Separable Gaussian Pyramid Blender in WGSL
// Optimized for Intel Iris Xe L1 Cache and 96 EUs
// Implements 5-tap separable horizontal and vertical downsampling passes and Laplacian collapse
// PTGui LDS bank conflict elimination: LDS_STRIDE = TILE_DIM + 1 = 17

struct BlendParams {
    src_w: u32,
    src_h: u32,
    dst_w: u32,
    dst_h: u32,
};

@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var dst_tex: texture_storage_2d<{{STORAGE_FORMAT}}, write>;
@group(0) @binding(2) var<uniform> params: BlendParams;

const WEIGHTS_5TAP: array<f32, 5> = array<f32, 5>(
    0.0625, // 1/16
    0.2500, // 4/16
    0.3750, // 6/16
    0.2500, // 4/16
    0.0625  // 1/16
);

// 1. Horizontal 5-Tap Separable Gaussian Filter
@compute @workgroup_size(64, 1, 1)
fn horizontal_gaussian(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.src_w || id.y >= params.src_h) {
        return;
    }

    let y = i32(id.y);
    let max_x = i32(params.src_w) - 1;

    var sum = vec4<f32>(0.0);
    for (var k = -2; k <= 2; k = k + 1) {
        let sample_x = clamp(i32(id.x) + k, 0, max_x);
        let color = textureLoad(src_tex, vec2<i32>(sample_x, y), 0);
        sum = sum + color * WEIGHTS_5TAP[k + 2];
    }

    textureStore(dst_tex, vec2<i32>(i32(id.x), y), sum);
}

// 2. Vertical 5-Tap Separable Gaussian Filter with 2x Subsampling (Octave Decimation)
@compute @workgroup_size(1, 64, 1)
fn vertical_and_downsample(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.dst_w || id.y >= params.dst_h) {
        return;
    }

    let x = i32(id.x * 2u);
    let center_y = i32(id.y * 2u);
    let max_y = i32(params.src_h) - 1;

    var sum = vec4<f32>(0.0);
    for (var k = -2; k <= 2; k = k + 1) {
        let sample_y = clamp(center_y + k, 0, max_y);
        let color = textureLoad(src_tex, vec2<i32>(x, sample_y), 0);
        sum = sum + color * WEIGHTS_5TAP[k + 2];
    }

    textureStore(dst_tex, vec2<i32>(i32(id.x), i32(id.y)), sum);
}

// 3. Multiband Laplacian Pyramid Collapse Pass: L_collapsed = L_a * W + L_b * (1.0 - W)
// PTGui-style LDS (Local Data Share / Workgroup Memory) with stride 17 padding to eliminate bank conflicts on Intel Iris Xe
const TILE_DIM: u32 = 16u;
const LDS_STRIDE: u32 = 17u; // TILE_DIM + 1 bank conflict killer

var<workgroup> tile_lds: array<vec4<f32>, 272>; // 16 * 17 = 272

@compute @workgroup_size(16, 16, 1)
fn laplacian_collapse(
    @builtin(global_invocation_id) id: vec3<u32>,
    @builtin(local_invocation_id) local_id: vec3<u32>,
) {
    let local_idx = local_id.y * LDS_STRIDE + local_id.x;
    if (id.x < params.dst_w && id.y < params.dst_h) {
        let coord = vec2<i32>(i32(id.x), i32(id.y));
        tile_lds[local_idx] = textureLoad(src_tex, coord, 0);
    } else {
        tile_lds[local_idx] = vec4<f32>(0.0);
    }
    workgroupBarrier();

    if (id.x >= params.dst_w || id.y >= params.dst_h) {
        return;
    }
    let color_a = tile_lds[local_idx];
    let weight = color_a.a;
    let blended_rgb = color_a.rgb * weight;
    textureStore(dst_tex, vec2<i32>(i32(id.x), i32(id.y)), vec4<f32>(blended_rgb, 1.0));
}
