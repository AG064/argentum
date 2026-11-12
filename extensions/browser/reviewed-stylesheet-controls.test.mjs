import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createServer, request, ServerResponse } from 'node:http';
import { writeFileSync } from 'node:fs';
import { mkdtemp, mkdir, writeFile, readFile, symlink, unlink, rmdir, lstat } from 'node:fs/promises';
import { join, dirname, isAbsolute } from 'node:path';
import { tmpdir } from 'node:os';
import { pathToFileURL } from 'node:url';

const selectedAdapter = process.env.ARGENTUM_PREVIEW_ADAPTER;
if (selectedAdapter && (!isAbsolute(selectedAdapter) || !(await lstat(selectedAdapter)).isFile())) throw new Error('Selected adapter must be an existing absolute file');
const { BrowserAdapter } = await import(selectedAdapter ? pathToFileURL(selectedAdapter).href : new URL('./adapter.mjs', import.meta.url).href);

const CSS = Buffer.from('/* REVIEWED_PUBLIC_STYLESHEET_SENTINEL café 界 */\n#sample { color: rgb(17, 34, 51); }\n');
const CHANGED = Buffer.from(CSS.toString().replace('17, 34, 51', '71, 43, 15'));
const digest = bytes => createHash('sha256').update(bytes).digest('hex');

async function fixture() {
  const root = await mkdtemp(join(tmpdir(), 'argentum-reviewed-style-'));
  const site = join(root, 'site');
  const path = 'styles/tokens.css';
  await mkdir(join(site, 'styles'), { recursive: true });
  for (const relative of [path, 'styles/token.css', 'styles/main.css', 'credentials.css', 'secret.css', 'secrets/tokens.css', 'tokens/ordinary.css']) {
    const file = join(site, relative);
    await mkdir(dirname(file), { recursive: true });
    await writeFile(file, CSS);
  }
  await writeFile(join(site, 'index.html'), '<link rel="stylesheet" href="/styles/tokens.css"><p id="sample">Public style control</p>');
  await writeFile(join(site, 'keep.txt'), 'UNCHANGED_REVIEWED_STYLE_CONTROL');
  const reservation = createServer();
  await new Promise(resolve => reservation.listen(0, '127.0.0.1', resolve));
  const port = reservation.address().port;
  await new Promise(resolve => reservation.close(resolve));
  const origin = `http://127.0.0.1:${port}`;
  const declaration = { path, sha256: digest(CSS), bytes: CSS.length };
  const config = { allowedOrigins: [origin], artifactDirectory: join(root, 'images'), previewWorkspace: site, previewPort: port };
  return { root, site, origin, declaration, config };
}

async function http(f, path, method = 'GET') {
  return new Promise((resolve, reject) => {
    const call = request(f.origin, { path, method }, response => {
      const chunks = [];
      response.on('data', chunk => chunks.push(chunk));
      response.on('end', () => resolve({ status: response.statusCode, headers: response.headers, body: Buffer.concat(chunks) }));
      response.on('error', reject);
    });
    call.on('error', reject);
    call.end();
  });
}

async function denied(f, path) {
  for (const method of ['GET', 'HEAD']) {
    const response = await http(f, path, method);
    assert.equal(response.status, 404, `${method} ${path}`);
    assert.equal(response.body.includes(Buffer.from('REVIEWED_PUBLIC_STYLESHEET_SENTINEL')), false);
    if (method === 'HEAD') assert.equal(response.body.length, 0);
  }
}

test('default private guard denies token styles for GET and HEAD', async () => {
  const f = await fixture();
  const adapter = new BrowserAdapter(f.config);
  try {
    await adapter.call('browser_preview_start', {});
    await denied(f, '/styles/tokens.css');
    const ordinary = await http(f, '/styles/main.css');
    assert.equal(ordinary.status, 200);
    assert.deepEqual(ordinary.body, CSS);
  } finally { await adapter.close(); }
});

