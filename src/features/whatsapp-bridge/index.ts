/**
 * WhatsApp Bridge Feature
 *
 * WhatsApp Business API integration for sending/receiving messages.
 * Provides webhook handling for incoming messages and structured outbound sending.
 * All activity is logged to SQLite.
 */

import { mkdirSync, existsSync } from 'fs';
import { dirname, resolve } from 'path';

import Database from 'better-sqlite3';

import {
  type FeatureModule,
  type FeatureContext,
  type FeatureMeta,
  type HealthStatus,
} from '../../core/plugin-loader';

/** WhatsApp configuration */
export interface WhatsAppConfig {
  apiToken: string;
  phoneNumberId: string;
  businessId?: string;
  webhookSecret?: string; // For verifying webhook signatures
}

/** WhatsApp message (incoming/outgoing) */
export interface WhatsAppMessage {
  id: string;
  from: string; // Phone number with WhatsApp ID
  to: string; // Business phone number ID or contact
  body: string;
  type: 'text' | 'image' | 'audio' | 'video' | 'document' | 'location' | 'contacts';
  timestamp: number;
  status: 'sent' | 'delivered' | 'read' | 'failed';
  mediaUrl?: string; // For media messages
  mediaId?: string;
  metadata?: Record<string, unknown>;
}

/** Webhook payload (from WhatsApp) */
export interface WebhookPayload {
  object: string;
  entry: Array<{
    id: string;
    changes: Array<{
      value: {
        messaging_product: string;
        metadata: {
          display_phone_number: string;
          phone_number_id: string;
        };
        contacts?: Array<{
          profile: { name: string };
          wa_id: string;
        }>;
        messages?: Array<{
          id: string;
          from: string;
          timestamp: string;
          type: string;
          text?: { body: string };
          image?: { id: string; mime_type?: string };
          audio?: { id: string; mime_type?: string };
          video?: { id: string; mime_type?: string };
          document?: { id: string; filename?: string; mime_type?: string };
          location?: { latitude: string; longitude: string; name?: string; address?: string };
          contacts?: Array<{
            name: { formatted_name: string };
            phones: Array<{ phone: string }>;
          }>;
        }>;
        statuses?: Array<{
          id: string;
          status: 'sent' | 'delivered' | 'read' | 'failed';
          timestamp: string;
          error?: { code: number; title: string; message_data?: unknown };
        }>;
      };
      field: string;
    }>;
  }>;
}

/** Feature configuration */
export interface WhatsAppBridgeConfig {
  dbPath?: string;
  webhookPath?: string;
  maxMessageHistory?: number;
  autoAck?: boolean; // Auto-acknowledge inbound messages
}

/**
 * WhatsAppBridge — WhatsApp Business API integration.
 *
 * Provides:
 * - API token and phone number configuration
 * - Outbound message sending structure
 * - Inbound webhook parsing and handling
 * - Message history logging
 * - Status update handling
 *
 * Note: Real API calls are not implemented (would require HTTPS endpoint for webhooks).
 */
class WhatsAppBridgeFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'whatsapp-bridge',
    version: '0.0.2',
    description: 'WhatsApp Business API integration with webhook support',
    dependencies: [],
  };

  private config: Required<WhatsAppBridgeConfig>;
  private ctx!: FeatureContext;
  private db!: Database.Database;
  private apiToken: string = '';
  private phoneNumberId: string = '';
  private businessId: string = '';
  private webhookSecret: string = '';
  private autoAck: boolean = true;
  private messageHandlers: Array<(msg: WhatsAppMessage) => Promise<void>> = [];

  constructor() {
    this.config = {
      dbPath: './data/whatsapp-bridge.db',
      webhookPath: '/webhooks/whatsapp',
      maxMessageHistory: 10000,
      autoAck: true,
    };
  }

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = {
      dbPath: (config['dbPath'] as string) ?? this.config['dbPath'],
      webhookPath: (config['webhookPath'] as string) ?? this.config['webhookPath'],
      maxMessageHistory:
        (config['maxMessageHistory'] as number) ?? this.config['maxMessageHistory'],
      autoAck: (config['autoAck'] as boolean) ?? this.config['autoAck'],
    };

    this.initDatabase();

    // Load persisted configuration if exists
    const cfg = this.db.prepare('SELECT * FROM config WHERE key = ?').get('whatsapp') as
      | { value: string }
      | undefined;
    if (cfg) {
      const parsed = JSON.parse(cfg.value);
      this.apiToken = parsed.apiToken;
      this.phoneNumberId = parsed.phoneNumberId;
      this.webhookSecret = parsed.webhookSecret;
    }
  }

  async start(): Promise<void> {
    this.ctx.logger.info('WhatsAppBridge active', {
      dbPath: this.config.dbPath,
      configured: !!this.apiToken,
      webhookPath: this.config.webhookPath,
    });
  }

  async stop(): Promise<void> {
    if (this.db) {
      this.db.close();
      this.ctx.logger.info('WhatsAppBridge stopped');
    }
  }

  async healthCheck(): Promise<HealthStatus> {
    try {
      const msgCount = (
        this.db.prepare('SELECT COUNT(*) as c FROM messages').get() as { c: number }
      ).c;
      const inboundToday = (
        this.db
          .prepare(
            `
        SELECT COUNT(*) as c FROM messages
        WHERE direction = 'inbound' AND timestamp > ?
      `,
          )
          .get(Date.now() - 86400000) as { c: number }
      ).c;

      return {
        healthy: true,
        details: {
          totalMessages: msgCount,
          inboundToday,
          configured: !!this.apiToken,
        },
      };
    } catch (err) {
      return {
        healthy: false,
        message: err instanceof Error ? err.message : String(err),
      };
    }
  }

  /**
   * Configure WhatsApp Business API credentials.
   *
   * @param apiToken - Permanent access token
   * @param phoneNumberId - WhatsApp Business phone number ID
   * @param webhookSecret - Optional secret for webhook verification
   * @param businessId - Optional business ID
   */
  configure(
    apiToken: string,
    phoneNumberId: string,
    webhookSecret?: string,
    businessId?: string,
  ): void {
    this.apiToken = apiToken;
    this.phoneNumberId = phoneNumberId;
    this.webhookSecret = webhookSecret ?? '';
    this.businessId = businessId ?? '';

    const value = JSON.stringify({
      apiToken: this.apiToken,
      phoneNumberId: this.phoneNumberId,
      webhookSecret: this.webhookSecret,
      businessId: this.businessId,
    });

    this.db
      .prepare('INSERT OR REPLACE INTO config (key, value) VALUES (?, ?)')
      .run('whatsapp', value);

    this.ctx.logger.info('WhatsApp configured', { phoneNumberId });
  }

  /**
   * Check if WhatsApp is configured.
   */
  isConfigured(): boolean {
    return !!(this.apiToken && this.phoneNumberId);
  }

  /**
   * Register a handler for incoming messages.
   *
   * @param handler - Async function called for each inbound message
   */
  onMessage(handler: (msg: WhatsAppMessage) => Promise<void>): void {
    this.messageHandlers.push(handler);
  }

  /**
   * Send a WhatsApp message (stub - would call WhatsApp API in real implementation).
   *
   * @param to - Recipient WhatsApp ID (phone number with country code)
   * @param body - Text message
   * @param type - Message type (default text)
   * @returns Sent WhatsAppMessage with status 'sent'
   */
  async send(
    to: string,
    body: string,
    type: WhatsAppMessage['type'] = 'text',
  ): Promise<WhatsAppMessage> {
    if (!this.isConfigured()) {
      throw new Error('WhatsApp not configured. Call configure() first.');
    }

    const msgId = `waba-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
    const now = Date.now();

    // In real implementation, this would POST to WhatsApp Cloud API
    // https://graph.facebook.com/v18.0/{phone-number-id}/messages

    const message: WhatsAppMessage = {
      id: msgId,
      from: this.phoneNumberId,
      to,
      body,
      type,
      timestamp: now,
      status: 'sent',
    };

    this.logMessage({ ...message, direction: 'outbound' });
    this.ctx.logger.info('WhatsApp message sent (stub)', { to, type, messageId: msgId });

    return message;
  }

  /**
   * Get message history.
   *
   * @param limit - Max messages to return
   * @param direction - Filter by 'inbound', 'outbound', or undefined for all
   * @returns Messages ordered by timestamp descending
   */
  getMessages(limit: number = 100, direction?: 'inbound' | 'outbound'): WhatsAppMessage[] {
    let query = 'SELECT * FROM messages';
    const params: (string | number)[] = [];

    if (direction) {
      query += ' WHERE direction = ?';
      params.push(direction);
    }

    query += ' ORDER BY timestamp DESC LIMIT ?';
    params.push(limit);

    const rows = this.db.prepare(query).all(...params) as Array<{
      id: string;
      direction: string;
      from_addr: string;
      to_addr: string;
      body: string;
      type: string;
      timestamp: number;
      status: string;
      media_url: string | null;
      media_id: string | null;
      metadata: string | null;
    }>;

    return rows.map((r) => ({
      id: r.id,
      from: r.from_addr,
      to: r.to_addr,
      body: r.body,
      type: r.type as WhatsAppMessage['type'],
      timestamp: r.timestamp,
      status: r.status as WhatsAppMessage['status'],
      mediaUrl: r.media_url ?? undefined,
      mediaId: r.media_id ?? undefined,
      metadata: r.metadata ? JSON.parse(r.metadata) : undefined,
    }));
  }

  /**
   * Handle an incoming webhook from WhatsApp.
   *
   * @param payload - Raw webhook JSON payload
   * @param verifySignature - Optional signature verification function (returns boolean)
   * @returns true if webhook processed successfully
   */
  async webhook(
    payload: WebhookPayload,
    _verifySignature?: (sig: string, body: string) => boolean,
  ): Promise<boolean> {
    // In real implementation, verify X-Hub-Signature-256 header against webhookSecret

    try {
      for (const entry of payload.entry) {
        for (const change of entry.changes) {
          const value = change.value;

          // Handle messages
          if (value.messages) {
            for (const msg of value.messages) {
              const waMsg: WhatsAppMessage = {
                id: msg.id,
                from: msg.from,
                to: value.metadata.phone_number_id,
                body: msg.text?.body ?? '',
                type: msg.type as WhatsAppMessage['type'],
                timestamp: parseInt(msg.timestamp, 10),
                status: 'sent', // inbound messages don't have status
                mediaId: msg.image?.id ?? msg.audio?.id ?? msg.video?.id ?? msg.document?.id,
                metadata: {
                  contactName: value.contacts?.[0]?.profile.name,
                  mimeType:
                    msg.image?.mime_type ??
                    msg.audio?.mime_type ??
                    msg.video?.mime_type ??
                    msg.document?.mime_type,
                  filename: msg.document?.filename,
                  location: msg.location
                    ? {
                        lat: msg.location.latitude,
                        lng: msg.location.longitude,
                        name: msg.location.name,
                        address: msg.location.address,
                      }
                    : undefined,
                },
              };

              this.logMessage({ ...waMsg, direction: 'inbound' });

              // Invoke handlers
              for (const handler of this.messageHandlers) {
                try {
                  await handler(waMsg);
                } catch (err) {
                  this.ctx.logger.error('Webhook handler error', {
                    error: err instanceof Error ? err.message : String(err),
                  });
                }
              }
            }
          }

          // Handle status updates
          if (value.statuses) {
            for (const status of value.statuses) {
              this.db
                .prepare(
                  `
                UPDATE messages SET status = ? WHERE id = ?
              `,
                )
                .run(status.status, status.id);

              if (status.error) {
                this.ctx.logger.warn('Message status error', {
                  messageId: status.id,
                  status: status.status,
                  error: status.error.title,
                });
              }
            }
          }
        }
      }

      this.ctx.logger.debug('Webhook processed', { entryCount: payload.entry.length });
      return true;
    } catch (err) {
      this.ctx.logger.error('Webhook processing failed', {
        error: err instanceof Error ? err.message : String(err),
      });
      return false;
    }
  }

  /**
   * Get statistics about message activity.
   */
  getStats(): { total: number; inbound: number; outbound: number } {
    const total = (this.db.prepare('SELECT COUNT(*) as c FROM messages').get() as { c: number }).c;
    const inbound = (
      this.db.prepare("SELECT COUNT(*) as c FROM messages WHERE direction = 'inbound'").get() as {
        c: number;
      }
    ).c;
    const outbound = total - inbound;
    return { total, inbound, outbound };
  }

  /** Log a message to database */
  private logMessage(msg: WhatsAppMessage & { direction: 'inbound' | 'outbound' }): void {
    const stmt = this.db.prepare(`
      INSERT INTO messages (id, direction, from_addr, to_addr, body, type, timestamp, status, media_url, media_id, metadata)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    `);
    stmt.run(
      msg.id,
      msg.direction,
      msg.from,
      msg.to,
      msg.body,
      msg.type,
      msg.timestamp,
      msg.status,
      msg.mediaUrl ?? null,
      msg.mediaId ?? null,
      msg.metadata ? JSON.stringify(msg.metadata) : null,
    );

    // Enforce max message history
    if (this.config.maxMessageHistory > 0) {
      const count = (this.db.prepare('SELECT COUNT(*) as c FROM messages').get() as { c: number })
        .c;
      if (count > this.config.maxMessageHistory) {
        const deleteCount = count - this.config.maxMessageHistory;
        this.db
          .prepare(
            'DELETE FROM messages WHERE id IN (SELECT id FROM messages ORDER BY timestamp ASC LIMIT ?)',
          )
          .run(deleteCount);
      }
    }
  }

  /** Initialize database and create tables */
  private initDatabase(): void {
    const fullPath = resolve(this.config.dbPath);
    if (!existsSync(dirname(fullPath))) {
      mkdirSync(dirname(fullPath), { recursive: true });
    }

    this.db = new Database(fullPath);
    this.db.pragma('journal_mode = WAL');
    this.db.pragma('synchronous = NORMAL');
    this.db.pragma('foreign_keys = ON');

    this.db.exec(`
      CREATE TABLE IF NOT EXISTS config (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
      );

      CREATE TABLE IF NOT EXISTS messages (
        id TEXT PRIMARY KEY,
        direction TEXT NOT NULL CHECK(direction IN ('inbound', 'outbound')),
        from_addr TEXT NOT NULL,
        to_addr TEXT NOT NULL,
        body TEXT NOT NULL,
        type TEXT NOT NULL,
        timestamp INTEGER NOT NULL,
        status TEXT NOT NULL,
        media_url TEXT,
        media_id TEXT,
        metadata TEXT,
        FOREIGN KEY (id) references messages(id) ON DELETE CASCADE
      );

      CREATE INDEX IF NOT EXISTS idx_messages_timestamp ON messages(timestamp DESC);
      CREATE INDEX IF NOT EXISTS idx_messages_direction ON messages(direction);
      CREATE INDEX IF NOT EXISTS idx_messages_status ON messages(status);
    `);
  }
}

export default new WhatsAppBridgeFeature();
