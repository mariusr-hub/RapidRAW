use crate::render::gpu::context::GpuContext;
use crate::render::gpu::processor::GpuProcessor;
use std::sync::{Arc, Mutex};

enum CheckError {
    NoAdapter(String),
    Failed(String),
}

fn check(backend: &str) -> Result<String, CheckError> {
    let backends = match backend.to_ascii_lowercase().as_str() {
        "vulkan" => wgpu::Backends::VULKAN,
        "gl" => wgpu::Backends::GL,
        "dx12" => wgpu::Backends::DX12,
        "metal" => wgpu::Backends::METAL,
        other => return Err(CheckError::Failed(format!("Unknown backend '{}'", other))),
    };
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
    descriptor.backends = backends;
    let instance = wgpu::Instance::new(descriptor);
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    }))
    .map_err(|e| CheckError::NoAdapter(e.to_string()))?;
    let info = adapter.get_info();

    let mut required_features = wgpu::Features::empty();
    if adapter
        .features()
        .contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES)
    {
        required_features |= wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;
    }
    let limits = adapter.limits();
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("Shader Check Device"),
        required_features,
        required_limits: limits.clone(),
        experimental_features: wgpu::ExperimentalFeatures::default(),
        memory_hints: wgpu::MemoryHints::Performance,
        trace: wgpu::Trace::Off,
    }))
    .map_err(|e| CheckError::Failed(format!("Could not open a {} device: {}", backend, e)))?;

    let errors = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = Arc::clone(&errors);
    device.on_uncaptured_error(Arc::new(move |error: wgpu::Error| {
        sink.lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(error.to_string());
    }));

    let device = Arc::new(device);
    let context = GpuContext {
        instance,
        adapter,
        device: Arc::clone(&device),
        queue: Arc::new(queue),
        limits,
        backend: info.backend,
        display: Default::default(),
        viewer_display: Default::default(),
        pipelines: Default::default(),
        compute: Default::default(),
    };

    GpuProcessor::new(context, 512, 512)
        .map_err(CheckError::Failed)?
        .warm_all_shaders();
    crate::render::gpu::compute::ComputePipelines::new(&device).warm(&device);
    let decode_errors = [
        crate::decode::demosaic::warm(&device),
        crate::decode::develop::warm(&device),
    ]
    .into_iter()
    .filter_map(|built| built.err().map(|e| e.to_string()));
    crate::render::gpu::display::warm_display_pipelines(&device);
    let _ = device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    });

    let mut errors = errors.lock().unwrap_or_else(|e| e.into_inner());
    errors.extend(decode_errors);
    let adapter_name = format!("{} ({:?}, {:?})", info.name, info.backend, info.device_type);
    if errors.is_empty() {
        Ok(format!(
            "{}: all shaders and pipelines compiled",
            adapter_name
        ))
    } else {
        Err(CheckError::Failed(format!(
            "{}:\n{}",
            adapter_name,
            errors.join("\n\n")
        )))
    }
}

fn default_backend() -> &'static str {
    if cfg!(any(target_os = "macos", target_os = "ios")) {
        "metal"
    } else if cfg!(target_os = "windows") {
        "dx12"
    } else {
        "vulkan"
    }
}

