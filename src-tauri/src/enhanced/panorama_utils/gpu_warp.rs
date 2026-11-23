//! Tiled GPU Backward Warper for RapidRAW Panoramas
//!
//! Executes backward Catmull-Rom bicubic ray-traced projection on Intel Iris Xe (96 EUs)
//! in 2048x2048 tiles to bypass maximum texture dimensions (16,384px) and prevent VRAM exhaustion.
//! Features automatic Rgba16Float storage format selection with transparent Rgba32Float fallback.

use crate::enhanced::panorama_utils::camera_model::{CameraPose, PanoramaProjection};
use image::{GrayImage, Rgb32FImage};
use nalgebra::Vector3;
use pollster::FutureExt;

pub const WARP_TILE_SIZE: u32 = 2048;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuWarpUniforms {
    pub canvas_w: u32,
    pub canvas_h: u32,
    pub panel_w: u32,
    pub panel_h: u32,
    pub tile_x: u32,
    pub tile_y: u32,
    pub tile_w: u32,
    pub tile_h: u32,
    pub canvas_cx: f32,
    pub canvas_cy: f32,
    pub canvas_f: f32,
    pub mid_theta: f32,
    pub mid_phi: f32,
    pub projection_type: u32,
    pub gain_r: f32,
    pub gain_g: f32,
    pub gain_b: f32,
    pub cam_f: f32,
    pub cam_cx: f32,
    pub cam_cy: f32,
    pub vig_v1: f32,
    pub vig_v2: f32,
    pub _pad0: f32,
    pub _pad1: f32,
    pub r_inv_0: [f32; 4],
    pub r_inv_1: [f32; 4],
    pub r_inv_2: [f32; 4],
    pub distortion: [f32; 4],
}

const PANO_WARP_WGSL_TEMPLATE: &str = include_str!("shaders/pano_warp.wgsl");

/// Selects storage texture format based on available features on the device.
pub fn select_storage_texture_format(device: &wgpu::Device) -> wgpu::TextureFormat {
    if device
        .features()
        .contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES)
    {
        wgpu::TextureFormat::Rgba16Float
    } else {
        wgpu::TextureFormat::Rgba32Float
    }
}

/// Warps a single panel image onto the canvas using the GPU tiled backward warper.
/// Automatically falls back to high-accuracy CPU Catmull-Rom projection if GPU is absent or encounters error.
pub fn warp_panel_tiled(
    device_opt: Option<(&wgpu::Device, &wgpu::Queue)>,
    src_image: &Rgb32FImage,
    pose: &CameraPose,
    gain: [f32; 3],
    canvas_cx: f64,
    canvas_cy: f64,
    canvas_f: f64,
    mid_theta: f64,
    mid_phi: f64,
    projection: PanoramaProjection,
    panel_img: &mut Rgb32FImage,
    panel_mask: &mut GrayImage,
) -> Result<(), String> {
    if let Some((device, queue)) = device_opt {
        match warp_panel_gpu_internal(
            device,
            queue,
            src_image,
            pose,
            gain,
            canvas_cx,
            canvas_cy,
            canvas_f,
            mid_theta,
            mid_phi,
            projection,
            panel_img,
            panel_mask,
        ) {
            Ok(_) => return Ok(()),
            Err(e) => {
                log::warn!("GPU backward warper fell back to CPU: {}", e);
            }
        }
    }

    // CPU Fallback
    warp_panel_cpu_fallback(
        src_image,
        pose,
        gain,
        canvas_cx,
        canvas_cy,
        canvas_f,
        mid_theta,
        mid_phi,
        projection,
        panel_img,
        panel_mask,
    );
    Ok(())
}