test('exact reviewed bytes are served without granting another literal path', async () => {
  const f = await fixture();
  const adapter = new BrowserAdapter({ ...f.config, reviewedStylesheet: f.declaration });
  try {
    await adapter.call('browser_preview_start', {});
    for (const method of ['GET', 'HEAD']) {
      const response = await http(f, '/styles/tokens.css?revision=1', method);
      assert.equal(response.status, 200);
      assert.equal(response.headers['x-argentum-source-sha256'], f.declaration.sha256);
      assert.match(response.headers['content-type'], /^text\/css/);
      assert.deepEqual(response.body, method === 'HEAD' ? Buffer.alloc(0) : CSS);
    }
    for (const path of ['/styles/token.css', '/credentials.css', '/secret.css', '/secrets/tokens.css', '/tokens/ordinary.css', '/styles/nested/../tokens.css', '/styles/%2e%2e/styles/tokens.css', '/styles//tokens.css', '/styles%5ctokens.css', '/styles/tokens.css:stream']) {
      await denied(f, path);
    }
    assert.equal(await readFile(join(f.site, 'keep.txt'), 'utf8'), 'UNCHANGED_REVIEWED_STYLE_CONTROL');
  } finally { await adapter.close(); }
});

test('malformed declarations reject before starting any preview or browser', async () => {
  const f = await fixture();
  const invalid = [null, [], {}, { ...f.declaration, extra: true },
    { ...f.declaration, sha256: 'A'.repeat(64) },
    ...[-1, 0.5, Number.MAX_SAFE_INTEGER, '1', 4 * 1024 * 1024 + 1].map(bytes => ({ ...f.declaration, bytes })),
    ...['', '/styles/tokens.css', './styles/tokens.css', '../tokens.css', 'styles//tokens.css', 'styles\\tokens.css',
      '.private/tokens.css', 'styles./tokens.css', 'styles /tokens.css', 'C:/tokens.css', 'styles/tokens.css:stream',
      'CON/tokens.css', 'nul.txt/tokens.css', 'COM1/tokens.css', 'LPT\u00b2/tokens.css', 'credentials/tokens.css',
      'secrets/tokens.css', 'tokens/tokens.css', 'credentials.css', 'secret.css', 'styles/credentials.css', 'styles/secret.css', 'styles/main.css',
      'styles/tokens.json', 'styles/tokens.dark.css', 'styles/tokens.css\0'].map(path => ({ ...f.declaration, path }))];
  for (const reviewedStylesheet of invalid) {
    assert.throws(() => new BrowserAdapter({ ...f.config, reviewedStylesheet }), /invalid_reviewed_stylesheet/);
  }
  assert.throws(() => new BrowserAdapter({ allowedOrigins: [f.origin], artifactDirectory: f.config.artifactDirectory, reviewedStylesheet: f.declaration }), /requires_preview/);
  assert.throws(() => new BrowserAdapter({ ...f.config, publicStylesheet: f.declaration }), /invalid_browser_configuration/);
});

test('wrong digest or approved length discloses no stylesheet body', async () => {
  assert.notEqual(CSS.length, CSS.toString().length, 'fixture distinguishes UTF-8 bytes from characters');
  const f = await fixture();
  for (const reviewedStylesheet of [{ ...f.declaration, sha256: '0'.repeat(64) }, { ...f.declaration, bytes: CSS.length + 1 }, { ...f.declaration, bytes: CSS.toString().length }]) {
    const adapter = new BrowserAdapter({ ...f.config, reviewedStylesheet });
    try { await adapter.call('browser_preview_start', {}); await denied(f, '/styles/tokens.css'); }
    finally { await adapter.close(); }
  }
});

test('changed same-length or longer content stays denied without rewriting approval', async () => {
  assert.equal(CHANGED.length, CSS.length);
  const f = await fixture();
  const adapter = new BrowserAdapter({ ...f.config, reviewedStylesheet: f.declaration });
  try {
    await adapter.call('browser_preview_start', {});
    for (const bytes of [CHANGED, Buffer.concat([CSS, Buffer.from('changed')])]) {
      await writeFile(join(f.site, f.declaration.path), bytes);
      await denied(f, '/styles/tokens.css');
      assert.equal(adapter.config.reviewedStylesheet.sha256, digest(CSS));
    }
    await writeFile(join(f.site, f.declaration.path), CSS);
    assert.deepEqual((await http(f, '/styles/tokens.css')).body, CSS);
  } finally { await adapter.close(); }
});

test('model arguments cannot create or change a stylesheet declaration', async () => {
  const f = await fixture();
  const adapter = new BrowserAdapter(f.config);
  try {
    await assert.rejects(adapter.call('browser_preview_start', { reviewedStylesheet: f.declaration }), /invalid_tool_arguments/);
    assert.equal(adapter.preview, null);
    assert.equal(adapter.browser, null);
  } finally { await adapter.close(); }
});

