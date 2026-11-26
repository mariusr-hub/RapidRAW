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
