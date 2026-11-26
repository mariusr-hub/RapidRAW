use crate::platform::monitor::WideGamut;
use crate::render::color_space::{OutputColorSpace, TransferCurve, working_profile};
use glam::Vec3;
use half::f16;
use moxcms::{
    ColorProfile, DataColorSpace, Layout, LutWarehouse, Matrix3d, ProfileText, RenderingIntent,
    TransformF32Executor, TransformOptions,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::SystemTime;

pub const LUT_SIZE: u32 = 65;
pub const LUT_SHAPER_GAMMA: f32 = 2.2;
const IDENTITY_TOLERANCE: f32 = 0.5 / 255.0;
const GAMUT_TOLERANCE: f32 = 0.04;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum DisplayColorMode {
    #[default]
    Auto,
    Off,
    Custom,
}

#[derive(Clone)]
pub enum MonitorColor {
    Unmanaged,
    System(WideGamut),
    Profile(Arc<MonitorProfile>),
}

impl PartialEq for MonitorColor {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Unmanaged, Self::Unmanaged) => true,
            (Self::System(a), Self::System(b)) => a == b,
            (Self::Profile(a), Self::Profile(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl MonitorColor {
    pub fn preferred_space(&self) -> OutputColorSpace {
        match self {
            Self::Unmanaged | Self::System(WideGamut::Unavailable) => OutputColorSpace::Srgb,
            Self::System(_) => OutputColorSpace::DisplayP3,
            Self::Profile(profile) => profile.space,
        }
    }
}

pub fn shared_space(spaces: impl IntoIterator<Item = OutputColorSpace>) -> OutputColorSpace {
    spaces
        .into_iter()
        .reduce(|shared, space| match (shared, space) {
            (a, b) if a == b => a,
            (OutputColorSpace::Srgb, wide) | (wide, OutputColorSpace::Srgb) => wide,
            _ => OutputColorSpace::Rec2020,
        })
        .unwrap_or_default()
}

fn transform(from: &ColorProfile, to: &ColorProfile) -> Result<Arc<TransformF32Executor>, String> {
    from.create_transform_f32(
        Layout::Rgb,
        to,
        Layout::Rgb,
        TransformOptions {
            rendering_intent: RenderingIntent::RelativeColorimetric,
            ..TransformOptions::default()
        },
    )
    .map_err(|e| e.to_string())
}

fn without_unreliable_tables(mut profile: ColorProfile) -> Result<ColorProfile, String> {
    if profile.color_space != DataColorSpace::Rgb {
        return Err("not an RGB profile".to_string());
    }
    if profile.is_matrix_shaper() {
        profile.lut_a_to_b_perceptual = None;
        profile.lut_a_to_b_colorimetric = None;
        profile.lut_a_to_b_saturation = None;
        profile.lut_b_to_a_perceptual = None;
        profile.lut_b_to_a_colorimetric = None;
        profile.lut_b_to_a_saturation = None;
        return Ok(profile);
    }
    let needs_matrix = |table: &Option<LutWarehouse>| matches!(table, Some(LutWarehouse::Lut(lut)) if lut.matrix != Matrix3d::IDENTITY);
    if profile.pcs == DataColorSpace::Xyz
        && [
            &profile.lut_b_to_a_perceptual,
            &profile.lut_b_to_a_colorimetric,
            &profile.lut_b_to_a_saturation,
        ]
        .into_iter()
        .any(needs_matrix)
    {
        return Err("table profiles with an XYZ matrix are not supported".to_string());
    }
    Ok(profile)
}

pub struct MonitorProfile {
    pub name: String,
    pub space: OutputColorSpace,
    profile: ColorProfile,
}

impl MonitorProfile {
    fn read(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let profile = without_unreliable_tables(
            ColorProfile::new_from_slice(&bytes).map_err(|e| e.to_string())?,
        )?;

        let primaries = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0f32];
        let mut encoded = [0.0f32; 9];
        transform(&profile, &working_profile())?
            .transform(&primaries, &mut encoded)
            .map_err(|e| e.to_string())?;
        let primaries: Vec<Vec3> = encoded
            .as_chunks::<3>()
            .0
            .iter()
            .map(|rgb| Vec3::from_array(*rgb).map(|c| TransferCurve::SRGB.decode(c.max(0.0))))
            .collect();
        let contains = |space: &OutputColorSpace| {
            let from_working = space.working_matrix();
            primaries.iter().all(|primary| {
                let rgb = from_working * *primary;
                rgb.min_element() >= -GAMUT_TOLERANCE * rgb.max_element()
            })
        };
        let space = [
            OutputColorSpace::Srgb,
            OutputColorSpace::DisplayP3,
            OutputColorSpace::AdobeRgb,
        ]
        .into_iter()
        .find(contains)
        .unwrap_or(OutputColorSpace::Rec2020);

        let name = match &profile.description {
            Some(ProfileText::PlainString(text)) => Some(text.clone()),
            Some(ProfileText::Localizable(texts)) => texts.first().map(|text| text.value.clone()),
            Some(ProfileText::Description(text)) => Some(text.ascii_string.clone()),
            None => None,
        }
        .map(|name| name.trim_matches(['\0', ' ']).to_string())
        .filter(|name| !name.is_empty())
        .or_else(|| path.file_stem().map(|stem| stem.to_string_lossy().into()))
        .unwrap_or_default();

        Ok(Self {
            name,
            space,
            profile,
        })
    }

    pub fn load(path: &Path) -> Option<Arc<Self>> {
        type Entry = (Option<SystemTime>, Option<Arc<MonitorProfile>>);
        static CACHE: LazyLock<Mutex<HashMap<PathBuf, Entry>>> = LazyLock::new(Default::default);

        let modified = std::fs::metadata(path)
            .and_then(|meta| meta.modified())
            .ok();
        let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((cached_at, profile)) = cache.get(path)
            && *cached_at == modified
        {
            return profile.clone();
        }
        let profile = match Self::read(path) {
            Ok(profile) => {
                log::info!(
                    "Monitor profile '{}' ({}), previewing in {:?}",
                    profile.name,
                    path.display(),
                    profile.space
                );
                Some(Arc::new(profile))
            }
            Err(e) => {
                log::warn!("Cannot use monitor profile {}: {}", path.display(), e);
                None
            }
        };
        cache.insert(path.to_path_buf(), (modified, profile.clone()));
        profile
    }

    pub fn lut_from(&self, space: OutputColorSpace) -> Result<Option<ColorLut>, String> {
        let shaper = TransferCurve::gamma(LUT_SHAPER_GAMMA);
        let executor = transform(&space.profile_with_curve(shaper), &self.profile)?;
        let size = LUT_SIZE as usize;
        let step = 1.0 / (size - 1) as f32;
        let source: Vec<f32> = (0..size * size * size)
            .flat_map(|index| {
                [index % size, index / size % size, index / (size * size)]
                    .map(|node| node as f32 * step)
            })
            .collect();
        let mut target = vec![0.0f32; source.len()];
        executor
            .transform(&source, &mut target)
            .map_err(|e| e.to_string())?;

        let curve = space.curve();
        let identity = source.iter().zip(&target).all(|(from, to)| {
            (curve.encode(shaper.decode(*from)) - to).abs() <= IDENTITY_TOLERANCE
        });
        Ok((!identity).then(|| ColorLut {
            texels: target
                .as_chunks::<3>()
                .0
                .iter()
                .map(|rgb| {
                    let [r, g, b] = rgb.map(|channel| f16::from_f32(channel.clamp(0.0, 1.0)));
                    [r, g, b, f16::ONE]
                })
                .collect(),
            curve,
            shaper,
            executor,
        }))
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SoftProofConfig {
    pub enabled: bool,
    pub target: String,
    pub intent: String,
    pub oog_warning: bool,
    pub custom_icc_path: Option<String>,
}

pub fn build_soft_proof_lut(
    frame_space: OutputColorSpace,
    monitor: &MonitorColor,
    config: &SoftProofConfig,
) -> Result<Option<ColorLut>, String> {
    if !config.enabled {
        return Ok(None);
    }

    let intent = if config.intent.eq_ignore_ascii_case("perceptual") {
        RenderingIntent::Perceptual
    } else {
        RenderingIntent::RelativeColorimetric
    };

    let custom_target = config
        .custom_icc_path
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|bytes| ColorProfile::new_from_slice(&bytes).ok());

    let (target_profile, target_space_opt) = if let Some(prof) = custom_target {
        (prof, None)
    } else {
        let sp = match config.target.to_ascii_lowercase().as_str() {
            "displayp3" | "display_p3" | "p3" => OutputColorSpace::DisplayP3,
            "adobergb" | "adobe_rgb" | "adobergb1998" => OutputColorSpace::AdobeRgb,
            "rec2020" | "rec_2020" | "bt2020" => OutputColorSpace::Rec2020,
            _ => OutputColorSpace::Srgb,
        };
        (sp.profile(), Some(sp))
    };

    let shaper = TransferCurve::gamma(LUT_SHAPER_GAMMA);
    let src_profile = frame_space.profile_with_curve(shaper);
    let monitor_out_profile = match monitor {
        MonitorColor::Profile(mp) => mp.profile.clone(),
        _ => frame_space.profile(),
    };

    let layout_for_profile = |prof: &ColorProfile| -> Layout {
        match prof.color_space {
            DataColorSpace::Cmyk => Layout::Rgba,
            _ => Layout::Rgb,
        }
    };
    let target_layout = layout_for_profile(&target_profile);

    let to_target = src_profile
        .create_transform_f32(
            Layout::Rgb,
            &target_profile,
            target_layout,
            TransformOptions {
                rendering_intent: intent,
                ..TransformOptions::default()
            },
        )
        .map_err(|e| format!("Soft-proof forward transform error: {}", e))?;

    let to_monitor = target_profile
        .create_transform_f32(
            target_layout,
            &monitor_out_profile,
            Layout::Rgb,
            TransformOptions {
                rendering_intent: RenderingIntent::RelativeColorimetric,
                ..TransformOptions::default()
            },
        )
        .map_err(|e| format!("Soft-proof monitor transform error: {}", e))?;

    let size = LUT_SIZE as usize;
    let step = 1.0 / (size - 1) as f32;
    let nodes = size * size * size;
    let mut source = Vec::with_capacity(nodes * 3);
    for index in 0..nodes {
        let r = (index % size) as f32 * step;
        let g = ((index / size) % size) as f32 * step;
        let b = (index / (size * size)) as f32 * step;
        source.extend_from_slice(&[r, g, b]);
    }

    let target_channels = if target_layout == Layout::Rgba { 4 } else { 3 };
    let mut proof_buf = vec![0.0f32; nodes * target_channels];
    to_target
        .transform(&source, &mut proof_buf)
        .map_err(|e| format!("Soft-proof transform failed: {}", e))?;

    let mut mon_buf = vec![0.0f32; nodes * 3];
    to_monitor
        .transform(&proof_buf, &mut mon_buf)
        .map_err(|e| format!("Soft-proof display transform failed: {}", e))?;

    // Check round-trip color shift back to frame working space + gamut bounds to mark OOG texels
    let frame_working = frame_space.working_matrix().inverse();
    let texels: Vec<[f16; 4]> = (0..nodes)
        .map(|i| {
            let mon_rgb = &mon_buf[i * 3..i * 3 + 3];
            let src_sh = &source[i * 3..i * 3 + 3];
            let oog = if let Some(sp) = target_space_opt {
                let lin_frame = Vec3::new(
                    shaper.decode(src_sh[0]),
                    shaper.decode(src_sh[1]),
                    shaper.decode(src_sh[2]),
                );
                let working = frame_working * lin_frame;
                crate::render::color_space::is_out_of_gamut_for_space(
                    working.to_array(),
                    sp,
                    0.025,
                )
            } else {
                // For CMYK/custom ICC profiles, flag significant perceptual roundtrip compression
                let dr = (shaper.decode(src_sh[0]) - TransferCurve::SRGB.decode(mon_rgb[0].clamp(0.0, 1.0))).abs();
                let dg = (shaper.decode(src_sh[1]) - TransferCurve::SRGB.decode(mon_rgb[1].clamp(0.0, 1.0))).abs();
                let db = (shaper.decode(src_sh[2]) - TransferCurve::SRGB.decode(mon_rgb[2].clamp(0.0, 1.0))).abs();
                dr.max(dg).max(db) > 0.08
            };
            let [r, g, b] = [
                f16::from_f32(mon_rgb[0].clamp(0.0, 1.0)),
                f16::from_f32(mon_rgb[1].clamp(0.0, 1.0)),
                f16::from_f32(mon_rgb[2].clamp(0.0, 1.0)),
            ];
            let a = f16::from_f32(if oog { 1.0 } else { 0.0 });
            [r, g, b, a]
        })
        .collect();

    Ok(Some(ColorLut {
        texels,
        curve: frame_space.curve(),
        shaper,
        executor: to_monitor,
    }))
}

pub struct ColorLut {
    pub texels: Vec<[f16; 4]>,
    pub curve: TransferCurve,
    shaper: TransferCurve,
    executor: Arc<TransformF32Executor>,
}

impl ColorLut {
    pub fn convert(&self, rgb: [f32; 3]) -> [f32; 3] {
        let shaped = rgb.map(|channel| self.shaper.encode(self.curve.decode(channel)));
        let mut converted = [0.0f32; 3];
        match self.executor.transform(&shaped, &mut converted) {
            Ok(()) => converted.map(|channel| channel.clamp(0.0, 1.0)),
            Err(_) => rgb,
        }
    }
}
