import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT = path.resolve(__dirname, '..');

const pkgJson = JSON.parse(fs.readFileSync(path.join(ROOT, 'package.json'), 'utf8'));
const tauriConf = JSON.parse(fs.readFileSync(path.join(ROOT, 'src-tauri', 'tauri.conf.json'), 'utf8'));

const displayVersion = pkgJson.version;
const bundlerVersion = tauriConf.version;

function runNodeTool(label, scriptPath, extraArgs = []) {
  console.log(`[build-installer] ${label}...`);
  const res = spawnSync(process.execPath, [scriptPath, ...extraArgs], {
    cwd: ROOT,
    stdio: 'inherit',
    shell: false,
  });
  if (res.status !== 0) {
    console.error(`[build-installer] ${label} failed with exit code ${res.status ?? 1}`);
    process.exit(res.status ?? 1);
  }
}

// 1. Pre-build safety gates: refuse to package installers if contracts or TypeScript fail.
runNodeTool('Running contract & regression verification', path.join(ROOT, 'scripts', 'verify-contracts.mjs'));
runNodeTool(
  'Running TypeScript typecheck',
  path.join(ROOT, 'node_modules', 'typescript', 'bin', 'tsc'),
  ['--noEmit'],
);

const targetArgIdx = process.argv.indexOf('--target');
const targetTriple = targetArgIdx !== -1 ? process.argv[targetArgIdx + 1] : null;
const targetBase = targetTriple
  ? path.join(ROOT, 'src-tauri', 'target', targetTriple, 'release', 'bundle')
  : path.join(ROOT, 'src-tauri', 'target', 'release', 'bundle');

const sourcePrefix = `RapidRAW_${bundlerVersion}_`;
const targetPrefix = `RapidRAW_${displayVersion}_`;

// 2. Purge stale versioned installers so we never report pre-existing stale bundles.
for (const sub of ['nsis', 'msi']) {
  const dir = path.join(targetBase, sub);
  if (!fs.existsSync(dir)) continue;
  for (const entry of fs.readdirSync(dir)) {
    if (entry.startsWith(targetPrefix) || entry.startsWith(sourcePrefix)) {
      try {
        fs.unlinkSync(path.join(dir, entry));
      } catch {
        // Ignore if already removed
      }
    }
  }
}

const buildStartMs = Date.now() - 1000;
const tauriCliJs = path.join(ROOT, 'node_modules', '@tauri-apps', 'cli', 'tauri.js');
runNodeTool(
  `Running Tauri release build (display=${displayVersion}, bundler=${bundlerVersion})`,
  tauriCliJs,
  ['build', ...process.argv.slice(2)],
);

const bundleDirs = ['nsis', 'msi']
  .map((sub) => path.join(targetBase, sub))
  .filter((dir) => fs.existsSync(dir));

const produced = [];

for (const dir of bundleDirs) {
  for (const entry of fs.readdirSync(dir)) {
    if (entry.startsWith(targetPrefix)) {
      const targetPath = path.join(dir, entry);
      const stat = fs.statSync(targetPath);
      if (stat.mtimeMs < buildStartMs) continue;
      const sizeMb = (stat.size / (1024 * 1024)).toFixed(2);
      produced.push(`${targetPath} (${sizeMb} MB)`);
      continue;
    }
    if (!entry.startsWith(sourcePrefix)) continue;
    const oldPath = path.join(dir, entry);
    const oldStat = fs.statSync(oldPath);
    if (oldStat.mtimeMs < buildStartMs) continue;
    const newName = `${targetPrefix}${entry.slice(sourcePrefix.length)}`;
    const newPath = path.join(dir, newName);
    if (oldPath !== newPath) {
      if (fs.existsSync(newPath)) {
        fs.unlinkSync(newPath);
      }
      fs.renameSync(oldPath, newPath);
    }
    const sizeMb = (fs.statSync(newPath).size / (1024 * 1024)).toFixed(2);
    produced.push(`${newPath} (${sizeMb} MB)`);
  }
}

if (produced.length === 0) {
  console.error(`[build-installer] No fresh installer files found in ${bundleDirs.join(', ')}`);
  process.exit(1);
}

console.log('[build-installer] Compiled installers:');
for (const line of produced) {
  console.log(`  - ${line}`);
}