#[test]
fn shaders_compile_on_gpu_backends() {
    let requested = std::env::var("RAPIDRAW_SHADER_BACKENDS").ok();
    let backends = requested
        .clone()
        .unwrap_or_else(|| default_backend().to_string());
    let mut failures = Vec::new();
    for backend in backends.split_whitespace() {
        match check(backend) {
            Ok(report) => println!("[{}] {}", backend, report),
            Err(CheckError::NoAdapter(reason)) if requested.is_none() => {
                println!("[{}] skipped, no adapter: {}", backend, reason);
            }
            Err(CheckError::NoAdapter(reason)) => {
                failures.push(format!("[{}] no adapter: {}", backend, reason));
            }
            Err(CheckError::Failed(reason)) => failures.push(format!("[{}] {}", backend, reason)),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn test_wgsl_shader_compiles_with_tetrahedral_hsl_and_soft_proof() {
    let main_src = crate::render::gpu::pipelines::output_shader_source(
        crate::render::gpu::processor::RenderOutputPrecision::EightBit,
    );
    let disp_src = crate::render::color_space::with_shader_prelude(include_str!("../../shaders/display.wgsl"));
    assert!(main_src.contains("hsl_mode: u32"));
    assert!(main_src.contains("hsl_skin_lock: f32"));
    assert!(main_src.contains("fn apply_hsl_tetrahedral"));
    assert!(main_src.contains("fn compute_skin_lock_weight"));
    assert!(disp_src.contains("soft_proof_oog_opacity: f32"));

    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
    descriptor.backends = match default_backend() {
        "dx12" => wgpu::Backends::DX12,
        "metal" => wgpu::Backends::METAL,
        _ => wgpu::Backends::VULKAN,
    };
    let instance = wgpu::Instance::new(descriptor);
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    })) else {
        println!("[shader_check] no GPU adapter in test env, skipping device compile");
        return;
    };
    let Ok((device, _queue)) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("WGSL Module Check Device"),
        required_features: wgpu::Features::empty(),
        required_limits: adapter.limits(),
        experimental_features: wgpu::ExperimentalFeatures::default(),
        memory_hints: wgpu::MemoryHints::Performance,
        trace: wgpu::Trace::Off,
    })) else {
        println!("[shader_check] could not open GPU device, skipping device compile");
        return;
    };

    let errors = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = Arc::clone(&errors);
    device.on_uncaptured_error(Arc::new(move |err: wgpu::Error| {
        sink.lock().unwrap_or_else(|e| e.into_inner()).push(err.to_string());
    }));

    let _main_mod = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("shader.wgsl"),
        source: wgpu::ShaderSource::Wgsl(main_src.into()),
    });
    let _disp_mod = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("display.wgsl"),
        source: wgpu::ShaderSource::Wgsl(disp_src.into()),
    });
    let _ = device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    });
    let errs = errors.lock().unwrap_or_else(|e| e.into_inner());
    assert!(errs.is_empty(), "WGSL shader module errors: {}", errs.join("\n"));
}

