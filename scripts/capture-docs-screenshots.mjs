import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, '..');
const DIST_DIR = path.join(ROOT, 'dist');
const OUT_DIR = path.join(ROOT, 'docs', 'images', 'v1.7.1.3');
const SAMPLE_DIR = 'D:\\neapdirbti';

const TARGET_CR2_FILES = [
  'IMG_4857.CR2',
  'IMG_4770.CR2',
  'IMG_4753.CR2',
  'IMG_4728.CR2',
  'IMG_4729.CR2',
  'IMG_4730.CR2',
  'IMG_4731.CR2',
  'IMG_4732.CR2',
  'IMG_5042.CR2',
  'IMG_4378.CR2',
  'IMG_5009.CR2',
];

const MIME_TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'application/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.jpg': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.webp': 'image/webp',
  '.woff2': 'font/woff2',
  '.woff': 'font/woff',
  '.ttf': 'font/ttf',
};

function extractCr2EmbeddedJpeg(filePath) {
  const fd = fs.openSync(filePath, 'r');
  try {
    const header = Buffer.alloc(16);
    fs.readSync(fd, header, 0, 16, 0);
    const isLE = header[0] === 0x49 && header[1] === 0x49;
    if (!isLE) return null;

    const ifd0Offset = header.readUInt32LE(4);
    const countBuf = Buffer.alloc(2);
    fs.readSync(fd, countBuf, 0, 2, ifd0Offset);
    const entryCount = countBuf.readUInt16LE(0);

    const entriesBuf = Buffer.alloc(entryCount * 12);
    fs.readSync(fd, entriesBuf, 0, entryCount * 12, ifd0Offset + 2);

    let stripOffset = 0;
    let stripLength = 0;
    for (let i = 0; i < entryCount; i++) {
      const off = i * 12;
      const tag = entriesBuf.readUInt16LE(off);
      const val = entriesBuf.readUInt32LE(off + 8);
      if (tag === 0x0111 || tag === 0x0201) stripOffset = val;
      if (tag === 0x0117 || tag === 0x0202) stripLength = val;
    }

    if (!stripOffset || !stripLength) return null;

    const jpg = Buffer.alloc(stripLength);
    fs.readSync(fd, jpg, 0, stripLength, stripOffset);
    if (jpg[0] !== 0xff || jpg[1] !== 0xd8) return null;
    return jpg;
  } finally {
    fs.closeSync(fd);
  }
}

function prepareSamplePhotos() {
  const photoTempDir = path.join(os.tmpdir(), 'rapidraw-docs-photos');
  fs.mkdirSync(photoTempDir, { recursive: true });

  const photos = [];
  for (const name of TARGET_CR2_FILES) {
    const srcPath = path.join(SAMPLE_DIR, name);
    const stem = path.basename(name, '.CR2');
    const outJpg = path.join(photoTempDir, `${stem}.jpg`);

    if (fs.existsSync(srcPath)) {
      try {
        if (!fs.existsSync(outJpg)) {
          const jpg = extractCr2EmbeddedJpeg(srcPath);
          if (jpg) fs.writeFileSync(outJpg, jpg);
        }
        if (fs.existsSync(outJpg)) {
          photos.push({
            name,
            stem,
            virtualPath: `D:/neapdirbti/${name}`,
            url: `/__docs_photos__/${stem}.jpg`,
            stat: fs.statSync(srcPath),
          });
          continue;
        }
      } catch (err) {
        console.warn(`Warning: failed to extract ${name}: ${err.message}`);
      }
    }
  }

  if (photos.length === 0) {
    const fallbacks = ['splash-dark.jpg', 'splash-enhanced.jpg', 'splash-before.jpg', 'splash-light.jpg', 'splash-grey.jpg'];
    for (const fb of fallbacks) {
      const fbPath = path.join(DIST_DIR, fb);
      if (fs.existsSync(fbPath)) {
        const stem = path.basename(fb, '.jpg');
        photos.push({
          name: `${stem}.CR2`,
          stem,
          virtualPath: `D:/neapdirbti/${stem}.CR2`,
          url: `/${fb}`,
          stat: fs.statSync(fbPath),
        });
      }
    }
  }

  console.log(`Prepared ${photos.length} sample photo(s) for documentation capture.`);
  return { photoTempDir, photos };
}

function startStaticServer(photoTempDir) {
  return new Promise((resolve, reject) => {
    const server = http.createServer((req, res) => {
      const urlPath = decodeURIComponent((req.url || '/').split('?')[0]);
      let filePath;
      if (urlPath.startsWith('/__docs_photos__/')) {
        const rel = urlPath.replace('/__docs_photos__/', '');
        filePath = path.join(photoTempDir, path.basename(rel));
      } else {
        const rel = urlPath === '/' ? 'index.html' : urlPath.replace(/^\/+/, '');
        filePath = path.join(DIST_DIR, rel);
      }

      if (!fs.existsSync(filePath) || fs.statSync(filePath).isDirectory()) {
        filePath = path.join(DIST_DIR, 'index.html');
      }

      const ext = path.extname(filePath).toLowerCase();
      res.writeHead(200, { 'Content-Type': MIME_TYPES[ext] || 'application/octet-stream' });
      fs.createReadStream(filePath).pipe(res);
    });

    server.on('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const addr = server.address();
      resolve({ server, port: addr.port });
    });
  });
}

function findBrowserBinary() {
  const candidates = [
    'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe',
    'C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe',
    'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe',
    'C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe',
  ];
  for (const p of candidates) {
    if (fs.existsSync(p)) return p;
  }
  throw new Error('Could not find Edge or Chrome executable.');
}

