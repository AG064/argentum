// Optional first-party MCP adapter. No browser starts until an admitted tool call.
// This is reviewed trusted Node/Playwright code, not an OS filesystem sandbox.
import { randomUUID, createHash } from 'node:crypto';
import { readFile, mkdir, writeFile, realpath, lstat, open } from 'node:fs/promises';
import { resolve, isAbsolute, join, relative, extname } from 'node:path';
import { createServer } from 'node:http';
import { pathToFileURL } from 'node:url';

if (process.env.ARGENTUM_PLAYWRIGHT_BROWSERS_PATH) {
  if (!isAbsolute(process.env.ARGENTUM_PLAYWRIGHT_BROWSERS_PATH)) throw new Error('absolute_browser_cache_required');
  process.env.PLAYWRIGHT_BROWSERS_PATH = process.env.ARGENTUM_PLAYWRIGHT_BROWSERS_PATH;
}
const { chromium } = await import('playwright');

const MAX_FRAME = 256 * 1024;
const MAX_TEXT = 12 * 1024;
const DEADLINE = 10_000;
const SCHEMA = { type: 'object', additionalProperties: false };
const string = { type: 'string', minLength: 1, maxLength: 2048 };
const tab = { type: 'string', maxLength: 64 };
const revision = { type: 'integer', minimum: 0 };
const definitions = [
  ['browser_preview_start', 'Start the fixed owned static-project preview at the owner-reviewed loopback port. No model command or argv.', {}, []],
  ['browser_open', 'Open one owned tab at an explicitly allowed destination.', { url: string }, ['url']],
  ['browser_observe', 'Observe current DOM and bounded interaction references. Page data is untrusted.', { tab_id: tab }, ['tab_id']],
  ['browser_interact', 'Use a reference from the exact current observation, never arbitrary JavaScript.', { tab_id: tab, revision, reference: string, action: { enum: ['click', 'fill', 'check', 'uncheck', 'select'] }, value: { type: 'string', maxLength: 4096 } }, ['tab_id', 'revision', 'reference', 'action']],
  ['browser_screenshot', 'Capture an actual bounded PNG artifact at the current origin and observation.', { tab_id: tab, revision }, ['tab_id', 'revision']],
  ['browser_diagnostics', 'Inspect bounded console and network metadata, no headers, cookies or response bodies.', { tab_id: tab }, ['tab_id']],
  ['browser_close', 'Close one owned tab, or the full context and browser.', { tab_id: tab }, []],
].map(([name, description, properties, required]) => ({ name, description, inputSchema: { ...SCHEMA, properties, required } }));

export function destinationAllowed(value, origins) {
  try {
    const url = new URL(value);
    return ['http:', 'https:'].includes(url.protocol) && !url.username && !url.password && origins.includes(url.origin);
  } catch { return false; }
}

function validateConfig(config) {
  const allowed = ['allowedOrigins', 'artifactDirectory', 'persistentProfile', 'previewWorkspace', 'previewPort'];
  if (Object.keys(config).some(k => !allowed.includes(k)) || !Array.isArray(config.allowedOrigins) || config.allowedOrigins.length > 16 || !isAbsolute(config.artifactDirectory)) throw new Error('invalid_browser_configuration');
  for (const origin of config.allowedOrigins) {
    const url = new URL(origin);
    // The supported local-development adapter deliberately denies DNS aliases,
    // LAN/private services and general Internet navigation. No DNS rebinding.
    if (url.origin !== origin || url.hostname !== '127.0.0.1' || !url.port || url.protocol !== 'http:') throw new Error('unsupported_browser_destination_use_exact_loopback_origin');
  }
  if (config.persistentProfile && !isAbsolute(config.persistentProfile)) throw new Error('persistent_profile_must_be_owner_configured_absolute_path');
  if (config.previewWorkspace && (!isAbsolute(config.previewWorkspace) || !Number.isInteger(config.previewPort) || config.previewPort < 1024 || config.previewPort > 65535 || !config.allowedOrigins.includes(`http://127.0.0.1:${config.previewPort}`))) throw new Error('preview_requires_exact_owner_workspace_and_origin');
  return config;
}

