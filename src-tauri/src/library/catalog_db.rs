use crate::app::settings::{load_settings, resolved_app_data_dir};
use crate::app::state::AppState;
use crate::decode::formats::is_supported_image_file;
use crate::library::file_ops::{is_cloud_placeholder, read_file_mapped};
use crate::library::listing::ImageFile;
use crate::library::sidecars::{parse_sidecar_name, parse_virtual_path};
use crate::metadata::exif_processing::read_rrexif_sidecar;
use crate::metadata::sidecar::{ImageFlag, load_sidecar, read_exif_data};
use half::f16;
use rusqlite::{Connection, OptionalExtension, params, types::Value as SqlValue};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, State};
use walkdir::WalkDir;

pub static IS_FOREGROUND_BUSY: AtomicBool = AtomicBool::new(false);

pub fn is_foreground_busy() -> bool {
    IS_FOREGROUND_BUSY.load(Ordering::Relaxed)
}

#[tauri::command]
pub fn set_foreground_busy(busy: bool) {
    IS_FOREGROUND_BUSY.store(busy, Ordering::SeqCst);
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SmartCollectionRule {
    #[serde(default)]
    pub min_rating: Option<u8>,
    #[serde(default)]
    pub flag: Option<String>,
    #[serde(default)]
    pub is_edited: Option<bool>,
    #[serde(default)]
    pub min_iso: Option<i64>,
    #[serde(default)]
    pub color_label: Option<String>,
    #[serde(default)]
    pub camera_contains: Option<String>,
    #[serde(default)]
    pub lens_contains: Option<String>,
    #[serde(default)]
    pub tag_contains: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmartCollectionInfo {
    pub id: String,
    pub name: String,
    pub builtin: bool,
    pub rules: SmartCollectionRule,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticSearchHit {
    pub path: String,
    pub score: f32,
}

#[derive(Debug, Clone)]
pub(crate) struct ClipEmbeddingSlab {
    pub paths: Vec<String>,
    pub data: Vec<u16>,
    pub dim: usize,
}

static CLIP_SLAB_CACHE: OnceLock<Mutex<Option<Arc<ClipEmbeddingSlab>>>> = OnceLock::new();
static SYNC_POOL_GUARD: OnceLock<Mutex<()>> = OnceLock::new();

fn slab_cache_lock() -> &'static Mutex<Option<Arc<ClipEmbeddingSlab>>> {
    CLIP_SLAB_CACHE.get_or_init(|| Mutex::new(None))
}

fn invalidate_clip_slab() {
    if let Ok(mut guard) = slab_cache_lock().lock() {
        *guard = None;
    }
}

pub(crate) fn catalog_db_path(app_handle: &AppHandle) -> Result<PathBuf, String> {
    let dir = resolved_app_data_dir(app_handle)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("catalog.db"))
}

pub(crate) fn open_catalog_at(db_path: &Path) -> Result<Connection, String> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let conn = Connection::open(db_path).map_err(|e| format!("Failed to open catalog DB: {e}"))?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA temp_store = MEMORY;
         PRAGMA busy_timeout = 5000;

         CREATE TABLE IF NOT EXISTS catalog_items (
             path TEXT PRIMARY KEY,
             folder TEXT NOT NULL,
             mtime_ms INTEGER NOT NULL,
             file_size INTEGER NOT NULL,
             camera_make TEXT,
             camera_model TEXT,
             lens_model TEXT,
             iso INTEGER,
             aperture REAL,
             shutter_secs REAL,
             focal_mm REAL,
             capture_ts INTEGER,
             rating INTEGER NOT NULL DEFAULT 0,
             flag TEXT,
             color_label TEXT,
             is_edited INTEGER NOT NULL DEFAULT 0,
             tags_json TEXT,
             clip_f16 BLOB
         );
         CREATE INDEX IF NOT EXISTS idx_catalog_folder ON catalog_items(folder);
         CREATE INDEX IF NOT EXISTS idx_catalog_rating_flag ON catalog_items(rating, flag, is_edited);

         CREATE TABLE IF NOT EXISTS smart_collections (
             id TEXT PRIMARY KEY,
             name TEXT NOT NULL,
             rules_json TEXT NOT NULL
         );",
    )
    .map_err(|e| format!("Failed to initialize catalog schema: {e}"))?;
    Ok(conn)
}

fn open_catalog(app_handle: &AppHandle) -> Result<Connection, String> {
    let db_path = catalog_db_path(app_handle)?;
    open_catalog_at(&db_path)
}

fn flag_to_db(flag: Option<ImageFlag>) -> Option<&'static str> {
    match flag {
        Some(ImageFlag::Pick) => Some("pick"),
        Some(ImageFlag::Reject) => Some("reject"),
        None => None,
    }
}

fn db_to_flag(flag: Option<String>) -> Option<ImageFlag> {
    match flag.as_deref() {
        Some("pick") => Some(ImageFlag::Pick),
        Some("reject") => Some(ImageFlag::Reject),
        _ => None,
    }
}

fn color_from_tags(tags: &Option<Vec<String>>) -> Option<String> {
    tags.as_ref().and_then(|list| {
        list.iter()
            .find_map(|t| t.strip_prefix("color:").map(|s| s.to_string()))
    })
}

