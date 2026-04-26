'use strict';
/**
 * Webhooks Feature
 *
 * Receives and dispatches webhook events with signature verification,
 * retry logic, and event routing.
 */
Object.defineProperty(exports, '__esModule', { value: true });
const crypto_1 = require('crypto');
/**
 * Webhooks feature — receive and dispatch webhook events.
 *
 * Provides HTTP endpoint for receiving webhooks, signature verification,
 * event routing to handlers, and outbound webhook delivery with retries.
 */
class WebhooksFeature {
  constructor() {
    this.meta = {
      name: 'webhooks',
      version: '0.0.2',
      description: 'Webhook receiver and dispatcher with signature verification',
      dependencies: [],
    };
    this.config = {
      enabled: false,
      port: 3002,
      path: '/webhooks',
      secret: '',
      maxRetries: 3,
      retryDelayMs: 1000,
    };
    this.handlers = new Map();
    this.subscriptions = new Map();
  }
  async init(config, context) {
    this.ctx = context;
    this.config = { ...this.config, ...config };
  }
  async start() {
    this.ctx.logger.info('Webhooks active', {
      port: this.config.port,
      path: this.config.path,
    });
  }
  async stop() {
    this.handlers.clear();
    this.subscriptions.clear();
  }
  async healthCheck() {
    return {
      healthy: true,
      details: {
        subscriptions: this.subscriptions.size,
        handlerTypes: this.handlers.size,
      },
    };
  }
  /** Register a handler for a specific event type */
  on(eventType, handler) {
    const handlers = this.handlers.get(eventType) ?? [];
    handlers.push(handler);
    this.handlers.set(eventType, handlers);
  }
  /** Verify webhook signature */
  verifySignature(payload, signature, secret) {
    const expected = (0, crypto_1.createHmac)('sha256', secret).update(payload).digest('hex');
    const provided = signature.replace('sha256=', '');
    return expected === provided;
  }
  /** Process an incoming webhook event */
  async processEvent(event) {
    this.ctx.logger.info('Processing webhook event', {
      source: event.source,
      type: event.type,
    });
    const handlers = this.handlers.get(event.type) ?? this.handlers.get('*') ?? [];
    for (const handler of handlers) {
      try {
        await handler(event);
      } catch (err) {
        this.ctx.logger.error('Webhook handler error', {
          type: event.type,
          error: err instanceof Error ? err.message : String(err),
        });
      }
    }
  }
  /** Subscribe to outbound webhooks */
  subscribe(url, events, secret) {
    const id = `sub_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
    const subscription = {
      id,
      url,
      events,
      secret,
      active: true,
      createdAt: Date.now(),
    };
    this.subscriptions.set(id, subscription);
    return subscription;
  }
  /** Dispatch event to all matching subscribers */
  async dispatch(event) {
    for (const [, sub] of this.subscriptions) {
      if (!sub.active) continue;
      if (!sub.events.includes(event.type) && !sub.events.includes('*')) continue;
      await this.deliverWithRetry(sub, event);
    }
  }
  isInternalHostname(host) {
    if (!host) return false;
    const h = host.toLowerCase();
    if (h === 'localhost' || h === '::1') return true;
    if (h.endsWith('.local')) return true;
    // IPv4 checks
    const parts = h.split('.');
    if (parts.length === 4) {
      const [a, b] = parts.map((p) => parseInt(p, 10));
      if (a === 127) return true;
      if (a === 10) return true;
      if (a === 192 && b === 168) return true;
      if (a === 169 && b === 254) return true;
      if (a === 172 && b >= 16 && b <= 31) return true;
    }
    return false;
  }
  validateUrl(u) {
    try {
      const parsed = new URL(u);
      const host = parsed.hostname;
      if (this.isInternalHostname(host)) return false;
      return true;
    } catch {
      return false;
    }
  }
  /** Deliver webhook with retry logic */
  async deliverWithRetry(sub, event, attempt = 1) {
    try {
      if (!this.validateUrl(sub.url)) {
        this.ctx.logger.warn('Blocked webhook delivery to internal or invalid URL', {
          url: sub.url,
        });
        return;
      }
      const body = JSON.stringify(event);
      const signature = (0, crypto_1.createHmac)('sha256', sub.secret).update(body).digest('hex');
      const response = await fetch(sub.url, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          'X-Webhook-Signature': `sha256=${signature}`,
          'X-Webhook-Event': event.type,
          'X-Webhook-Id': event.id,
        },
        body,
      });
      if (!response.ok && attempt < this.config.maxRetries) {
        await new Promise((resolve) => setTimeout(resolve, this.config.retryDelayMs * attempt));
        await this.deliverWithRetry(sub, event, attempt + 1);
      }
    } catch (err) {
      if (attempt < this.config.maxRetries) {
        await new Promise((resolve) => setTimeout(resolve, this.config.retryDelayMs * attempt));
        await this.deliverWithRetry(sub, event, attempt + 1);
      } else {
        this.ctx.logger.error('Webhook delivery failed after retries', {
          url: sub.url,
          attempts: attempt,
        });
      }
    }
  }
  /** Unsubscribe */
  unsubscribe(id) {
    return this.subscriptions.delete(id);
  }
}
exports.default = new WebhooksFeature();