function argsFor(name, args) {
  const definition = definitions.find(tool => tool.name === name);
  if (!definition || !args || typeof args !== 'object' || Array.isArray(args)) throw new Error('invalid_tool_arguments');
  const schema = definition.inputSchema;
  if (Object.keys(args).some(key => !Object.hasOwn(schema.properties, key)) || schema.required.some(key => !Object.hasOwn(args, key))) throw new Error('invalid_tool_arguments');
  for (const [key, value] of Object.entries(args)) {
    const rule = schema.properties[key];
    if (rule.enum && !rule.enum.includes(value)) throw new Error('invalid_enum_argument');
    if (rule.type === 'string' && (typeof value !== 'string' || value.length > rule.maxLength || value.length < (rule.minLength ?? 0))) throw new Error('invalid_string_argument');
    if (rule.type === 'integer' && (!Number.isSafeInteger(value) || value < rule.minimum)) throw new Error('invalid_integer_argument');
  }
  return args;
}

export class BrowserAdapter {
  constructor(config) { this.config = validateConfig(config); this.browserId = randomUUID(); this.contextId = null; this.preview = null; this.tabs = new Map(); this.admittedPages = new WeakSet(); this.browser = null; this.context = null; this.closed = false; }
  async previewStart() {
    if (!this.config.previewWorkspace) throw new Error('preview_not_owner_configured');
    const origin = `http://127.0.0.1:${this.config.previewPort}`;
    if (this.preview) return { origin, process_id: process.pid, already_running: true };
    const root = await realpath(this.config.previewWorkspace);
    const server = createServer(async (request, response) => {
      try {
        if (!['GET', 'HEAD'].includes(request.method)) { response.writeHead(405); response.end(); return; }
        const url = new URL(request.url, origin);
        const decoded = decodeURIComponent(url.pathname);
        if (decoded.includes('\\') || decoded.includes('\0') || decoded.split('/').some(part => part.startsWith('.') || part.includes(':'))) throw new Error('path_denied');
        const candidate = resolve(root, `.${decoded === '/' ? '/index.html' : decoded}`);
        const path = await realpath(candidate); const within = relative(root, path);
        if (within.startsWith('..') || isAbsolute(within)) throw new Error('path_denied');
        const types = { '.html': 'text/html; charset=utf-8', '.css': 'text/css; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.json': 'application/json', '.svg': 'image/svg+xml', '.png': 'image/png', '.jpg': 'image/jpeg', '.woff2': 'font/woff2' };
        const contentType = types[extname(path).toLowerCase()]; if (!contentType) throw new Error('unsupported_preview_file');
        if (within.split(/[\\/]/).some(part => /^(credentials?|secrets?|tokens?)(\.|$)/i.test(part))) throw new Error('private_preview_resource_denied');
        const metadata = await lstat(path, { bigint:true }); if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size > 4n * 1024n * 1024n) throw new Error('preview_file_capacity_exceeded');
        const file = await open(path, 'r'); let bytes;
        try {
          const held = await file.stat({ bigint:true });
          if (held.dev !== metadata.dev || held.ino !== metadata.ino || !held.isFile()) throw new Error('preview_source_changed');
          const buffer = Buffer.alloc(Number(metadata.size) + 1); const read = await file.read(buffer, 0, buffer.length, 0);
          if (read.bytesRead !== Number(metadata.size)) throw new Error('preview_source_changed');
          const after = await file.stat({ bigint:true });
          if (after.size !== metadata.size || after.mtimeNs !== metadata.mtimeNs) throw new Error('preview_source_changed');
          bytes = buffer.subarray(0, read.bytesRead);
        } finally { await file.close(); }
        response.writeHead(200, { 'Content-Type': contentType, 'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff', 'X-Argentum-Source-Sha256': createHash('sha256').update(bytes).digest('hex') });
        response.end(request.method === 'HEAD' ? undefined : bytes);
      } catch { response.writeHead(404); response.end('Preview resource unavailable'); }
    });
    await new Promise((resolve, reject) => { server.once('error', reject); server.listen(this.config.previewPort, '127.0.0.1', resolve); });
    this.preview = server;
    return { origin, process_id: process.pid, mode: 'fixed_static_project_preview', arbitrary_process_execution: false };
  }
  async start() {
    if (this.closed) throw new Error('adapter_retired');
    if (this.context) return;
    const options = { headless: true, viewport: { width: 1200, height: 800 }, acceptDownloads: false, serviceWorkers: 'block', timeout: DEADLINE };
    if (this.config.persistentProfile) {
      this.context = await chromium.launchPersistentContext(this.config.persistentProfile, options);
    } else {
      this.browser = await chromium.launch({ headless: true, timeout: DEADLINE });
      this.context = await this.browser.newContext(options);
    }
    this.contextId = randomUUID();
    this.context.setDefaultTimeout(DEADLINE);
    this.context.setDefaultNavigationTimeout(DEADLINE);
    await this.context.route('**/*', async route => {
      let page;
      try { page = route.request().frame().page(); } catch { /* Worker requests have no admitted frame. */ }
      if (page && this.admittedPages.has(page) && destinationAllowed(route.request().url(), this.config.allowedOrigins)) {
        try {
          const response = await route.fetch({ maxRedirects: 0, timeout: DEADLINE });
          // Never let an enclosing Chromium request follow an unobserved
          // redirect. Redirect navigation is deliberately unavailable here.
          if (response.status() >= 300 && response.status() < 400) {
            for (const entry of this.tabs.values()) this.record(entry, 'network', 'redirect_denied');
            await route.abort('blockedbyclient'); return;
          }
          const headers = response.headers();
          if (Number(headers['content-length'] ?? 0) > 4 * 1024 * 1024) { await route.abort('blockedbyclient'); return; }
          headers['x-dns-prefetch-control'] = 'off';
          if ((headers['content-type'] ?? '').includes('text/html')) {
            const csp = `default-src 'self' data:; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; connect-src 'self'; frame-src 'none'; object-src 'none'; base-uri 'self'`;
            headers['content-security-policy'] = headers['content-security-policy'] ? `${headers['content-security-policy']}, ${csp}` : csp;
          }
          await route.fulfill({ response, headers });
        } catch { await route.abort('failed'); }
      }
      else {
        for (const entry of this.tabs.values()) this.record(entry, 'network', 'destination_denied');
        await route.abort('blockedbyclient');
      }
    });
    await this.context.routeWebSocket('**/*', socket => socket.close({ code: 1008, reason: 'websocket_unavailable' }));
    this.context.on('page', page => {
      setImmediate(() => { if (!this.admittedPages.has(page)) void page.close(); });
      page.on('download', download => { void download.cancel(); });
      page.on('filechooser', chooser => { void chooser.setFiles([]); });
      page.on('dialog', dialog => { void dialog.dismiss(); });
    });
    // launchPersistentContext may create an initial blank tab; it is not an
    // admitted model target and is closed before any operation.
    for (const page of this.context.pages()) await page.close();
  }
  record(entry, kind, message) {
    entry.diagnostics.push({ kind, message: String(message).slice(0, 2048) });
    if (entry.diagnostics.length > 32) entry.diagnostics.shift();
  }
  current(id) {
    const entry = this.tabs.get(id);
    if (!entry || entry.page.isClosed()) throw new Error('stale_or_foreign_tab');
    if (!destinationAllowed(entry.page.url(), this.config.allowedOrigins)) throw new Error('tab_origin_unavailable');
    return entry;
  }
  async observation(entry) {
    const content = await entry.page.evaluate(() => document.documentElement.outerHTML.slice(0, 1024 * 1024 + 1));
    if (content.length > 1024 * 1024) throw new Error('dom_exceeds_observation_limit');
    return createHash('sha256').update(content).digest('hex');
  }
  async requireCurrent(entry, expected) {
    if (await this.observation(entry) !== entry.domDigest || entry.observedRevision !== expected || entry.observedUrl !== entry.page.url()) throw new Error('stale_observation_observe_again');
  }
  async call(name, supplied) {
    const args = argsFor(name, supplied);
    if (this.closed) throw new Error('adapter_retired');
    if (name === 'browser_preview_start') return this.previewStart();
    if (name === 'browser_close') {
      if (!args.tab_id) { await this.close(); return { closed: true }; }
      const entry = this.current(args.tab_id); await entry.page.close(); this.tabs.delete(args.tab_id); return { tab_closed: args.tab_id };
    }
    await this.start();
    if (name === 'browser_open') {
      if (!destinationAllowed(args.url, this.config.allowedOrigins)) throw new Error('destination_denied');
      if (this.tabs.size >= 4) throw new Error('tab_limit_reached');
      const page = await this.context.newPage(); this.admittedPages.add(page);
      const id = randomUUID(); const entry = { page, references: new Map(), diagnostics: [], revision: 0, domDigest: null, observedRevision: null, observedUrl: null };
      this.tabs.set(id, entry);
      page.on('console', event => { if (['warning', 'error'].includes(event.type())) this.record(entry, 'console', event.text()); });
      page.on('pageerror', error => this.record(entry, 'pageerror', error.message));
      page.on('response', response => { if (response.status() >= 400) { const url = new URL(response.url()); this.record(entry, 'network', `${response.status()} ${url.origin}${url.pathname}`); } });
      page.on('requestfailed', () => this.record(entry, 'network', 'request_failed'));
      try { await page.goto(args.url, { waitUntil: 'domcontentloaded' }); this.current(id); }
      catch { await page.close(); this.tabs.delete(id); throw new Error('navigation_failed_or_destination_denied'); }
      const visibleUrl = new URL(page.url()); return { browser_id: this.browserId, context_id: this.contextId, tab_id: id, origin: visibleUrl.origin, path: visibleUrl.pathname };
    }
    const entry = this.current(args.tab_id);
    if (name === 'browser_diagnostics') return { tab_id: args.tab_id, diagnostics: [...entry.diagnostics], bounded: true };
    if (name === 'browser_observe') {
      const before = await this.observation(entry);
      entry.references.clear();
      const elements = entry.page.locator('button, a[href], input:not([type=password]):not([type=file]):not([type=hidden]), select, textarea, [role=button]');
      const count = Math.min(await elements.count(), 64); const targets = [];
      for (let index = 0; index < count; index++) {
        const locator = elements.nth(index);
        if (!await locator.isVisible()) continue;
        const details = await locator.evaluate(element => ({ tag: element.tagName.toLowerCase(), name: (element.getAttribute('aria-label') || element.textContent || element.getAttribute('placeholder') || '').slice(0, 160) }));
        const reference = `e${index}`; entry.references.set(reference, locator); targets.push({ reference, ...details });
      }
      const accessibility = (await entry.page.locator('body').ariaSnapshot()).slice(0, MAX_TEXT);
      const after = await this.observation(entry);
      if (after !== before) { entry.references.clear(); throw new Error('observation_changed_retry'); }
      entry.revision++; entry.domDigest = after; entry.observedRevision = entry.revision; entry.observedUrl = entry.page.url();
      return { browser_id: this.browserId, context_id: this.contextId, tab_id: args.tab_id, revision: entry.revision, origin: new URL(entry.page.url()).origin, accessibility, targets, bounded: true };
    }
    await this.requireCurrent(entry, args.revision);
    if (name === 'browser_screenshot') {
      const png = await entry.page.screenshot({ type: 'png', fullPage: false, animations: 'disabled', timeout: DEADLINE });
      if (png.length > 128 * 1024) throw new Error('screenshot_exceeds_byte_limit');
      await this.requireCurrent(entry, args.revision);
      await mkdir(this.config.artifactDirectory, { recursive: true });
      const artifact = `${randomUUID()}.png`;
      await writeFile(join(this.config.artifactDirectory, artifact), png, { flag: 'wx' });
      return { artifact, sha256: createHash('sha256').update(png).digest('hex'), mime_type: 'image/png', bytes: png.length, browser_id:this.browserId, context_id:this.contextId, origin: new URL(entry.page.url()).origin, tab_id: args.tab_id, revision: args.revision };
    }
    const locator = entry.references.get(args.reference);
    if (!locator || await locator.count() !== 1) throw new Error('stale_or_unknown_reference');
    // Only predetermined Playwright actions. Model input is never evaluated.
    if (args.action === 'click') await locator.click();
    else if (args.action === 'fill') { if (args.value === undefined) throw new Error('fill_requires_value'); await locator.fill(args.value); }
    else if (args.action === 'check') await locator.check();
    else if (args.action === 'uncheck') await locator.uncheck();
    else if (args.action === 'select') { if (args.value === undefined) throw new Error('select_requires_value'); await locator.selectOption(args.value); }
    this.current(args.tab_id); entry.references.clear(); entry.observedRevision = null;
    return { performed: args.action, tab_id: args.tab_id, observe_again: true };
  }
  async close() {
    this.closed = true; this.tabs.clear();
    if (this.context) await this.context.close();
    if (this.browser) await this.browser.close();
    if (this.preview) { this.preview.closeAllConnections(); await new Promise(resolve => this.preview.close(resolve)); }
    this.preview = null;
    this.context = null; this.browser = null;
  }
}

