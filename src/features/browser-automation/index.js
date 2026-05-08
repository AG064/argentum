'use strict';
/**
 * Browser Automation Feature
 *
 * Full Puppeteer/Playwright integration for headless browser control.
 * Navigate, click, type, screenshot, extract content, multi-tab support,
 * proxy/VPN support, domain allowlisting.
 */
var __createBinding =
  (this && this.__createBinding) ||
  (Object.create
    ? function (o, m, k, k2) {
        if (k2 === undefined) k2 = k;
        var desc = Object.getOwnPropertyDescriptor(m, k);
        if (!desc || ('get' in desc ? !m.__esModule : desc.writable || desc.configurable)) {
          desc = {
            enumerable: true,
            get: function () {
              return m[k];
            },
          };
        }
        Object.defineProperty(o, k2, desc);
      }
    : function (o, m, k, k2) {
        if (k2 === undefined) k2 = k;
        o[k2] = m[k];
      });
var __setModuleDefault =
  (this && this.__setModuleDefault) ||
  (Object.create
    ? function (o, v) {
        Object.defineProperty(o, 'default', { enumerable: true, value: v });
      }
    : function (o, v) {
        o['default'] = v;
      });
var __importStar =
  (this && this.__importStar) ||
  (function () {
    var ownKeys = function (o) {
      ownKeys =
        Object.getOwnPropertyNames ||
        function (o) {
          var ar = [];
          for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
          return ar;
        };
      return ownKeys(o);
    };
    return function (mod) {
      if (mod && mod.__esModule) return mod;
      var result = {};
      if (mod != null)
        for (var k = ownKeys(mod), i = 0; i < k.length; i++)
          if (k[i] !== 'default') __createBinding(result, mod, k[i]);
      __setModuleDefault(result, mod);
      return result;
    };
  })();
Object.defineProperty(exports, '__esModule', { value: true });
/**
 * Browser Automation feature — full headless browser control.
 *
 * Supports Puppeteer and Playwright engines with proxy/VPN,
 * multi-tab sessions, content extraction, and screenshots.
 */