fn file_mtime_and_size(path: &Path) -> (i64, i64) {
    let Ok(meta) = std::fs::metadata(path) else {
        return (0, 0);
    };
    let mtime_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as i64);
    (mtime_ms, meta.len() as i64)
}

fn combined_item_mtime_ms(source_path: &Path, sidecar_path: &Path) -> (i64, i64) {
    let (src_mtime, src_size) = file_mtime_and_size(source_path);
    let (sc_mtime, _) = file_mtime_and_size(sidecar_path);
    // Combine image mtime + sidecar mtime so editing `.rrdata` invalidates cached row cleanly
    (src_mtime.max(sc_mtime), src_size)
}

fn parse_first_f64(raw: &str) -> Option<f64> {
    let clean = raw
        .trim()
        .trim_start_matches("f/")
        .trim_end_matches("mm")
        .trim_end_matches('s')
        .trim();
    if let Some((num, den)) = clean.split_once('/') {
        let n = num.trim().parse::<f64>().ok()?;
        let d = den.trim().parse::<f64>().ok()?;
        if d.abs() > 1e-12 {
            return Some(n / d);
        }
    }
    clean
        .split_whitespace()
        .next()
        .and_then(|tok| tok.parse::<f64>().ok())
}

fn parse_iso(raw: &str) -> Option<i64> {
    raw.split(|c: char| !c.is_ascii_digit())
        .find(|s| !s.is_empty())
        .and_then(|s| s.parse::<i64>().ok())
}

#[derive(Default)]
struct ExtractedExif {
    camera_make: Option<String>,
    camera_model: Option<String>,
    lens_model: Option<String>,
    iso: Option<i64>,
    aperture: Option<f64>,
    shutter_secs: Option<f64>,
    focal_mm: Option<f64>,
    capture_ts: Option<i64>,
}

fn extract_exif_fields(source_path: &Path) -> ExtractedExif {
    let map = if let Some(m) = read_rrexif_sidecar(source_path) {
        m
    } else if is_cloud_placeholder(source_path) {
        HashMap::new()
    } else if let Ok(mmap) = read_file_mapped(source_path) {
        let path_str = source_path.to_string_lossy();
        read_exif_data(&path_str, &mmap)
    } else {
        HashMap::new()
    };

    if map.is_empty() {
        return ExtractedExif::default();
    }

    let iso = map
        .get("PhotographicSensitivity")
        .or_else(|| map.get("ISOSpeedRatings"))
        .or_else(|| map.get("ISO"))
        .and_then(|v| parse_iso(v));
    let aperture = map.get("FNumber").and_then(|v| parse_first_f64(v));
    let shutter_secs = map.get("ExposureTime").and_then(|v| parse_first_f64(v));
    let focal_mm = map
        .get("FocalLength")
        .or_else(|| map.get("FocalLengthIn35mmFormat"))
        .and_then(|v| parse_first_f64(v));
    let capture_ts = crate::metadata::exif_processing::try_get_exif_creation_date(source_path)
        .map(|dt| dt.timestamp());

    ExtractedExif {
        camera_make: map.get("Make").cloned().filter(|s| !s.is_empty()),
        camera_model: map.get("Model").cloned().filter(|s| !s.is_empty()),
        lens_model: map
            .get("LensModel")
            .or_else(|| map.get("Lens"))
            .cloned()
            .filter(|s| !s.is_empty()),
        iso,
        aperture,
        shutter_secs,
        focal_mm,
        capture_ts,
    }
}