async function serve(configPath) {
  if (!isAbsolute(configPath)) throw new Error('absolute_owner_configuration_required');
  const adapter = new BrowserAdapter(JSON.parse(await readFile(configPath, 'utf8')));
  let buffer = Buffer.alloc(0); let pending = Promise.resolve();
  const info = { name: 'argentum-playwright', version: '0.1.0' };
  const capabilities = { tools: {} };
  function output(value) {
    const bytes = Buffer.from(JSON.stringify(value) + '\n');
    if (bytes.length > MAX_FRAME) throw new Error('response_exceeds_frame_limit');
    process.stdout.write(bytes);
  }
  async function dispatch(message) {
    if (!message || message.jsonrpc !== '2.0' || typeof message.method !== 'string') throw new Error('invalid_jsonrpc');
    if (message.id === undefined) return;
    let result;
    if (message.method === 'initialize') {
      if (message.params?.protocolVersion !== '2025-11-25') { output({ jsonrpc: '2.0', id: message.id, error: { code: -32602, message: 'Supported legacy version: 2025-11-25; modern version: 2026-07-28' } }); return; }
      result = { protocolVersion: '2025-11-25', serverInfo: info, capabilities };
    } else if (message.method === 'server/discover') result = { resultType: 'complete', supportedVersions: ['2026-07-28', '2025-11-25'], capabilities, ttlMs: 0, cacheScope: 'private', _meta: { 'io.modelcontextprotocol/serverInfo': info } };
    else if (message.method === 'tools/list') result = { tools: definitions };
    else if (message.method === 'tools/call') {
      try {
        const resultValue = await adapter.call(message.params?.name, message.params?.arguments ?? {});
        result = { content: [{ type: 'text', text: JSON.stringify(resultValue) }], structuredContent: resultValue };
      } catch (error) { result = { content: [{ type: 'text', text: String(error.message).slice(0, 512) }], isError: true }; }
    } else { output({ jsonrpc: '2.0', id: message.id, error: { code: -32601, message: 'Unsupported method; sampling, roots, elicitation and resource callbacks are unavailable' } }); return; }
    output({ jsonrpc: '2.0', id: message.id, result });
  }
  process.stdin.on('data', data => {
    buffer = Buffer.concat([buffer, data]);
    if (buffer.length > MAX_FRAME && !buffer.includes(10)) { process.stdin.destroy(); void adapter.close().finally(() => process.exit(2)); return; }
    for (let end; (end = buffer.indexOf(10)) >= 0;) {
      const frame = buffer.subarray(0, end); buffer = buffer.subarray(end + 1);
      if (frame.length > MAX_FRAME) { void adapter.close().finally(() => process.exit(2)); return; }
      pending = pending.then(() => dispatch(JSON.parse(frame.toString('utf8')))).catch(() => { process.stdin.destroy(); return adapter.close().finally(() => process.exit(2)); });
    }
  });
  process.stdin.on('end', () => { void pending.finally(() => adapter.close()); });
  for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => { void adapter.close().finally(() => process.exit(0)); });
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  if (process.argv.length !== 3) throw new Error('usage: node adapter.mjs ABSOLUTE_OWNER_CONFIG');
  await serve(process.argv[2]);
}