class BrowserAutomationFeature {
  constructor() {
    this.meta = {
      name: 'browser-automation',
      version: '0.0.5',
      description: 'Headless browser with Puppeteer/Playwright, proxy, multi-tab',
      dependencies: [],
    };
    this.config = {
      enabled: false,
      engine: 'puppeteer',
      headless: true,
      timeout: 30000,
      maxPages: 10,
      allowedDomains: ['*'],
      blockedDomains: [],
      viewportWidth: 1280,
      viewportHeight: 720,
    };
    this.sessions = new Map();
    this.engine = null;
  }
  async init(config, context) {
    this.ctx = context;
    this.config = { ...this.config, ...config };
  }
  async start() {
    try {
      if (this.config.engine === 'playwright') {
        this.engine = await Promise.resolve().then(() => __importStar(require('playwright')));
      } else {
        this.engine = await Promise.resolve().then(() => __importStar(require('puppeteer')));
      }
      this.ctx.logger.info(`Browser engine loaded: ${this.config.engine}`);
    } catch {
      this.ctx.logger.warn(
        `${this.config.engine} not installed — browser features will be limited to session tracking`,
      );
    }
  }
  async stop() {
    for (const [id] of this.sessions) await this.closeSession(id);
  }
  async healthCheck() {
    return {
      healthy: true,
      message: this.engine ? 'Engine loaded' : 'Engine not installed (limited mode)',
      details: {
        engine: this.config.engine,
        activeSessions: this.sessions.size,
        proxy: this.config.proxyUrl ? 'configured' : 'none',
      },
    };
  }
  /** Check if URL is allowed by domain policy */
  isUrlAllowed(url) {
    try {
      const domain = new URL(url).hostname;
      if (this.config.blockedDomains.some((d) => domain.includes(d))) return false;
      if (this.config.allowedDomains.includes('*')) return true;
      return this.config.allowedDomains.some((d) => domain.includes(d));
    } catch {
      return false;
    }
  }
  /** Launch browser instance */
  async launchBrowser(proxyUrl) {
    const proxy = proxyUrl ?? this.config.proxyUrl;
    if (!this.engine) throw new Error('Browser engine not installed');
    const engine = this.engine;
    if (this.config.engine === 'playwright') {
      const chromium = engine.chromium;
      return chromium.launch({
        headless: this.config.headless,
        proxy: proxy ? { server: proxy } : undefined,
        args: ['--no-sandbox', '--disable-setuid-sandbox'],
      });
    } else {
      return engine.default.launch({
        headless: this.config.headless ? 'new' : false,
        proxy: proxy ? { server: proxy } : undefined,
        args: ['--no-sandbox', '--disable-setuid-sandbox'],
      });
    }
  }
  /** Create a new browser session */
  async createSession(url, proxyUrl) {
    if (this.sessions.size >= this.config.maxPages) {
      throw new Error(`Maximum page limit (${this.config.maxPages}) reached`);
    }
    const sessionId = `session_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
    let browser = null;
    let page = null;
    let context = null;
    if (this.engine) {
      try {
        browser = await this.launchBrowser(proxyUrl);
        const b = browser;
        if (this.config.engine === 'playwright') {
          context = await b.newContext({
            viewport: { width: this.config.viewportWidth, height: this.config.viewportHeight },
            userAgent: this.config.userAgent,
          });
          page = await context.newPage();
        } else {
          page = await b.newPage();
          await page.setViewport({
            width: this.config.viewportWidth,
            height: this.config.viewportHeight,
          });
          if (this.config.userAgent) {
            await page.setUserAgent(this.config.userAgent);
          }
        }
        if (url && this.isUrlAllowed(url)) {
          await page.goto(url, {
            waitUntil: 'networkidle2',
            timeout: this.config.timeout,
          });
        }
      } catch (err) {
        this.ctx.logger.error('Failed to create browser session', {
          error: err instanceof Error ? err.message : String(err),
        });
        throw err;
      }
    }
    const session = {
      id: sessionId,
      browser,
      page,
      context,
      url: url ?? 'about:blank',
      createdAt: Date.now(),
      lastAction: Date.now(),
      proxy: proxyUrl ?? this.config.proxyUrl,
    };
    this.sessions.set(sessionId, session);
    this.ctx.logger.debug('Browser session created', { sessionId, url, proxy: session.proxy });
    return sessionId;
  }
  /** Navigate to URL */
  async navigate(sessionId, options) {
    if (!this.isUrlAllowed(options.url)) {
      return { success: false, error: `URL not allowed: ${options.url}` };
    }
    const session = this.sessions.get(sessionId);
    if (!session) return { success: false, error: 'Session not found' };
    session.url = options.url;
    session.lastAction = Date.now();
    if (session.page) {
      try {
        await session.page.goto(options.url, {
          waitUntil: options.waitUntil ?? 'networkidle2',
          timeout: options.timeout ?? this.config.timeout,
        });
        return { success: true, data: { url: options.url, title: await session.page.title() } };
      } catch (err) {
        return { success: false, error: err instanceof Error ? err.message : String(err) };
      }
    }
    return { success: true, data: { url: options.url } };
  }
  /** Execute a page action */
  async executeAction(sessionId, action) {
    const session = this.sessions.get(sessionId);
    if (!session) return { success: false, error: 'Session not found' };
    session.lastAction = Date.now();
    const page = session.page;
    if (!page) return { success: true, data: { note: 'No browser engine, action logged only' } };
    try {
      switch (action.type) {
        case 'click':
          if (action.waitForNavigation) {
            await Promise.all([
              page.waitForNavigation({ timeout: this.config.timeout }),
              page.click(action.selector),
            ]);
          } else {
            await page.click(action.selector);
          }
          return { success: true };
        case 'type':
          await page.type(action.selector, action.text, { delay: action.delay ?? 0 });
          return { success: true };
        case 'select':
          await page.select(action.selector, action.value);
          return { success: true };
        case 'screenshot': {
          const buf = await page.screenshot({
            fullPage: action.fullPage ?? false,
            type: action.format ?? 'png',
          });
          return { success: true, screenshot: Buffer.from(buf) };
        }
        case 'extract': {
          if (action.extractAll) {
            const data = await page.$$eval(
              action.selector,
              (els, attr) =>
                els.map((el) => (attr ? el.getAttribute(attr) : el.textContent?.trim())),
              action.attribute,
            );
            return { success: true, data };
          }
          const data = action.attribute
            ? await page.$eval(
                action.selector,
                (el, attr) => el.getAttribute(attr),
                action.attribute,
              )
            : await page.$eval(action.selector, (el) => el.textContent?.trim());
          return { success: true, data };
        }
        case 'evaluate': {
          const result = await page.evaluate(action.script);
          return { success: true, data: result };
        }
        case 'waitFor':
          await page.waitForSelector(action.selector, {
            timeout: action.timeout ?? this.config.timeout,
          });
          return { success: true };
        case 'scroll': {
          const amount = action.amount ?? 500;
          const y = action.direction === 'down' ? amount : -amount;
          await page.evaluate((dy) => window.scrollBy(0, dy), y);
          return { success: true };
        }
        case 'hover':
          await page.hover(action.selector);
          return { success: true };
        case 'press':
          await page.keyboard.press(action.key);
          return { success: true };
        case 'back':
          await page.goBack();
          session.url = page.url();
          return { success: true, data: { url: session.url } };
        case 'forward':
          await page.goForward();
          session.url = page.url();
          return { success: true, data: { url: session.url } };
        case 'reload':
          await page.reload({ waitUntil: 'networkidle2' });
          return { success: true };
        case 'pdf': {
          const pdfBuf = await page.pdf({ format: action.format ?? 'A4' });
          return { success: true, data: Buffer.from(pdfBuf).toString('base64') };
        }
        default:
          return { success: false, error: `Unknown action: ${action.type}` };
      }
    } catch (err) {
      return { success: false, error: err instanceof Error ? err.message : String(err) };
    }
  }
  /** Extract full page content */
  async extractPageContent(sessionId) {
    const session = this.sessions.get(sessionId);
    if (!session?.page) return null;
    const page = session.page;
    try {
      return await page.evaluate(() => {
        const getText = (el) => el.textContent?.trim() ?? '';
        return {
          title: document.title,
          url: location.href,
          text: document.body?.innerText?.slice(0, 50000) ?? '',
          links: Array.from(document.querySelectorAll('a[href]'))
            .map((a) => ({
              text: getText(a),
              href: a.href,
            }))
            .filter((l) => l.href.startsWith('http'))
            .slice(0, 200),
          images: Array.from(document.querySelectorAll('img[src]'))
            .map((img) => ({
              src: img.src,
              alt: img.alt,
            }))
            .slice(0, 100),
          meta: Object.fromEntries(
            Array.from(document.querySelectorAll('meta[name], meta[property]'))
              .map((m) => [m.name || m.getAttribute('property') || '', m.content])
              .filter(([k]) => k),
          ),
        };
      });
    } catch (err) {
      this.ctx.logger.error('Extract content failed', {
        error: err instanceof Error ? err.message : String(err),
      });
      return null;
    }
  }
  /** Close a browser session */
  async closeSession(sessionId) {
    const session = this.sessions.get(sessionId);
    if (!session) return;
    try {
      if (session.page) await session.page.close?.();
      if (session.context) await session.context.close?.();
      if (session.browser) await session.browser.close?.();
    } catch {
      // ignore cleanup errors
    }
    this.sessions.delete(sessionId);
    this.ctx.logger.debug('Browser session closed', { sessionId });
  }
  /** List all active sessions */
  listSessions() {
    return Array.from(this.sessions.values()).map((s) => ({
      id: s.id,
      url: s.url,
      proxy: s.proxy,
      createdAt: s.createdAt,
    }));
  }
}
exports.default = new BrowserAutomationFeature();
