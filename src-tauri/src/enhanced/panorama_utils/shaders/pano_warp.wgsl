// Tiled Catmull-Rom backward projection compute shader
// Tailored for Intel Iris Xe Graphics (96 EUs) + UMA Unified Memory
// Tile size: 2048x2048 to bypass 16384px maximum texture dimensions

struct WarpUniforms {
    canvas_w: u32,
    canvas_h: u32,
    panel_w: u32,
    panel_h: u32,
    tile_x: u32,
    tile_y: u32,
    tile_w: u32,
    tile_h: u32,
    canvas_cx: f32,
    canvas_cy: f32,
    canvas_f: f32,
    mid_theta: f32,
    mid_phi: f32,
    projection_type: u32, // 0 = Cylindrical, 1 = Spherical, 2 = Planar, 3 = Panini, 4 = Stereographic
    gain_r: f32,
    gain_g: f32,
    gain_b: f32,
    cam_f: f32,
    cam_cx: f32,
    cam_cy: f32,
    vig_v1: f32,
    vig_v2: f32,
    _pad0: f32,
    _pad1: f32,
    r_inv_0: vec4<f32>, // Row 0 of inverse rotation matrix
    r_inv_1: vec4<f32>, // Row 1 of inverse rotation matrix
    r_inv_2: vec4<f32>, // Row 2 of inverse rotation matrix
    distortion: vec4<f32>, // dist_a, dist_b, dist_c, 1 - a - b - c
};

@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var dst_tex: texture_storage_2d<{{STORAGE_FORMAT}}, write>;
@group(0) @binding(2) var<uniform> uniforms: WarpUniforms;

fn catmull_rom_weight(t: f32) -> vec4<f32> {
    let t2 = t * t;
    let t3 = t2 * t;
    let w0 = 0.5 * (-t3 + 2.0 * t2 - t);
    let w1 = 0.5 * (3.0 * t3 - 5.0 * t2 + 2.0);
    let w2 = 0.5 * (-3.0 * t3 + 4.0 * t2 + t);
    let w3 = 0.5 * (t3 - t2);
    return vec4<f32>(w0, w1, w2, w3);
}

fn sample_catmull_rom(u: f32, v: f32, w: u32, h: u32) -> vec3<f32> {
    let px = floor(u - 0.5);
    let py = floor(v - 0.5);
    let fx = (u - 0.5) - px;
    let fy = (v - 0.5) - py;
    let wx = catmull_rom_weight(fx);
    let wy = catmull_rom_weight(fy);

    var color = vec3<f32>(0.0);
    let max_x = i32(w) - 1;
    let max_y = i32(h) - 1;

    for (var j = 0; j < 4; j = j + 1) {
        let coord_y = clamp(i32(py) - 1 + j, 0, max_y);
        var row_sum = vec3<f32>(0.0);
        for (var i = 0; i < 4; i = i + 1) {
            let coord_x = clamp(i32(px) - 1 + i, 0, max_x);
            let s = textureLoad(src_tex, vec2<i32>(coord_x, coord_y), 0).rgb;
            row_sum = row_sum + s * wx[i];
        }
        color = color + row_sum * wy[j];
    }
    return max(color, vec3<f32>(0.0));
}

fn vignetting_gain(u: f32, v: f32) -> f32 {
    if (uniforms.vig_v1 == 0.0 && uniforms.vig_v2 == 0.0) {
        return 1.0;
    }
    let max_r = f32(max(uniforms.panel_w, uniforms.panel_h)) * 0.6;
    let dx = (u - uniforms.cam_cx) / max_r;
    let dy = (v - uniforms.cam_cy) / max_r;
    let r_sq = min(dx * dx + dy * dy, 1.5);
    let boost = 1.0 + uniforms.vig_v1 * r_sq + uniforms.vig_v2 * r_sq * r_sq;
    return clamp(boost, 0.5, 3.0);
}

