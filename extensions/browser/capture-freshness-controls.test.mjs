import test from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { mkdtemp, readFile, readdir } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BrowserAdapter } from './adapter.mjs';

async function withFreshPage(check) {
  const root = await mkdtemp(join(tmpdir(), 'argentum-capture-freshness-'));
  const server = createServer((_request, response) => {
    response.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
    response.end('<main><h1>Capture control</h1><label>Query<input id=query value="initial"></label><p>Unchanged public content</p></main>');
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const origin = `http://127.0.0.1:${server.address().port}`;
  const adapter = new BrowserAdapter({ allowedOrigins: [origin], artifactDirectory: root });
  try {
    const opened = await adapter.call('browser_open', { url: origin });
    const entry = adapter.tabs.get(opened.tab_id);
    await entry.page.locator('#query').focus();
    const observation = await adapter.call('browser_observe', { tab_id: opened.tab_id });
    const input = observation.targets.find(target => target.tag === 'input');
    assert.ok(input);
    assert.equal(await entry.page.locator('#query').getAttribute('style'), null);
    assert.equal(await entry.page.locator('#query').evaluate(element => element === document.activeElement), true);
    await check({ adapter, entry, opened, observation, input, root, origin });
  } finally {
    await adapter.close();
    server.closeAllConnections();
    await new Promise(resolve => server.close(resolve));
  }
}

test('first focused-input screenshot preserves the observed DOM on a fresh page', async () => {
  await withFreshPage(async ({ adapter, entry, opened, observation, root, origin }) => {
    const before = await entry.page.evaluate(() => document.documentElement.outerHTML);
    const digest = entry.domDigest;
    const image = await adapter.call('browser_screenshot', { tab_id: opened.tab_id, revision: observation.revision });
    const pixels = await readFile(join(root, image.artifact));
    assert.equal(pixels.subarray(0, 8).toString('hex'), '89504e470d0a1a0a');
    assert.equal(image.origin, origin);
    assert.equal(image.revision, observation.revision);
    assert.equal(await entry.page.evaluate(() => document.documentElement.outerHTML), before);
    assert.equal(await entry.page.locator('#query').getAttribute('style'), null);
    assert.equal(await adapter.observation(entry), digest);
  });
});

test('an actual input change still rejects the prior capture revision before screenshot', async () => {
  await withFreshPage(async ({ adapter, entry, opened, observation, input, root }) => {
    await adapter.call('browser_interact', { tab_id: opened.tab_id, revision: observation.revision, reference: input.reference, action: 'fill', value: 'changed' });
    const before = await readdir(root);
    let screenshots = 0;
    const capture = entry.page.screenshot.bind(entry.page);
    entry.page.screenshot = async options => { screenshots++; return capture(options); };
    try {
      await assert.rejects(adapter.call('browser_screenshot', { tab_id: opened.tab_id, revision: observation.revision }), /stale_observation/);
      assert.equal(screenshots, 0);
      assert.deepEqual(await readdir(root), before);
    } finally { entry.page.screenshot = capture; }
  });
});

test('a mutation after actual capture is rejected with no new image artifact', async () => {
  await withFreshPage(async ({ adapter, entry, opened, observation, input, root }) => {
    const before = await readdir(root);
    const capture = entry.page.screenshot.bind(entry.page);
    let changedAfterCapture = false;
    entry.page.screenshot = async options => {
      const pixels = await capture(options);
      await adapter.call('browser_interact', { tab_id: opened.tab_id, revision: observation.revision, reference: input.reference, action: 'fill', value: 'changed after capture' });
      changedAfterCapture = true;
      return pixels;
    };
    try {
      await assert.rejects(adapter.call('browser_screenshot', { tab_id: opened.tab_id, revision: observation.revision }), /stale_observation/);
      assert.equal(changedAfterCapture, true);
      assert.deepEqual(await readdir(root), before);
    } finally { entry.page.screenshot = capture; }
  });
});