#[test]
fn neutral_defaults_preserve_color_fidelity_on_gpu() {
    use crate::app::state::GpuImageEntry;
    use crate::decode::as_shot::AsShot;
    use crate::render::gpu::compute;
    use crate::render::gpu::processor::{RenderOutputPrecision, RenderRequest, RenderedPixels};
    use crate::render::options::RenderOptions;
    use crate::render::uniforms::get_all_adjustments_from_json;
    use image::{DynamicImage, ImageBuffer, Rgb, Rgba};

    let defaults_json: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../src/utils/defaultAdjustments.json"
    ))
    .expect("defaultAdjustments.json must parse");

    assert_eq!(
        defaults_json["hlrEnabled"].as_bool(),
        Some(false),
        "hlrEnabled must default to false"
    );
    assert_eq!(
        defaults_json["skinShield"].as_f64(),
        Some(0.0),
        "skinShield must default to 0"
    );
    assert_eq!(
        defaults_json["contrastCleaner"].as_f64(),
        Some(0.0),
        "contrastCleaner must default to 0"
    );

    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
    descriptor.backends = match default_backend() {
        "dx12" => wgpu::Backends::DX12,
        "metal" => wgpu::Backends::METAL,
        _ => wgpu::Backends::VULKAN,
    };
    let instance = wgpu::Instance::new(descriptor);
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        ..Default::default()
    })) else {
        println!("[color_fidelity] no GPU adapter in test env, skipping");
        return;
    };

    let mut required_features = wgpu::Features::empty();
    if adapter
        .features()
        .contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES)
    {
        required_features |= wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES;
    }
    let limits = adapter.limits();
    let info = adapter.get_info();
    let Ok((device, queue)) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("Color Fidelity Check Device"),
        required_features,
        required_limits: limits.clone(),
        experimental_features: wgpu::ExperimentalFeatures::default(),
        memory_hints: wgpu::MemoryHints::Performance,
        trace: wgpu::Trace::Off,
    })) else {
        println!("[color_fidelity] could not open GPU device, skipping");
        return;
    };

    let context = GpuContext {
        instance,
        adapter,
        device: Arc::new(device),
        queue: Arc::new(queue),
        limits,
        backend: info.backend,
        display: Default::default(),
        viewer_display: Default::default(),
        pipelines: Default::default(),
        compute: Default::default(),
    };
    let processor = GpuProcessor::new(context.clone(), 64, 64).expect("GpuProcessor::new");

    let run_pass = |entry_id: u64,
                    img: &DynamicImage,
                    is_raw: bool,
                    adjustments: crate::render::uniforms::AllAdjustments|
     -> Vec<u8> {
        let slot = compute::upload_image(&context, img);
        let (w, h) = slot.size;
        let is_raw_u32 = u32::from(is_raw);
        let guided = processor.build_guided_coeffs(&slot.view, w, h, is_raw_u32);
        let entry = GpuImageEntry {
            id: entry_id,
            texture: slot.texture,
            texture_view: slot.view,
            gf_coeffs_view: guided.coeffs,
            tone_base_view: guided.tone_base,
            haze_view: guided.haze,
            airlight_view: guided.airlight,
            is_raw: is_raw_u32,
            width: w,
            height: h,
            transform_key: entry_id,
            source_identity: entry_id,
        };
        let mask_view = processor.mask_view(&[], w, h, None);
        let (pixels, _, _, _, _) = processor
            .run(
                &entry,
                &mask_view,
                RenderRequest {
                    adjustments,
                    mask_bitmaps: &[],
                    mask_cache_key: None,
                    lut: None,
                    dcp_lut: None,
                    roi: None,
                    hsl_probe: false,
                    motion_blur: Default::default(),
                },
                false,
                RenderOutputPrecision::EightBit,
            )
            .expect("GpuProcessor::run");
        match pixels {
            RenderedPixels::U8(bytes) => bytes,
            RenderedPixels::U16(_) => panic!("Expected 8-bit rendered pixels"),
        }
    };

    // Test 1: Non-RAW sRGB Neutral Identity across 8 patches (64x64 grid: 4x2 of 16x32 blocks)
    let srgb_patches: [[u8; 3]; 8] = [
        [128, 128, 128], // mid grey
        [0, 0, 178],     // saturated deep blue (Rec.2020 luma ~0.041 < 0.045)
        [0, 0, 77],      // dark royal blue
        [110, 0, 0],     // deep red
        [252, 248, 40],  // bright yellow highlight (> 0.95 linear)
        [70, 180, 253],  // bright sky blue
        [245, 212, 188], // warm skin highlight (luma > 0.72)
        [255, 255, 255], // pure white
    ];
    let mut srgb_buf = ImageBuffer::<Rgba<u8>, Vec<u8>>::new(64, 64);
    let to_working = crate::render::color_space::srgb_to_working();
    for (y, row) in srgb_buf.enumerate_rows_mut() {
        for (x, _, px) in row {
            let idx = (x as usize / 16) + (y as usize / 32) * 4;
            let [r, g, b] = srgb_patches[idx];
            // Non-RAW images are loaded into Working (Rec.2020) primaries with sRGB transfer curve
            let lin_srgb = glam::Vec3::new(
                crate::render::color_space::TransferCurve::SRGB.decode(r as f32 / 255.0),
                crate::render::color_space::TransferCurve::SRGB.decode(g as f32 / 255.0),
                crate::render::color_space::TransferCurve::SRGB.decode(b as f32 / 255.0),
            );
            let working = (to_working * lin_srgb).max(glam::Vec3::ZERO);
            let enc = [
                (crate::render::color_space::TransferCurve::SRGB.encode(working.x).clamp(0.0, 1.0) * 255.0).round() as u8,
                (crate::render::color_space::TransferCurve::SRGB.encode(working.y).clamp(0.0, 1.0) * 255.0).round() as u8,
                (crate::render::color_space::TransferCurve::SRGB.encode(working.z).clamp(0.0, 1.0) * 255.0).round() as u8,
                255,
            ];
            *px = Rgba(enc);
        }
    }
    let srgb_dyn = DynamicImage::ImageRgba8(srgb_buf);
    let mut non_raw_adj =
        get_all_adjustments_from_json(&defaults_json, false, AsShot::reference(), RenderOptions::default());
    non_raw_adj.global.exposure = 0.0;
    let out_srgb = run_pass(1, &srgb_dyn, false, non_raw_adj);
    for (idx, [er, eg, eb]) in srgb_patches.iter().enumerate() {
        let cx = (idx % 4) * 16 + 8;
        let cy = (idx / 4) * 32 + 16;
        let off = (cy * 64 + cx) * 4;
        let (ar, ag, ab) = (out_srgb[off], out_srgb[off + 1], out_srgb[off + 2]);
        assert!(
            ar.abs_diff(*er) <= 2 && ag.abs_diff(*eg) <= 2 && ab.abs_diff(*eb) <= 2,
            "Non-RAW neutral patch {} drifted: expected [{},{},{}], got [{},{},{}]",
            idx,
            er,
            eg,
            eb,
            ar,
            ag,
            ab
        );
    }

    // Test 2: RAW Linear Neutral Identity vs Explicit Zeroed Enhanced Uniforms
    let raw_patches: [[f32; 3]; 4] = [
        [0.18, 0.18, 0.18], // 18% grey
        [0.35, 0.85, 1.80], // super-white blue sky
        [0.95, 0.78, 0.62], // sunlit warm skin
        [0.0, 0.0, 0.45],   // deep saturated blue
    ];
    let mut raw_buf = ImageBuffer::<Rgb<f32>, Vec<f32>>::new(64, 64);
    for (y, row) in raw_buf.enumerate_rows_mut() {
        for (x, _, px) in row {
            let idx = (x as usize / 32) + (y as usize / 32) * 2;
            *px = Rgb(raw_patches[idx]);
        }
    }
    let raw_dyn = DynamicImage::ImageRgb32F(raw_buf);
    let raw_default_adj =
        get_all_adjustments_from_json(&defaults_json, true, AsShot::reference(), RenderOptions::default());
    let mut raw_explicit_zero = raw_default_adj;
    raw_explicit_zero.global.hlr_enabled = 0;
    raw_explicit_zero.global.skin_shield_strength = 0.0;
    raw_explicit_zero.global.contrast_cleaner_strength = 0.0;

    let out_raw_default = run_pass(2, &raw_dyn, true, raw_default_adj);
    let out_raw_zero = run_pass(3, &raw_dyn, true, raw_explicit_zero);
    let max_diff = out_raw_default
        .iter()
        .zip(out_raw_zero.iter())
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap_or(0);
    assert_eq!(
        max_diff, 0,
        "RAW defaultAdjustments.json must match explicit zeroed enhanced uniforms bit-for-bit"
    );

    // Check sky patch (idx 1) and deep blue patch (idx 3) retain strong blue saturation
    let sky_off = (16 * 64 + 48) * 4;
    assert!(
        out_raw_default[sky_off + 2] > out_raw_default[sky_off].saturating_add(20),
        "RAW super-white sky must remain strongly blue, got [{},{},{}]",
        out_raw_default[sky_off],
        out_raw_default[sky_off + 1],
        out_raw_default[sky_off + 2]
    );
    let blue_off = (48 * 64 + 48) * 4;
    assert!(
        out_raw_default[blue_off + 2] > out_raw_default[blue_off].saturating_add(30),
        "RAW deep blue must not desaturate toward grey, got [{},{},{}]",
        out_raw_default[blue_off],
        out_raw_default[blue_off + 1],
        out_raw_default[blue_off + 2]
    );
}