fn warp_panel_gpu_internal(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    src_image: &Rgb32FImage,
    pose: &CameraPose,
    gain: [f32; 3],
    canvas_cx: f64,
    canvas_cy: f64,
    canvas_f: f64,
    mid_theta: f64,
    mid_phi: f64,
    projection: PanoramaProjection,
    panel_img: &mut Rgb32FImage,
    panel_mask: &mut GrayImage,
) -> Result<(), String> {
    let (src_w, src_h) = src_image.dimensions();
    let (canvas_w, canvas_h) = panel_img.dimensions();

    let storage_format = select_storage_texture_format(device);
    let format_str = match storage_format {
        wgpu::TextureFormat::Rgba16Float => "rgba16float",
        _ => "rgba32float",
    };
    let shader_source = PANO_WARP_WGSL_TEMPLATE.replace("{{STORAGE_FORMAT}}", format_str);

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Pano Warp Compute Shader"),
        source: wgpu::ShaderSource::Wgsl(shader_source.into()),
    });

    // 1. Create source texture (RGBA32F)
    let mut rgba_src = Vec::with_capacity((src_w * src_h * 4) as usize);
    for pixel in src_image.pixels() {
        rgba_src.push(pixel[0]);
        rgba_src.push(pixel[1]);
        rgba_src.push(pixel[2]);
        rgba_src.push(1.0f32);
    }

    let src_tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Warp Src Texture"),
        size: wgpu::Extent3d {
            width: src_w,
            height: src_h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &src_tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bytemuck::cast_slice(&rgba_src),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(src_w * 16),
            rows_per_image: Some(src_h),
        },
        wgpu::Extent3d {
            width: src_w,
            height: src_h,
            depth_or_array_layers: 1,
        },
    );

    let src_view = src_tex.create_view(&wgpu::TextureViewDescriptor::default());

    // 2. Prepare pipeline layout
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Pano Warp BGL"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: storage_format,
                    view_dimension: wgpu::TextureViewDimension::D2,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Pano Warp Pipeline Layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });

    let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("Pano Warp Pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });

    // 3. Setup tile destination texture and staging buffer
    let tile_max_w = WARP_TILE_SIZE;
    let tile_max_h = WARP_TILE_SIZE;

    let dst_tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Warp Tile Dst Texture"),
        size: wgpu::Extent3d {
            width: tile_max_w,
            height: tile_max_h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: storage_format,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let dst_view = dst_tex.create_view(&wgpu::TextureViewDescriptor::default());

    let bytes_per_pixel = match storage_format {
        wgpu::TextureFormat::Rgba16Float => 8u32,
        _ => 16u32,
    };
    let bytes_per_row = tile_max_w * bytes_per_pixel;
    let staging_buffer_size = (bytes_per_row * tile_max_h) as u64;

    let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Warp Tile Staging Buffer"),
        size: staging_buffer_size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let r_inv = pose.rotation_matrix().transpose();
    let d = 1.0 - pose.dist_a - pose.dist_b - pose.dist_c;

    let proj_type_u32 = match projection {
        PanoramaProjection::Cylindrical => 0,
        PanoramaProjection::Spherical => 1,
        PanoramaProjection::Planar => 2,
        PanoramaProjection::Panini => 3,
        PanoramaProjection::Stereographic => 4,
    };

    // 4. Iterate over tiles across canvas
    let tiles_x = canvas_w.div_ceil(WARP_TILE_SIZE);
    let tiles_y = canvas_h.div_ceil(WARP_TILE_SIZE);

    for ty in 0..tiles_y {
        for tx in 0..tiles_x {
            let tile_x = tx * WARP_TILE_SIZE;
            let tile_y = ty * WARP_TILE_SIZE;
            let tile_w = (canvas_w - tile_x).min(WARP_TILE_SIZE);
            let tile_h = (canvas_h - tile_y).min(WARP_TILE_SIZE);

            let uniforms = GpuWarpUniforms {
                canvas_w,
                canvas_h,
                panel_w: src_w,
                panel_h: src_h,
                tile_x,
                tile_y,
                tile_w,
                tile_h,
                canvas_cx: canvas_cx as f32,
                canvas_cy: canvas_cy as f32,
                canvas_f: canvas_f as f32,
                mid_theta: mid_theta as f32,
                mid_phi: mid_phi as f32,
                projection_type: proj_type_u32,
                gain_r: gain[0],
                gain_g: gain[1],
                gain_b: gain[2],
                cam_f: pose.f as f32,
                cam_cx: pose.cx as f32,
                cam_cy: pose.cy as f32,
                vig_v1: pose.vig_v1 as f32,
                vig_v2: pose.vig_v2 as f32,
                _pad0: 0.0,
                _pad1: 0.0,
                r_inv_0: [r_inv[(0, 0)] as f32, r_inv[(0, 1)] as f32, r_inv[(0, 2)] as f32, 0.0],
                r_inv_1: [r_inv[(1, 0)] as f32, r_inv[(1, 1)] as f32, r_inv[(1, 2)] as f32, 0.0],
                r_inv_2: [r_inv[(2, 0)] as f32, r_inv[(2, 1)] as f32, r_inv[(2, 2)] as f32, 0.0],
                distortion: [pose.dist_a as f32, pose.dist_b as f32, pose.dist_c as f32, d as f32],
            };

            let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Warp Uniform Buffer"),
                size: std::mem::size_of::<GpuWarpUniforms>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Pano Warp Bind Group"),
                layout: &bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&src_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&dst_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: uniform_buffer.as_entire_binding(),
                    },
                ],
            });

            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Warp Command Encoder"),
            });

            {
                let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("Pano Warp Pass"),
                    timestamp_writes: None,
                });
                compute_pass.set_pipeline(&compute_pipeline);
                compute_pass.set_bind_group(0, &bind_group, &[]);
                compute_pass.dispatch_workgroups(tile_w.div_ceil(16), tile_h.div_ceil(16), 1);
            }

            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &dst_tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &staging_buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(bytes_per_row),
                        rows_per_image: Some(tile_max_h),
                    },
                },
                wgpu::Extent3d {
                    width: tile_w,
                    height: tile_h,
                    depth_or_array_layers: 1,
                },
            );

            queue.submit(Some(encoder.finish()));

            // Map buffer and copy to output image
            let buffer_slice = staging_buffer.slice(..);
            let (sender, receiver) = futures::channel::oneshot::channel();
            buffer_slice.map_async(wgpu::MapMode::Read, move |res| {
                let _ = sender.send(res);
            });
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: None,
                })
                .map_err(|e| e.to_string())?;
            receiver.block_on().map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;

            {
                let mapped = buffer_slice.get_mapped_range();
                match storage_format {
                    wgpu::TextureFormat::Rgba16Float => {
                        let f16_slice: &[half::f16] = bytemuck::cast_slice(&mapped);
                        let row_f16_count = (bytes_per_row / 2) as usize;
                        for y in 0..tile_h {
                            let src_row = &f16_slice[y as usize * row_f16_count..];
                            let cy = tile_y + y;
                            for x in 0..tile_w {
                                let cx = tile_x + x;
                                let a = src_row[x as usize * 4 + 3].to_f32();
                                if a > 0.0 {
                                    let r = src_row[x as usize * 4].to_f32();
                                    let g = src_row[x as usize * 4 + 1].to_f32();
                                    let b = src_row[x as usize * 4 + 2].to_f32();
                                    panel_img.put_pixel(cx, cy, image::Rgb([r, g, b]));
                                    panel_mask.put_pixel(cx, cy, image::Luma([255]));
                                }
                            }
                        }
                    }
                    _ => {
                        let f32_slice: &[f32] = bytemuck::cast_slice(&mapped);
                        let row_f32_count = (bytes_per_row / 4) as usize;
                        for y in 0..tile_h {
                            let src_row = &f32_slice[y as usize * row_f32_count..];
                            let cy = tile_y + y;
                            for x in 0..tile_w {
                                let cx = tile_x + x;
                                let a = src_row[x as usize * 4 + 3];
                                if a > 0.0 {
                                    let r = src_row[x as usize * 4];
                                    let g = src_row[x as usize * 4 + 1];
                                    let b = src_row[x as usize * 4 + 2];
                                    panel_img.put_pixel(cx, cy, image::Rgb([r, g, b]));
                                    panel_mask.put_pixel(cx, cy, image::Luma([255]));
                                }
                            }
                        }
                    }
                }
            }
            staging_buffer.unmap();
        }
    }

    Ok(())
}

