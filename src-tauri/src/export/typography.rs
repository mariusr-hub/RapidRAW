use ab_glyph::{Font, FontVec, PxScale, ScaleFont, point};
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

static UI_FONT: OnceLock<Option<FontVec>> = OnceLock::new();

fn load_system_font() -> Option<FontVec> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    #[cfg(target_os = "windows")]
    {
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".to_string());
        let fonts_dir = Path::new(&windir).join("Fonts");
        candidates.push(fonts_dir.join("segoeui.ttf"));
        candidates.push(fonts_dir.join("arial.ttf"));
        candidates.push(fonts_dir.join("consola.ttf"));
    }
    #[cfg(target_os = "macos")]
    {
        candidates.push(Path::new("/System/Library/Fonts/Helvetica.ttc").to_path_buf());
        candidates.push(Path::new("/System/Library/Fonts/SFNS.ttf").to_path_buf());
        candidates.push(Path::new("/Library/Fonts/Arial.ttf").to_path_buf());
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        candidates.push(Path::new("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf").to_path_buf());
        candidates.push(Path::new("/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf").to_path_buf());
        candidates.push(Path::new("/usr/share/fonts/TTF/DejaVuSans.ttf").to_path_buf());
    }

    for path in candidates {
        if let Ok(bytes) = std::fs::read(&path)
            && let Ok(font) = FontVec::try_from_vec(bytes)
        {
            return Some(font);
        }
    }
    None
}

fn system_font() -> Option<&'static FontVec> {
    UI_FONT.get_or_init(load_system_font).as_ref()
}

/// Measures width and height of a single-line string at the requested pixel size.
pub fn measure_text(text: &str, px_size: f32) -> (f32, f32) {
    let px = px_size.max(6.0);
    let Some(font) = system_font() else {
        return ((text.chars().count() as f32) * px * 0.6, px * 1.2);
    };
    let scaled = font.as_scaled(PxScale::from(px));
    let mut width = 0.0f32;
    let mut prev: Option<ab_glyph::GlyphId> = None;
    for ch in text.chars() {
        if ch == '\n' || ch == '\r' {
            continue;
        }
        let gid = font.glyph_id(ch);
        if let Some(p) = prev {
            width += scaled.kern(p, gid);
        }
        width += scaled.h_advance(gid);
        prev = Some(gid);
    }
    let height = (scaled.ascent() - scaled.descent()).max(px);
    (width, height)
}

/// Draws anti-aliased TrueType text onto an RGB8 image. Returns `false` if no OS font was available.
pub fn draw_text_rgb8(
    img: &mut image::RgbImage,
    x: f32,
    y: f32,
    text: &str,
    px_size: f32,
    color: [u8; 3],
) -> bool {
    let Some(font) = system_font() else {
        return false;
    };
    let px = px_size.max(6.0);
    let scaled = font.as_scaled(PxScale::from(px));
    let (w, h) = img.dimensions();
    let baseline = y + scaled.ascent();
    let mut caret_x = x;
    let mut prev: Option<ab_glyph::GlyphId> = None;

    for ch in text.chars() {
        if ch == '\n' || ch == '\r' {
            continue;
        }
        let gid = font.glyph_id(ch);
        if let Some(p) = prev {
            caret_x += scaled.kern(p, gid);
        }
        let glyph = gid.with_scale_and_position(PxScale::from(px), point(caret_x, baseline));
        if let Some(outlined) = font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, cov| {
                let px_x = bounds.min.x as i32 + gx as i32;
                let px_y = bounds.min.y as i32 + gy as i32;
                if px_x >= 0 && px_x < w as i32 && px_y >= 0 && px_y < h as i32 {
                    let a = cov.clamp(0.0, 1.0);
                    if a > 0.01 {
                        let p = img.get_pixel_mut(px_x as u32, px_y as u32);
                        for c in 0..3 {
                            p[c] = ((p[c] as f32) * (1.0 - a) + (color[c] as f32) * a)
                                .round()
                                .clamp(0.0, 255.0) as u8;
                        }
                    }
                }
            });
        }
        caret_x += scaled.h_advance(gid);
        prev = Some(gid);
    }
    true
}

