import test from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { BrowserAdapter } from './adapter.mjs';

test('bounded viewport and concise form observations preserve fresh targeting', async () => {
  const root = await mkdtemp(join(tmpdir(), 'argentum-browser-completion-'));
  const server = createServer((_request, response) => {
    response.writeHead(200, { 'content-type': 'text/html' });
    response.end('<main><h1>Public control</h1><label><input type=checkbox>Selected item</label><label>Filter<input placeholder=Search></label><input type=password role=button value=PRIVATE_PASSWORD><input type=hidden role=button value=PRIVATE_HIDDEN><input type=file role=button><button>Refresh</button></main>');
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const origin = `http://127.0.0.1:${server.address().port}`;
  const adapter = new BrowserAdapter({ allowedOrigins: [origin], artifactDirectory: root });
  try {
    for (const args of [
      { tab_id: 'unused', width: '390', height: 800 },
      { tab_id: 'unused', width: 1201, height: 800 },
      { tab_id: 'unused', width: 390, height: 801 },
      { tab_id: 'unused', width: 319, height: 800 },
    ]) await assert.rejects(adapter.call('browser_viewport', args), /invalid_integer_argument/);
    await assert.rejects(adapter.call('browser_observe', { tab_id: 'unused', include_accessibility: 'false' }), /invalid_boolean_argument/);
    assert.equal(adapter.browser, null, 'invalid selections must not launch a browser');
    const opened = await adapter.call('browser_open', { url: origin });
    const before = await adapter.call('browser_observe', { tab_id: opened.tab_id });
    const selected = before.targets.find(target => target.name === 'Selected item');
    assert.ok(selected);
    assert.equal(selected.checked, false);
    assert.ok(!JSON.stringify(before).includes('PRIVATE_PASSWORD'));
    assert.ok(!JSON.stringify(before.targets).includes('PRIVATE_HIDDEN'));
    await adapter.call('browser_viewport', { tab_id: opened.tab_id, width: 390, height: 800 });
    await assert.rejects(adapter.call('browser_interact', { tab_id: opened.tab_id, revision: before.revision, reference: selected.reference, action: 'check' }), /stale_observation/);
    let observed = await adapter.call('browser_observe', { tab_id: opened.tab_id, include_accessibility: false });
    assert.equal(observed.accessibility, '');
    assert.deepEqual(observed.viewport, { width: 390, height: 800 });
    assert.equal(observed.layout.horizontal_overflow, false);
    const current = observed.targets.find(target => target.name === 'Selected item');
    await adapter.call('browser_interact', { tab_id: opened.tab_id, revision: observed.revision, reference: current.reference, action: 'check' });
    observed = await adapter.call('browser_observe', { tab_id: opened.tab_id, text_limit: 256 });
    assert.equal(observed.targets.find(target => target.name === 'Selected item').checked, true);
    assert.ok(observed.accessibility.length <= 256);
    assert.ok(adapter.actionStarts >= 4, 'server action starts are a separate observed layer');
    const closed = await adapter.call('browser_close', {});
    assert.equal(closed.closed, true);
    assert.equal(closed.context_id, opened.context_id);
    assert.equal(adapter.browser, null);
    await assert.rejects(adapter.call('browser_observe', { tab_id: opened.tab_id }), /adapter_retired/);
  } finally {
    await adapter.close();
    server.closeAllConnections();
    await new Promise(resolve => server.close(resolve));
    await rm(root, { recursive: true, force: true });
  }
});
