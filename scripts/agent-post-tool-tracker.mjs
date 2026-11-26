import fs from 'node:fs';
import path from 'node:path';

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

const root = resolveRoot(payload);
const cacheDir = path.join(root, '.agents', '.cache');
try {
  fs.mkdirSync(cacheDir, { recursive: true });
} catch {
  process.stdout.write('{}');
  process.exit(0);
}

const logPath = path.join(cacheDir, `.agy-dirty-${safeId(payload.conversationId)}.jsonl`);
const args = payload?.toolCall?.args || {};
const toolName = String(payload?.toolCall?.name || payload?.toolName || '');
const toolError = payload?.error;
const resultText = typeof payload?.toolResult === 'string'
  ? payload.toolResult
  : JSON.stringify(payload?.toolResult ?? payload?.result ?? '');
const now = Date.now();

// Track only actual file write/edit calls (never read_file/view_file) regardless of internal tool-name aliasing.
const touchedPath = args.Path || args.TargetFile || args.path || args.filePath;
const hasWritePayload =
  args.Replacement !== undefined ||
  args.Content !== undefined ||
  args.CodeContent !== undefined ||
  args.ReplacementContent !== undefined ||
  args.ReplacementChunks !== undefined ||
  /(?:edit|write|replace|patch)/i.test(toolName);

if (typeof touchedPath === 'string' && hasWritePayload && !toolError) {
  const normalized = touchedPath.replace(/\\/g, '/').toLowerCase();
  const rel = path.relative(root, touchedPath).replace(/\\/g, '/').toLowerCase();
  const isInsideWorkspace = !rel.startsWith('..') && !path.isAbsolute(rel);

  if (isInsideWorkspace) {
    let kind = null;
    if (normalized.endsWith('.rs')) {
      kind = 'rs';
    } else if (normalized.endsWith('.ts') || normalized.endsWith('.tsx')) {
      kind = 'ts';
    } else if (
      (rel.startsWith('scripts/') && normalized.endsWith('.mjs')) ||
      rel === 'src-tauri/tauri.conf.json' ||
      rel === 'src-tauri/nsis-hooks.nsh' ||
      rel === 'src-tauri/cargo.toml' ||
      rel === 'package.json'
    ) {
      kind = 'script';
    }

    if (kind) {
      try {
        fs.appendFileSync(logPath, `${JSON.stringify({ k: kind, p: rel, t: now })}\n`, 'utf8');
      } catch {
        // Ignore transient write errors
      }
    }
  }
}

// Track passing Rust check/test commands so Stop hook verifies Rust proof without re-running slow cargo builds.
const cmdLine = args.CommandLine || args.command;
const cmdFailed = /Command exited with code [1-9]|error\[E\d+\]|test result: FAILED/i.test(resultText);
if (typeof cmdLine === 'string' && !toolError && !cmdFailed) {
  if (/\bcargo\s+(check|test)\b/i.test(cmdLine) || /\btauri:build\b/i.test(cmdLine)) {
    try {
      fs.appendFileSync(logPath, `${JSON.stringify({ k: 'rust_ok', t: now })}\n`, 'utf8');
    } catch {
      // Ignore transient write errors
    }
  }
}

process.stdout.write('{}');