function launchHeadlessBrowser(browserExe, userDataDir) {
  return new Promise((resolve, reject) => {
    const args = [
      '--headless=new',
      '--disable-gpu',
      '--no-first-run',
      '--no-default-browser-check',
      '--disable-extensions',
      '--disable-background-networking',
      '--remote-debugging-port=0',
      '--window-size=1440,900',
      `--user-data-dir=${userDataDir}`,
      'about:blank',
    ];
    const proc = spawn(browserExe, args, { shell: false, stdio: ['ignore', 'pipe', 'pipe'] });
    let stderr = '';
    const timer = setTimeout(() => {
      reject(new Error(`Browser CDP did not start in time. Stderr: ${stderr}`));
    }, 15000);

    proc.stderr.on('data', (chunk) => {
      stderr += chunk.toString();
      const match = stderr.match(/DevTools listening on (ws:\/\/[^\s\r\n]+)/);
      if (match) {
        clearTimeout(timer);
        resolve({ proc, wsBrowserUrl: match[1] });
      }
    });

    proc.on('exit', (code) => {
      clearTimeout(timer);
      reject(new Error(`Browser exited prematurely (${code}): ${stderr}`));
    });
  });
}

class CdpClient {
  constructor(wsUrl) {
    this.ws = new WebSocket(wsUrl);
    this.nextId = 1;
    this.pending = new Map();
    this.events = [];
    this.ready = new Promise((resolve, reject) => {
      this.ws.addEventListener('open', resolve, { once: true });
      this.ws.addEventListener('error', reject, { once: true });
    });
    this.ws.addEventListener('message', (evt) => {
      const msg = JSON.parse(evt.data);
      if (msg.id && this.pending.has(msg.id)) {
        const { resolve, reject } = this.pending.get(msg.id);
        this.pending.delete(msg.id);
        if (msg.error) reject(new Error(`${msg.error.message} (${msg.error.code})`));
        else resolve(msg.result);
      } else if (msg.method) {
        this.events.push(msg);
      }
    });
  }

  async send(method, params = {}) {
    await this.ready;
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.ws.send(JSON.stringify({ id, method, params }));
    });
  }

  async eval(expression) {
    const res = await this.send('Runtime.evaluate', {
      expression,
      awaitPromise: true,
      returnByValue: true,
    });
    if (res.exceptionDetails) {
      throw new Error(`Page eval error: ${JSON.stringify(res.exceptionDetails)}`);
    }
    return res.result?.value;
  }

  close() {
    try {
      this.ws.close();
    } catch {}
  }
}