@compute @workgroup_size(16, 16, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= uniforms.tile_w || id.y >= uniforms.tile_h) {
        return;
    }

    let canvas_x = id.x + uniforms.tile_x;
    let canvas_y = id.y + uniforms.tile_y;

    if (canvas_x >= uniforms.canvas_w || canvas_y >= uniforms.canvas_h) {
        textureStore(dst_tex, vec2<i32>(i32(id.x), i32(id.y)), vec4<f32>(0.0, 0.0, 0.0, 0.0));
        return;
    }

    let d_theta = (f32(canvas_x) - uniforms.canvas_cx) / uniforms.canvas_f + uniforms.mid_theta;
    let h_cyl = (f32(canvas_y) - uniforms.canvas_cy) / uniforms.canvas_f;

    var world_ray: vec3<f32>;

    if (uniforms.projection_type == 0u) {
        // Cylindrical
        let sin_t = sin(d_theta);
        let cos_t = cos(d_theta);
        let cos_pitch = cos(uniforms.mid_phi);
        let sin_pitch = sin(uniforms.mid_phi);
        let rx = sin_t;
        let ry = h_cyl * cos_pitch + cos_t * sin_pitch;
        let rz = -h_cyl * sin_pitch + cos_t * cos_pitch;
        world_ray = normalize(vec3<f32>(rx, ry, rz));
    } else if (uniforms.projection_type == 1u) {
        // Spherical
        let phi_rad = (f32(canvas_y) - uniforms.canvas_cy) / uniforms.canvas_f + uniforms.mid_phi;
        let cos_p = cos(phi_rad);
        world_ray = vec3<f32>(cos_p * sin(d_theta), sin(phi_rad), cos_p * cos(d_theta));
    } else if (uniforms.projection_type == 2u) {
        // Planar
        let phi = (f32(canvas_y) - uniforms.canvas_cy) / uniforms.canvas_f + uniforms.mid_phi;
        let len = sqrt(d_theta * d_theta + phi * phi + 1.0);
        world_ray = vec3<f32>(d_theta / len, phi / len, 1.0 / len);
    } else if (uniforms.projection_type == 3u) {
        // Panini
        let theta = d_theta;
        let phi = (f32(canvas_y) - uniforms.canvas_cy) / uniforms.canvas_f + uniforms.mid_phi;
        let d = 1.0;
        let scale_v = (d + 1.0) / (d + max(cos(theta), 0.0001));
        let rx = sin(theta);
        let ry = phi / scale_v;
        let rz = cos(theta);
        world_ray = normalize(vec3<f32>(rx, ry, rz));
    } else {
        // Stereographic
        let theta = d_theta;
        let phi = (f32(canvas_y) - uniforms.canvas_cy) / uniforms.canvas_f + uniforms.mid_phi;
        let r = sqrt(theta * theta + phi * phi);
        if (r < 0.000001) {
            world_ray = vec3<f32>(0.0, 0.0, 1.0);
        } else {
            let ang = 2.0 * atan(r / 2.0);
            let sin_a = sin(ang);
            let cos_a = cos(ang);
            world_ray = normalize(vec3<f32>(sin_a * (theta / r), sin_a * (phi / r), cos_a));
        }
    }

    // Multiply by r_inv rows
    let cam_ray_x = dot(uniforms.r_inv_0.xyz, world_ray);
    let cam_ray_y = dot(uniforms.r_inv_1.xyz, world_ray);
    let cam_ray_z = dot(uniforms.r_inv_2.xyz, world_ray);

    if (cam_ray_z <= 0.01) {
        textureStore(dst_tex, vec2<i32>(i32(id.x), i32(id.y)), vec4<f32>(0.0, 0.0, 0.0, 0.0));
        return;
    }

    let u_norm = cam_ray_x / cam_ray_z;
    let v_norm = cam_ray_y / cam_ray_z;

    var u_corr = u_norm;
    var v_corr = v_norm;
    let r_sq = u_norm * u_norm + v_norm * v_norm;
    let r = sqrt(r_sq);

    if (r > 0.000001 && (uniforms.distortion.x != 0.0 || uniforms.distortion.y != 0.0 || uniforms.distortion.z != 0.0)) {
        let d = uniforms.distortion.w;
        let poly = uniforms.distortion.x * r_sq * r + uniforms.distortion.y * r_sq + uniforms.distortion.z * r + d;
        u_corr = u_norm * poly;
        v_corr = v_norm * poly;
    }

    let u = uniforms.cam_f * u_corr + uniforms.cam_cx;
    let v = uniforms.cam_f * v_corr + uniforms.cam_cy;

    if (u < 0.0 || u >= f32(uniforms.panel_w) || v < 0.0 || v >= f32(uniforms.panel_h)) {
        textureStore(dst_tex, vec2<i32>(i32(id.x), i32(id.y)), vec4<f32>(0.0, 0.0, 0.0, 0.0));
        return;
    }

    let raw_color = sample_catmull_rom(u, v, uniforms.panel_w, uniforms.panel_h);
    let vig = vignetting_gain(u, v);
    let gain = vec3<f32>(uniforms.gain_r, uniforms.gain_g, uniforms.gain_b);
    let final_rgb = max(raw_color * gain * vig, vec3<f32>(0.0));

    // Alpha = 1.0 indicates valid pixel mapped inside panel
    textureStore(dst_tex, vec2<i32>(i32(id.x), i32(id.y)), vec4<f32>(final_rgb, 1.0));
}
