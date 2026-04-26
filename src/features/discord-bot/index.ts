/**
 * Discord Bot Feature
 *
 * Discord bot integration for sending messages and listening to events.
 * Structured as a Discord.js-based bot plugin. Real bot connection not implemented.
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

/** Discord message (simplified) */
export interface DiscordMessage {
  id: string;
  channelId: string;
  guildId?: string;
  author: {
    id: string;
    username: string;
    discriminator?: string;
    bot?: boolean;
  };
  content: string;
  timestamp: number;
  attachments?: Array<{ id: string; filename: string; url: string }>;
  embeds?: unknown[]; // Rich embed objects
  referencedMessage?: DiscordMessage;
}

/** Bot configuration */
export interface DiscordBotConfig {
  token: string;
  clientId?: string;
  guildId?: string; // For slash command registration
  commandPrefix?: string;
  intents?: string[]; // Discord Gateway intents
}

/** Feature configuration */
export interface DiscordBotFeatureConfig {
  dbPath?: string;
  tokenEnvVar?: string; // Load token from environment variable
  defaultPrefix?: string;
}

/**
 * DiscordBot — Discord bot integration structure.
 *
 * Provides:
 * - Token configuration and storage
 * - Message sending to channels
 * - Incoming message event handling
 * - Command prefix framework
 * - Message history logging
 *
 * Note: Real Discord WebSocket connection is not implemented.
 * This is a structure for future Discord.js integration.
 */
class DiscordBotFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'discord-bot',
    version: '0.0.2',
    description: 'Discord bot integration with message sending and event handling',
    dependencies: [],
  };

  private config: Required<DiscordBotFeatureConfig>;
  private ctx!: FeatureContext;
  private db!: Database.Database;
  private token: string = '';
  private clientId: string = '';
  private guildId: string | null = null;
  private commandPrefix: string = '!';
  private messageHandlers: Array<(msg: DiscordMessage) => Promise<void>> = [];
  private ready: boolean = false;

  constructor() {
    this.config = {
      dbPath: './data/discord-bot.db',
      tokenEnvVar: 'DISCORD_BOT_TOKEN',
      defaultPrefix: '!',
    };
  }

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = {
      dbPath: (config['dbPath'] as string) ?? this.config['dbPath'],
      tokenEnvVar: (config['tokenEnvVar'] as string) ?? this.config['tokenEnvVar'],
      defaultPrefix: (config['defaultPrefix'] as string) ?? this.config['defaultPrefix'],
    };

    this.commandPrefix = this.config.defaultPrefix;
    this.initDatabase();

    // Load persisted configuration
    const cfg = this.db.prepare('SELECT * FROM config WHERE key = ?').get('discord') as
      | { value: string }
      | undefined;
    if (cfg) {
      try {
        const parsed = JSON.parse(cfg.value);
        this.token = parsed.token;
        this.clientId = parsed.clientId ?? '';
        this.guildId = parsed.guildId ?? null;
        this.commandPrefix = parsed.commandPrefix ?? this.commandPrefix;
        if (this.token) {
          this.ready = true;
        }
      } catch {
        this.ctx.logger.warn('Failed to parse stored Discord config');
      }
    }

    // Also check env var if token not set
    if (!this.token && process.env[this.config.tokenEnvVar]) {
      const envToken = process.env[this.config.tokenEnvVar];
      if (envToken) {
        this.token = envToken;
      }
    }
  }

  async start(): Promise<void> {
    if (!this.token) {
      this.ctx.logger.warn('DiscordBot not configured: no token set');
      return;
    }

    this.ready = true;
    this.ctx.logger.info('DiscordBot active (stub connection)', {
      configured: true,
      guildId: this.guildId,
      prefix: this.commandPrefix,
    });

    // In real implementation, would create Discord.Client and connect
    // const client = new Client({ intents: [...] });
    // await client.login(this.token);
  }

  async stop(): Promise<void> {
    this.ready = false;
    this.ctx.logger.info('DiscordBot stopped');
  }

  async healthCheck(): Promise<HealthStatus> {
    try {
      const sentCount = (
        this.db.prepare('SELECT COUNT(*) as c FROM sent_messages').get() as { c: number }
      ).c;
      const receivedCount = (
        this.db.prepare('SELECT COUNT(*) as c FROM received_messages').get() as { c: number }
      ).c;

      return {
        healthy: true,
        details: {
          configured: !!this.token,
          sentCount,
          receivedCount,
          handlers: this.messageHandlers.length,
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
   * Configure the Discord bot.
   *
   * @param token - Bot token from Discord Developer Portal
   * @param clientId - Application client ID (optional)
   * @param guildId - Test guild ID for slash commands (optional)
   * @param commandPrefix - Command prefix (default !)
   */
  configure(token: string, clientId?: string, guildId?: string, commandPrefix?: string): void {
    this.token = token;
    this.clientId = clientId ?? '';
    this.guildId = guildId ?? null;
    if (commandPrefix) this.commandPrefix = commandPrefix;

    const value = JSON.stringify({
      token: this.token,
      clientId: this.clientId,
      guildId: this.guildId,
      commandPrefix: this.commandPrefix,
    });

    this.db
      .prepare('INSERT OR REPLACE INTO config (key, value) VALUES (?, ?)')
      .run('discord', value);
    this.ctx.logger.info('DiscordBot configured', {
      hasGuild: !!guildId,
      prefix: this.commandPrefix,
    });
  }

  /**
   * Check if bot is configured and ready.
   */
  isReady(): boolean {
    return this.ready && !!this.token;
  }

  /**
   * Set the command prefix for text commands.
   */
  setCommandPrefix(prefix: string): void {
    this.commandPrefix = prefix;
    this.db.prepare('UPDATE config SET value = ? WHERE key = ?').run(
      JSON.stringify({
        token: this.token,
        clientId: this.clientId,
        guildId: this.guildId,
        commandPrefix: this.commandPrefix,
      }),
      'discord',
    );
  }

  /**
   * Register a handler for incoming messages.
   *
   * @param handler - Async function called for each message
   */
  onMessage(handler: (msg: DiscordMessage) => Promise<void>): void {
    this.messageHandlers.push(handler);
  }

  /**
   * Send a message to a channel (stub).
   *
   * @param channelId - Discord channel ID
   * @param content - Message text content
   * @returns The sent message object (simulated)
   */
  async sendMessage(channelId: string, content: string): Promise<DiscordMessage> {
    if (!this.isReady()) {
      throw new Error('Discord bot not configured. Call configure() first.');
    }

    const msgId = `discord-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
    const now = Date.now();

    // Simulate sending
    const message: DiscordMessage = {
      id: msgId,
      channelId,
      content,
      timestamp: now,
      author: {
        id: 'bot',
        username: 'AG-Claw',
        bot: true,
      },
    };

    this.logSentMessage(message);

    this.ctx.logger.info('Discord message sent (stub)', { channelId, length: content.length });
    return message;
  }

  /**
   * Process an incoming Discord message (stub).
   * Would be called by gateway event in real implementation.
   */
  async receiveMessage(
    raw: Partial<DiscordMessage> & {
      id: string;
      channelId: string;
      author: { id: string; username: string };
    },
  ): Promise<void> {
    const message: DiscordMessage = {
      id: raw.id,
      channelId: raw.channelId,
      guildId: raw.guildId,
      author: {
        id: raw.author.id,
        username: raw.author.username,
        discriminator: raw.author.discriminator,
        bot: raw.author.bot,
      },
      content: raw.content ?? '',
      timestamp: raw.timestamp ?? Date.now(),
      attachments: raw.attachments,
      embeds: raw.embeds,
      referencedMessage: raw.referencedMessage,
    };

    this.logReceivedMessage(message);

    this.ctx.logger.debug('Discord message received', {
      channelId: message.channelId,
      author: message.author.username,
    });

    // Invoke handlers
    for (const handler of this.messageHandlers) {
      try {
        await handler(message);
      } catch (err) {
        this.ctx.logger.error('Discord message handler error', {
          error: err instanceof Error ? err.message : String(err),
        });
      }
    }
  }

  /**
   * Get message history for a channel (stub).
   */
  getHistory(channelId: string, limit: number = 50): DiscordMessage[] {
    const rows = this.db
      .prepare(
        `
      SELECT * FROM received_messages
      WHERE channel_id = ?
      ORDER BY timestamp DESC
      LIMIT ?
    `,
      )
      .all(channelId, limit) as Array<{
      id: string;
      channel_id: string;
      guild_id: string | null;
      author_id: string;
      author_username: string;
      author_discriminator: string | null;
      author_bot: number;
      content: string;
      timestamp: number;
      attachments: string | null;
      embeds: string | null;
    }>;

    return rows.map((r) => ({
      id: r.id,
      channelId: r.channel_id,
      guildId: r.guild_id ?? undefined,
      author: {
        id: r.author_id,
        username: r.author_username,
        discriminator: r.author_discriminator ?? undefined,
        bot: r.author_bot === 1,
      },
      content: r.content,
      timestamp: r.timestamp,
      attachments: r.attachments ? JSON.parse(r.attachments) : undefined,
      embeds: r.embeds ? JSON.parse(r.embeds) : undefined,
    }));
  }

  /**
   * Get bot statistics.
   */
  getStats(): {
    configured: boolean;
    messagesSent: number;
    messagesReceived: number;
    activeHandlers: number;
  } {
    const sent = (this.db.prepare('SELECT COUNT(*) as c FROM sent_messages').get() as { c: number })
      .c;
    const received = (
      this.db.prepare('SELECT COUNT(*) as c FROM received_messages').get() as { c: number }
    ).c;

    return {
      configured: this.isReady(),
      messagesSent: sent,
      messagesReceived: received,
      activeHandlers: this.messageHandlers.length,
    };
  }

  /** Log sent message */
  private logSentMessage(msg: DiscordMessage): void {
    this.db
      .prepare(
        `
      INSERT INTO sent_messages (id, channel_id, content, sent_at)
      VALUES (?, ?, ?, ?)
    `,
      )
      .run(msg.id, msg.channelId, msg.content, msg.timestamp);
  }

  /** Log received message */
  private logReceivedMessage(msg: DiscordMessage): void {
    this.db
      .prepare(
        `
      INSERT INTO received_messages (id, channel_id, guild_id, author_id, author_username, author_discriminator, author_bot, content, timestamp, attachments, embeds)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    `,
      )
      .run(
        msg.id,
        msg.channelId,
        msg.guildId ?? null,
        msg.author.id,
        msg.author.username,
        msg.author.discriminator ?? null,
        msg.author.bot ? 1 : 0,
        msg.content,
        msg.timestamp,
        msg.attachments ? JSON.stringify(msg.attachments) : null,
        msg.embeds ? JSON.stringify(msg.embeds) : null,
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
        channel_id TEXT NOT NULL,
        content TEXT NOT NULL,
        sent_at INTEGER NOT NULL
      );

      CREATE TABLE IF NOT EXISTS received_messages (
        id TEXT PRIMARY KEY,
        channel_id TEXT NOT NULL,
        guild_id TEXT,
        author_id TEXT NOT NULL,
        author_username TEXT NOT NULL,
        author_discriminator TEXT,
        author_bot INTEGER DEFAULT 0,
        content TEXT NOT NULL,
        timestamp INTEGER NOT NULL,
        attachments TEXT,
        embeds TEXT
      );

      CREATE INDEX IF NOT EXISTS idx_sent_channel ON sent_messages(channel_id);
      CREATE INDEX IF NOT EXISTS idx_sent_at ON sent_messages(sent_at DESC);
      CREATE INDEX IF NOT EXISTS idx_recv_channel ON received_messages(channel_id);
      CREATE INDEX IF NOT EXISTS idx_recv_timestamp ON received_messages(timestamp DESC);
      CREATE INDEX IF NOT EXISTS idx_recv_author ON received_messages(author_id);
    `);
  }
}

export default new DiscordBotFeature();
