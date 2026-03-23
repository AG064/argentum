/**
 * Browser Automation Feature
 *
 * Full Puppeteer/Playwright integration for headless browser control.
 * Navigate, click, type, screenshot, extract content, multi-tab support,
 * proxy/VPN support, domain allowlisting.
 */

import { FeatureModule, FeatureContext, FeatureMeta, HealthStatus } from '../../core/plugin-loader';

/** Browser automation configuration */
export interface BrowserAutomationConfig {
  enabled: boolean;
  engine: 'puppeteer' | 'playwright';
  headless: boolean;
  timeout: number;
  maxPages: number;
  proxyUrl?: string;
  allowedDomains: string[];
  blockedDomains: string[];
  userAgent?: string;
  viewportWidth: number;
  viewportHeight: number;
}

/** Navigation options */
export interface NavigateOptions {
  url: string;
  waitUntil?: 'load' | 'domcontentloaded' | 'networkidle0' | 'networkidle2';
  timeout?: number;
}

/** Page action types */
export type PageAction =
  | { type: 'click'; selector: string; waitForNavigation?: boolean }
  | { type: 'type'; selector: string; text: string; delay?: number }
  | { type: 'select'; selector: string; value: string }
  | { type: 'screenshot'; path?: string; fullPage?: boolean; format?: 'png' | 'jpeg' }
  | { type: 'extract'; selector: string; attribute?: string; extractAll?: boolean }
  | { type: 'evaluate'; script: string }
  | { type: 'waitFor'; selector: string; timeout?: number }
  | { type: 'scroll'; direction: 'up' | 'down'; amount?: number }
  | { type: 'hover'; selector: string }
  | { type: 'press'; key: string }
  | { type: 'back' }
  | { type: 'forward' }
  | { type: 'reload' }
  | { type: 'pdf'; path?: string; format?: string };

/** Action result */
export interface ActionResult {
  success: boolean;
  data?: unknown;
  error?: string;
  screenshot?: Buffer;
}

/** Extracted page content */
export interface PageContent {
  title: string;
  url: string;
  text: string;
  links: Array<{ text: string; href: string }>;
  images: Array<{ src: string; alt: string }>;
  meta: Record<string, string>;
}

/** Page session */
interface PageSession {
  id: string;
  browser: unknown;
  page: unknown;
  context: unknown;
  url: string;
  createdAt: number;
  lastAction: number;
  proxy?: string;
}

/**
 * Browser Automation feature — full headless browser control.
 *
 * Supports Puppeteer and Playwright engines with proxy/VPN,
 * multi-tab sessions, content extraction, and screenshots.
 */
class BrowserAutomationFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'browser-automation',
    version: '0.2.0',
    description: 'Headless browser with Puppeteer/Playwright, proxy, multi-tab',
    dependencies: [],
  };

  private config: BrowserAutomationConfig = {
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
  private ctx!: FeatureContext;
  private sessions: Map<string, PageSession> = new Map();
  private engine: unknown = null;

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...(config as Partial<BrowserAutomationConfig>) };
  }

  async start(): Promise<void> {
    try {
      if (this.config.engine === 'playwright') {
        this.engine = await import('playwright');
      } else {
        this.engine = await import('puppeteer');
      }
      this.ctx.logger.info(`Browser engine loaded: ${this.config.engine}`);
    } catch {
      this.ctx.logger.warn(
        `${this.config.engine} not installed — browser features will be limited to session tracking`,
      );
    }
  }

  async stop(): Promise<void> {
    for (const [id] of this.sessions) await this.closeSession(id);
  }

  async healthCheck(): Promise<HealthStatus> {
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
  isUrlAllowed(url: string): boolean {
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
  private async launchBrowser(proxyUrl?: string): Promise<unknown> {
    const proxy = proxyUrl ?? this.config.proxyUrl;
    if (!this.engine) throw new Error('Browser engine not installed');

    const engine = this.engine as Record<string, Function>;
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
  async createSession(url?: string, proxyUrl?: string): Promise<string> {
    if (this.sessions.size >= this.config.maxPages) {
      throw new Error(`Maximum page limit (${this.config.maxPages}) reached`);
    }

    const sessionId = `session_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
    let browser: unknown = null;
    let page: unknown = null;
    let context: unknown = null;

    if (this.engine) {
      try {
        browser = await this.launchBrowser(proxyUrl);
        const b = browser as Record<string, Function>;
        if (this.config.engine === 'playwright') {
          context = await b.newContext({
            viewport: { width: this.config.viewportWidth, height: this.config.viewportHeight },
            userAgent: this.config.userAgent,
          });
          page = await (context as Record<string, Function>).newPage();
        } else {
          page = await b.newPage();
          await (page as Record<string, Function>).setViewport({
            width: this.config.viewportWidth,
            height: this.config.viewportHeight,
          });
          if (this.config.userAgent) {
            await (page as Record<string, Function>).setUserAgent(this.config.userAgent);
          }
        }

        if (url && this.isUrlAllowed(url)) {
          await (page as Record<string, Function>).goto(url, {
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

    const session: PageSession = {
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
  async navigate(sessionId: string, options: NavigateOptions): Promise<ActionResult> {
    if (!this.isUrlAllowed(options.url)) {
      return { success: false, error: `URL not allowed: ${options.url}` };
    }
    const session = this.sessions.get(sessionId);
    if (!session) return { success: false, error: 'Session not found' };

    session.url = options.url;
    session.lastAction = Date.now();

    if (session.page) {
      try {
        await (session.page as Record<string, Function>).goto(options.url, {
          waitUntil: options.waitUntil ?? 'networkidle2',
          timeout: options.timeout ?? this.config.timeout,
        });
        return {
          success: true,
          data: {
            url: options.url,
            title: await (session.page as Record<string, Function>).title(),
          },
        };
      } catch (err) {
        return { success: false, error: err instanceof Error ? err.message : String(err) };
      }
    }
    return { success: true, data: { url: options.url } };
  }

  /** Execute a page action */
  async executeAction(sessionId: string, action: PageAction): Promise<ActionResult> {
    const session = this.sessions.get(sessionId);
    if (!session) return { success: false, error: 'Session not found' };
    session.lastAction = Date.now();

    const page = session.page as Record<string, Function> | null;
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
          return { success: true, screenshot: Buffer.from(buf as ArrayBuffer) };
        }

        case 'extract': {
          if (action.extractAll) {
            const data = await page.$$eval(
              action.selector,
              (els: Element[], attr?: string) =>
                els.map((el) => (attr ? el.getAttribute(attr) : el.textContent?.trim())),
              action.attribute,
            );
            return { success: true, data };
          }
          const data = action.attribute
            ? await page.$eval(
                action.selector,
                (el: Element, attr: string) => el.getAttribute(attr),
                action.attribute,
              )
            : await page.$eval(action.selector, (el: Element) => el.textContent?.trim());
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
          await page.evaluate((dy: number) => window.scrollBy(0, dy), y);
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
          return { success: true, data: Buffer.from(pdfBuf as ArrayBuffer).toString('base64') };
        }

        default:
          return { success: false, error: `Unknown action: ${(action as PageAction).type}` };
      }
    } catch (err) {
      return { success: false, error: err instanceof Error ? err.message : String(err) };
    }
  }

  /** Extract full page content */
  async extractPageContent(sessionId: string): Promise<PageContent | null> {
    const session = this.sessions.get(sessionId);
    if (!session?.page) return null;

    const page = session.page as Record<string, Function>;
    try {
      return await page.evaluate(() => {
        const getText = (el: Element) => el.textContent?.trim() ?? '';
        return {
          title: document.title,
          url: location.href,
          text: document.body?.innerText?.slice(0, 50000) ?? '',
          links: Array.from(document.querySelectorAll('a[href]'))
            .map((a) => ({
              text: getText(a),
              href: (a as HTMLAnchorElement).href,
            }))
            .filter((l) => l.href.startsWith('http'))
            .slice(0, 200),
          images: Array.from(document.querySelectorAll('img[src]'))
            .map((img) => ({
              src: (img as HTMLImageElement).src,
              alt: (img as HTMLImageElement).alt,
            }))
            .slice(0, 100),
          meta: Object.fromEntries(
            Array.from(document.querySelectorAll('meta[name], meta[property]'))
              .map((m) => [
                (m as HTMLMetaElement).name ||
                  (m as HTMLMetaElement).getAttribute('property') ||
                  '',
                (m as HTMLMetaElement).content,
              ])
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
  async closeSession(sessionId: string): Promise<void> {
    const session = this.sessions.get(sessionId);
    if (!session) return;

    try {
      if (session.page) await (session.page as Record<string, Function>).close?.();
      if (session.context) await (session.context as Record<string, Function>).close?.();
      if (session.browser) await (session.browser as Record<string, Function>).close?.();
    } catch {
      // ignore cleanup errors
    }
    this.sessions.delete(sessionId);
    this.ctx.logger.debug('Browser session closed', { sessionId });
  }

  /** List all active sessions */
  listSessions(): Array<{ id: string; url: string; proxy?: string; createdAt: number }> {
    return Array.from(this.sessions.values()).map((s) => ({
      id: s.id,
      url: s.url,
      proxy: s.proxy,
      createdAt: s.createdAt,
    }));
  }
}

export default new BrowserAutomationFeature();