pub(crate) fn upsert_image_batch(
    conn: &mut Connection,
    folder: &str,
    items: &[ImageFile],
) -> Result<usize, String> {
    if items.is_empty() {
        return Ok(0);
    }

    let mut existing: HashMap<String, (i64, i64, i64)> = HashMap::new();
    {
        let mut stmt = conn
            .prepare("SELECT path, mtime_ms, file_size, rating FROM catalog_items WHERE folder = ?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([folder], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        for row in rows.flatten() {
            existing.insert(row.0, (row.1, row.2, row.3));
        }
    }

    let mut changed_indices = Vec::new();
    let seen_paths: HashSet<&str> = items.iter().map(|it| it.path.as_str()).collect();
    for (idx, item) in items.iter().enumerate() {
        let (source_path, sidecar_path) = parse_virtual_path(&item.path);
        let (mtime_ms, file_size) = combined_item_mtime_ms(&source_path, &sidecar_path);
        let up_to_date = existing
            .get(&item.path)
            .is_some_and(|&(em, es, er)| em == mtime_ms && es == file_size && er == item.rating as i64);
        if !up_to_date {
            changed_indices.push((idx, mtime_ms, file_size));
        }
    }

    let mut upserted = 0usize;
    for chunk in changed_indices.chunks(200) {
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        {
            let mut stmt = tx
                .prepare_cached(
                    "INSERT INTO catalog_items (
                        path, folder, mtime_ms, file_size, camera_make, camera_model, lens_model,
                        iso, aperture, shutter_secs, focal_mm, capture_ts,
                        rating, flag, color_label, is_edited, tags_json
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
                    ON CONFLICT(path) DO UPDATE SET
                        folder = excluded.folder,
                        mtime_ms = excluded.mtime_ms,
                        file_size = excluded.file_size,
                        camera_make = COALESCE(excluded.camera_make, catalog_items.camera_make),
                        camera_model = COALESCE(excluded.camera_model, catalog_items.camera_model),
                        lens_model = COALESCE(excluded.lens_model, catalog_items.lens_model),
                        iso = COALESCE(excluded.iso, catalog_items.iso),
                        aperture = COALESCE(excluded.aperture, catalog_items.aperture),
                        shutter_secs = COALESCE(excluded.shutter_secs, catalog_items.shutter_secs),
                        focal_mm = COALESCE(excluded.focal_mm, catalog_items.focal_mm),
                        capture_ts = COALESCE(excluded.capture_ts, catalog_items.capture_ts),
                        rating = excluded.rating,
                        flag = excluded.flag,
                        color_label = excluded.color_label,
                        is_edited = excluded.is_edited,
                        tags_json = excluded.tags_json",
                )
                .map_err(|e| e.to_string())?;

            for &(idx, mtime_ms, file_size) in chunk {
                let item = &items[idx];
                let (source_path, _) = parse_virtual_path(&item.path);
                let exif = extract_exif_fields(&source_path);
                let tags_json = item
                    .tags
                    .as_ref()
                    .and_then(|t| serde_json::to_string(t).ok());
                let color_label = color_from_tags(&item.tags);
                stmt.execute(params![
                    item.path,
                    folder,
                    mtime_ms,
                    file_size,
                    exif.camera_make,
                    exif.camera_model,
                    exif.lens_model,
                    exif.iso,
                    exif.aperture,
                    exif.shutter_secs,
                    exif.focal_mm,
                    exif.capture_ts,
                    item.rating as i64,
                    flag_to_db(item.flag),
                    color_label,
                    if item.is_edited { 1i64 } else { 0i64 },
                    tags_json,
                ])
                .map_err(|e| e.to_string())?;
                upserted += 1;
            }
        }
        tx.commit().map_err(|e| e.to_string())?;
    }

    // Prune deleted files within this folder
    let stale: Vec<String> = existing
        .into_keys()
        .filter(|p| !seen_paths.contains(p.as_str()))
        .collect();
    if !stale.is_empty() {
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        {
            let mut stmt = tx
                .prepare_cached("DELETE FROM catalog_items WHERE path = ?1")
                .map_err(|e| e.to_string())?;
            for p in &stale {
                let _ = stmt.execute([p]);
            }
        }
        tx.commit().map_err(|e| e.to_string())?;
    }

    Ok(upserted)
}

