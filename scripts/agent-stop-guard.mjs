import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

function readStdin() {
  try {
    return fs.readFileSync(0, 'utf8');
  } catch {
    return '';
  }
}

function resolveRoot(payload) {
  const candidates = [
    Array.isArray(payload?.workspacePaths) ? payload.workspacePaths[0] : null,
    payload?.cwd,
    path.resolve(path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1')), '..'),
  ].filter(Boolean);
  for (const c of candidates) {
    if (fs.existsSync(path.join(c, 'package.json'))) return c;
  }
  return candidates[0] || process.cwd();
}

function safeId(rawId) {
  return String(rawId || 'default').replace(/[^a-zA-Z0-9_-]/g, '_').slice(0, 64);
}

const raw = readStdin().trim();
if (!raw) {
  process.stdout.write('{}');
  process.exit(0);
}

let payload = {};
try {
  payload = JSON.parse(raw);
} catch {
  process.stdout.write('{}');
  process.exit(0);
}

// Allow stopping if not a normal idle completion, or if bounded retry cap (>= 2 continuations) is reached.
const terminationReason = payload?.terminationReason || 'NO_TOOL_CALL';
const fullyIdle = payload?.fullyIdle !== false;
const executionNum = Number(payload?.executionNum ?? 0);

if (terminationReason !== 'NO_TOOL_CALL' || !fullyIdle || executionNum >= 2) {
  process.stdout.write('{}');
  process.exit(0);
}

const root = resolveRoot(payload);
const logPath = path.join(root, '.agents', '.cache', `.agy-dirty-${safeId(payload.conversationId)}.jsonl`);
if (!fs.existsSync(logPath)) {
  // No code edits recorded by this agent in this conversation (read-only / Q&A turn).
  process.stdout.write('{}');
  process.exit(0);
}

let latestTs = 0;
let latestRs = 0;
let latestScript = 0;
let latestRustOk = 0;

try {
  const lines = fs.readFileSync(logPath, 'utf8').split(/\r?\n/).filter(Boolean);
  for (const line of lines) {
    try {
      const entry = JSON.parse(line);
      const ts = Number(entry.t || 0);
      if (entry.k === 'ts') latestTs = Math.max(latestTs, ts);
      else if (entry.k === 'rs') latestRs = Math.max(latestRs, ts);
      else if (entry.k === 'script') latestScript = Math.max(latestScript, ts);
      else if (entry.k === 'rust_ok') latestRustOk = Math.max(latestRustOk, ts);
    } catch {
      // Skip malformed line
    }
  }
} catch {
  process.stdout.write('{}');
  process.exit(0);
}

if (latestTs === 0 && latestRs === 0 && latestScript === 0) {
  process.stdout.write('{}');
  process.exit(0);
}

const failures = [];

// 1. Always run fast contract & semantic regression gate (<1s) whenever TS, Rust, or scripts changed.
const contractsScript = path.join(root, 'scripts', 'verify-contracts.mjs');
if (fs.existsSync(contractsScript)) {
  const res = spawnSync(process.execPath, [contractsScript], {
    cwd: root,
    encoding: 'utf8',
    shell: false,
    timeout: 15000,
  });
  if (res.status !== 0) {
    const snippet = `${res.stderr || ''}\n${res.stdout || ''}`.trim().slice(-900);
    failures.push(`verify-contracts.mjs failed:\n${snippet}`);
  }
}

// 2. Run TypeScript typecheck directly via node (no npx/shell) if TS/TSX files changed.
if (latestTs > 0) {
  const tscBin = path.join(root, 'node_modules', 'typescript', 'bin', 'tsc');
  if (fs.existsSync(tscBin)) {
    const res = spawnSync(process.execPath, [tscBin, '--noEmit'], {
      cwd: root,
      encoding: 'utf8',
      shell: false,
      timeout: 60000,
    });
    if (res.status !== 0) {
      const errMsg = res.error ? ` (${res.error.message})` : '';
      const snippet = `${res.stdout || ''}\n${res.stderr || ''}`.trim().slice(-900);
      failures.push(`tsc --noEmit failed${errMsg}:\n${snippet}`);
    }
  }
}

// 3. Enforce Rust verification proof if .rs files were modified after the last passing cargo check/test.
if (latestRs > latestRustOk) {
  failures.push(
    'Rust files (.rs) were edited after the last passing cargo check/test. Run `cargo check --manifest-path src-tauri/Cargo.toml --bins` and `cargo test --manifest-path src-tauri/Cargo.toml --lib -- --skip shaders_compile_on_gpu_backends` (BypassSandbox: true) before finishing.',
  );
}

if (failures.length > 0) {
  process.stdout.write(
    JSON.stringify({
      decision: 'continue',
      reason: `[REGRESSION GUARD BLOCKED STOP]\n${failures.join('\n\n')}`,
    }),
  );
  process.exit(0);
}

// All gates passed — clear the conversation dirty log so subsequent Q&A turns stay instant.
try {
  fs.unlinkSync(logPath);
} catch {
  // Ignore cleanup error
}

process.stdout.write('{}');
