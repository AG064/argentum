import test from 'node:test';
import assert from 'node:assert/strict';
import { BrowserAdapter, destinationAllowed } from './adapter.mjs';

test('exact loopback destinations, not LAN, aliases, files or credentials', () => {
  const origins = ['http://127.0.0.1:18080'];
  assert.equal(destinationAllowed('http://127.0.0.1:18080/app', origins), true);
  for (const url of ['http://127.0.0.1:18081/', 'http://localhost:18080/', 'http://192.168.1.1/', 'file:///C:/private', 'http://user:password@127.0.0.1:18080/', 'https://example.com/']) assert.equal(destinationAllowed(url, origins), false);
});
test('construction is inert and malformed configuration cannot start a browser', () => {
  const adapter = new BrowserAdapter({ allowedOrigins: ['http://127.0.0.1:18080'], artifactDirectory: process.cwd() });
  assert.equal(adapter.browser, null); assert.equal(adapter.context, null);
  assert.throws(() => new BrowserAdapter({ allowedOrigins: ['http://localhost:18080'], artifactDirectory: process.cwd() }));
  assert.throws(() => new BrowserAdapter({ allowedOrigins: [], artifactDirectory: process.cwd(), cookies: [] }));
});