test('reviewed bytes cannot override size or outside-root admission', async () => {
  const f = await fixture();
  let adapter = new BrowserAdapter({ ...f.config, reviewedStylesheet: f.declaration });
  try {
    await writeFile(join(f.site, f.declaration.path), Buffer.alloc(4 * 1024 * 1024 + 1, 65));
    await adapter.call('browser_preview_start', {});
    await denied(f, '/styles/tokens.css');
  } finally { await adapter.close(); }
  const outside = join(f.root, 'outside');
  const alias = join(f.site, 'external');
  await mkdir(outside);
  await writeFile(join(outside, 'tokens.css'), CSS);
  await symlink(outside, alias, process.platform === 'win32' ? 'junction' : 'dir');
  adapter = new BrowserAdapter({ ...f.config, reviewedStylesheet: { ...f.declaration, path: 'external/tokens.css' } });
  try {
    await adapter.call('browser_preview_start', {});
    await denied(f, '/external/tokens.css');
  } finally {
    await adapter.close();
    assert.equal((await lstat(alias)).isSymbolicLink(), true);
    try { await unlink(alias); }
    catch (error) { if (!['EPERM', 'EISDIR'].includes(error.code)) throw error; await rmdir(alias); }
  }
});

test('the same admitted buffer is served even when the file changes at response headers', async t => {
  const f = await fixture();
  const adapter = new BrowserAdapter({ ...f.config, reviewedStylesheet: f.declaration });
  let changed = false;
  const original = ServerResponse.prototype.writeHead;
  t.mock.method(ServerResponse.prototype, 'writeHead', function (status, headers) {
    if (status === 200 && headers?.['X-Argentum-Source-Sha256'] === f.declaration.sha256 && !changed) {
      writeFileSync(join(f.site, f.declaration.path), CHANGED);
      changed = true;
    }
    return original.apply(this, arguments);
  });
  try {
    await adapter.call('browser_preview_start', {});
    const response = await http(f, '/styles/tokens.css');
    assert.equal(changed, true);
    assert.equal(response.status, 200);
    assert.deepEqual(response.body, CSS);
    assert.deepEqual(await readFile(join(f.site, f.declaration.path)), CHANGED);
    await denied(f, '/styles/tokens.css');
  } finally { t.mock.restoreAll(); await adapter.close(); }
});

test('real browser applies the reviewed stylesheet and produces a current PNG', async () => {
  let forbiddenRequests = 0;
  const forbidden = createServer((_request, response) => { forbiddenRequests++; response.end('Not an admitted subresource'); });
  await new Promise(resolve => forbidden.listen(0, '127.0.0.1', resolve));
  let adapter;
  try {
    const f = await fixture();
    const styled = Buffer.concat([CSS, Buffer.from(`#sample { background-image: url("http://127.0.0.1:${forbidden.address().port}/not-admitted.png"); }\n`)]);
    await writeFile(join(f.site, f.declaration.path), styled);
    const declaration = { ...f.declaration, sha256: digest(styled), bytes: styled.length };
    adapter = new BrowserAdapter({ ...f.config, reviewedStylesheet: declaration });
    await adapter.call('browser_preview_start', {});
    const opened = await adapter.call('browser_open', { url: f.origin });
    const entry = adapter.tabs.get(opened.tab_id);
    await entry.page.waitForLoadState('networkidle');
    assert.equal(await entry.page.locator('#sample').evaluate(element => getComputedStyle(element).color), 'rgb(17, 34, 51)');
    const observed = await adapter.call('browser_observe', { tab_id: opened.tab_id });
    const image = await adapter.call('browser_screenshot', { tab_id: opened.tab_id, revision: observed.revision });
    const pixels = await readFile(join(f.config.artifactDirectory, image.artifact));
    assert.equal(pixels.subarray(0, 8).toString('hex'), '89504e470d0a1a0a');
    assert.equal(digest(pixels), image.sha256);
    assert.equal(image.revision, observed.revision);
    assert.equal(forbiddenRequests, 0);
    console.log(JSON.stringify({ reviewed_stylesheet_evidence: { png: join(f.config.artifactDirectory, image.artifact), png_sha256: image.sha256,
      stylesheet_sha256: declaration.sha256, stylesheet_bytes: declaration.bytes, computed_color: 'rgb(17, 34, 51)', unadmitted_subresource_requests: forbiddenRequests } }));
  } finally {
    if (adapter) await adapter.close();
    forbidden.closeAllConnections();
    await new Promise(resolve => forbidden.close(resolve));
  }
});