function buildTauriMockInitScript(photos) {
  const serializedPhotos = JSON.stringify(photos);
  return `
    (() => {
      const PHOTOS = ${serializedPhotos};
      window.__DOCS_CAPTURE__ = true;

      const photoMap = Object.fromEntries(PHOTOS.map(p => [p.virtualPath, p]));
      const firstPhoto = PHOTOS[0];

      const exifProfiles = [
        { Make: 'Canon', Model: 'Canon EOS 5D Mark IV', LensModel: 'EF 24-70mm f/2.8L II USM', FNumber: 'f/2.8', ExposureTime: '1/500s', ISOSpeedRatings: '200', FocalLength: '50mm', DateTimeOriginal: '2026:10:18 17:42:10' },
        { Make: 'Canon', Model: 'Canon EOS 5D Mark IV', LensModel: 'EF 85mm f/1.4L IS USM', FNumber: 'f/1.8', ExposureTime: '1/1000s', ISOSpeedRatings: '100', FocalLength: '85mm', DateTimeOriginal: '2026:10:18 17:58:22' },
        { Make: 'Canon', Model: 'Canon EOS 5D Mark IV', LensModel: 'EF 16-35mm f/2.8L III USM', FNumber: 'f/8.0', ExposureTime: '1/125s', ISOSpeedRatings: '400', FocalLength: '24mm', DateTimeOriginal: '2026:10:18 18:14:05' },
        { Make: 'Canon', Model: 'Canon EOS 5D Mark IV', LensModel: 'EF 70-200mm f/2.8L IS III USM', FNumber: 'f/4.0', ExposureTime: '1/640s', ISOSpeedRatings: '800', FocalLength: '135mm', DateTimeOriginal: '2026:10:18 18:22:41' },
      ];

      const mockImageFiles = PHOTOS.map((p, i) => ({
        path: p.virtualPath,
        is_edited: i % 2 === 0,
        modified: Date.now() - i * 3600000,
        rating: i === 0 || i === 1 || i === 4 ? 5 : i % 3 === 0 ? 4 : 3,
        flag: i === 9 ? 'reject' : i < 6 ? 'pick' : null,
        tags: i < 3 ? ['landscape', 'golden-hour', 'hero'] : ['portrait', 'outdoor'],
        exif: exifProfiles[i % exifProfiles.length],
        is_virtual_copy: false,
        is_cloud_placeholder: false,
        is_raw: true,
        group_id: i >= 3 && i <= 7 ? 'grp-bracket-4728' : null,
        needs_ai: false,
      }));

      const mockSettings = {
        theme: 'dark',
        language: 'en',
        hdrDisplay: false,
        libraryPath: 'D:/neapdirbti',
        rootFolders: ['D:/neapdirbti'],
        lastRootPath: 'D:/neapdirbti',
        lastFolderState: { currentFolderPath: 'D:/neapdirbti', expandedFolders: ['D:/neapdirbti'] },
        pinnedFolders: ['D:/neapdirbti'],
        isAiFreeMode: false,
        showFilmstrip: true,
        thumbnailSize: 'medium',
        thumbnailAspectRatio: 'cover',
        libraryViewMode: 'flat',
        libraryDisplayMode: 'grid',
        uiScale: 1,
        autoLoadLastFolder: true,
        continueLastSession: true,
        showTooltips: true,
        useWgpuRenderer: false,
        savedWorkspacePresets: [],
        sortCriteria: { key: 'name', order: 'asc' },
        collapsibleSectionsState: { basic: true, color: true, curves: true, details: true, effects: true },
        uiVisibility: { filmstrip: true, leftPanel: true, rightPanel: true, quickFilter: true },
      };

      const smartCollections = [
        { id: 'sc-heroes', name: '5★ Hero Shots', builtin: true, rules: { minRating: 5 }, count: 3 },
        { id: 'sc-brackets', name: 'Bracketed / Pano Sequences', builtin: true, rules: { rawOnly: true }, count: 5 },
        { id: 'sc-high-iso', name: 'High ISO (Needs Denoise)', builtin: true, rules: { minIso: 400 }, count: 4 },
        { id: 'sc-unedited', name: 'Unedited RAWs', builtin: true, rules: { editedOnly: false }, count: 5 },
      ];

      const folderTree = {
        name: 'neapdirbti',
        path: 'D:/neapdirbti',
        imageCount: PHOTOS.length,
        children: [
          { name: '01_Golden_Hour_RAW', path: 'D:/neapdirbti/01_Golden_Hour_RAW', imageCount: 4, children: [] },
          { name: '02_Bracketed_Pano', path: 'D:/neapdirbti/02_Bracketed_Pano', imageCount: 5, children: [] },
          { name: '03_Night_Sky_Astro', path: 'D:/neapdirbti/03_Night_Sky_Astro', imageCount: 2, children: [] },
        ],
      };

      const callbacks = new Map();
      let cbSeq = 1;

      window.__TAURI_OS_PLUGIN_INTERNALS__ = {
        osType: 'windows',
        family: 'windows',
        platform: 'windows',
        arch: 'x86_64',
        version: '10.0.22631',
        locale: 'en-US',
      };

      window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
        unregisterListener: () => {},
      };

      window.__TAURI_INTERNALS__ = {
        metadata: {
          currentWindow: { label: 'main' },
          currentWebview: { label: 'main' },
        },
        convertFileSrc: (filePath) => {
          const hit = photoMap[filePath];
          if (hit) return hit.url;
          return firstPhoto ? firstPhoto.url : '';
        },
        transformCallback: (cb, once) => {
          const id = cbSeq++;
          callbacks.set(id, { cb, once });
          return id;
        },
        unregisterCallback: (id) => {
          callbacks.delete(id);
        },
        invoke: async (cmd, args = {}) => {
          if (cmd === 'plugin:app|version') return '1.7.1.3';
          if (cmd === 'plugin:app|name') return 'RapidRAW';
          if (cmd === 'plugin:window|scale_factor') return 1.5;
          if (cmd === 'plugin:window|is_fullscreen' || cmd === 'plugin:window|is_maximized') return false;
          if (cmd.startsWith('plugin:event|')) return 1;
          if (cmd.startsWith('plugin:os|')) return 'windows';
          if (cmd.startsWith('plugin:window|')) return null;

          switch (cmd) {
            case 'load_settings':
              return mockSettings;
            case 'save_settings':
            case 'frontend_log':
            case 'set_ui_scale':
            case 'set_foreground_busy':
            case 'start_background_indexing':
            case 'preload_images':
            case 'subscribe_analytics':
            case 'update_wgpu_transform':
            case 'release_original_preview':
            case 'discard_merge':
            case 'cancel_ai_denoise':
            case 'cancel_drizzle':
            case 'cancel_astro_stack':
              return null;
            case 'get_supported_file_types':
              return { raw: ['cr2', 'cr3', 'nef', 'arw', 'dng', 'raf', 'orf', 'rw2'], nonRaw: ['jpg', 'jpeg', 'png', 'tif', 'tiff', 'webp', 'heic', 'avif'] };
            case 'get_or_create_internal_library_root':
              return 'D:/neapdirbti';
            case 'get_folder_tree':
              return [folderTree];
            case 'get_pinned_folder_trees':
              return [folderTree];
            case 'get_folder_children':
              return folderTree.children;
            case 'list_images_in_dir':
            case 'list_images_recursive':
              return mockImageFiles;
            case 'list_smart_collections':
              return smartCollections;
            case 'query_smart_collection':
              return mockImageFiles.slice(0, 5).map(f => f.path);
            case 'semantic_search_catalog':
              return mockImageFiles.slice(0, 4).map((f, idx) => ({ path: f.path, score: 0.94 - idx * 0.06 }));
            case 'get_albums':
              return [
                { id: 'alb-client-1', name: 'Client Selects — Autumn', type: 'album', icon: 'heart', images: PHOTOS.slice(0, 4).map(p => p.virtualPath) },
                { id: 'alb-pano-1', name: 'Panorama & HDR Sets', type: 'album', icon: 'mountain', images: PHOTOS.slice(3, 8).map(p => p.virtualPath) },
              ];
            case 'get_album_images':
              return mockImageFiles.slice(0, 4);
            case 'read_exif_for_paths': {
              const out = {};
              (args.paths || []).forEach((p, idx) => {
                out[p] = exifProfiles[idx % exifProfiles.length];
              });
              return out;
            }
            case 'get_display_status':
              return {
                iccProfileName: 'Dell U2723QE (Display P3 / sRGB)',
                colorSpace: 'sRGB',
                hdrCapable: true,
                hdrActive: false,
                headroom: 1.0,
              };
            case 'is_wgpu_renderer_supported':
              return false;
            case 'is_tethering_supported':
              return true;
            case 'tether_list_cameras':
              return [{ id: 'cam-1', name: 'Canon EOS 5D Mark IV (USB)', port: 'USB001' }];
            case 'get_ai_jobs_status':
              return { current: null, queued: 0, completed: 11, paused: false, failed: [] };
            case 'get_pending_ai_paths':
              return [];
            case 'check_ai_connector_status':
              return { connected: true, modelsInstalled: true };
            case 'load_presets':
              return [
                {
                  folder: {
                    id: 'f1',
                    name: 'Film Looks',
                    children: [
                      { id: 'p-1', name: 'Cinematic Warmth', adjustments: { contrast: 14, vibrance: 18, temperature: 6 }, presetType: 'style' },
                      { id: 'p-2', name: 'Crisp Landscape', adjustments: { clarity: 22, dehaze: 15, shadows: 24 }, presetType: 'style' },
                    ],
                  },
                },
                {
                  folder: {
                    id: 'f2',
                    name: 'Imported Lightroom (.lrtemplate)',
                    children: [
                      { id: 'p-3', name: 'Kodak Portra 400 (.lrtemplate)', adjustments: { contrast: -6, shadows: 18, saturation: 8 }, presetType: 'style' },
                      { id: 'p-4', name: 'B&W Silver Gelatin (.lrtemplate)', adjustments: { isBlackAndWhite: true, contrast: 25 }, presetType: 'style' },
                    ],
                  },
                },
              ];
            case 'load_image': {
              const p = photoMap[args.path] || firstPhoto;
              return {
                path: p.virtualPath,
                width: 6720,
                height: 4480,
                isRaw: true,
                isReady: true,
                thumbnailUrl: p.url,
                asShotWhiteBalance: { temperature: 5600, tint: 4 },
                exif: exifProfiles[0],
                metadata: { cameraMake: 'Canon', cameraModel: 'Canon EOS 5D Mark IV', lensModel: 'EF 24-70mm f/2.8L II USM' },
              };
            }
            case 'load_metadata':
              return { adjustments: null, snapshots: [] };
            case 'get_default_adjustments':
              return { adjustments: null, camera: { make: 'Canon', model: 'Canon EOS 5D Mark IV' }, source: 'camera' };
            case 'apply_adjustments':
              return new ArrayBuffer(0);
            case 'generate_preview_for_path':
            case 'generate_uncropped_preview': {
              const p = photoMap[args.path] || firstPhoto;
              const res = await window.fetch(window.location.origin + p.url);
              return new Uint8Array(await res.arrayBuffer());
            }
            case 'preview_negative_conversion': {
              const p = photoMap[args.path] || firstPhoto;
              return p.url;
            }
            case 'get_storage_usage':
              return {
                thumbnailsBytes: 48 * 1024 * 1024,
                aiDenoiseCacheBytes: 312 * 1024 * 1024,
                aiUpscaleCacheBytes: 128 * 1024 * 1024,
                aiMapsCacheBytes: 86 * 1024 * 1024,
                aiModelsBytes: 1420 * 1024 * 1024,
                metadataCacheBytes: 14 * 1024 * 1024,
                immichCacheBytes: 0,
                catalogDbBytes: 6 * 1024 * 1024,
                cullerProfiles: 12,
                cameraKeys: ['Canon EOS 5D Mark IV'],
              };
            case 'get_local_priors_summary':
              return {
                totalSamples: 42,
                rejectionCount: 3,
                cullerProfiles: 12,
                cameraKeys: ['Canon EOS 5D Mark IV'],
                scenes: [
                  { scene: 'Nature / Landscape', count: 18, exposureOffset: 0.15, contrastOffset: 6 },
                  { scene: 'Portrait / Wedding', count: 14, exposureOffset: 0.25, shadowsOffset: 12 },
                  { scene: 'Night Sky', count: 10, exposureOffset: 0.4, dehazeOffset: 8 },
                ],
              };
            case 'analyze_image_noise_profile':
              return {
                recommendedTier: 'High-Detail NAFNet',
                iso: 1600,
                estimatedSigma: 0.028,
                sensorType: 'Bayer RGGB',
                reason: 'ISO 1600 Canon CR2 — Smart Auto-Tier selected Detail-Preserving AI Denoise',
              };
            case 'analyze_culling_frame':
              return {
                sharpness: 94,
                exposure: 91,
                earScore: 0.34,
                eyesOpen: true,
                isBlurry: false,
                overallScore: 93,
              };
            case 'batch_auto_triage_cull':
              return {
                results: PHOTOS.slice(0, 6).map((p, idx) => ({
                  path: p.virtualPath,
                  sharpness: 0.92 - idx * 0.07,
                  earScore: idx === 2 ? 0.14 : 0.34 - idx * 0.02,
                  eyesOpen: idx !== 2,
                  isBlurry: idx === 5,
                  overallScore: Math.round((0.94 - idx * 0.08) * 100),
                  recommendedFlag: idx < 2 ? 'pick' : idx === 2 ? 'reject' : null,
                })),
              };
            case 'curate_hero_shots':
              return {
                groups: [
                  {
                    groupId: 'grp-1',
                    label: 'Golden Hour Sequence (5 frames)',
                    heroPath: PHOTOS[3].virtualPath,
                    candidates: PHOTOS.slice(3, 8).map((p, idx) => ({
                      path: p.virtualPath,
                      score: 96 - idx * 7,
                      sharpness: 94 - idx * 5,
                      composition: 92 - idx * 4,
                      exposure: 95 - idx * 3,
                      isHero: idx === 0,
                    })),
                  },
                ],
              };
            case 'inspect_hdr_pano_grouping':
            case 'inspect_panorama_brackets':
              return { valid: true, groups: [{ paths: PHOTOS.slice(3, 8).map(p => p.virtualPath), kind: 'panorama' }], message: '5 aligned frames ready' };
            case 'validate_hdr_brackets':
              return { is_perfect: true, status_message: '3 aligned HDR brackets', dynamic_range_ev: 4.2 };
            case 'start_merge':
            case 'update_merge_preview':
              return {
                previewUrl: (PHOTOS[3] || firstPhoto).url,
                width: 9840,
                height: 4120,
                projection: args.settings?.projection || 'panini',
                autoCropRect: { x: 0.03, y: 0.04, width: 0.94, height: 0.92 },
              };
            case 'autodetect_lens':
            case 'get_lens_distortion_params':
              return {
                cameraMake: 'Canon',
                cameraModel: 'Canon EOS 5D Mark IV',
                lensName: 'Canon EF 24-70mm f/2.8L II USM',
                focalLength: 35,
                aperture: 2.8,
                k1: -0.024,
                k2: 0.008,
                vignetting: 0.35,
                caRed: 0.0012,
                caBlue: -0.0009,
              };
            case 'list_camera_profiles':
              return [
                { id: 'dcp-canon-std', name: 'Canon EOS 5D Mark IV Standard (DCP)', source: 'builtin' },
                { id: 'dcp-canon-neutral', name: 'Canon Neutral Faithful (DCP)', source: 'builtin' },
              ];
            case 'estimate_export_sizes':
              return { estimatedBytes: 18.4 * 1024 * 1024, count: 1 };
            case 'scan_active_image_compliance':
              return {
                passed: true,
                score: 94,
                checks: [
                  { id: 'res', label: 'Resolution >= 4MP (30.1 MP)', passed: true },
                  { id: 'noise', label: 'Shadow Noise Floor', passed: true },
                  { id: 'clip', label: 'Highlight Clipping < 0.5%', passed: true },
                  { id: 'trademark', label: 'No Detected Logos / Watermarks', passed: true },
                ],
              };
            case 'auto_remove_dust_and_blemishes':
              return {
                total_detected: 4,
                message: 'Detected 4 sensor dust spots.',
                spots: [
                  { x: 1850, y: 920, radius: 28, norm_x: 0.28, norm_y: 0.21, kind: 'spot', points: [], routing: 'heal' },
                  { x: 3120, y: 1240, radius: 22, norm_x: 0.46, norm_y: 0.28, kind: 'spot', points: [], routing: 'heal' },
                  { x: 4540, y: 810, radius: 34, norm_x: 0.68, norm_y: 0.18, kind: 'spot', points: [], routing: 'lama' },
                  { x: 5120, y: 1560, radius: 20, norm_x: 0.76, norm_y: 0.35, kind: 'spot', points: [], routing: 'heal' },
                ],
              };
            case 'get_lensfun_makers':
              return ['Canon', 'Nikon', 'Sony', 'Fujifilm', 'Sigma', 'Tamron'];
            case 'get_lensfun_models':
              return ['EF 24-70mm f/2.8L II USM', 'EF 85mm f/1.4L IS USM', 'EF 16-35mm f/2.8L III USM'];
            case 'list_luts':
              return [
                { name: 'Neutral Film', path: 'builtin/neutral.cube', isBuiltIn: true },
                { name: 'Kodak 2383 Print', path: 'builtin/kodak2383.cube', isBuiltIn: true },
              ];
            case 'get_lut_preview':
              return firstPhoto ? firstPhoto.url : null;
            default:
              if (cmd.startsWith('list_')) return [];
              return null;
          }
        },
      };
    })();
  `;
}

