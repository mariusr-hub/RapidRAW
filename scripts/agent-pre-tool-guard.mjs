import fs from 'node:fs';

function readStdin() {
  try {
    return fs.readFileSync(0, 'utf8');
  } catch {
    return '';
  }
}

const raw = readStdin().trim();
if (!raw) {
  process.stdout.write(JSON.stringify({ decision: 'allow' }));
  process.exit(0);
}

let payload = {};
try {
  payload = JSON.parse(raw);
} catch {
  process.stdout.write(JSON.stringify({ decision: 'allow' }));
  process.exit(0);
}

const cmd = String(payload?.toolCall?.args?.CommandLine ?? payload?.toolCall?.args?.command ?? '');
const destructivePatterns = [
  /\bgit\s+reset\s+--hard\b/i,
  /\bgit\s+checkout\s+--\s/i,
  /\bgit\s+clean\s+-[a-z]*f/i,
];

for (const pattern of destructivePatterns) {
  if (pattern.test(cmd)) {
    process.stdout.write(
      JSON.stringify({
        decision: 'ask',
        reason: `Destructive git command detected ("${cmd.slice(0, 120)}"). Confirm before wiping workspace changes.`,
      }),
    );
    process.exit(0);
  }
}

process.stdout.write(JSON.stringify({ decision: 'allow' }));
