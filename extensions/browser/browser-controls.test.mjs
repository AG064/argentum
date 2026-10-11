import test from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { mkdtemp, readFile, readdir } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BrowserAdapter } from './adapter.mjs';

test('real rendered interaction, stale reference, screenshot and subresource denial', async () => {
  const root = await mkdtemp(join(tmpdir(), 'argentum-browser-control-'));
  let outsideHits = 0;
  const outside = createServer((request, response) => { outsideHits++; response.end('not authorized'); });
  await new Promise(resolve => outside.listen(0, '127.0.0.1', resolve));
  const outsideUrl = `http://127.0.0.1:${outside.address().port}`;
  const server = createServer((request, response) => {
    if (request.url === '/redirect') { response.writeHead(302, { Location: outsideUrl }); response.end(); return; }
    if (request.url === '/download') { response.writeHead(200, { 'Content-Disposition': 'attachment; filename=denied.txt' }); response.end('control'); return; }
    response.writeHead(200, { 'Content-Type': 'text/html' });
    response.end(`<main><h1>Counter</h1><p id=count>0</p><button id=increment>Increment</button><button id=popup>Popup</button><a href=/download>Download</a><input type=file><img src="${outsideUrl}/private"><script>document.querySelector('#increment').onclick=()=>document.querySelector('#count').textContent=Number(document.querySelector('#count').textContent)+1;document.querySelector('#popup').onclick=()=>window.open('${outsideUrl}');console.error('synthetic-console-control');</script></main>`);
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const origin = `http://127.0.0.1:${server.address().port}`;
  const adapter = new BrowserAdapter({ allowedOrigins: [origin], artifactDirectory: root });
  try {
    const opened = await adapter.call('browser_open', { url: origin });
    let observation = await adapter.call('browser_observe', { tab_id: opened.tab_id });
    const button = observation.targets.find(target => target.name === 'Increment'); assert.ok(button);
    await adapter.call('browser_interact', { tab_id: opened.tab_id, revision: observation.revision, reference: button.reference, action: 'click' });
    await assert.rejects(adapter.call('browser_interact', { tab_id: opened.tab_id, revision: observation.revision, reference: button.reference, action: 'click' }), /stale_observation/);
    observation = await adapter.call('browser_observe', { tab_id: opened.tab_id }); assert.match(observation.accessibility, /1/);
    // Playwright restores a file input's style to an empty attribute on its
    // first screenshot. This is a real DOM change, so old observations fail
    // closed. A fresh observation admits a new capture, not a silently rebased
    // old reference.
    await assert.rejects(adapter.call('browser_screenshot', { tab_id: opened.tab_id, revision: observation.revision }), /stale_observation/);
    observation = await adapter.call('browser_observe', { tab_id: opened.tab_id });
    const capture = await adapter.call('browser_screenshot', { tab_id: opened.tab_id, revision: observation.revision });
    const png = await readFile(join(root, capture.artifact)); assert.equal(png.subarray(0, 8).toString('hex'), '89504e470d0a1a0a'); assert.equal(capture.origin, origin);
    const popup = observation.targets.find(target => target.name === 'Popup');
    await adapter.call('browser_interact', { tab_id: opened.tab_id, revision: observation.revision, reference: popup.reference, action: 'click' });
    await new Promise(resolve => setTimeout(resolve, 100));
    assert.equal(outsideHits, 0); assert.equal(adapter.tabs.size, 1);
    const diagnostics = await adapter.call('browser_diagnostics', { tab_id: opened.tab_id });
    assert.ok(diagnostics.diagnostics.some(entry => entry.message === 'synthetic-console-control'));
    assert.ok(diagnostics.diagnostics.some(entry => entry.message === 'destination_denied'));
    observation = await adapter.call('browser_observe', { tab_id: opened.tab_id });
    assert.ok(!observation.targets.some(target => target.tag === 'input'));
    const download = observation.targets.find(target => target.name === 'Download');
    await adapter.call('browser_interact', { tab_id: opened.tab_id, revision: observation.revision, reference: download.reference, action: 'click' });
    await new Promise(resolve => setTimeout(resolve, 100));
    assert.deepEqual((await readdir(root)).filter(name => !name.endsWith('.png')), []);
    observation = await adapter.call('browser_observe', { tab_id: opened.tab_id });
    const replaced = observation.targets.find(target => target.name === 'Increment');
    await adapter.tabs.get(opened.tab_id).page.evaluate(() => { const button=document.querySelector('#increment'); button.replaceWith(button.cloneNode(true)); });
    await assert.rejects(adapter.call('browser_interact', { tab_id: opened.tab_id, revision: observation.revision, reference: replaced.reference, action: 'click' }), /stale_or_unknown_reference/);
    await assert.rejects(adapter.call('browser_open', { url: `${origin}/redirect` }), /navigation_failed/);
    await assert.rejects(adapter.call('browser_open', { url: outsideUrl }), /destination_denied/);
    await adapter.call('browser_close', { tab_id: opened.tab_id });
    await assert.rejects(adapter.call('browser_observe', { tab_id: opened.tab_id }), /stale_or_foreign_tab/);
    assert.equal(outsideHits, 0);
    await adapter.close();
    await assert.rejects(adapter.call('browser_open', {url:origin}), /adapter_retired/);
  } finally {
    await adapter.close(); await new Promise(resolve => server.close(resolve)); await new Promise(resolve => outside.close(resolve));
  }
});