async function main() {
  if (!fs.existsSync(DIST_DIR)) {
    throw new Error('dist/ not found. Run npm.cmd run build first.');
  }
  fs.mkdirSync(OUT_DIR, { recursive: true });

  const { photoTempDir, photos } = prepareSamplePhotos();
  const { server, port } = await startStaticServer(photoTempDir);
  const browserExe = findBrowserBinary();
  const userDataDir = fs.mkdtempSync(path.join(os.tmpdir(), 'rapidraw-cdp-'));

  let browserProc;
  let cdp;

  try {
    const launched = await launchHeadlessBrowser(browserExe, userDataDir);
    browserProc = launched.proc;

    const listRes = await fetch(`http://127.0.0.1:${new URL(launched.wsBrowserUrl).port}/json/list`);
    const targets = await listRes.json();
    const pageTarget = targets.find((t) => t.type === 'page') || targets[0];
    if (!pageTarget?.webSocketDebuggerUrl) {
      throw new Error('No page target found in headless browser.');
    }

    cdp = new CdpClient(pageTarget.webSocketDebuggerUrl);
    cdp.ws.addEventListener('message', (evt) => {
      const m = JSON.parse(evt.data);
      if (m.method === 'Runtime.exceptionThrown') {
        console.error('PAGE EXCEPTION:', m.params?.exceptionDetails?.text, m.params?.exceptionDetails?.exception?.description);
      } else if (m.method === 'Runtime.consoleAPICalled' && m.params?.type === 'error') {
        console.error('PAGE CONSOLE ERROR:', (m.params.args || []).map((a) => a.value || a.description).join(' '));
      }
    });
    await cdp.send('Page.enable');
    await cdp.send('Runtime.enable');
    await cdp.send('Emulation.setDeviceMetricsOverride', {
      width: 1440,
      height: 900,
      deviceScaleFactor: 1.5,
      mobile: false,
    });
    await cdp.send('Page.addScriptToEvaluateOnNewDocument', {
      source: buildTauriMockInitScript(photos),
    });

    const appUrl = process.env.DOCS_TEST_URL || `http://127.0.0.1:${port}/`;
    await cdp.send('Page.navigate', { url: appUrl });

    // Wait for React + __DOCS_STORES__ to mount
    let mounted = false;
    for (let i = 0; i < 60; i++) {
      const ready = await cdp.eval(`Boolean(window.__DOCS_STORES__ && document.querySelector('#root')?.children.length > 0)`);
      if (ready) {
        mounted = true;
        break;
      }
      await new Promise((r) => setTimeout(r, 200));
    }
    const diag = await cdp.eval(`({ mounted: ${mounted}, hasStores: Boolean(window.__DOCS_STORES__), rootChildren: document.querySelector('#root')?.children.length || 0, bodyLen: document.body.innerText.length })`);
    console.log('Page mount status:', JSON.stringify(diag));

    // Allow initial async effects (load_settings, folder tree) to settle
    await new Promise((r) => setTimeout(r, 1000));

    // Seed rich initial state into stores
    await cdp.eval(`
      (() => {
        const S = window.__DOCS_STORES__;
        if (!S) throw new Error('__DOCS_STORES__ not available');
        const photos = ${JSON.stringify(photos)};
        const thumbMap = {};
        photos.forEach(p => { thumbMap[p.virtualPath] = p.url; });

        S.useProcessStore.getState().setProcess({
          thumbnails: thumbMap,
          mediumThumbnails: thumbMap,
          aiJobs: { current: null, queued: 0, completed: 11, paused: false, failed: [] },
        });

        const exifByPath = {};
        photos.forEach((p, i) => {
          exifByPath[p.virtualPath] = {
            Make: 'Canon',
            Model: 'Canon EOS 5D Mark IV',
            LensModel: 'EF 24-70mm f/2.8L II USM',
            FNumber: i % 2 === 0 ? 'f/2.8' : 'f/5.6',
            ExposureTime: '1/500s',
            ISOSpeedRatings: i % 3 === 0 ? '1600' : '200',
            FocalLength: '50mm',
            DateTimeOriginal: '2026:10:18 17:42:10',
          };
        });

        const seededFiles = photos.map((p, i) => ({
          path: p.virtualPath,
          is_edited: i % 2 === 0,
          modified: Date.now() - i * 3600000,
          rating: i === 0 || i === 1 || i === 4 ? 5 : i % 3 === 0 ? 4 : 3,
          flag: i === 9 ? 'reject' : i < 6 ? 'pick' : null,
          tags: i < 3 ? ['landscape', 'golden-hour', 'hero'] : ['portrait', 'outdoor'],
          exif: exifByPath[p.virtualPath],
          is_virtual_copy: false,
          is_cloud_placeholder: false,
          is_raw: true,
          group_id: i >= 3 && i <= 7 ? 'grp-bracket-4728' : null,
          needs_ai: false,
        }));

        S.useLibraryStore.getState().setLibrary({
          rootPaths: ['D:/neapdirbti'],
          currentFolderPath: 'D:/neapdirbti',
          imageList: seededFiles,
          exifByPath,
          libraryActivePath: photos[0].virtualPath,
          multiSelectedPaths: photos.slice(3, 8).map(p => p.virtualPath),
          filterCriteria: { colors: [], rating: 0, rawStatus: 'all', editedStatus: 'all', flagStatus: 'all' },
        });

        // Generate synthetic luma/RGB histogram bins for a vivid scope
        const bins = Array.from({ length: 256 }, (_, x) => Math.round(40 + 80 * Math.exp(-Math.pow((x - 118) / 52, 2))));
        const rBins = Array.from({ length: 256 }, (_, x) => Math.round(35 + 75 * Math.exp(-Math.pow((x - 130) / 48, 2))));
        const gBins = Array.from({ length: 256 }, (_, x) => Math.round(35 + 78 * Math.exp(-Math.pow((x - 116) / 50, 2))));
        const bBins = Array.from({ length: 256 }, (_, x) => Math.round(30 + 70 * Math.exp(-Math.pow((x - 98) / 54, 2))));

        S.useEditorStore.getState().setEditor({
          isWaveformVisible: true,
          histogram: {
            luma: bins,
            red: rBins,
            green: gBins,
            blue: bBins,
          },
          displaySize: { width: 900, height: 600 },
          baseRenderSize: { width: 900, height: 600, containerWidth: 960, containerHeight: 720, offsetX: 30, offsetY: 60 },
          originalSize: { width: 6720, height: 4480 },
        });
      })()
    `);
    console.log('Seed completed.');

    const shots = [
      {
        file: '01-library-smart-collections.jpg',
        setup: `
          S.useUIStore.getState().setUI({
            activeView: 'library',
            isSettingsOpen: false,
            uiVisibility: { filmstrip: true, leftPanel: true, rightPanel: true, quickFilter: true },
          });
          S.useLibraryStore.getState().setLibrary({ activeSmartCollectionId: null });
        `,
      },
      {
        file: '02-speed-culler-ear.jpg',
        setup: `
          S.useUIStore.getState().setUI({
            activeView: 'library',
            speedCullerModalState: {
              isOpen: true,
              selectedPaths: PHOTOS.slice(0, 6).map(p => p.virtualPath),
              initialIndex: 0,
            },
          });
        `,
      },
      {
        file: '03-hero-curator.jpg',
        setup: `
          S.useUIStore.getState().setUI({
            activeView: 'library',
            heroCuratorModalState: {
              isOpen: true,
              selectedPaths: PHOTOS.slice(3, 8).map(p => p.virtualPath),
            },
          });
        `,
      },
      {
        file: '04-editor-workspace-overview.jpg',
        setup: `
          S.useUIStore.getState().setUI({
            activeView: 'editor',
            isSettingsOpen: false,
            uiVisibility: { filmstrip: true, leftPanel: true, rightPanel: true, quickFilter: true },
            collapsibleSectionsState: { basic: true, color: false, curves: false, details: false, effects: false },
          });
          S.useUIStore.getState().setPanel('adjustments');
          S.useEditorStore.getState().setEditor({
            selectedImage: {
              path: PHOTOS[0].virtualPath, width: 6720, height: 4480, isRaw: true, isReady: true, thumbnailUrl: PHOTOS[0].url,
              asShotWhiteBalance: { temperature: 5600, tint: 4 },
              exif: S.useLibraryStore.getState().exifByPath[PHOTOS[0].virtualPath],
            },
            finalPreviewUrl: PHOTOS[0].url,
            compareMode: 'none',
            isDustSpotEditActive: false,
          });
        `,
      },
      {
        file: '05-smart-auto-edit-priors.jpg',
        setup: `
          S.useUIStore.getState().setUI({
            activeView: 'editor',
            collapsibleSectionsState: { basic: true, color: false, curves: false, details: false, effects: false },
          });
          S.useUIStore.getState().setPanel('adjustments');
          S.useEditorStore.getState().setEditor({
            pendingAutoEdit: {
              path: PHOTOS[0].virtualPath,
              scene: 'Nature / Golden Hour',
              previousAdjustments: S.useEditorStore.getState().adjustments,
            },
          });
        `,
      },
      {
        file: '06-lights-supercontrast-ribbon.jpg',
        setup: `
          S.useUIStore.getState().setUI({
            activeView: 'editor',
            collapsibleSectionsState: { basic: true, color: false, curves: false, details: false, effects: false },
          });
          S.useUIStore.getState().setPanel('adjustments');
          S.useEditorStore.getState().setEditor({ pendingAutoEdit: null });
          setTimeout(() => {
            const panel = document.querySelector('[data-panel-content="adjustments"]') || document.querySelector('.overflow-y-auto');
            if (panel) panel.scrollTop = 520;
          }, 120);
        `,
      },
      {
        file: '07-tones-studio-panel.jpg',
        setup: `
          S.useUIStore.getState().setUI({ activeView: 'editor' });
          S.useUIStore.getState().setPanel('tones');
        `,
      },
      {
        file: '08-color-curves-scopes.jpg',
        setup: `
          S.useUIStore.getState().setUI({
            activeView: 'editor',
            collapsibleSectionsState: { basic: false, color: true, curves: true, details: false, effects: false },
          });
          S.useUIStore.getState().setPanel('adjustments');
        `,
      },
      {
        file: '09-split-compare-4way-matrix.jpg',
        setup: `
          S.useUIStore.getState().setUI({ activeView: 'editor' });
          S.useUIStore.getState().setPanel('adjustments');
          S.useEditorStore.getState().setEditor({
            compareMode: 'vertical_split',
            splitPosition: 0.5,
            originalPreview: { path: PHOTOS[0].virtualPath, url: PHOTOS[1]?.url || PHOTOS[0].url },
          });
        `,
      },
      {
        file: '10-masks-panel-refinement.jpg',
        setup: `
          S.useUIStore.getState().setUI({ activeView: 'editor' });
          S.useUIStore.getState().setPanel('masks');
          S.useEditorStore.getState().setEditor({ compareMode: 'none' });
        `,
      },
      {
        file: '11-ai-panel-dust-scratch-pro.jpg',
        setup: `
          S.useUIStore.getState().setUI({ activeView: 'editor' });
          S.useUIStore.getState().setPanel('ai');
          S.useEditorStore.getState().setEditor({
            isDustSpotEditActive: true,
            dustSpotCandidates: [
              { x: 1850, y: 920, radius: 28, norm_x: 0.28, norm_y: 0.21, kind: 'spot', points: [], routing: 'heal' },
              { x: 3120, y: 1240, radius: 22, norm_x: 0.46, norm_y: 0.28, kind: 'spot', points: [], routing: 'heal' },
              { x: 4540, y: 810, radius: 34, norm_x: 0.68, norm_y: 0.18, kind: 'spot', points: [], routing: 'lama' },
              { x: 5120, y: 1560, radius: 20, norm_x: 0.76, norm_y: 0.35, kind: 'spot', points: [], routing: 'heal' },
            ],
          });
        `,
      },
      {
        file: '12-denoise-modal-minimap.jpg',
        setup: `
          S.useEditorStore.getState().setEditor({ isDustSpotEditActive: false, dustSpotCandidates: [] });
          S.useUIStore.getState().setUI({
            denoiseModalState: {
              isOpen: true,
              isProcessing: false,
              previewBase64: PHOTOS[0].url,
              originalBase64: PHOTOS[0].url,
              error: null,
              targetPaths: [PHOTOS[0].virtualPath],
              progressMessage: null,
              isRaw: true,
            },
          });
        `,
      },
      {
        file: '13-bokeh-fusion-modal.jpg',
        setup: `
          S.useUIStore.getState().setUI({
            bokehModalState: { isOpen: true },
          });
        `,
      },
      {
        file: '14-crop-lens-transform.jpg',
        setup: `
          S.useUIStore.getState().setUI({ activeView: 'editor' });
          S.useUIStore.getState().setPanel('crop');
        `,
      },
      {
        file: '15-merge-modal-panorama.jpg',
        setup: `
          S.useUIStore.getState().setUI({
            mergeModalState: {
              isOpen: true,
              isProcessing: false,
              isUpdatingPreview: false,
              kind: 'panorama',
              sourcePaths: PHOTOS.slice(3, 8).map(p => p.virtualPath),
              error: null,
              progressMessage: null,
              preview: {
                base64: PHOTOS[3].url,
                width: 9840,
                height: 4120,
              },
              settings: {
                autoCrop: true,
                boundaryWarp: 25,
                exposureCompensation: true,
                projection: 'panini',
                yawDeg: 0,
                pitchDeg: 0,
                rollDeg: 0,
                fastDraft: false,
                halfSize: false,
                applyEdits: true,
                hdrPanorama: null,
                hdrProfile: 'natural',
                hdrStrength: 100,
                exposureBias: 0,
                highlightRecovery: 50,
                shadowLift: 30,
                detailBoost: 1.05,
                outputFormat: 'tiff',
              },
            },
          });
        `,
      },
      {
        file: '16-merge-modal-hdr-focus.jpg',
        setup: `
          S.useUIStore.getState().setUI({
            mergeModalState: {
              isOpen: true,
              isProcessing: false,
              isUpdatingPreview: false,
              kind: 'hdr',
              sourcePaths: PHOTOS.slice(3, 6).map(p => p.virtualPath),
              error: null,
              progressMessage: null,
              preview: {
                base64: PHOTOS[4].url,
                width: 6720,
                height: 4480,
              },
              settings: {
                autoCrop: true,
                halfSize: false,
                applyEdits: true,
                profile: 'agxScene',
                hdrStrength: 100,
                deghostSensitivity: 'medium',
                deghostMode: 'opticalFlow',
                previewMode: 'sdr',
                exposureBias: 0,
                highlightRecovery: 50,
                shadowLift: 30,
                detailBoost: 1.05,
                outputFormat: 'tiff',
              },
            },
          });
        `,
      },
      {
        file: '17-drizzle-superres-modal.jpg',
        setup: `
          S.useUIStore.getState().setUI({
            drizzleModalState: {
              isOpen: true,
              isProcessing: false,
              sourcePaths: PHOTOS.slice(3, 8).map(p => p.virtualPath),
              scaleFactor: 2,
              pixfrac: 0.7,
              bayerCfaMode: true,
              alignmentMode: 'stars',
              progressMessage: null,
              error: null,
              finalImageBase64: PHOTOS[0].url,
              originalBase64: PHOTOS[0].url,
              meta: { width: 13440, height: 8960, scale: 2, frames_stacked: 5, snr_boost: '+6.8 dB' },
            },
          });
        `,
      },
      {
        file: '18-night-sky-focus-dropzones.jpg',
        setup: `
          S.useUIStore.getState().setUI({
            activeView: 'editor',
            nightSkyState: {
              ...S.useUIStore.getState().nightSkyState,
              isExpanded: true,
              targetPaths: PHOTOS.slice(3, 8).map(p => p.virtualPath),
            },
          });
          S.useUIStore.getState().setPanel('adjustments');
        `,
      },
      {
        file: '19-color-matcher-sot.jpg',
        setup: `
          S.useUIStore.getState().setUI({
            colorMatcherModalState: {
              isOpen: true,
              heroPath: PHOTOS[0].virtualPath,
              targetPaths: PHOTOS.slice(1, 5).map(p => p.virtualPath),
              initialMode: 'matcher',
            },
          });
        `,
      },
      {
        file: '20-negative-conversion-lock.jpg',
        setup: `
          S.useUIStore.getState().setUI({
            negativeModalState: {
              isOpen: true,
              targetPaths: [PHOTOS[0].virtualPath],
            },
          });
        `,
      },
      {
        file: '21-batch-polish-modal.jpg',
        setup: `
          S.useUIStore.getState().setUI({
            batchPolishModalState: {
              isOpen: true,
              selectedPaths: PHOTOS.slice(0, 6).map(p => p.virtualPath),
            },
          });
        `,
      },
      {
        file: '22-presets-lrtemplate.jpg',
        setup: `
          S.useUIStore.getState().setUI({ activeView: 'editor' });
          S.useUIStore.getState().setPanel('presets');
        `,
      },
      {
        file: '23-export-typography-advanced.jpg',
        setup: `
          S.useUIStore.getState().setUI({ activeView: 'editor' });
          S.useUIStore.getState().setPanel('export');
        `,
      },
      {
        file: '24-client-delivery-modal.jpg',
        setup: `
          S.useUIStore.getState().setUI({
            clientDeliveryModalState: {
              isOpen: true,
              selectedPaths: PHOTOS.slice(0, 6).map(p => p.virtualPath),
            },
          });
        `,
      },
      {
        file: '25-collage-tethering.jpg',
        setup: `
          S.useUIStore.getState().setUI({
            collageModalState: {
              isOpen: true,
              sourceImages: PHOTOS.slice(0, 4).map(p => ({ path: p.virtualPath })),
            },
          });
        `,
      },
      {
        file: '26-settings-data-management.jpg',
        setup: `
          S.useUIStore.getState().setUI({
            isSettingsOpen: true,
          });
        `,
      },
    ];

    const resetModalsExpr = `
      (() => {
        const S = window.__DOCS_STORES__;
        S.useUIStore.getState().setUI({
          isSettingsOpen: false,
          isLensCorrectionModalOpen: false,
          isTransformModalOpen: false,
          speedCullerModalState: { isOpen: false, selectedPaths: [], initialIndex: 0 },
          heroCuratorModalState: { isOpen: false, selectedPaths: [] },
          denoiseModalState: { isOpen: false, isProcessing: false, previewBase64: null, originalBase64: null, error: null, targetPaths: [], progressMessage: null, isRaw: true },
          bokehModalState: { isOpen: false },
          mergeModalState: { isOpen: false, isProcessing: false, isUpdatingPreview: false, kind: 'panorama', preview: null, progressMessage: null, error: null, settings: {}, sourcePaths: [] },
          drizzleModalState: { isOpen: false, isProcessing: false, sourcePaths: [], scaleFactor: 2, pixfrac: 0.7, progressMessage: null, error: null, finalImageBase64: null, originalBase64: null, meta: null },
          colorMatcherModalState: { isOpen: false, heroPath: null, targetPaths: [] },
          negativeModalState: { isOpen: false, targetPaths: [] },
          batchPolishModalState: { isOpen: false, selectedPaths: [] },
          clientDeliveryModalState: { isOpen: false, selectedPaths: [] },
          collageModalState: { isOpen: false, sourceImages: [] },
          advancedExportModalState: { isOpen: false, selectedPaths: [] },
        });
      })()
    `;

    for (const shot of shots) {
      await cdp.eval(resetModalsExpr);
      await cdp.eval(`
        (() => {
          const S = window.__DOCS_STORES__;
          const PHOTOS = ${JSON.stringify(photos)};
          ${shot.setup}
        })()
      `);

      // Wait for lazy Suspense & images to finish loading
      await new Promise((r) => setTimeout(r, 700));
      await cdp.eval(`
        Promise.all(
          Array.from(document.images)
            .filter(img => !img.complete)
            .map(img => new Promise(res => { img.onload = res; img.onerror = res; setTimeout(res, 2000); }))
        )
      `);
      await new Promise((r) => setTimeout(r, 250));

      const health = await cdp.eval(`
        (() => {
          const txt = document.body.innerText || '';
          return {
            hasModalError: txt.includes('Modal Error'),
            childCount: document.querySelector('#root')?.children.length || 0,
          };
        })()
      `);
      if (health.hasModalError) {
        console.warn(`WARNING: Modal Error detected on ${shot.file}`);
      }

      const shotRes = await cdp.send('Page.captureScreenshot', { format: 'jpeg', quality: 90 });
      const outPath = path.join(OUT_DIR, shot.file);
      fs.writeFileSync(outPath, Buffer.from(shotRes.data, 'base64'));
      const sizeKb = Math.round(fs.statSync(outPath).size / 1024);
      console.log(`Captured ${shot.file} (${sizeKb} KB)`);
    }

    console.log('All 26 screenshots captured successfully.');
  } finally {
    if (cdp) cdp.close();
    if (browserProc) {
      try {
        browserProc.kill();
      } catch {}
    }
    server.close();
    try {
      fs.rmSync(userDataDir, { recursive: true, force: true });
    } catch {}
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
