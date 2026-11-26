import fs from 'node:fs';
import path from 'node:path';
import ts from 'typescript';

const ROOT = process.cwd();
const RUST_SRC = path.join(ROOT, 'src-tauri', 'src');
const TS_SRC = path.join(ROOT, 'src');

function walk(dir, exts) {
  const results = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      results.push(...walk(full, exts));
    } else if (exts.some((ext) => entry.name.endsWith(ext))) {
      results.push(full);
    }
  }
  return results;
}

function lineNumberAt(content, index) {
  return content.slice(0, index).split(/\r?\n/).length;
}

function snakeToCamel(s) {
  return s.replace(/_([a-z0-9])/g, (_, c) => c.toUpperCase());
}

const rustFiles = walk(RUST_SRC, ['.rs']);
const tsFiles = walk(TS_SRC, ['.ts', '.tsx']);

// =========================================================================
// 1. Bidirectional Tauri Event Parity Check
// =========================================================================
const emittedEvents = new Map();
const emitRegex = /(?:\.emit\(\s*"([a-zA-Z0-9_-]+)"|\.emit_to\(\s*[^,]+,\s*"([a-zA-Z0-9_-]+)"|notify_viewer\(\s*[^,]+,\s*"([a-zA-Z0-9_-]+)")/g;
for (const file of rustFiles) {
  const content = fs.readFileSync(file, 'utf8');
  let m;
  while ((m = emitRegex.exec(content)) !== null) {
    const eventName = m[1] || m[2] || m[3];
    if (!emittedEvents.has(eventName)) emittedEvents.set(eventName, []);
    emittedEvents.get(eventName).push(`${path.relative(ROOT, file)}:${lineNumberAt(content, m.index)}`);
  }
}

const listenedEvents = new Map();
const listenRegex = /\blisten(?:<[^>]+>)?\(\s*['"]([a-zA-Z0-9_-]+)['"]/g;
for (const file of tsFiles) {
  const content = fs.readFileSync(file, 'utf8');
  let m;
  while ((m = listenRegex.exec(content)) !== null) {
    const evt = m[1];
    if (!listenedEvents.has(evt)) listenedEvents.set(evt, []);
    listenedEvents.get(evt).push(`${path.relative(ROOT, file)}:${lineNumberAt(content, m.index)}`);
  }
}

const ALLOWED_UNLISTENED_EVENTS = new Set([
  'ai-job-result',
  'immich-sync',
  'immich-upload',
  'immich-trash',
  'immich-library-changed',
  'ui-scale-changed',
  'upscale-progress',
]);

// Events that are emitted by Tauri itself or platform-specific modules
const ALLOWED_EXTERNAL_LISTENERS = new Set([
  'tauri://drag-drop',
  'tauri://drag-enter',
  'tauri://drag-leave',
  'tauri://file-drop',
  'tauri://close-requested',
  'tauri://resize',
  'tauri://move',
  'display-changed',
]);

let errors = 0;
console.log('=== 1. Bidirectional Tauri Event Parity Check ===');
for (const [evt, locs] of emittedEvents.entries()) {
  if (!listenedEvents.has(evt) && !ALLOWED_UNLISTENED_EVENTS.has(evt)) {
    console.error(`ORPHANED EVENT: "${evt}" emitted at ${locs[0]} has no listen("${evt}") in src/`);
    errors++;
  }
}
for (const [evt, locs] of listenedEvents.entries()) {
  if (!emittedEvents.has(evt) && !ALLOWED_EXTERNAL_LISTENERS.has(evt)) {
    console.error(`DEAD LISTENER: listen("${evt}") at ${locs[0]} is never emitted in src-tauri/src/`);
    errors++;
  }
}
console.log(`Checked ${emittedEvents.size} emitted events and ${listenedEvents.size} frontend listeners.`);

// =========================================================================
// 2. Tauri Command Registration Parity Check
// =========================================================================
console.log('\n=== 2. Tauri Command Parity Check ===');
const invokesTs = fs.readFileSync(path.join(TS_SRC, 'types', 'invokes.ts'), 'utf8');
const libRs = fs.readFileSync(path.join(RUST_SRC, 'lib.rs'), 'utf8');
const invokeEnumRegex = /(\w+)\s*=\s*'([a-zA-Z0-9_]+)'/g;
const enumToCommand = new Map();
const registeredCommands = new Set();
const handlerBlockMatch = libRs.match(/tauri::generate_handler!\[([\s\S]*?)\]/);
if (handlerBlockMatch) {
  const block = handlerBlockMatch[1];
  for (const line of block.split(/\r?\n/)) {
    const cleaned = line.replace(/\/\/.*$/, '').trim().replace(/,$/, '');
    if (!cleaned) continue;
    const parts = cleaned.split('::');
    registeredCommands.add(parts[parts.length - 1]);
  }
}

let invokeCount = 0;
let m;
while ((m = invokeEnumRegex.exec(invokesTs)) !== null) {
  const enumKey = m[1];
  const cmd = m[2];
  enumToCommand.set(enumKey, cmd);
  invokeCount++;
  if (!registeredCommands.has(cmd)) {
    console.error(`UNREGISTERED COMMAND: Invokes.${enumKey} ("${cmd}") is not in tauri::generate_handler![] in lib.rs`);
    errors++;
  }
}

const directInvokeRegex = /\binvoke(?:<[^>]+>)?\(\s*['"]([a-zA-Z0-9_]+)['"]/g;
let directInvokeCount = 0;
for (const file of tsFiles) {
  const content = fs.readFileSync(file, 'utf8');
  while ((m = directInvokeRegex.exec(content)) !== null) {
    const cmd = m[1];
    directInvokeCount++;
    if (!registeredCommands.has(cmd)) {
      console.error(
        `UNREGISTERED DIRECT INVOKE: "${cmd}" at ${path.relative(ROOT, file)}:${lineNumberAt(content, m.index)} is not in tauri::generate_handler![]`,
      );
      errors++;
    }
  }
}
console.log(`Checked ${invokeCount} Invokes entries + ${directInvokeCount} direct invoke() calls against ${registeredCommands.size} registered handlers.`);

// =========================================================================
// 3. Tauri Command Argument Contract Parity Check
// =========================================================================
console.log('\n=== 3. Tauri Command Argument Contract Parity Check ===');
const rustCommandParams = new Map();
const cmdFnRegex = /#\[tauri::command(?:\([^\)]*\))?\]\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+([a-zA-Z0-9_]+)\s*(?:<[^>]*>)?\s*\(([\s\S]*?)\)\s*(?:->|\{)/g;

for (const file of rustFiles) {
  const content = fs.readFileSync(file, 'utf8');
  let cm;
  while ((cm = cmdFnRegex.exec(content)) !== null) {
    const fnName = cm[1];
    const paramsRaw = cm[2];
    const renameSnake = cm[0].includes('rename_all = "snake_case"');
    const paramNames = new Set();

    // Split top-level comma-separated parameters
    let depth = 0;
    let current = '';
    for (const ch of paramsRaw) {
      if (ch === '<' || ch === '(' || ch === '[' || ch === '{') depth++;
      else if (ch === '>' || ch === ')' || ch === ']' || ch === '}') depth--;
      if (ch === ',' && depth === 0) {
        if (current.trim()) paramNames.add(current.trim());
        current = '';
      } else {
        current += ch;
      }
    }
    if (current.trim()) paramNames.add(current.trim());

    const allowedKeys = new Set();
    for (const p of paramNames) {
      const colonIdx = p.indexOf(':');
      if (colonIdx === -1) continue;
      const rawName = p.slice(0, colonIdx).replace(/^mut\s+/, '').trim();
      const rawType = p.slice(colonIdx + 1).trim();
      if (/\b(?:State|AppHandle|Window|WebviewWindow)\b/.test(rawType)) {
        continue;
      }
      const cleanName = rawName.replace(/^_/, '');
      allowedKeys.add(renameSnake ? cleanName : snakeToCamel(cleanName));
    }
    rustCommandParams.set(fnName, allowedKeys);
  }
}

const invokePayloadRegex = /\binvoke(?:<[^>]*>)?\(\s*(?:Invokes\.(\w+)|['"]([a-zA-Z0-9_]+)['"])\s*,\s*\{([\s\S]*?)\}\s*\)/g;
let checkedArgCalls = 0;
for (const file of tsFiles) {
  const content = fs.readFileSync(file, 'utf8');
  let pm;
  while ((pm = invokePayloadRegex.exec(content)) !== null) {
    const cmdName = pm[1] ? enumToCommand.get(pm[1]) : pm[2];
    if (!cmdName || !rustCommandParams.has(cmdName)) continue;
    const expectedParams = rustCommandParams.get(cmdName);
    const body = pm[3];
    if (body.includes('...')) continue; // skip spread objects

    checkedArgCalls++;
    // Extract top-level object literal keys
    let depth = 0;
    let token = '';
    const keys = [];
    let expectingKey = true;
    for (let i = 0; i < body.length; i++) {
      const ch = body[i];
      if (ch === '{' || ch === '[' || ch === '(') {
        depth++;
        expectingKey = false;
      } else if (ch === '}' || ch === ']' || ch === ')') {
        depth--;
      } else if (ch === ',' && depth === 0) {
        const trimmed = token.trim();
        if (expectingKey && /^[a-zA-Z0-9_]+$/.test(trimmed)) {
          keys.push(trimmed);
        }
        token = '';
        expectingKey = true;
      } else if (ch === ':' && depth === 0 && expectingKey) {
        const trimmed = token.trim();
        if (/^[a-zA-Z0-9_]+$/.test(trimmed)) {
          keys.push(trimmed);
        }
        token = '';
        expectingKey = false;
      } else if (depth === 0 && expectingKey) {
        token += ch;
      }
    }
    const lastTrimmed = token.trim();
    if (expectingKey && /^[a-zA-Z0-9_]+$/.test(lastTrimmed)) {
      keys.push(lastTrimmed);
    }

    for (const k of keys) {
      if (!expectedParams.has(k)) {
        console.error(
          `IPC ARG MISMATCH: "${cmdName}" at ${path.relative(ROOT, file)}:${lineNumberAt(content, pm.index)} passes "${k}", expected one of [${[...expectedParams].join(', ')}]`,
        );
        errors++;
      }
    }
  }
}
console.log(`Checked ${checkedArgCalls} invoke() calls with object payloads against ${rustCommandParams.size} #[tauri::command] signatures.`);

// =========================================================================
// 4. React Event-Smuggling Guard Check
// =========================================================================
console.log('\n=== 4. React Event-Smuggling Guard Check ===');
const productivityTs = fs.readFileSync(path.join(TS_SRC, 'hooks', 'useProductivityActions.ts'), 'utf8');
if (!productivityTs.includes("typeof kindOverride === 'string'")) {
  console.error('EVENT-SMUGGLING RISK: handleStartMerge in useProductivityActions.ts must guard typeof kindOverride === "string"');
  errors++;
}
console.log('Event-smuggling guard checks passed.');

// =========================================================================
// 5. Version Display Parity Check
// =========================================================================
console.log('\n=== 5. Version Display Parity Check ===');
const pkgJson = JSON.parse(fs.readFileSync(path.join(ROOT, 'package.json'), 'utf8'));
const mainLibTsx = fs.readFileSync(path.join(TS_SRC, 'components', 'panel', 'MainLibrary.tsx'), 'utf8');
const displayVersionMatch = mainLibTsx.match(/APP_DISPLAY_VERSION\s*=\s*['"]([^'"]+)['"]/);
if (!displayVersionMatch || displayVersionMatch[1] !== pkgJson.version) {
  console.error(
    `VERSION MISMATCH: package.json (${pkgJson.version}) != APP_DISPLAY_VERSION (${displayVersionMatch?.[1] ?? 'missing'}) in MainLibrary.tsx`,
  );
  errors++;
}
const cargoToml = fs.readFileSync(path.join(ROOT, 'src-tauri', 'Cargo.toml'), 'utf8');
const tauriConf = JSON.parse(fs.readFileSync(path.join(ROOT, 'src-tauri', 'tauri.conf.json'), 'utf8'));
const cargoVerMatch = cargoToml.match(/^version\s*=\s*"([^"]+)"/m);
if (!cargoVerMatch || !/^\d+\.\d+\.\d+$/.test(cargoVerMatch[1]) || cargoVerMatch[1] !== tauriConf.version) {
  console.error(
    `BUNDLER SEMVER MISMATCH: Cargo.toml (${cargoVerMatch?.[1]}) and tauri.conf.json (${tauriConf.version}) must match 3-part SemVer`,
  );
  errors++;
}
const wixVersion = tauriConf.bundle?.windows?.wix?.version;
if (wixVersion !== pkgJson.version) {
  console.error(
    `WIX VERSION MISMATCH: bundle.windows.wix.version (${wixVersion}) != package.json (${pkgJson.version})`,
  );
  errors++;
}
const nsisHooksRel = tauriConf.bundle?.windows?.nsis?.installerHooks;
const nsisHooksPath = nsisHooksRel ? path.join(ROOT, 'src-tauri', nsisHooksRel) : null;
if (!nsisHooksRel || !nsisHooksPath || !fs.existsSync(nsisHooksPath)) {
  console.error(
    `NSIS HOOKS MISSING: bundle.windows.nsis.installerHooks (${nsisHooksRel ?? 'unset'}) must point to an existing .nsh file in src-tauri/`,
  );
  errors++;
} else {
  const nshContent = fs.readFileSync(nsisHooksPath, 'utf8');
  const displayVerHookRe = new RegExp(`WriteRegStr\\s+SHCTX\\s+"\\$\\{UNINSTKEY\\}"\\s+"DisplayVersion"\\s+"${pkgJson.version.replace(/\./g, '\\.')}"`);
  if (!displayVerHookRe.test(nshContent)) {
    console.error(
      `NSIS DISPLAYVERSION MISMATCH: ${path.relative(ROOT, nsisHooksPath)} must write DisplayVersion "${pkgJson.version}" in NSIS_HOOK_POSTINSTALL`,
    );
    errors++;
  }
}
if (!pkgJson.scripts?.['tauri:build'] || !fs.existsSync(path.join(ROOT, 'scripts', 'build-installer.mjs'))) {
  console.error('INSTALLER SCRIPT MISSING: package.json must define "tauri:build" and scripts/build-installer.mjs must exist');
  errors++;
}
console.log(`Version parity passed: UI=${pkgJson.version}, WiX=${wixVersion}, NSIS=${nsisHooksRel}, Bundler=${tauriConf.version}.`);

// =========================================================================
// 6. AppSettings Rust <-> TypeScript Field Parity Check
// =========================================================================
console.log('\n=== 6. AppSettings Rust <-> TypeScript Parity Check ===');
const settingsRs = fs.readFileSync(path.join(RUST_SRC, 'app', 'settings.rs'), 'utf8');
const settingsTs = fs.readFileSync(path.join(TS_SRC, 'types', 'settings.ts'), 'utf8');
const rustStructMatch = settingsRs.match(/pub struct AppSettings\s*\{([\s\S]*?)\n\}/);
const tsInterfaceMatch = settingsTs.match(/export interface AppSettings\s*\{([\s\S]*?)\n\}/);

if (!rustStructMatch || !tsInterfaceMatch) {
  console.error('Could not locate AppSettings in settings.rs or settings.ts');
  errors++;
} else {
  const rustFields = new Set();
  for (const line of rustStructMatch[1].split(/\r?\n/)) {
    const fm = line.match(/^\s*pub\s+([a-z0-9_]+)\s*:/);
    if (fm) rustFields.add(snakeToCamel(fm[1]));
  }
  const tsFields = new Set();
  for (const line of tsInterfaceMatch[1].split(/\r?\n/)) {
    const fm = line.match(/^\s*([a-zA-Z0-9_]+)\??\s*:/);
    if (fm) tsFields.add(fm[1]);
  }
  for (const rf of rustFields) {
    if (!tsFields.has(rf)) {
      console.error(`SETTINGS PARITY: Rust AppSettings field "${rf}" is missing in src/types/settings.ts`);
      errors++;
    }
  }
  for (const tf of tsFields) {
    if (!rustFields.has(tf)) {
      console.error(`SETTINGS PARITY: TS AppSettings field "${tf}" is missing in src-tauri/src/app/settings.rs`);
      errors++;
    }
  }
  console.log(`Checked ${rustFields.size} Rust AppSettings fields against ${tsFields.size} TS AppSettings fields.`);
}

// =========================================================================
// 7. *_if_enabled Processor Hook Check
// =========================================================================
console.log('\n=== 7. *_if_enabled Processor Hook Check ===');
const enhancedFiles = walk(path.join(RUST_SRC, 'enhanced'), ['.rs']);
const pipelineCorpus = [
  fs.readFileSync(path.join(RUST_SRC, 'decode', 'image_loader.rs'), 'utf8'),
  ...walk(path.join(RUST_SRC, 'render'), ['.rs']).map((f) => fs.readFileSync(f, 'utf8')),
  ...walk(path.join(RUST_SRC, 'export'), ['.rs']).map((f) => fs.readFileSync(f, 'utf8')),
].join('\n');

let hookCount = 0;
for (const file of enhancedFiles) {
  const content = fs.readFileSync(file, 'utf8');
  const fnRegex = /pub fn (apply_[a-zA-Z0-9_]+_if_enabled)\b/g;
  while ((m = fnRegex.exec(content)) !== null) {
    hookCount++;
    const fnName = m[1];
    if (!pipelineCorpus.includes(fnName)) {
      console.error(`ORPHANED PROCESSOR: ${fnName} in ${path.relative(ROOT, file)} is not called in decode/render/export!`);
      errors++;
    }
  }
}
console.log(`Checked ${hookCount} *_if_enabled processors.`);

// =========================================================================
// 8. Adjustment Key Parity (cache_utils & enhanced <-> defaultAdjustments.json)
// =========================================================================
console.log('\n=== 8. Adjustment Key Parity Check ===');
const defaultAdj = new Set(Object.keys(JSON.parse(fs.readFileSync(path.join(TS_SRC, 'utils', 'defaultAdjustments.json'), 'utf8'))));
const cacheUtilsRs = fs.readFileSync(path.join(RUST_SRC, 'render', 'cache_utils.rs'), 'utf8');
const hashKeyBlockRegex = /hash_effect_keys\([\s\S]*?&\[([\s\S]*?)\]/g;
let adjKeysChecked = 0;
while ((m = hashKeyBlockRegex.exec(cacheUtilsRs)) !== null) {
  for (const km of m[1].matchAll(/"([a-zA-Z0-9_]+)"/g)) {
    adjKeysChecked++;
    if (!defaultAdj.has(km[1])) {
      console.error(`CACHE KEY PARITY: "${km[1]}" hashed in cache_utils.rs is missing from defaultAdjustments.json`);
      errors++;
    }
  }
}
for (const file of enhancedFiles) {
  const content = fs.readFileSync(file, 'utf8');
  for (const km of content.matchAll(/\badjustments\[["']([a-zA-Z0-9_]+)["']\]/g)) {
    adjKeysChecked++;
    if (!defaultAdj.has(km[1])) {
      console.error(
        `ENHANCED ADJ KEY PARITY: adjustments["${km[1]}"] in ${path.relative(ROOT, file)}:${lineNumberAt(content, km.index)} is missing from defaultAdjustments.json`,
      );
      errors++;
    }
  }
}
console.log(`Checked ${adjKeysChecked} adjustment key references against defaultAdjustments.json.`);

// =========================================================================
// 9. Ban Empty .catch(() => {}) on invoke() Calls
// =========================================================================
console.log('\n=== 9. Empty invoke().catch(() => {}) Guard ===');
const emptyCatchRegex = /\binvoke(?:<[^>]+>)?\([\s\S]{0,220}?\.catch\(\s*\(\s*\)\s*=>\s*\{\s*\}\s*\)/g;
let emptyCatchCount = 0;
for (const file of tsFiles) {
  const content = fs.readFileSync(file, 'utf8');
  while ((m = emptyCatchRegex.exec(content)) !== null) {
    emptyCatchCount++;
    console.error(
      `SILENT ERROR SWALLOW: Empty .catch(() => {}) on invoke() at ${path.relative(ROOT, file)}:${lineNumberAt(content, m.index)}`,
    );
    errors++;
  }
}
console.log(`Found ${emptyCatchCount} empty .catch(() => {}) blocks on invoke() calls.`);

// =========================================================================
// 10. CopyPasteSettings & Preset Key Parity Check
// =========================================================================
console.log('\n=== 10. CopyPasteSettings & Preset Key Parity Check ===');
const allAvailMatch = settingsRs.match(/pub fn all_available_adjustments\(\)[\s\S]*?\[([\s\S]*?)\]/);
const adjustmentsTs = fs.readFileSync(path.join(TS_SRC, 'utils', 'adjustments.ts'), 'utf8');
const adjGroupsMatch = adjustmentsTs.match(/export const ADJUSTMENT_GROUPS[\s\S]*?=\s*\{([\s\S]*?)\n\};/);

if (!allAvailMatch || !adjGroupsMatch) {
  console.error('Could not parse all_available_adjustments() or ADJUSTMENT_GROUPS');
  errors++;
} else {
  const rustCopyKeys = new Set([...allAvailMatch[1].matchAll(/"([a-zA-Z0-9_\u00C0-\u024F]+)"/g)].map((x) => x[1]));
  // Resolve enum member references like BasicAdjustment.Exposure -> 'exposure'
  const enumValMap = new Map();
  for (const em of adjustmentsTs.matchAll(/(?:export\s+)?enum\s+(\w+)\s*\{([\s\S]*?)\}/g)) {
    const eName = em[1];
    for (const kv of em[2].matchAll(/([A-Za-z0-9_\u00C0-\u024F]+)\s*=\s*'([^']+)'/g)) {
      enumValMap.set(`${eName}.${kv[1]}`, kv[2]);
    }
  }
  const tsGroupKeys = new Set();
  const groupText = adjGroupsMatch[1];
  for (const km of groupText.matchAll(/\bkeys:\s*\[([\s\S]*?)\]/g)) {
    const arr = km[1];
    for (const strM of arr.matchAll(/'([^']+)'/g)) tsGroupKeys.add(strM[1]);
    for (const enumM of arr.matchAll(/([A-Z]\w+\.[A-Za-z0-9_\u00C0-\u024F]+)/g)) {
      if (enumValMap.has(enumM[1])) tsGroupKeys.add(enumValMap.get(enumM[1]));
    }
    if (arr.includes('MONOCHROME_ADJUSTMENT_KEYS')) {
      for (const mk of [
        'monochromeEnabled',
        'monochromeFilm',
        'monochromeFilterHue',
        'monochromeFilterStrength',
        'monochromeMix',
        'monochromeToning',
      ]) {
        tsGroupKeys.add(mk);
      }
    }
  }

  for (const rk of rustCopyKeys) {
    if (!defaultAdj.has(rk)) {
      console.error(`COPY/PASTE PARITY: Rust key "${rk}" in all_available_adjustments() missing from defaultAdjustments.json`);
      errors++;
    }
    if (!tsGroupKeys.has(rk)) {
      console.error(`COPY/PASTE PARITY: Rust key "${rk}" in all_available_adjustments() missing from ADJUSTMENT_GROUPS in adjustments.ts`);
      errors++;
    }
  }
  for (const tk of tsGroupKeys) {
    if (!rustCopyKeys.has(tk)) {
      console.error(`COPY/PASTE PARITY: TS key "${tk}" in ADJUSTMENT_GROUPS missing from all_available_adjustments() in settings.rs`);
      errors++;
    }
  }
  console.log(`Checked ${rustCopyKeys.size} Rust Copy/Paste keys against ${tsGroupKeys.size} TS ADJUSTMENT_GROUPS keys.`);
}

// =========================================================================
// 11. activeTools Uniform Scope Parity Check
// =========================================================================
console.log('\n=== 11. activeTools Uniform Scope Parity Check ===');
const uniformsRs = fs.readFileSync(path.join(RUST_SRC, 'render', 'uniforms.rs'), 'utf8');
const sectionToolsMatch = adjustmentsTs.match(/export const ADJUSTMENT_SECTION_TOOLS[\s\S]*?=\s*\{([\s\S]*?)\n\};/);
if (!sectionToolsMatch) {
  console.error('Could not locate ADJUSTMENT_SECTION_TOOLS in adjustments.ts');
  errors++;
} else {
  const validTools = new Set(['basic', 'color', 'details', 'effects', 'curves', 'toneMapper', 'hsl', 'colorCalibration', 'monochrome', 'lensCorrection']);
  for (const tm of sectionToolsMatch[1].matchAll(/\bid:\s*'([^']+)'/g)) validTools.add(tm[1]);
  for (const sm of sectionToolsMatch[1].matchAll(/\bsubTools:\s*\[([^\]]+)\]/g)) {
    for (const s of sm[1].matchAll(/'([^']+)'/g)) validTools.add(s[1]);
  }
  const checkedScopes = new Set();
  for (const um of uniformsRs.matchAll(/\b(?:get_val|get_scoped_val|is_visible)\(\s*"([a-zA-Z0-9_]+)"/g)) {
    const scope = um[1];
    checkedScopes.add(scope);
    if (!validTools.has(scope)) {
      console.error(
        `UNIFORM TOOL SCOPE MISMATCH: "${scope}" in uniforms.rs:${lineNumberAt(uniformsRs, um.index)} is not in ADJUSTMENT_SECTION_TOOLS`,
      );
      errors++;
    }
  }
  console.log(`Checked ${checkedScopes.size} uniform tool scopes against ADJUSTMENT_SECTION_TOOLS.`);
}

// =========================================================================
// 12. Tauri Command Blocking-I/O Ratchet (Sync + Async Outside spawn_blocking)
// =========================================================================
console.log('\n=== 12. Tauri Command Blocking-I/O Ratchet ===');
// Shrinking legacy allowlist of pre-existing untouched commands. Never add new commands here.
const LEGACY_BLOCKING_IO_COMMANDS = new Set([
  'analyze_and_polish_active_image',
  'analyze_image_noise_profile',
  'analyze_night_sky_active_image',
  'apply_denoising',
  'auto_detect_negative_border_mask',
  'batch_auto_triage_cull',
  'calculate_auto_adjustments',
  'estimate_export_sizes',
  'get_photographic_critic_report',
  'preview_denoised_roi',
  'preview_negative_conversion',
  'record_auto_edit_rejection',
  'record_dust_spot_rejection',
  'sample_negative_border_mask',
  'save_denoised_image',
  'set_flag_for_paths',
]);

const HARDENED_NO_BLOCKING_IO = new Set([
  'validate_hdr_brackets',
  'detect_panorama_sequences',
  'inspect_panorama_brackets',
  'inspect_hdr_pano_grouping',
  'auto_remove_dust_and_blemishes',
]);

for (const h of HARDENED_NO_BLOCKING_IO) {
  if (LEGACY_BLOCKING_IO_COMMANDS.has(h)) {
    console.error(`CHECK 12 CONFIG ERROR: Hardened command "${h}" must never be in LEGACY_BLOCKING_IO_COMMANDS`);
    errors++;
  }
}

const tauriCmdBodyRe =
  /#\[tauri::command(?:\([^\)]*\))?\]\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+([a-zA-Z0-9_]+)\s*(?:<[^>]*>)?\s*\([\s\S]*?\)\s*(?:->[\s\S]*?)?\{/g;

function stripSpawnBlockingBodies(body) {
  const sbRe = /spawn_blocking\s*\(\s*(?:move\s*)?\|\|\s*\{/g;
  let clean = '';
  let pos = 0;
  let sm;
  while ((sm = sbRe.exec(body)) !== null) {
    clean += body.slice(pos, sm.index);
    let j = sm.index + sm[0].length;
    let depth = 1;
    while (j < body.length && depth > 0) {
      if (body[j] === '{') depth++;
      else if (body[j] === '}') depth--;
      j++;
    }
    pos = j;
  }
  clean += body.slice(pos);
  return clean;
}

let blockingCmdChecked = 0;
let blockingLegacySeen = 0;
for (const file of rustFiles) {
  const content = fs.readFileSync(file, 'utf8');
  tauriCmdBodyRe.lastIndex = 0;
  let cm;
  while ((cm = tauriCmdBodyRe.exec(content)) !== null) {
    blockingCmdChecked++;
    const fnName = cm[1];
    let i = cm.index + cm[0].length;
    let depth = 1;
    while (i < content.length && depth > 0) {
      if (content[i] === '{') depth++;
      else if (content[i] === '}') depth--;
      i++;
    }
    const body = content.slice(cm.index + cm[0].length, i - 1);
    const outsideBlocking = stripSpawnBlockingBodies(body);
    if (/\b(?:std::)?fs::read\s*\(|\bimage::open\s*\(/.test(outsideBlocking)) {
      if (LEGACY_BLOCKING_IO_COMMANDS.has(fnName)) {
        blockingLegacySeen++;
      } else {
        console.error(
          `BLOCKING I/O IN TAURI COMMAND: "${fnName}" at ${path.relative(ROOT, file)}:${lineNumberAt(content, cm.index)} calls fs::read/image::open outside tokio::task::spawn_blocking`,
        );
        errors++;
      }
    }
  }
}
console.log(
  `Scanned ${blockingCmdChecked} #[tauri::command] bodies (${blockingLegacySeen} grandfathered legacy commands, 0 new regressions).`,
);

// =========================================================================
// 13. TypeScript AST Guard (Panel/Event Race, Dependency-less Listeners, Mask Isolation)
// =========================================================================
console.log('\n=== 13. TypeScript Compiler AST Guard ===');
let astFilesChecked = 0;
for (const file of tsFiles) {
  astFilesChecked++;
  const relPath = path.relative(ROOT, file).replace(/\\/g, '/');
  const content = fs.readFileSync(file, 'utf8');
  const scriptKind = file.endsWith('.tsx') ? ts.ScriptKind.TSX : ts.ScriptKind.TS;
  const sf = ts.createSourceFile(file, content, ts.ScriptTarget.Latest, true, scriptKind);

  function visitNode(node) {
    // 13a. Ban calling setPanel(...) and window.dispatchEvent(...) in the same block/function body
    if (ts.isBlock(node)) {
      let hasSetPanel = false;
      let hasWindowDispatch = false;
      for (const stmt of node.statements) {
        const text = stmt.getText(sf);
        if (/\bsetPanel\s*\(/.test(text)) hasSetPanel = true;
        if (/\bwindow\.dispatchEvent\s*\(/.test(text)) hasWindowDispatch = true;
      }
      if (hasSetPanel && hasWindowDispatch) {
        const line = sf.getLineAndCharacterOfPosition(node.getStart(sf)).line + 1;
        console.error(
          `PANEL/EVENT MOUNT RACE: ${relPath}:${line} calls both setPanel(...) and window.dispatchEvent(...) in the same block — use useUIStore.triggerQuickAction() instead`,
        );
        errors++;
      }
    }

    // 13b. Ban useEffect(() => { ...addEventListener... }) without a dependency array
    // 13c. Require isForMask guard on window.addEventListener inside Basic.tsx
    if (
      ts.isCallExpression(node) &&
      ts.isIdentifier(node.expression) &&
      node.expression.text === 'useEffect'
    ) {
      const cb = node.arguments[0];
      if (cb) {
        const cbText = cb.getText(sf);
        if (/\baddEventListener\s*\(/.test(cbText) && node.arguments.length < 2) {
          const line = sf.getLineAndCharacterOfPosition(node.getStart(sf)).line + 1;
          console.error(
            `DEPENDENCY-LESS LISTENER EFFECT: ${relPath}:${line} registers addEventListener inside useEffect without a dependency array`,
          );
          errors++;
        }
        if (relPath.endsWith('components/adjustments/Basic.tsx') && /\bwindow\.addEventListener\s*\(/.test(cbText)) {
          if (!/\bisForMask\b/.test(cbText)) {
            const line = sf.getLineAndCharacterOfPosition(node.getStart(sf)).line + 1;
            console.error(
              `MASK ISOLATION VIOLATION: ${relPath}:${line} registers window.addEventListener without guarding isForMask`,
            );
            errors++;
          }
        }
      }
    }

    ts.forEachChild(node, visitNode);
  }
  visitNode(sf);
}
console.log(`Checked ${astFilesChecked} TypeScript/TSX ASTs for mount races, listener leaks, and mask isolation.`);

// =========================================================================
// 14. Modal Cancel Domain Guard
// =========================================================================
console.log('\n=== 14. Modal Cancel Domain Guard ===');
const mergeModalTsx = fs.readFileSync(path.join(TS_SRC, 'components', 'modals', 'MergeModal.tsx'), 'utf8');
if (mergeModalTsx.includes('Invokes.CancelAiDenoise')) {
  console.error('MODAL CANCEL DOMAIN MISMATCH: MergeModal.tsx must not call Invokes.CancelAiDenoise');
  errors++;
}
if (!mergeModalTsx.includes('Invokes.DiscardMerge')) {
  console.error('MODAL CANCEL DOMAIN MISMATCH: MergeModal.tsx must call Invokes.DiscardMerge to cancel merge sessions');
  errors++;
}
console.log('Modal cancel domain guard passed.');

// =========================================================================
// 15. Keybind Shadowing Guard
// =========================================================================
console.log('\n=== 15. Keybind Shadowing Guard ===');
const keyboardHookTs = fs.readFileSync(path.join(TS_SRC, 'hooks', 'useKeyboardShortcuts.ts'), 'utf8');
const builtinStart = keyboardHookTs.indexOf('const builtinShortcuts = [');
const builtinEnd = keyboardHookTs.indexOf('const handleKeyDown =', builtinStart);
if (builtinStart === -1 || builtinEnd === -1) {
  console.error('KEYBIND GUARD ERROR: Could not locate builtinShortcuts block in useKeyboardShortcuts.ts');
  errors++;
} else {
  const builtinBlock = keyboardHookTs.slice(builtinStart, builtinEnd);
  const bareLetterRe = /e\.code\s*===\s*['"]Key[A-Z]['"]/g;
  let bm;
  while ((bm = bareLetterRe.exec(builtinBlock)) !== null) {
    console.error(
      `HARDCODED KEYBIND SHADOWING: builtinShortcuts in useKeyboardShortcuts.ts:${lineNumberAt(keyboardHookTs, builtinStart + bm.index)} matches ${bm[0]} — move it into KEYBIND_DEFINITIONS so users can rebind it`,
    );
    errors++;
  }
}
console.log('Keybind shadowing guard passed.');

// =========================================================================
// 16. Merge Exposure & Color Parity Guard
// =========================================================================
console.log('\n=== 16. Merge Exposure & Color Parity Guard ===');
const mergeRustFiles = walk(path.join(RUST_SRC, 'merge'), ['.rs']);
for (const file of mergeRustFiles) {
  const content = fs.readFileSync(file, 'utf8');
  const mergeOutputRe = /MergeOutput\s*\{([\s\S]*?)\}/g;
  let mom;
  while ((mom = mergeOutputRe.exec(content)) !== null) {
    if (/scene_exposure\s*:\s*0(?:\.0+)?\s*,/.test(mom[1])) {
      console.error(
        `HARDCODED ZERO EXPOSURE: MergeOutput literal at ${path.relative(ROOT, file)}:${lineNumberAt(content, mom.index)} hardcodes scene_exposure: 0.0 instead of self.rendition.scene_exposure()`,
      );
      errors++;
    }
  }
}
const outputRs = fs.readFileSync(path.join(RUST_SRC, 'merge', 'output.rs'), 'utf8');
const imgProcRs = fs.readFileSync(path.join(RUST_SRC, 'render', 'image_processing.rs'), 'utf8');
const toSrgb16Match = outputRs.match(/pub fn to_srgb16\b[\s\S]*?\n    \}/);
const workingDispMatch = imgProcRs.match(/pub fn working_display_to_srgb8\b[\s\S]*?\n\}/);
if (!toSrgb16Match || !toSrgb16Match[0].includes('OutputTransform::srgb(false)')) {
  console.error('COLOR PARITY VIOLATION: output.rs to_srgb16 must use OutputTransform::srgb(false)');
  errors++;
}
if (!workingDispMatch || !workingDispMatch[0].includes('OutputTransform::srgb(false)')) {
  console.error('COLOR PARITY VIOLATION: image_processing.rs working_display_to_srgb8 must use OutputTransform::srgb(false)');
  errors++;
}
console.log('Merge exposure and color parity guard passed.');

// =========================================================================
// 17. Node DEP0190 Child-Process Guard (Ban shell: true in scripts/*.mjs)
// =========================================================================
console.log('\n=== 17. Node DEP0190 Child-Process Guard ===');
const scriptFiles = walk(path.join(ROOT, 'scripts'), ['.mjs']);
for (const file of scriptFiles) {
  const content = fs.readFileSync(file, 'utf8');
  const spawnRe = /\b(?:spawn|spawnSync)\s*\([\s\S]{0,400}?\bshell\s*:\s*true\b/g;
  let spm;
  while ((spm = spawnRe.exec(content)) !== null) {
    console.error(
      `DEP0190 RISK: ${path.relative(ROOT, file)}:${lineNumberAt(content, spm.index)} passes shell: true to spawn/spawnSync`,
    );
    errors++;
  }
}
console.log(`Checked ${scriptFiles.length} script(s) in scripts/*.mjs for DEP0190 safety.`);

// =========================================================================
// 18. IPC Return-Struct Field Parity (Rust #[derive(Serialize)] <-> TS)
// =========================================================================
console.log('\n=== 18. IPC Return-Struct Field Parity ===');
const IPC_RETURN_PAIRS = [
  { rs: 'merge/output.rs', rsStruct: 'MergePreview', ts: 'types/library.ts', tsIface: 'MergePreview' },
  { rs: 'enhanced/hdr_panorama.rs', rsStruct: 'HdrPanoGroupingReport', ts: 'enhanced/modals/HdrPanoPreflightModal.tsx', tsIface: 'HdrPanoGroupingReport' },
  { rs: 'enhanced/hdr_panorama.rs', rsStruct: 'ExposureBracketGroup', ts: 'enhanced/modals/HdrPanoPreflightModal.tsx', tsIface: 'ExposureBracketGroup' },
  { rs: 'enhanced/speed_culler.rs', rsStruct: 'FaceLoupeCrop', ts: 'enhanced/modals/SpeedCullerModal.tsx', tsIface: 'FaceLoupeCrop' },
  { rs: 'enhanced/speed_culler.rs', rsStruct: 'CullingFrameAnalysis', ts: 'enhanced/modals/SpeedCullerModal.tsx', tsIface: 'CullingFrameAnalysis' },
  { rs: 'enhanced/speed_culler.rs', rsStruct: 'BurstGroup', ts: 'enhanced/modals/SpeedCullerModal.tsx', tsIface: 'BurstGroup' },
  { rs: 'enhanced/defect_repair.rs', rsStruct: 'DetectedSpot', ts: 'enhanced/panels/DustScratchProSection.tsx', tsIface: 'DetectedSpot' },
  { rs: 'enhanced/defect_repair.rs', rsStruct: 'AutoDefectRepairResult', ts: 'enhanced/panels/DustScratchProSection.tsx', tsIface: 'AutoDefectRepairResult' },
  { rs: 'enhanced/defect_repair.rs', rsStruct: 'SensorDustMapSpot', ts: 'enhanced/panels/DustScratchProSection.tsx', tsIface: 'SensorDustMapSpot' },
  { rs: 'enhanced/defect_repair.rs', rsStruct: 'ShootDustMapReport', ts: 'enhanced/panels/DustScratchProSection.tsx', tsIface: 'ShootDustMapReport' },
  { rs: 'enhanced/astro_stacking.rs', rsStruct: 'G2vCalibrationResult', ts: 'enhanced/types/astro.ts', tsIface: 'G2vCalibrationResult' },
  { rs: 'enhanced/astro_starless.rs', rsStruct: 'StarSeparationExportResult', ts: 'enhanced/types/astro.ts', tsIface: 'StarSeparationExportResult' },
  { rs: 'decode/camera_profile.rs', rsStruct: 'CameraProfileInfo', ts: 'enhanced/types/color.ts', tsIface: 'CameraProfileInfo' },
  { rs: 'enhanced/denoising.rs', rsStruct: 'DenoiseRoiPreview', ts: 'enhanced/types/denoise.ts', tsIface: 'DenoiseRoiPreview' },
  { rs: 'masks/face.rs', rsStruct: 'SkinToneMatchResult', ts: 'types/invokes.ts', tsIface: 'SkinToneMatchResult' },
];

for (const pair of IPC_RETURN_PAIRS) {
  const rsContent = fs.readFileSync(path.join(RUST_SRC, pair.rs), 'utf8');
  const tsContent = fs.readFileSync(path.join(TS_SRC, pair.ts), 'utf8');

  const rsRe = new RegExp(`pub\\s+struct\\s+${pair.rsStruct}\\s*\\{([\\s\\S]*?)\\n\\}`);
  const rsMatch = rsRe.exec(rsContent);
  const tsRe = new RegExp(`(?:export\\s+)?interface\\s+${pair.tsIface}\\s*\\{([\\s\\S]*?)\\n\\}`);
  const tsMatch = tsRe.exec(tsContent);

  if (!rsMatch) {
    console.error(`IPC RETURN PARITY: Could not locate Rust struct ${pair.rsStruct} in ${pair.rs}`);
    errors++;
    continue;
  }
  if (!tsMatch) {
    console.error(`IPC RETURN PARITY: Could not locate TS interface ${pair.tsIface} in ${pair.ts}`);
    errors++;
    continue;
  }

  const prevBlockEnd = rsContent.lastIndexOf('\n}', rsMatch.index);
  const attrHeader = rsContent.slice(prevBlockEnd === -1 ? 0 : prevBlockEnd + 2, rsMatch.index);
  const usesCamelCase = /rename_all\s*=\s*"camelCase"/.test(attrHeader);

  const rsFields = new Set();
  const rsLines = rsMatch[1].split(/\r?\n/);
  let prevNonEmpty = '';
  for (const line of rsLines) {
    const fm = line.match(/^\s*pub\s+([a-z0-9_]+)\s*:/);
    if (fm) {
      if (!prevNonEmpty.includes('#[serde(skip')) {
        rsFields.add(usesCamelCase ? snakeToCamel(fm[1]) : fm[1]);
      }
    }
    const trimmed = line.trim();
    if (trimmed) prevNonEmpty = trimmed;
  }

  const tsFields = new Set();
  for (const line of tsMatch[1].split(/\r?\n/)) {
    const fm = line.match(/^\s*([A-Za-z0-9_]+)\??\s*:/);
    if (fm) tsFields.add(fm[1]);
  }

  for (const tf of tsFields) {
    if (!rsFields.has(tf)) {
      console.error(
        `IPC RETURN PARITY: TS ${pair.tsIface}.${tf} (${pair.ts}) is not serialized by Rust ${pair.rsStruct} (${pair.rs})`,
      );
      errors++;
    }
  }
  const extraRust = [...rsFields].filter((rf) => !tsFields.has(rf));
  if (extraRust.length > 0) {
    console.log(`  [info] Rust ${pair.rsStruct} has extra serialized field(s) not typed in TS ${pair.tsIface}: ${extraRust.join(', ')}`);
  }
}
console.log(`Checked ${IPC_RETURN_PAIRS.length} IPC return-struct pairs.`);

// =========================================================================
// 19. Adjustment-Key Synchronization Check (defaultAdjustments <-> TS <-> Rust)
// =========================================================================
console.log('\n=== 19. Adjustment-Key Synchronization Check ===');
const NON_COPYABLE_ADJUSTMENT_KEYS = new Set([
  'activeTools',
  'aiPatches',
  'baselineExposure',
  'lensBlurDepthMap',
  'relightNormalMap',
  'fogDepthMap',
  'motionBlurDepthMap',
  'lensDistortionParams',
  'showClipping',
]);
const enhancedTs = fs.readFileSync(path.join(TS_SRC, 'types', 'enhanced.ts'), 'utf8');
const adjIfaceMatch = adjustmentsTs.match(/export interface Adjustments[^{]*\{([\s\S]*?)\r?\n\}/);
const enhIfaceMatch = enhancedTs.match(/export interface EnhancedAdjustments[^{]*\{([\s\S]*?)\r?\n\}/);
const tsIfaceKeys = new Set();
for (const body of [adjIfaceMatch ? adjIfaceMatch[1] : '', enhIfaceMatch ? enhIfaceMatch[1] : '']) {
  for (const km of body.matchAll(/^\s*([A-Za-z0-9_\u00C0-\u024F]+)\??\s*:/gm)) {
    tsIfaceKeys.add(km[1]);
  }
}
const rustAvailKeys = allAvailMatch
  ? new Set([...allAvailMatch[1].matchAll(/"([a-zA-Z0-9_\u00C0-\u024F]+)"/g)].map((x) => x[1]))
  : new Set();

if (tsIfaceKeys.size === 0 || rustAvailKeys.size === 0) {
  console.error('ADJUSTMENT SYNC: Could not parse TS Adjustments/EnhancedAdjustments or Rust all_available_adjustments()');
  errors++;
} else {
  for (const k of defaultAdj) {
    if (!tsIfaceKeys.has(k)) {
      console.error(
        `ADJUSTMENT SYNC: "${k}" in defaultAdjustments.json is missing from TS Adjustments / EnhancedAdjustments interface`,
      );
      errors++;
    }
    if (!NON_COPYABLE_ADJUSTMENT_KEYS.has(k) && !rustAvailKeys.has(k)) {
      console.error(
        `ADJUSTMENT SYNC: "${k}" in defaultAdjustments.json is missing from all_available_adjustments() in settings.rs (or NON_COPYABLE_ADJUSTMENT_KEYS)`,
      );
      errors++;
    }
  }
  console.log(
    `Checked ${defaultAdj.size} defaultAdjustments.json keys against TS interfaces (${tsIfaceKeys.size}) and Rust all_available_adjustments() (${rustAvailKeys.size}).`,
  );
}

if (errors > 0) {
  console.error(`\nContract verification FAILED with ${errors} error(s).`);
  process.exit(1);
}
console.log('\nAll 19 contract checks PASSED.');