pub fn sync_listed_images_background(app_handle: AppHandle, folder: String, items: Vec<ImageFile>) {
    std::thread::spawn(move || {
        let guard = SYNC_POOL_GUARD.get_or_init(|| Mutex::new(()));
        let Ok(_lock) = guard.lock() else {
            return;
        };
        if is_foreground_busy() {
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        let Ok(mut conn) = open_catalog(&app_handle) else {
            return;
        };
        let _ = upsert_image_batch(&mut conn, &folder, &items);
    });
}

pub fn update_ratings(app_handle: &AppHandle, paths: &[String], rating: u8) {
    let Ok(mut conn) = open_catalog(app_handle) else {
        return;
    };
    let Ok(tx) = conn.transaction() else {
        return;
    };
    if let Ok(mut stmt) = tx.prepare_cached(
        "UPDATE catalog_items
         SET rating = ?2,
             flag = CASE WHEN ?2 > 0 AND flag = 'reject' THEN NULL ELSE flag END
         WHERE path = ?1",
    ) {
        for p in paths {
            let _ = stmt.execute(params![p, rating as i64]);
        }
    }
    let _ = tx.commit();
}

pub fn update_flags(app_handle: &AppHandle, paths: &[String], flag: Option<ImageFlag>) {
    let Ok(mut conn) = open_catalog(app_handle) else {
        return;
    };
    let Ok(tx) = conn.transaction() else {
        return;
    };
    let flag_str = flag_to_db(flag);
    if let Ok(mut stmt) = tx.prepare_cached("UPDATE catalog_items SET flag = ?2 WHERE path = ?1") {
        for p in paths {
            let _ = stmt.execute(params![p, flag_str]);
        }
    }
    let _ = tx.commit();
}

pub fn update_color_labels(app_handle: &AppHandle, paths: &[String], color: Option<&str>) {
    let Ok(mut conn) = open_catalog(app_handle) else {
        return;
    };
    let Ok(tx) = conn.transaction() else {
        return;
    };
    let clean = color.map(str::trim).filter(|c| !c.is_empty());
    if let Ok(mut stmt) =
        tx.prepare_cached("UPDATE catalog_items SET color_label = ?2 WHERE path = ?1")
    {
        for p in paths {
            let _ = stmt.execute(params![p, clean]);
        }
    }
    let _ = tx.commit();
}

pub fn update_tags(app_handle: &AppHandle, path: &str, tags: &Option<Vec<String>>) {
    let Ok(conn) = open_catalog(app_handle) else {
        return;
    };
    let tags_json = tags.as_ref().and_then(|t| serde_json::to_string(t).ok());
    let color_label = color_from_tags(tags);
    let _ = conn.execute(
        "UPDATE catalog_items SET tags_json = ?2, color_label = COALESCE(?3, color_label) WHERE path = ?1",
        params![path, tags_json, color_label],
    );
}

pub fn update_edited(app_handle: &AppHandle, path: &str, is_edited: bool) {
    let Ok(conn) = open_catalog(app_handle) else {
        return;
    };
    let _ = conn.execute(
        "UPDATE catalog_items SET is_edited = ?2 WHERE path = ?1",
        params![path, if is_edited { 1i64 } else { 0i64 }],
    );
}

pub fn rename_path(app_handle: &AppHandle, old_path: &str, new_path: &str) {
    let Ok(conn) = open_catalog(app_handle) else {
        return;
    };
    let folder = Path::new(new_path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let _ = conn.execute(
        "UPDATE catalog_items SET path = ?2, folder = CASE WHEN ?3 = '' THEN folder ELSE ?3 END WHERE path = ?1",
        params![old_path, new_path, folder],
    );
    invalidate_clip_slab();
}

pub fn delete_paths(app_handle: &AppHandle, paths: &[String]) {
    if paths.is_empty() {
        return;
    }
    let Ok(mut conn) = open_catalog(app_handle) else {
        return;
    };
    let Ok(tx) = conn.transaction() else {
        return;
    };
    if let Ok(mut stmt) = tx.prepare_cached("DELETE FROM catalog_items WHERE path = ?1") {
        for p in paths {
            let _ = stmt.execute([p]);
        }
    }
    let _ = tx.commit();
    invalidate_clip_slab();
}

pub(crate) fn encode_f16_unit(vec_f32: &[f32]) -> Vec<u8> {
    let norm = vec_f32.iter().map(|&v| v * v).sum::<f32>().sqrt();
    let inv = if norm > 1e-8 { 1.0 / norm } else { 1.0 };
    let mut bytes = Vec::with_capacity(vec_f32.len() * 2);
    for &v in vec_f32 {
        bytes.extend_from_slice(&f16::from_f32(v * inv).to_le_bytes());
    }
    bytes
}

pub(crate) fn decode_f16_slice(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(2)
        .map(|c| f16::from_le_bytes([c[0], c[1]]).to_f32())
        .collect()
}

pub fn upsert_clip_embedding(app_handle: &AppHandle, path: &str, embedding: &[f32]) {
    if embedding.is_empty() {
        return;
    }
    let Ok(conn) = open_catalog(app_handle) else {
        return;
    };
    let blob = encode_f16_unit(embedding);
    let _ = conn.execute(
        "UPDATE catalog_items SET clip_f16 = ?2 WHERE path = ?1",
        params![path, blob],
    );
    invalidate_clip_slab();
}

pub(crate) fn get_clip_embeddings_for_paths(
    app_handle: &AppHandle,
    paths: &[String],
) -> HashMap<String, Vec<f32>> {
    let mut out = HashMap::new();
    let Ok(conn) = open_catalog(app_handle) else {
        return out;
    };
    let Ok(mut stmt) =
        conn.prepare_cached("SELECT clip_f16 FROM catalog_items WHERE path = ?1 AND clip_f16 IS NOT NULL")
    else {
        return out;
    };
    for p in paths {
        if let Ok(Some(blob)) = stmt.query_row([p], |r| r.get::<_, Vec<u8>>(0)).optional() {
            let decoded = decode_f16_slice(&blob);
            if !decoded.is_empty() {
                out.insert(p.clone(), decoded);
            }
        }
    }
    out
}

pub(crate) fn rank_cosine_similarity_f16_slab(
    slab: &ClipEmbeddingSlab,
    query: &[f32],
    top_k: usize,
    min_score: f32,
) -> Vec<(String, f32)> {
    if slab.dim == 0 || query.len() != slab.dim || slab.paths.is_empty() || top_k == 0 {
        return Vec::new();
    }
    let q_norm = query.iter().map(|&v| v * v).sum::<f32>().sqrt();
    let inv_q = if q_norm > 1e-8 { 1.0 / q_norm } else { 1.0 };
    let q_unit: Vec<f32> = query.iter().map(|&v| v * inv_q).collect();

    let mut scored: Vec<(usize, f32)> = Vec::with_capacity(slab.paths.len());
    for (idx, chunk) in slab.data.chunks_exact(slab.dim).enumerate() {
        let mut dot = 0.0f32;
        for (&bits, &qv) in chunk.iter().zip(&q_unit) {
            dot += f16::from_bits(bits).to_f32() * qv;
        }
        if dot >= min_score {
            scored.push((idx, dot));
        }
    }
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    scored.truncate(top_k);
    scored
        .into_iter()
        .map(|(i, s)| (slab.paths[i].clone(), s))
        .collect()
}

fn load_or_build_clip_slab(app_handle: &AppHandle) -> Result<Arc<ClipEmbeddingSlab>, String> {
    if let Ok(guard) = slab_cache_lock().lock()
        && let Some(cached) = guard.as_ref()
    {
        return Ok(Arc::clone(cached));
    }

    let conn = open_catalog(app_handle)?;
    let mut stmt = conn
        .prepare("SELECT path, clip_f16 FROM catalog_items WHERE clip_f16 IS NOT NULL")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?)))
        .map_err(|e| e.to_string())?;

    let mut paths = Vec::new();
    let mut data = Vec::new();
    let mut dim = 0usize;
    for row in rows.flatten() {
        let (path, blob) = row;
        if blob.len() < 4 || blob.len() % 2 != 0 {
            continue;
        }
        let d = blob.len() / 2;
        if dim == 0 {
            dim = d;
        } else if d != dim {
            continue;
        }
        paths.push(path);
        for c in blob.chunks_exact(2) {
            data.push(u16::from_le_bytes([c[0], c[1]]));
        }
    }

    let slab = Arc::new(ClipEmbeddingSlab { paths, data, dim });
    if let Ok(mut guard) = slab_cache_lock().lock() {
        *guard = Some(Arc::clone(&slab));
    }
    Ok(slab)
}

