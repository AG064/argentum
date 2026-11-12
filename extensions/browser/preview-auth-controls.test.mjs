import test from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { mkdtemp, writeFile, mkdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { BrowserAdapter } from './adapter.mjs';

test('fixed real source preview, guarded postimage revisit and owned teardown', async () => {
  const root = await mkdtemp(join(tmpdir(), 'argentum-preview-control-'));
  await writeFile(join(root, 'index.html'), '<h1>Original current source</h1>');
  await writeFile(join(root, 'credentials.json'), '{"synthetic":"private"}');
  const reservation = createServer(); await new Promise(resolve => reservation.listen(0, '127.0.0.1', resolve));
  const port = reservation.address().port; await new Promise(resolve => reservation.close(resolve));
  const origin = `http://127.0.0.1:${port}`;
  const adapter = new BrowserAdapter({ allowedOrigins:[origin], artifactDirectory:join(root, 'artifacts'), previewWorkspace:root, previewPort:port });
  try {
    const preview = await adapter.call('browser_preview_start', {}); assert.equal(preview.process_id, process.pid);
    assert.equal(adapter.browser, null);
    const first = await adapter.call('browser_open', {url:origin});
    assert.match((await adapter.call('browser_observe', {tab_id:first.tab_id})).accessibility, /Original current source/);
    await writeFile(join(root, 'index.html'), '<h1>Verified source postimage</h1>');
    const second = await adapter.call('browser_open', {url:origin});
    assert.match((await adapter.call('browser_observe', {tab_id:second.tab_id})).accessibility, /Verified source postimage/);
    assert.equal((await fetch(`${origin}/credentials.json`)).status, 404);
    assert.equal((await fetch(`${origin}/%2e%2e/private.json`)).status, 404);
  } finally { await adapter.close(); }
  await assert.rejects(fetch(origin));
});

test('controlled local auth survives only its explicitly configured persistent profile', async () => {
  const root = await mkdtemp(join(tmpdir(), 'argentum-auth-control-'));
  const server = createServer((request, response) => {
    if (request.url === '/login') { response.writeHead(200, {'Set-Cookie':'synthetic_session=approved-demo; HttpOnly; SameSite=Strict; Max-Age=3600; Path=/'}); response.end('ok'); return; }
    const signed = (request.headers.cookie ?? '').includes('synthetic_session=approved-demo');
    response.writeHead(200, {'Content-Type':'text/html'});
    response.end(signed ? '<h1>Authenticated synthetic demo</h1>' : '<h1>Anonymous</h1><button onclick="fetch(\'/login\').then(()=>location.reload())">Sign in demo</button>');
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const origin = `http://127.0.0.1:${server.address().port}`;
  const config = {allowedOrigins:[origin], artifactDirectory:join(root,'artifacts'), persistentProfile:join(root,'artifacts','profile')};
  let adapter = new BrowserAdapter(config);
  try {
    const tab = await adapter.call('browser_open', {url:origin});
    const observation = await adapter.call('browser_observe', {tab_id:tab.tab_id});
    const target = observation.targets.find(target => target.name === 'Sign in demo');
    await adapter.call('browser_interact', {tab_id:tab.tab_id,revision:observation.revision,reference:target.reference,action:'click'});
    await adapter.tabs.get(tab.tab_id).page.waitForFunction(() => document.querySelector('h1')?.textContent === 'Authenticated synthetic demo');
    await adapter.close(); adapter = new BrowserAdapter(config);
    const reopened = await adapter.call('browser_open', {url:origin});
    assert.match((await adapter.call('browser_observe', {tab_id:reopened.tab_id})).accessibility, /Authenticated synthetic demo/);
    assert.equal(typeof adapter.contextId, 'string');
  } finally { await adapter.close(); await new Promise(resolve => server.close(resolve)); }
});

test('persistent profiles cannot attach other applications or change cookie origin scope', async () => {
  const root = await mkdtemp(join(tmpdir(), 'argentum-profile-negative-'));
  const origin = 'http://127.0.0.1:18080';
  assert.throws(()=>new BrowserAdapter({allowedOrigins:[origin],artifactDirectory:join(root,'artifacts'),persistentProfile:join(root,'unrelated-profile')}),/dedicated_artifact_scope/);
  const config = {allowedOrigins:[origin],artifactDirectory:join(root,'artifacts'),persistentProfile:join(root,'artifacts','profile')};
  await mkdir(config.persistentProfile,{recursive:true});
  await writeFile(join(config.persistentProfile,'Cookies'),'synthetic_other_application_state');
  const existing = new BrowserAdapter(config); await assert.rejects(existing.start(),/existing_non_argentum_profile_denied/); assert.equal(existing.context,null); await existing.close();
  await writeFile(join(config.persistentProfile,'argentum-profile.json'),JSON.stringify({schema_version:1,allowed_origins:['http://127.0.0.1:18081']}));
  const changed = new BrowserAdapter(config); await assert.rejects(changed.start(),/profile_origin_scope_changed/); assert.equal(changed.context,null); await changed.close();
});
