// e2e global setup: mock upstream + embedded-UI server + provider config.
// State is written to a JSON file — globalSetup runs in its own process and
// does NOT share globalThis with test workers.
import { spawn, execSync } from 'node:child_process';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const BASE = 'http://127.0.0.1:30231';

async function waitFor(url: string, tries = 60): Promise<boolean> {
  for (let i = 0; i < tries; i++) {
    try {
      const r = await fetch(url);
      if (r.ok) return true;
    } catch {
      /* not up yet */
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  return false;
}

export default async function globalSetup() {
  const root = process.cwd();
  const dataDir = mkdtempSync(join(tmpdir(), 'mimotts-e2e-'));
  const db = join(dataDir, 'mimo.db');
  const exe = join(root, 'target', 'debug', process.platform === 'win32' ? 'mimotts.exe' : 'mimotts');

  // 1. mock upstream
  const mock = spawn(process.execPath, [join(root, 'e2e-v4', 'mock-mimo.mjs')], {
    stdio: 'ignore',
  });
  const mockOk = await waitFor('http://127.0.0.1:30250/health', 10);
  if (!mockOk) throw new Error('mock-mimo failed to start');

  // 2. API token
  const tokenOut = execSync(`"${exe}" key issue --data-dir "${dataDir}" --db "${db}"`).toString();
  const token = (tokenOut.match(/^([0-9a-f]{64})$/m) || [])[1];
  if (!token) throw new Error(`token not found in output:\n${tokenOut}`);

  // 3. embedded-UI server pointed at the mock upstream
  const server = spawn(
    exe,
    ['serve', '--port', '30231', '--data-dir', dataDir, '--db', db],
    {
      env: { ...process.env, MIMOTTS_BASE_URL: 'http://127.0.0.1:30250' },
      stdio: 'ignore',
    },
  );
  const up = await waitFor(`${BASE}/health`);
  if (!up) throw new Error('mimotts server failed to start');

  // 4. configure provider API key
  const r = await fetch(`${BASE}/api/v3/providers/xiaomi/key`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${token}` },
    body: JSON.stringify({ api_key: 'e2e-test-key' }),
  });
  if (!r.ok) throw new Error(`provider config failed: ${r.status}`);

  writeFileSync(
    join(root, 'e2e-v4', '.state.json'),
    JSON.stringify({ token, dataDir }),
  );
  // Teardown: kill both children.
  return async () => {
    server.kill();
    mock.kill();
  };
}
