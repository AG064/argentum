/**
 * Slack Integration Feature
 *
 * Slack bot integration using Bolt.js patterns. Supports sending messages
 * and receiving events via Events API and Interactivity. Stub implementation.
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

/** Slack message */
export interface SlackMessage {
  ts: string; // timestamp (unique per channel)
  channel: string;
  user?: string;
  username?: string; // bot username
  text: string;
  blocks?: unknown[]; // Block Kit UI
  attachments?: unknown[];
  threadTs?: string; // Parent thread timestamp
  botId?: string;
}

/** Slack event payload */
export interface SlackEventPayload {
  token: string;
  teamId: string;
  apiAppId: string;
  event: {
    type: string;
    user?: string;
    channel?: string;
    text?: string;
    ts: string;
    threadTs?: string;
    botId?: string;
    [key: string]: unknown;
  };
  type: string;
  eventId: string;
  eventTime: number;
}

/** Slack interaction payload (buttons, modals, etc.) */
export interface SlackInteraction {
  type: string;
  user: { id: string; username: string };
  channel?: { id: string; name: string };
  message?: SlackMessage;
  actions?: Array<{
    actionId: string;
    value: string;
    blockId: string;
  }>;
  submission?: Record<string, unknown>;
  view?: unknown;
}

/** Bot configuration */
export interface SlackBotConfig {
  botToken: string; // xoxb-...
  signingSecret: string; // for Events API verification
  appToken?: string; // xapp-... for Socket Mode
  socketMode?: boolean;
  appLevelToken?: string; // For Enterprise Grid
}

/** Feature configuration */
export interface SlackIntegrationConfig {
  dbPath?: string;
  tokenEnvVar?: string;
  secretEnvVar?: string;
}

/**
 * SlackIntegration — Slack bot with Events API support.
 *
 * Provides:
 * - Token and signing secret configuration
 * - Message sending to channels/DMs
 * - Event handling (message, reaction, etc.)
 * - Interactivity handling (buttons, modals)
 * - Request verification (signing secret)
 *
 * Real Slack connection (HTTP server + Events API) is not implemented.
 */
class SlackIntegrationFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'slack-integration',
    version: '0.0.3',
    description: 'Slack bot integration with message sending and event handling',
    dependencies: [],
  };

  private config: Required<SlackIntegrationConfig>;
  private ctx!: FeatureContext;
  private db!: Database.Database;
  private botToken: string = '';
  private signingSecret: string = '';
  private appToken: string | null = null;
  private socketMode: boolean = false;
  private eventHandlers: Map<string, Array<(event: SlackEventPayload['event']) => Promise<void>>> =
    new Map();
  private interactionHandlers: Array<(interaction: SlackInteraction) => Promise<void>> = [];

  constructor() {
    this.config = {
      dbPath: './data/slack-integration.db',
      tokenEnvVar: 'SLACK_BOT_TOKEN',
      secretEnvVar: 'SLACK_SIGNING_SECRET',
    };
  }

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = {
      dbPath: (config['dbPath'] as string) ?? this.config['dbPath'],
      tokenEnvVar: (config['tokenEnvVar'] as string) ?? this.config['tokenEnvVar'],
      secretEnvVar: (config['secretEnvVar'] as string) ?? this.config['secretEnvVar'],
    };

    this.initDatabase();

    // Load persisted config
    const cfg = this.db.prepare('SELECT * FROM config WHERE key = ?').get('slack') as
      | { value: string }
      | undefined;
    if (cfg) {
      try {
        const parsed = JSON.parse(cfg.value);
        this.botToken = parsed.botToken;
        this.signingSecret = parsed.signingSecret;
        this.appToken = parsed.appToken ?? null;
        this.socketMode = parsed.socketMode ?? false;
      } catch {
        this.ctx.logger.warn('Failed to parse stored Slack config');
      }
    }

    // Check environment variables if not set
    if (!this.botToken && process.env[this.config.tokenEnvVar]) {
      const tok = process.env[this.config.tokenEnvVar];
      if (tok) this.botToken = tok;
    }
    if (!this.signingSecret && process.env[this.config.secretEnvVar]) {
      const sec = process.env[this.config.secretEnvVar];
      if (sec) this.signingSecret = sec;
    }
  }

  async start(): Promise<void> {
    if (!this.botToken) {
      this.ctx.logger.warn('SlackIntegration not configured: missing bot token');
      return;
    }

    this.ctx.logger.info('SlackIntegration active (stub)', {
      socketMode: this.socketMode,
      eventCount: this.eventHandlers.size,
    });
  }

  async stop(): Promise<void> {
    this.ctx.logger.info('SlackIntegration stopped');
  }

  async healthCheck(): Promise<HealthStatus> {
    try {
      const sent = (
        this.db.prepare('SELECT COUNT(*) as c FROM sent_messages').get() as { c: number }
      ).c;
      const received = (
        this.db.prepare('SELECT COUNT(*) as c FROM received_events').get() as { c: number }
      ).c;

      return {
        healthy: true,
        details: {
          configured: !!this.botToken,
          messagesSent: sent,
          eventsReceived: received,
          eventTypes: this.eventHandlers.size,
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
   * Configure Slack bot credentials.
   *
   * @param botToken - Bot User OAuth Token (xoxb-...)
   * @param signingSecret - Signing Secret for Events API verification
   * @param appToken - App-Level Token for Socket Mode (xapp-..., optional)
   * @param socketMode - Use Socket Mode instead of HTTP Events API
   */
  configure(
    botToken: string,
    signingSecret: string,
    appToken?: string,
    socketMode: boolean = false,
  ): void {
    this.botToken = botToken;
    this.signingSecret = signingSecret;
    this.appToken = appToken ?? null;
    this.socketMode = socketMode;

    const value = JSON.stringify({
      botToken: this.botToken,
      signingSecret: this.signingSecret,
      appToken: this.appToken,
      socketMode: this.socketMode,
    });

    this.db.prepare('INSERT OR REPLACE INTO config (key, value) VALUES (?, ?)').run('slack', value);
    this.ctx.logger.info('SlackIntegration configured', { socketMode });
  }

  /**
   * Check if bot is configured.
   */
  isConfigured(): boolean {
    return !!this.botToken && !!this.signingSecret;
  }

  /**
   * Register an event handler.
   *
   * @param eventType - Slack event type (e.g., 'message', 'reaction_added')
   * @param handler - Async function to process event
   */
  onEvent(eventType: string, handler: (event: SlackEventPayload['event']) => Promise<void>): void {
    const handlers = this.eventHandlers.get(eventType) ?? [];
    handlers.push(handler);
    this.eventHandlers.set(eventType, handlers);
  }

  /**
   * Register an interaction handler (buttons, modals, shortcuts).
   */
  onInteraction(handler: (interaction: SlackInteraction) => Promise<void>): void {
    this.interactionHandlers.push(handler);
  }

  /**
   * Send a message to a Slack channel (stub).
   *
   * @param channel - Channel ID (Cxxxxxxxx) or DM (Dxxxxxxxx)
   * @param text - Plain text message (up to 4000 chars)
   * @param blocks - Optional Block Kit UI blocks
   * @returns The sent message with ts
   */
  async sendMessage(channel: string, text: string, blocks?: unknown[]): Promise<SlackMessage> {
    if (!this.isConfigured()) {
      throw new Error('Slack bot not configured. Call configure() first.');
    }

    const ts = `${Date.now()}.${Math.random().toString(36).substr(2, 6)}`;

    // In real implementation, would call Slack API chat.postMessage
    const message: SlackMessage = {
      ts,
      channel,
      username: 'Argentum',
      text,
      blocks,
    };

    this.logSentMessage(message);
    this.ctx.logger.info('Slack message sent (stub)', { channel, length: text.length });

    return message;
  }

  /**
   * Send an ephemeral message (only visible to user).
   */
  async sendEphemeral(
    channel: string,
    user: string,
    _text: string,
    _blocks?: unknown[],
  ): Promise<{ ok: boolean }> {
    // Stub implementation
    this.ctx.logger.debug('Ephemeral message (stub)', { channel, user });
    return { ok: true };
  }

  /**
   * Process an incoming Slack event (stub).
   * Would be called by HTTP endpoint in real implementation.
   */
  async receiveEvent(payload: SlackEventPayload): Promise<boolean> {
    // Verify signature would happen here using signingSecret

    this.logReceivedEvent(payload);

    const { event, eventId: _eventId, eventTime: _eventTime, type: _type } = payload;

    // Skip own bot messages
    if (event.botId) {
      return true;
    }

    this.ctx.logger.debug('Slack event received', { type: event.type, channel: event.channel });

    // Invoke event handlers
    const handlers = this.eventHandlers.get(event.type);
    if (handlers) {
      for (const handler of handlers) {
        try {
          await handler(event);
        } catch (err) {
          this.ctx.logger.error('Slack event handler error', {
            eventType: event.type,
            error: err instanceof Error ? err.message : String(err),
          });
        }
      }
    }

    return true;
  }

  /**
   * Process an incoming interaction (buttons, modals).
   */
  async receiveInteraction(interaction: SlackInteraction): Promise<boolean> {
    this.logReceivedInteraction(interaction);

    for (const handler of this.interactionHandlers) {
      try {
        await handler(interaction);
      } catch (err) {
        this.ctx.logger.error('Slack interaction handler error', {
          error: err instanceof Error ? err.message : String(err),
        });
      }
    }

    return true;
  }

  /**
   * Verify a Slack request signature (would use crypto in real impl).
   */
  verifySignature(
    _signingSecret: string,
    _timestamp: string,
    _signature: string,
    _body: string,
  ): boolean {
    // In real implementation, would compute HMAC-SHA256 of timestamp+body
    // and compare to signature header X-Slack-Signature
    return true; // Stub always returns true
  }

  /**
   * Get bot user info (stub - would call auth.test API).
   */
  async getBotInfo(): Promise<{ userId: string; teamId: string; username: string }> {
    return {
      userId: 'bot-user-id',
      teamId: 'T00000000',
      username: 'ag-claw',
    };
  }

  /**
   * Get statistics.
   */
  getStats(): {
    configured: boolean;
    eventsReceived: number;
    messagesSent: number;
    interactions: number;
    activeEventHandlers: number;
  } {
    const sent = (this.db.prepare('SELECT COUNT(*) as c FROM sent_messages').get() as { c: number })
      .c;
    const received = (
      this.db.prepare('SELECT COUNT(*) as c FROM received_events').get() as { c: number }
    ).c;
    const interactions = (
      this.db.prepare('SELECT COUNT(*) as c FROM received_interactions').get() as { c: number }
    ).c;

    return {
      configured: this.isConfigured(),
      messagesSent: sent,
      eventsReceived: received,
      interactions,
      activeEventHandlers: this.eventHandlers.size,
    };
  }

  /** Log sent message */
  private logSentMessage(msg: SlackMessage): void {
    this.db
      .prepare(
        `
      INSERT INTO sent_messages (id, channel, text, blocks, sent_at)
      VALUES (?, ?, ?, ?, ?)
    `,
      )
      .run(
        msg.ts,
        msg.channel,
        msg.text,
        msg.blocks ? JSON.stringify(msg.blocks) : null,
        Date.now(),
      );
  }

  /** Log received event */
  private logReceivedEvent(payload: SlackEventPayload): void {
    this.db
      .prepare(
        `
      INSERT INTO received_events (event_id, event_type, team_id, channel, user, text, ts, received_at)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?)
    `,
      )
      .run(
        payload.eventId,
        payload.event.type,
        payload.teamId,
        payload.event.channel ?? null,
        payload.event.user ?? null,
        payload.event.text ?? null,
        payload.event.ts,
        Date.now(),
      );
  }

  /** Log received interaction */
  private logReceivedInteraction(interaction: SlackInteraction): void {
    this.db
      .prepare(
        `
      INSERT INTO received_interactions (type, user_id, username, channel_id, timestamp)
      VALUES (?, ?, ?, ?, ?)
    `,
      )
      .run(
        interaction.type,
        interaction.user.id,
        interaction.user.username,
        interaction.channel?.id ?? null,
        Date.now(),
      );
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

      CREATE TABLE IF NOT EXISTS sent_messages (
        id TEXT PRIMARY KEY,
        channel TEXT NOT NULL,
        text TEXT NOT NULL,
        blocks TEXT,
        sent_at INTEGER NOT NULL
      );

      CREATE TABLE IF NOT EXISTS received_events (
        event_id TEXT PRIMARY KEY,
        event_type TEXT NOT NULL,
        team_id TEXT NOT NULL,
        channel TEXT,
        user TEXT,
        text TEXT,
        ts TEXT NOT NULL,
        received_at INTEGER NOT NULL
      );

      CREATE TABLE IF NOT EXISTS received_interactions (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        type TEXT NOT NULL,
        user_id TEXT NOT NULL,
        username TEXT NOT NULL,
        channel_id TEXT,
        timestamp INTEGER NOT NULL
      );

      CREATE INDEX IF NOT EXISTS idx_sent_channel ON sent_messages(channel);
      CREATE INDEX IF NOT EXISTS idx_sent_at ON sent_messages(sent_at DESC);
      CREATE INDEX IF NOT EXISTS idx_recv_events_type ON received_events(event_type);
      CREATE INDEX IF NOT EXISTS idx_recv_events_ts ON received_events(ts DESC);
    `);
  }
}

export default new SlackIntegrationFeature();