fn builtin_smart_collections() -> Vec<(&'static str, &'static str, SmartCollectionRule)> {
    vec![
        (
            "builtin:4plus",
            "4+ Stars",
            SmartCollectionRule {
                min_rating: Some(4),
                ..Default::default()
            },
        ),
        (
            "builtin:picked_unedited",
            "Picked & Unedited",
            SmartCollectionRule {
                flag: Some("pick".to_string()),
                is_edited: Some(false),
                ..Default::default()
            },
        ),
        (
            "builtin:high_iso",
            "High ISO (>=3200)",
            SmartCollectionRule {
                min_iso: Some(3200),
                ..Default::default()
            },
        ),
    ]
}

fn build_rule_where(rule: &SmartCollectionRule) -> (String, Vec<SqlValue>) {
    let mut clauses = Vec::new();
    let mut params: Vec<SqlValue> = Vec::new();
    if let Some(min_rating) = rule.min_rating {
        params.push(SqlValue::Integer(min_rating as i64));
        clauses.push(format!("rating >= ?{}", params.len()));
    }
    if let Some(flag) = &rule.flag
        && !flag.is_empty()
    {
        params.push(SqlValue::Text(flag.clone()));
        clauses.push(format!("flag = ?{}", params.len()));
    }
    if let Some(is_edited) = rule.is_edited {
        params.push(SqlValue::Integer(if is_edited { 1 } else { 0 }));
        clauses.push(format!("is_edited = ?{}", params.len()));
    }
    if let Some(min_iso) = rule.min_iso {
        params.push(SqlValue::Integer(min_iso));
        clauses.push(format!("iso >= ?{}", params.len()));
    }
    if let Some(color_label) = &rule.color_label
        && !color_label.is_empty()
    {
        params.push(SqlValue::Text(color_label.clone()));
        clauses.push(format!("color_label = ?{}", params.len()));
    }
    if let Some(cam) = &rule.camera_contains
        && !cam.trim().is_empty()
    {
        params.push(SqlValue::Text(format!("%{}%", cam.trim().to_lowercase())));
        let idx = params.len();
        clauses.push(format!(
            "(LOWER(COALESCE(camera_make, '')) LIKE ?{idx} OR LOWER(COALESCE(camera_model, '')) LIKE ?{idx})"
        ));
    }
    if let Some(lens) = &rule.lens_contains
        && !lens.trim().is_empty()
    {
        params.push(SqlValue::Text(format!("%{}%", lens.trim().to_lowercase())));
        clauses.push(format!("LOWER(COALESCE(lens_model, '')) LIKE ?{}", params.len()));
    }
    if let Some(tag) = &rule.tag_contains
        && !tag.trim().is_empty()
    {
        params.push(SqlValue::Text(format!("%{}%", tag.trim().to_lowercase())));
        clauses.push(format!("LOWER(COALESCE(tags_json, '')) LIKE ?{}", params.len()));
    }
    let where_sql = if clauses.is_empty() {
        "1 = 1".to_string()
    } else {
        clauses.join(" AND ")
    };
    (where_sql, params)
}

pub(crate) fn query_smart_collection_from_conn(
    conn: &Connection,
    rule: &SmartCollectionRule,
) -> Result<Vec<ImageFile>, String> {
    let (where_sql, sql_params) = build_rule_where(rule);
    let sql = format!(
        "SELECT path, mtime_ms, is_edited, rating, flag, tags_json
         FROM catalog_items
         WHERE {where_sql}
         ORDER BY COALESCE(capture_ts, mtime_ms / 1000) DESC, path ASC
         LIMIT 5000"
    );
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(sql_params), |r| {
            let path: String = r.get(0)?;
            let mtime_ms: i64 = r.get(1)?;
            let is_edited_int: i64 = r.get(2)?;
            let rating_int: i64 = r.get(3)?;
            let flag_str: Option<String> = r.get(4)?;
            let tags_json: Option<String> = r.get(5)?;
            Ok((path, mtime_ms, is_edited_int, rating_int, flag_str, tags_json))
        })
        .map_err(|e| e.to_string())?;

    let mut items = Vec::new();
    for row in rows.flatten() {
        let (path, mtime_ms, is_edited_int, rating_int, flag_str, tags_json) = row;
        let tags = tags_json.and_then(|j| serde_json::from_str::<Vec<String>>(&j).ok());
        let (source_path, _) = parse_virtual_path(&path);
        let is_raw = crate::decode::formats::is_raw_file(&source_path);
        items.push(ImageFile::from_catalog(
            path,
            (mtime_ms.max(0) / 1000) as u64,
            is_edited_int != 0,
            rating_int.clamp(0, 5) as u8,
            db_to_flag(flag_str),
            tags,
            is_raw,
        ));
    }
    Ok(items)
}