fn warp_panel_cpu_fallback(
    src_img: &Rgb32FImage,
    pose: &CameraPose,
    gain: [f32; 3],
    canvas_cx: f64,
    canvas_cy: f64,
    canvas_f: f64,
    mid_theta: f64,
    mid_phi: f64,
    active_projection: PanoramaProjection,
    panel_img: &mut Rgb32FImage,
    panel_mask: &mut GrayImage,
) {
    use crate::enhanced::panorama_utils::stitching::get_catmull_rom_bicubic_pixel;
    use rayon::prelude::*;

    let (out_width, _) = panel_img.dimensions();
    let r_inv = pose.rotation_matrix().transpose();
    let num_pixels_per_row = out_width as usize * 3;

    panel_img
        .par_chunks_mut(num_pixels_per_row)
        .zip(panel_mask.par_chunks_mut(out_width as usize))
        .enumerate()
        .for_each(|(y, (row_slice, mask_row))| {
            for x in 0..out_width {
                let d_theta = (x as f64 - canvas_cx) / canvas_f + mid_theta;
                let h_cyl = (y as f64 - canvas_cy) / canvas_f;

                let world_ray = match active_projection {
                    PanoramaProjection::Cylindrical => {
                        let sin_t = d_theta.sin();
                        let cos_t = d_theta.cos();
                        let cos_pitch = mid_phi.cos();
                        let sin_pitch = mid_phi.sin();
                        let ray_x = sin_t;
                        let ray_y = h_cyl * cos_pitch + cos_t * sin_pitch;
                        let ray_z = -h_cyl * sin_pitch + cos_t * cos_pitch;
                        Vector3::new(ray_x, ray_y, ray_z).normalize()
                    }
                    PanoramaProjection::Spherical => {
                        let phi_rad = (y as f64 - canvas_cy) / canvas_f + mid_phi;
                        let cos_p = phi_rad.cos();
                        Vector3::new(cos_p * d_theta.sin(), phi_rad.sin(), cos_p * d_theta.cos())
                    }
                    PanoramaProjection::Planar => {
                        let phi = (y as f64 - canvas_cy) / canvas_f + mid_phi;
                        let len = (d_theta * d_theta + phi * phi + 1.0).sqrt();
                        Vector3::new(d_theta / len, phi / len, 1.0 / len)
                    }
                    PanoramaProjection::Panini => {
                        let theta = d_theta;
                        let phi = (y as f64 - canvas_cy) / canvas_f + mid_phi;
                        let d = 1.0;
                        let scale_v = (d + 1.0) / (d + theta.cos().max(1e-4));
                        let ray_x = theta.sin();
                        let ray_y = phi / scale_v;
                        let ray_z = theta.cos();
                        Vector3::new(ray_x, ray_y, ray_z).normalize()
                    }
                    PanoramaProjection::Stereographic => {
                        let theta = d_theta;
                        let phi = (y as f64 - canvas_cy) / canvas_f + mid_phi;
                        let r = (theta * theta + phi * phi).sqrt();
                        if r < 1e-6 {
                            Vector3::new(0.0, 0.0, 1.0)
                        } else {
                            let ang: f64 = 2.0 * (r / 2.0).atan();
                            let sin_a = ang.sin();
                            let cos_a = ang.cos();
                            Vector3::new(sin_a * (theta / r), sin_a * (phi / r), cos_a).normalize()
                        }
                    }
                };

                if let Some((u, v)) = pose.project_ray(&world_ray, &r_inv) {
                    let color = get_catmull_rom_bicubic_pixel(src_img, u, v);
                    let vig = pose.vignetting_gain(u, v);
                    let start = x as usize * 3;
                    row_slice[start] = (color.0[0] * gain[0] * vig).max(0.0);
                    row_slice[start + 1] = (color.0[1] * gain[1] * vig).max(0.0);
                    row_slice[start + 2] = (color.0[2] * gain[2] * vig).max(0.0);
                    mask_row[x as usize] = 255;
                }
            }
        });
}
