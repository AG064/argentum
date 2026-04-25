/**
 * Webhooks Feature
 *
 * Receives and dispatches webhook events with signature verification,
 * retry logic, and event routing.
 */

import { createHmac } from 'crypto';
import { lookup } from 'dns/promises';
import { isIP } from 'net';

import {
  type FeatureModule,
  type FeatureContext,
  type FeatureMeta,
  type HealthStatus,
} from '../../core/plugin-loader';

/** Webhook configuration */
export interface WebhooksConfig {
  enabled: boolean;
  port: number;
  path: string;
  secret: string;
  maxRetries: number;
  retryDelayMs: number;
}

/** Webhook event */
export interface WebhookEvent {
  id: string;
  source: string;
  type: string;
  payload: Record<string, unknown>;
  headers: Record<string, string>;
  timestamp: number;
  signature?: string;
}

/** Webhook subscription */
export interface WebhookSubscription {
  id: string;
  url: string;
  events: string[];
  secret: string;
  active: boolean;
  createdAt: number;
}

/** Webhook handler function */
export type WebhookHandler = (event: WebhookEvent) => Promise<void>;

/**
 * Webhooks feature — receive and dispatch webhook events.
 *
 * Provides HTTP endpoint for receiving webhooks, signature verification,
 * event routing to handlers, and outbound webhook delivery with retries.
 */
class WebhooksFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'webhooks',
    version: '0.1.0',
    description: 'Webhook receiver and dispatcher with signature verification',
    dependencies: [],
  };

  private config: WebhooksConfig = {
    enabled: false,
    port: 3002,
    path: '/webhooks',
    secret: '',
    maxRetries: 3,
    retryDelayMs: 1000,
  };
  private ctx!: FeatureContext;
  private handlers: Map<string, WebhookHandler[]> = new Map();
  private subscriptions: Map<string, WebhookSubscription> = new Map();

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...(config as Partial<WebhooksConfig>) };
  }

  async start(): Promise<void> {
    this.ctx.logger.info('Webhooks active', {
      port: this.config.port,
      path: this.config.path,
    });
  }

  async stop(): Promise<void> {
    this.handlers.clear();
    this.subscriptions.clear();
  }

  async healthCheck(): Promise<HealthStatus> {
    return {
      healthy: true,
      details: {
        subscriptions: this.subscriptions.size,
        handlerTypes: this.handlers.size,
      },
    };
  }

  /** Register a handler for a specific event type */
  on(eventType: string, handler: WebhookHandler): void {
    const handlers = this.handlers.get(eventType) ?? [];
    handlers.push(handler);
    this.handlers.set(eventType, handlers);
  }

  /** Verify webhook signature */
  verifySignature(payload: string, signature: string, secret: string): boolean {
    const expected = createHmac('sha256', secret).update(payload).digest('hex');
    const provided = signature.replace('sha256=', '');
    return expected === provided;
  }

  /** Process an incoming webhook event */
  async processEvent(event: WebhookEvent): Promise<void> {
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
  subscribe(url: string, events: string[], secret: string): WebhookSubscription {
    if (!this.validateUrl(url)) {
      throw new Error('Webhook subscription URL must be a public HTTPS URL');
    }

    const id = `sub_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
    const subscription: WebhookSubscription = {
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
  async dispatch(event: WebhookEvent): Promise<void> {
    for (const [, sub] of this.subscriptions) {
      if (!sub.active) continue;
      if (!sub.events.includes(event.type) && !sub.events.includes('*')) continue;

      await this.deliverWithRetry(sub, event);
    }
  }

  private isInternalHostname(host: string): boolean {
    if (!host) return false;
    const h = host.toLowerCase().replace(/^\[|\]$/g, '');
    if (h === 'localhost' || h.endsWith('.localhost')) return true;
    if (h.endsWith('.local')) return true;
    if (isIP(h) === 4) return this.isPrivateIpv4(h);
    if (isIP(h) === 6) return this.isPrivateIpv6(h);
    return false;
  }

  private isPrivateIpv4(host: string): boolean {
    const parts = host.split('.');
    const [a, b] = parts.map((p) => parseInt(p, 10));
    if (parts.length !== 4 || parts.some((p) => p === '' || Number.isNaN(parseInt(p, 10)))) {
      return true;
    }

    if (a === 0 || a === 10 || a === 127) return true;
    if (a === 100 && b !== undefined && b >= 64 && b <= 127) return true;
    if (a === 169 && b === 254) return true;
    if (a === 172 && b !== undefined && b >= 16 && b <= 31) return true;
    if (a === 192 && (b === 0 || b === 168)) return true;
    if (a === 198 && b !== undefined && (b === 18 || b === 19 || b === 51)) return true;
    if (a === 203 && b === 0) return true;
    if (a !== undefined && a >= 224) return true;
    return false;
  }

  private isPrivateIpv6(host: string): boolean {
    if (host === '::' || host === '::1') return true;
    if (host.startsWith('fc') || host.startsWith('fd')) return true;
    if (host.startsWith('fe80:')) return true;
    if (host.startsWith('ff')) return true;
    if (host.startsWith('2001:db8:')) return true;
    if (host.startsWith('::ffff:')) {
      const ipv4 = host.slice('::ffff:'.length);
      return isIP(ipv4) === 4 ? this.isPrivateIpv4(ipv4) : true;
    }
    return false;
  }

  private validateUrl(u: string): boolean {
    try {
      const parsed = new URL(u);
      if (parsed.protocol !== 'https:') return false;
      if (parsed.username || parsed.password) return false;
      const host = parsed.hostname;
      if (this.isInternalHostname(host)) return false;
      return true;
    } catch {
      return false;
    }
  }

  private async validateDeliveryUrl(u: string): Promise<boolean> {
    if (!this.validateUrl(u)) return false;

    const parsed = new URL(u);
    const host = parsed.hostname.replace(/^\[|\]$/g, '');
    if (isIP(host)) return true;

    try {
      const records = await lookup(host, { all: true, verbatim: true });
      return records.every((record) => !this.isInternalHostname(record.address));
    } catch (err) {
      this.ctx.logger.warn('Blocked webhook delivery because hostname could not be resolved', {
        url: u,
        error: err instanceof Error ? err.message : String(err),
      });
      return false;
    }
  }

  /** Deliver webhook with retry logic */
  private async deliverWithRetry(
    sub: WebhookSubscription,
    event: WebhookEvent,
    attempt = 1,
  ): Promise<void> {
    try {
      if (!(await this.validateDeliveryUrl(sub.url))) {
        this.ctx.logger.warn('Blocked webhook delivery to internal or invalid URL', {
          url: sub.url,
        });
        return;
      }

      const body = JSON.stringify(event);
      const signature = createHmac('sha256', sub.secret).update(body).digest('hex');

      const response = await fetch(sub.url, {
        method: 'POST',
        redirect: 'manual',
        headers: {
          'Content-Type': 'application/json',
          'X-Webhook-Signature': `sha256=${signature}`,
          'X-Webhook-Event': event.type,
          'X-Webhook-Id': event.id,
        },
        body,
      });

      if (response.status >= 300 && response.status < 400) {
        const location = response.headers.get('location');
        const redirectTarget = location ? new URL(location, sub.url).toString() : '';
        if (!redirectTarget || !(await this.validateDeliveryUrl(redirectTarget))) {
          this.ctx.logger.warn('Blocked webhook redirect to internal or invalid URL', {
            url: sub.url,
            redirectTarget,
          });
        }
        return;
      }

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
  unsubscribe(id: string): boolean {
    return this.subscriptions.delete(id);
  }
}

export default new WebhooksFeature();