pub(crate) fn count_smart_collection_in_conn(
    conn: &Connection,
    rule: &SmartCollectionRule,
) -> Result<usize, String> {
    let (where_sql, sql_params) = build_rule_where(rule);
    let sql = format!("SELECT COUNT(*) FROM catalog_items WHERE {where_sql}");
    let count: i64 = conn
        .query_row(&sql, rusqlite::params_from_iter(sql_params), |r| r.get(0))
        .map_err(|e| e.to_string())?;
    Ok(count.max(0) as usize)
}

#[tauri::command]
pub async fn rebuild_catalog_index(
    root_paths: Vec<String>,
    app_handle: AppHandle,
) -> Result<usize, String> {
    tokio::task::spawn_blocking(move || {
        let settings = load_settings(app_handle.clone()).unwrap_or_default();
        let tm_override = crate::render::options::resolve_tonemapper_override(&settings, true);
        let mut conn = open_catalog(&app_handle)?;
        let mut total_indexed = 0usize;

        for root in &root_paths {
            let mut by_folder: HashMap<String, Vec<PathBuf>> = HashMap::new();
            let mut sidecars_by_path: HashMap<PathBuf, Vec<Option<String>>> = HashMap::new();

            for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
                let ft = entry.file_type();
                if !(ft.is_file() || (ft.is_symlink() && entry.path().is_file())) {
                    continue;
                }
                let file_name = entry.file_name().to_string_lossy();
                if let Some((source_filename, copy_id)) = parse_sidecar_name(&file_name) {
                    sidecars_by_path
                        .entry(entry.path().with_file_name(source_filename))
                        .or_default()
                        .push(copy_id.map(str::to_string));
                } else if is_supported_image_file(file_name.as_ref()) {
                    let parent = entry
                        .path()
                        .parent()
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_else(|| root.clone());
                    by_folder
                        .entry(parent)
                        .or_default()
                        .push(entry.into_path());
                }
            }

            for (folder, img_paths) in by_folder {
                let mut folder_items = Vec::with_capacity(img_paths.len());
                for img_path in img_paths {
                    let sidecars = sidecars_by_path
                        .remove(&img_path)
                        .unwrap_or_else(|| vec![None]);
                    let path_str = img_path.to_string_lossy().into_owned();
                    let file_name = img_path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    let (mtime_ms, _) = file_mtime_and_size(&img_path);
                    let is_raw = crate::decode::formats::is_raw_file(&img_path);
                    let effective_tm = if is_raw {
                        tm_override
                    } else {
                        crate::render::options::resolve_tonemapper_override(&settings, false)
                    };

                    for copy_id in sidecars {
                        let (virtual_path, sidecar_filename) = match copy_id {
                            Some(id) => (
                                format!("{path_str}?vc={id}"),
                                format!("{file_name}.{id}.rrdata"),
                            ),
                            None => (path_str.clone(), format!("{file_name}.rrdata")),
                        };
                        let sidecar_path = img_path.with_file_name(sidecar_filename);
                        let meta = if sidecar_path.exists() {
                            load_sidecar(&sidecar_path)
                        } else {
                            Default::default()
                        };
                        let is_edited = crate::metadata::sidecar::is_image_edited(
                            &meta.adjustments,
                            &img_path,
                            is_raw,
                            effective_tm,
                        );
                        folder_items.push(ImageFile::from_catalog(
                            virtual_path,
                            (mtime_ms.max(0) / 1000) as u64,
                            is_edited,
                            meta.rating,
                            meta.flag,
                            meta.tags,
                            is_raw,
                        ));
                    }
                }
                total_indexed += folder_items.len();
                let _ = upsert_image_batch(&mut conn, &folder, &folder_items)?;
            }
        }
        Ok(total_indexed)
    })
    .await
    .map_err(|e| format!("Catalog rebuild task failed: {e}"))?
}

#[tauri::command]
pub async fn list_smart_collections(
    app_handle: AppHandle,
) -> Result<Vec<SmartCollectionInfo>, String> {
    tokio::task::spawn_blocking(move || {
        let conn = open_catalog(&app_handle)?;
        let mut list = Vec::new();
        for (id, name, rules) in builtin_smart_collections() {
            let count = count_smart_collection_in_conn(&conn, &rules).unwrap_or(0);
            list.push(SmartCollectionInfo {
                id: id.to_string(),
                name: name.to_string(),
                builtin: true,
                rules,
                count,
            });
        }
        let mut stmt = conn
            .prepare("SELECT id, name, rules_json FROM smart_collections ORDER BY name ASC")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        for row in rows.flatten() {
            let (id, name, rules_json) = row;
            if let Ok(rules) = serde_json::from_str::<SmartCollectionRule>(&rules_json) {
                let count = count_smart_collection_in_conn(&conn, &rules).unwrap_or(0);
                list.push(SmartCollectionInfo {
                    id,
                    name,
                    builtin: false,
                    rules,
                    count,
                });
            }
        }
        Ok(list)
    })
    .await
    .map_err(|e| format!("Smart collections query failed: {e}"))?
}

