import test from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { BrowserAdapter } from './adapter.mjs';

test('literal reload and fixed keyboard focus retain fresh references and destination containment', async () => {
  const root = await mkdtemp(join(tmpdir(), 'argentum-browser-reload-focus-'));
  let loads = 0;
  let outsideHits = 0;
  let redirectReload = false;
  const outside = createServer((_request, response) => { outsideHits++; response.end('denied'); });
  await new Promise(resolve => outside.listen(0, '127.0.0.1', resolve));
  const outsideUrl = `http://127.0.0.1:${outside.address().port}`;
  const server = createServer((_request, response) => {
    loads++;
    if (redirectReload) { response.writeHead(302, { Location: outsideUrl }); response.end(); return; }
    response.writeHead(200, { 'content-type': 'text/html' });
    response.end('<style>:focus-visible{outline:3px solid blue}</style><main><label>Filter<input id=filter></label><label><input id=choice type=checkbox>Saved choice</label><button id=reset>Reset</button><script>const choice=document.querySelector("#choice");choice.checked=localStorage.getItem("choice")==="yes";choice.onchange=()=>localStorage.setItem("choice",choice.checked?"yes":"no");document.querySelector("#reset").onclick=()=>{choice.checked=false;localStorage.setItem("choice","no")};</script></main>');
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const origin = `http://127.0.0.1:${server.address().port}`;
  const adapter = new BrowserAdapter({ allowedOrigins: [origin], artifactDirectory: root });
  try {
    for (const action of ['press', 'evaluate', 'Ctrl+V', 'Enter']) {
      await assert.rejects(adapter.call('browser_interact', { tab_id: 'none', revision: 1, reference: 'none', action }), /invalid_enum_argument/);
    }
    for (const action of ['reload', 'tab', 'shift_tab']) {
      await assert.rejects(adapter.call('browser_interact', { tab_id: 'none', revision: 1, reference: 'none', action, value: 'unaccepted' }), /invalid_tool_arguments/);
    }
    assert.equal(adapter.browser, null, 'unsupported actions cannot start a browser');
    const opened = await adapter.call('browser_open', { url: origin });
    const observe = () => adapter.call('browser_observe', { tab_id: opened.tab_id });
    const interact = (observation, name, action) => adapter.call('browser_interact', {
      tab_id: opened.tab_id, revision: observation.revision,
      reference: observation.targets.find(target => target.name === name).reference, action,
    });
    let observation = await observe();
    await interact(observation, 'Saved choice', 'check');
    observation = await observe();
    const oldRevision = observation.revision;
    await interact(observation, 'Saved choice', 'reload');
    assert.equal(loads, 2, 'the same owned page made a literal second navigation');
    await assert.rejects(interact(observation, 'Saved choice', 'reload'), /stale_observation/);
    observation = await observe();
    assert(observation.revision > oldRevision);
    assert.equal(observation.targets.find(target => target.name === 'Saved choice').checked, true);
    const starts = adapter.actionStarts;
    await assert.rejects(adapter.call('browser_interact', { tab_id: opened.tab_id, revision: observation.revision, reference: 'foreign', action: 'tab' }), /stale_or_unknown_reference/);
    assert.equal(adapter.actionStarts, starts, 'a foreign reference never reaches a keyboard action');
    await interact(observation, 'Filter', 'tab');
    observation = await observe();
    assert.equal(observation.targets.find(target => target.name === 'Saved choice').focused, true);
    let capture;
    try {
      capture = await adapter.call('browser_screenshot', { tab_id: opened.tab_id, revision: observation.revision });
    } catch (error) {
      assert.match(error.message, /stale_observation/);
      observation = await observe();
      assert.equal(observation.targets.find(target => target.name === 'Saved choice').focused, true);
      capture = await adapter.call('browser_screenshot', { tab_id: opened.tab_id, revision: observation.revision });
    }
    assert.equal((await readFile(join(root, capture.artifact))).subarray(0, 8).toString('hex'), '89504e470d0a1a0a');
    await interact(observation, 'Saved choice', 'shift_tab');
    await assert.rejects(interact(observation, 'Saved choice', 'tab'), /stale_observation/);
    observation = await observe();
    assert.equal(observation.targets.find(target => target.name === 'Filter').focused, true);
    await interact(observation, 'Reset', 'click');
    observation = await observe();
    await interact(observation, 'Reset', 'reload');
    observation = await observe();
    assert.equal(observation.targets.find(target => target.name === 'Saved choice').checked, false);
    redirectReload = true;
    await assert.rejects(interact(observation, 'Reset', 'reload'));
    assert.equal(outsideHits, 0, 'reload cannot contact an unadmitted redirect origin');
    await adapter.close();
    await assert.rejects(interact(observation, 'Reset', 'reload'), /adapter_retired/);
  } finally {
    await adapter.close();
    server.closeAllConnections(); outside.closeAllConnections();
    await Promise.all([new Promise(resolve => server.close(resolve)), new Promise(resolve => outside.close(resolve))]);
    await rm(root, { recursive: true, force: true });
  }
});
