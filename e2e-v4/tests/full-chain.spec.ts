import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

const BASE = 'http://127.0.0.1:30231';
const API = `${BASE}/api/v3`;

const state = JSON.parse(readFileSync(join(process.cwd(), 'e2e-v4', '.state.json'), 'utf-8'));
const token = () => state.token as string;
const authHeaders = () => ({ Authorization: `Bearer ${token()}` });

test('auth guard: no token → 401', async ({ request }) => {
  const r = await request.get(`${API}/config`);
  expect(r.status()).toBe(401);
});

test('single task: create → done → wav audio', async ({ request }) => {
  const created = await request.post(`${API}/tasks`, {
    headers: { ...authHeaders() },
    data: { title: 'e2e-单任务', content: '你好世界。这是第二句话。', voice: '冰糖' },
  });
  expect(created.ok()).toBeTruthy();
  const task = await created.json();

  let status = 'pending';
  for (let i = 0; i < 40; i++) {
    const d = await request.get(`${API}/tasks/${task.id}`, { headers: { ...authHeaders() } });
    const detail = await d.json();
    status = detail.status;
    if (status === 'done' || status === 'failed') {
      expect(status).toBe('done');
      expect(detail.has_audio).toBe(true);
      expect(detail.chunks.length).toBe(1);
      expect(detail.chunks[0].duration_ms).toBe(500); // 24KB pcm16 @24kHz
      break;
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  expect(status).toBe('done');

  const audio = await request.get(`${API}/tasks/${task.id}/audio`, {
    headers: { ...authHeaders() },
  });
  expect(audio.status()).toBe(200);
  expect(audio.headers()['content-type']).toContain('audio/wav');

  // B1: token-query auth (native <audio> cannot send headers) + Range streaming.
  const ranged = await request.get(`${API}/tasks/${task.id}/audio?token=${token()}`, {
    headers: { Range: 'bytes=0-99' },
  });
  expect(ranged.status()).toBe(206);
  expect(ranged.headers()['content-range']).toMatch(/^bytes 0-99\/\d+$/);
});

test('batch import: 3 txt → session completed → zip export', async () => {
  const fd = new FormData();
  const files: [string, string][] = [
    ['a.txt', '第一段。第二段。'],
    ['b.txt', '另一篇内容。'],
    ['c.txt', '第三篇文本。'],
  ];
  for (const [name, content] of files) {
    fd.append('files', new Blob([content], { type: 'text/plain' }), name);
  }
  fd.append('voice', '茉莉');
  fd.append('model', 'mimo-v2.5-tts');
  const r = await fetch(`${API}/import`, {
    method: 'POST',
    headers: { ...authHeaders() },
    body: fd,
  });
  expect(r.status).toBe(202);
  const result = await r.json();
  expect(result.tasks_created).toBe(3);
  expect(result.rejected.length).toBe(0);

  let session: any = null;
  for (let i = 0; i < 40; i++) {
    const s = await fetch(`${API}/sessions/${result.session_id}`, {
      headers: { ...authHeaders() },
    });
    session = await s.json();
    if (session.status === 'completed' || session.status === 'failed') break;
    await new Promise((res) => setTimeout(res, 500));
  }
  expect(session.status).toBe('completed');
  expect(session.done_tasks).toBe(3);

  const zip = await fetch(`${API}/sessions/${result.session_id}/export`, {
    headers: { ...authHeaders() },
  });
  expect(zip.status).toBe(200);
  expect(zip.headers.get('content-type')).toContain('zip');
});

test('provider edit: repoint base_url & voice preview proxy', async () => {
  // B2: whitelisted preview proxy (302 to official CDN).
  const preview = await fetch(`${API}/voices/%E5%86%B0%E7%B3%96/preview?token=${token()}`, {
    redirect: 'manual',
  });
  expect(preview.status).toBe(302);
  expect(preview.headers.get('location')).toContain('aistudio-cdn.xiaomimimo.com');

  // B3: edit provider base_url (custom upstream) — then restore EXACTLY
  // what was there before (shared provider state must not leak across tests).
  const beforeList = await (await fetch(`${API}/providers`, { headers: { ...authHeaders() } })).json();
  const before = beforeList.find((p: any) => p.id === 'xiaomi');
  const edit = await fetch(`${API}/providers/xiaomi`, {
    method: 'PUT',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ base_url: 'http://127.0.0.1:30250/v1', budget_group: 'e2e-group' }),
  });
  expect(edit.status).toBe(200);
  const list = await (await fetch(`${API}/providers`, { headers: { ...authHeaders() } })).json();
  const xiaomi = list.find((p: any) => p.id === 'xiaomi');
  expect(xiaomi.base_url).toBe('http://127.0.0.1:30250/v1');
  expect(xiaomi.budget_group).toBe('e2e-group');
  const restore = await fetch(`${API}/providers/xiaomi`, {
    method: 'PUT',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ base_url: before.base_url, budget_group: before.budget_group }),
  });
  expect(restore.status).toBe(200);
  const afterList = await (await fetch(`${API}/providers`, { headers: { ...authHeaders() } })).json();
  const after = afterList.find((p: any) => p.id === 'xiaomi');
  expect(after.base_url).toBe(before.base_url);
  expect(after.budget_group).toBe(before.budget_group);
});