#[tauri::command]
pub async fn save_smart_collection(
    id: Option<String>,
    name: String,
    rules: SmartCollectionRule,
    app_handle: AppHandle,
) -> Result<SmartCollectionInfo, String> {
    tokio::task::spawn_blocking(move || {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err("Collection name cannot be empty".to_string());
        }
        let conn = open_catalog(&app_handle)?;
        let col_id = id.filter(|s| !s.is_empty() && !s.starts_with("builtin:")).unwrap_or_else(|| {
            format!("custom:{}", uuid::Uuid::new_v4().simple())
        });
        let rules_json = serde_json::to_string(&rules).map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO smart_collections (id, name, rules_json) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, rules_json = excluded.rules_json",
            params![col_id, trimmed, rules_json],
        )
        .map_err(|e| e.to_string())?;
        let count = count_smart_collection_in_conn(&conn, &rules).unwrap_or(0);
        Ok(SmartCollectionInfo {
            id: col_id,
            name: trimmed.to_string(),
            builtin: false,
            rules,
            count,
        })
    })
    .await
    .map_err(|e| format!("Save smart collection failed: {e}"))?
}

#[tauri::command]
pub async fn delete_smart_collection(id: String, app_handle: AppHandle) -> Result<(), String> {
    if id.starts_with("builtin:") {
        return Err("Cannot delete built-in Smart Collections".to_string());
    }
    tokio::task::spawn_blocking(move || {
        let conn = open_catalog(&app_handle)?;
        conn.execute("DELETE FROM smart_collections WHERE id = ?1", [id])
            .map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
    .map_err(|e| format!("Delete smart collection failed: {e}"))?
}

#[tauri::command]
pub async fn query_smart_collection(
    id: String,
    app_handle: AppHandle,
) -> Result<Vec<ImageFile>, String> {
    tokio::task::spawn_blocking(move || {
        let conn = open_catalog(&app_handle)?;
        if let Some((_, _, rule)) = builtin_smart_collections().into_iter().find(|(bid, _, _)| *bid == id) {
            return query_smart_collection_from_conn(&conn, &rule);
        }
        let rules_json: Option<String> = conn
            .query_row(
                "SELECT rules_json FROM smart_collections WHERE id = ?1",
                [&id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let Some(rules_json) = rules_json else {
            return Err(format!("Smart collection '{id}' not found"));
        };
        let rule: SmartCollectionRule =
            serde_json::from_str(&rules_json).map_err(|e| e.to_string())?;
        query_smart_collection_from_conn(&conn, &rule)
    })
    .await
    .map_err(|e| format!("Query smart collection failed: {e}"))?
}

#[tauri::command]
pub async fn semantic_search_catalog(
    query: String,
    limit: Option<usize>,
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<SemanticSearchHit>, String> {
    let trimmed = query.trim().to_string();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let max_hits = limit.unwrap_or(200).clamp(1, 1000);
    let settings = load_settings(app_handle.clone()).unwrap_or_default();
    let allow_clip = settings.enable_ai_tagging.unwrap_or(false);

    // Optional CLIP text embedding if AI models are enabled & already present or initialized
    let text_vec = if allow_clip {
        match crate::ai::models::get_or_init_clip_models(
            &app_handle,
            &state.ai_state,
            &state.ai_init_lock,
        )
        .await
        {
            Ok(clip_models) => tokio::task::spawn_blocking(move || {
                crate::library::tagging::extract_clip_text_embedding(
                    &trimmed,
                    &clip_models.model,
                    &clip_models.tokenizer,
                )
                .ok()
            })
            .await
            .ok()
            .flatten(),
            Err(_) => None,
        }
    } else {
        None
    };

    let q_lower = query.trim().to_lowercase();
    tokio::task::spawn_blocking(move || {
        let mut combined: HashMap<String, f32> = HashMap::new();
        if let Some(q_vec) = text_vec
            && let Ok(slab) = load_or_build_clip_slab(&app_handle)
        {
            for (path, score) in rank_cosine_similarity_f16_slab(&slab, &q_vec, max_hits, 0.18) {
                combined.insert(path, score);
            }
        }

        // Blend fast SQLite keyword / EXIF / tag matches
        let conn = open_catalog(&app_handle)?;
        let pattern = format!("%{q_lower}%");
        let mut stmt = conn
            .prepare(
                "SELECT path FROM catalog_items
                 WHERE LOWER(path) LIKE ?1
                    OR LOWER(COALESCE(tags_json, '')) LIKE ?1
                    OR LOWER(COALESCE(camera_make, '') || ' ' || COALESCE(camera_model, '')) LIKE ?1
                    OR LOWER(COALESCE(lens_model, '')) LIKE ?1
                 LIMIT ?2",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![pattern, max_hits as i64], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        for path in rows.flatten() {
            combined
                .entry(path)
                .and_modify(|s| *s = (*s + 0.35).min(1.0))
                .or_insert(0.55);
        }

        let mut hits: Vec<SemanticSearchHit> = combined
            .into_iter()
            .map(|(path, score)| SemanticSearchHit { path, score })
            .collect();
        hits.sort_by(|a, b| b.score.total_cmp(&a.score));
        hits.truncate(max_hits);
        Ok(hits)
    })
    .await
    .map_err(|e| format!("Semantic search failed: {e}"))?
}

#[tauri::command]
pub async fn find_visually_similar(
    path: String,
    limit: Option<usize>,
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<SemanticSearchHit>, String> {
    let max_hits = limit.unwrap_or(60).clamp(1, 500);
    let existing = {
        let app_clone = app_handle.clone();
        let path_clone = path.clone();
        tokio::task::spawn_blocking(move || {
            get_clip_embeddings_for_paths(&app_clone, std::slice::from_ref(&path_clone))
                .remove(&path_clone)
        })
        .await
        .ok()
        .flatten()
    };

    let query_vec = if let Some(v) = existing {
        v
    } else {
        let clip_models = crate::ai::models::get_or_init_clip_models(
            &app_handle,
            &state.ai_state,
            &state.ai_init_lock,
        )
        .await
        .map_err(|e| e.to_string())?;
        let app_clone = app_handle.clone();
        let path_clone = path.clone();
        let gpu_context =
            crate::render::gpu::context::get_or_init_gpu_context(&state, &app_handle).ok();
        let vec_res = tokio::task::spawn_blocking(move || -> Result<Vec<f32>, String> {
            let thumb = crate::library::thumbnails::get_cached_or_generate_thumbnail_image(
                &path_clone,
                &app_clone,
                gpu_context.as_ref(),
            )
            .map_err(|e| e.to_string())?;
            let emb = crate::library::tagging::extract_clip_image_embedding(
                &thumb,
                &clip_models.model,
                &clip_models.tokenizer,
            )
            .map_err(|e| e.to_string())?;
            upsert_clip_embedding(&app_clone, &path_clone, &emb);
            Ok(emb)
        })
        .await
        .map_err(|e| format!("Image similarity task failed: {e}"))?;
        vec_res?
    };

    tokio::task::spawn_blocking(move || {
        let slab = load_or_build_clip_slab(&app_handle)?;
        let ranked = rank_cosine_similarity_f16_slab(&slab, &query_vec, max_hits + 1, 0.25);
        Ok(ranked
            .into_iter()
            .filter(|(p, _)| p != &path)
            .take(max_hits)
            .map(|(path, score)| SemanticSearchHit { path, score })
            .collect())
    })
    .await
    .map_err(|e| format!("Similarity ranking failed: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catalog_db_wal_sync_smart_collections_and_crud() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("catalog.db");
        let mut conn = open_catalog_at(&db_path).unwrap();

        let mode: String = conn
            .query_row("PRAGMA journal_mode;", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mode.to_lowercase(), "wal");

        let img_a = dir.path().join("a.arw");
        let img_b = dir.path().join("b.arw");
        std::fs::write(&img_a, b"dummyA").unwrap();
        std::fs::write(&img_b, b"dummyB").unwrap();

        let items = vec![
            ImageFile::from_catalog(
                img_a.to_string_lossy().into_owned(),
                1000,
                false,
                5,
                Some(ImageFlag::Pick),
                Some(vec!["portrait".to_string(), "color:green".to_string()]),
                true,
            ),
            ImageFile::from_catalog(
                img_b.to_string_lossy().into_owned(),
                1000,
                true,
                2,
                None,
                None,
                true,
            ),
        ];
        let folder = dir.path().to_string_lossy().into_owned();
        let inserted = upsert_image_batch(&mut conn, &folder, &items).unwrap();
        assert_eq!(inserted, 2);

        // Second sync with unchanged timestamps & ratings should perform 0 writes
        let resync = upsert_image_batch(&mut conn, &folder, &items).unwrap();
        assert_eq!(resync, 0);

        let picks_unedited = query_smart_collection_from_conn(
            &conn,
            &SmartCollectionRule {
                flag: Some("pick".to_string()),
                is_edited: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(picks_unedited.len(), 1);
        assert_eq!(picks_unedited[0].path, items[0].path);
    }

    #[test]
    fn test_clip_f16_slab_cosine_ranking() {
        let mut a = vec![0.0f32; 512];
        a[0] = 1.0;
        let mut b = vec![0.0f32; 512];
        b[0] = 0.6;
        b[1] = 0.8;
        let mut c = vec![0.0f32; 512];
        c[2] = 1.0;

        let to_u16 = |v: &[f32]| -> Vec<u16> {
            encode_f16_unit(v)
                .chunks_exact(2)
                .map(|ch| u16::from_le_bytes([ch[0], ch[1]]))
                .collect()
        };

        let mut data = Vec::new();
        data.extend(to_u16(&a));
        data.extend(to_u16(&b));
        data.extend(to_u16(&c));

        let slab = ClipEmbeddingSlab {
            paths: vec!["a.jpg".into(), "b.jpg".into(), "c.jpg".into()],
            data,
            dim: 512,
        };
        let ranked = rank_cosine_similarity_f16_slab(&slab, &a, 5, 0.25);
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].0, "a.jpg");
        assert!(ranked[0].1 > 0.99);
        assert_eq!(ranked[1].0, "b.jpg");
        assert!((ranked[1].1 - 0.6).abs() < 0.02);
    }
}