/// Draws anti-aliased TrueType text onto an RGBA8 image. Returns `false` if no OS font was available.
pub fn draw_text_rgba8(
    img: &mut image::RgbaImage,
    x: f32,
    y: f32,
    text: &str,
    px_size: f32,
    color: [u8; 4],
) -> bool {
    let Some(font) = system_font() else {
        return false;
    };
    let px = px_size.max(6.0);
    let scaled = font.as_scaled(PxScale::from(px));
    let (w, h) = img.dimensions();
    let baseline = y + scaled.ascent();
    let mut caret_x = x;
    let mut prev: Option<ab_glyph::GlyphId> = None;
    let base_alpha = color[3] as f32 / 255.0;

    for ch in text.chars() {
        if ch == '\n' || ch == '\r' {
            continue;
        }
        let gid = font.glyph_id(ch);
        if let Some(p) = prev {
            caret_x += scaled.kern(p, gid);
        }
        let glyph = gid.with_scale_and_position(PxScale::from(px), point(caret_x, baseline));
        if let Some(outlined) = font.outline_glyph(glyph) {
            let bounds = outlined.px_bounds();
            outlined.draw(|gx, gy, cov| {
                let px_x = bounds.min.x as i32 + gx as i32;
                let px_y = bounds.min.y as i32 + gy as i32;
                if px_x >= 0 && px_x < w as i32 && px_y >= 0 && px_y < h as i32 {
                    let src_a = (cov * base_alpha).clamp(0.0, 1.0);
                    if src_a > 0.004 {
                        let p = img.get_pixel_mut(px_x as u32, px_y as u32);
                        let dst_a = p[3] as f32 / 255.0;
                        let out_a = src_a + dst_a * (1.0 - src_a);
                        if out_a > 1e-4 {
                            for c in 0..3 {
                                let blended = ((color[c] as f32 * src_a)
                                    + (p[c] as f32 * dst_a * (1.0 - src_a)))
                                    / out_a;
                                p[c] = blended.round().clamp(0.0, 255.0) as u8;
                            }
                            p[3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
                        }
                    }
                }
            });
        }
        caret_x += scaled.h_advance(gid);
        prev = Some(gid);
    }
    true
}

/// Expands EXIF placeholders in a watermark or caption template:
/// `{camera}`, `{lens}`, `{focal}`, `{aperture}`, `{shutter}`, `{iso}`, `{year}`.
pub fn expand_exif_template(template: &str, exif: Option<&HashMap<String, String>>) -> String {
    let get = |keys: &[&str]| -> String {
        let Some(map) = exif else {
            return String::new();
        };
        for k in keys {
            if let Some(v) = map.get(*k) {
                let trimmed = v.trim();
                if !trimmed.is_empty() {
                    return trimmed.to_string();
                }
            }
        }
        String::new()
    };

    let camera = get(&["Model", "CameraModelName", "Make"]);
    let lens = get(&["LensModel", "Lens", "LensSpec"]);
    let focal = get(&["FocalLength", "FocalLengthIn35mmFormat"])
        .trim_end_matches("mm")
        .trim()
        .to_string();
    let aperture = get(&["FNumber", "ApertureValue"])
        .trim_start_matches("f/")
        .trim_start_matches("F/")
        .trim()
        .to_string();
    let shutter = get(&["ExposureTime", "ShutterSpeedValue"]);
    let iso = get(&["ISOSpeedRatings", "ISO", "PhotographicSensitivity"]);
    let year_raw = get(&["DateTimeOriginal", "CreateDate", "DateTime"]);
    let year = year_raw
        .split([':', '-', ' '])
        .next()
        .filter(|s| s.len() == 4 && s.chars().all(|c| c.is_ascii_digit()))
        .map(str::to_string)
        .unwrap_or_else(|| chrono::Utc::now().format("%Y").to_string());

    let replaced = template
        .replace("{camera}", &camera)
        .replace("{lens}", &lens)
        .replace("{focal}", &focal)
        .replace("{aperture}", &aperture)
        .replace("{shutter}", &shutter)
        .replace("{iso}", &iso)
        .replace("{year}", &year);

    // Clean up empty separator tokens when some EXIF fields are absent
    replaced
        .split('·')
        .map(str::trim)
        .filter(|seg| {
            !seg.is_empty() && !matches!(*seg, "mm" | "f/" | "ISO")
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_exif_template_formats_fields_and_prunes_missing() {
        let mut exif = HashMap::new();
        exif.insert("Model".to_string(), "ILCE-7RM5".to_string());
        exif.insert("LensModel".to_string(), "FE 35mm F1.4 GM".to_string());
        exif.insert("FocalLength".to_string(), "35 mm".to_string());
        exif.insert("FNumber".to_string(), "1.4".to_string());
        exif.insert("ExposureTime".to_string(), "1/500".to_string());
        exif.insert("ISOSpeedRatings".to_string(), "100".to_string());
        exif.insert("DateTimeOriginal".to_string(), "2026:08:14 18:22:01".to_string());

        let out = expand_exif_template(
            "{camera} · {lens} · {focal}mm · f/{aperture} · {shutter} · ISO {iso} · © {year}",
            Some(&exif),
        );
        assert!(out.contains("ILCE-7RM5"));
        assert!(out.contains("FE 35mm F1.4 GM"));
        assert!(out.contains("35mm"));
        assert!(out.contains("f/1.4"));
        assert!(out.contains("1/500"));
        assert!(out.contains("ISO 100"));
        assert!(out.contains("© 2026"));

        let partial = expand_exif_template("{camera} · {lens} · ISO {iso}", None);
        assert_eq!(partial, "");
    }

    #[test]
    fn test_draw_text_rgb8_renders_if_system_font_present() {
        let mut img = image::RgbImage::from_pixel(320, 64, image::Rgb([10, 10, 10]));
        if draw_text_rgb8(&mut img, 12.0, 12.0, "RapidRAW 1.7", 28.0, [255, 255, 255]) {
            let lit = img.pixels().filter(|p| p[0] > 40).count();
            assert!(lit > 40, "Expected anti-aliased glyph pixels, got {}", lit);
        }
    }
}