test('scoped credentials: issue, scope binding, expiry-safe audio', async () => {
  // Issue a scoped token for a concrete task id.
  const taskId = '00000000-0000-7000-8000-000000000001';
  const issue = await fetch(`${API}/auth/scoped`, {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ scope: `audio:${taskId}` }),
  });
  expect(issue.status).toBe(200);
  const { token: scoped } = await issue.json();
  expect(scoped.startsWith('scoped:v1:')).toBe(true);

  // Wrong scope → 401 WITH a valid signature: issue a scoped token for a
  // different scope and use it against this task's audio. A tampered token
  // would only prove HMAC integrity, not scope binding.
  const other = await fetch(`${API}/auth/scoped`, {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ scope: `audio:00000000-0000-7000-8000-000000000099` }),
  });
  expect(other.status).toBe(200);
  const { token: otherScoped } = await other.json();
  const wrong = await fetch(`${API}/tasks/${taskId}/audio?token=${otherScoped}`);
  expect(wrong.status).toBe(401);

  // Real task: create, wait done, fetch audio with a correct scoped token.
  const created = await fetch(`${API}/tasks`, {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ title: 'scoped-e2e', content: '凭证测试文本。', voice: '冰糖' }),
  });
  const task = await created.json();
  for (let i = 0; i < 40; i++) {
    const d = await (await fetch(`${API}/tasks/${task.id}`, { headers: { ...authHeaders() } })).json();
    if (d.status === 'done' || d.status === 'failed') break;
    await new Promise((r) => setTimeout(r, 300));
  }
  const issue2 = await fetch(`${API}/auth/scoped`, {
    method: 'POST',
    headers: { ...authHeaders(), 'Content-Type': 'application/json' },
    body: JSON.stringify({ scope: `audio:${task.id}` }),
  });
  const { token: scoped2 } = await issue2.json();
  const audio = await fetch(`${API}/tasks/${task.id}/audio?token=${scoped2}`);
  expect(audio.status).toBe(200);
  // Scoped token is not a valid API token for header use.
  const unauth = await fetch(`${API}/config`, { headers: { Authorization: `Bearer ${scoped2}` } });
  expect(unauth.status).toBe(401);
});

test('UI shell: embedded SPA renders without page errors', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  await page.goto('/');
  await expect(page.locator('#root *').first()).toBeVisible();
  // SPA deep link falls back to index.html
  await page.goto('/tasks/deadbeef');
  await expect(page.locator('#root *').first()).toBeVisible();
  // 401 network noise is expected before a token is set; only page crashes fail.
  expect(errors).toEqual([]);
});

test('UI workflow: set token → submit synthesis → task row done → play audio', async ({
  page,
}) => {
  // localStorage key per frontend contract: `um-mimotts.token`
  await page.goto('/');
  await page.evaluate((t) => localStorage.setItem('um-mimotts.token', t), token());
  await page.reload();

  // Workbench: wait for config load (submit enabled), pick a voice, submit.
  const submit = page.getByTestId('workbench-submit');
  await expect(submit).toBeEnabled({ timeout: 15_000 });
  await page.getByRole('button', { name: /^冰糖/ }).click();
  await page.getByTestId('workbench-content').fill('这是端到端测试文本。第二句。');
  await submit.click();

  // Workbench "最近任务" card appears (static snapshot — live updates live on
  // the detail page via SSE). Jump straight to detail and wait for audio.
  const detailLink = page.getByRole('link', { name: '查看详情 →' }).first();
  await detailLink.waitFor({ timeout: 15_000 });
  await detailLink.click();
  await expect(page.getByTestId('task-detail-audio')).toHaveCount(1, { timeout: 30_000 });
});
