"use strict";
/**
 * Discord Bot Feature
 *
 * Discord bot integration for sending messages and listening to events.
 * Structured as a Discord.js-based bot plugin. Real bot connection not implemented.
 */
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const fs_1 = require("fs");
const path_1 = require("path");
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
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
class DiscordBotFeature {
    meta = {
        name: 'discord-bot',
        version: '0.1.0',
        description: 'Discord bot integration with message sending and event handling',
        dependencies: [],
    };
    config;
    ctx;
    db;
    token = '';
    clientId = '';
    guildId = null;
    commandPrefix = '!';
    messageHandlers = [];
    ready = false;
    constructor() {
        this.config = {
            dbPath: './data/discord-bot.db',
            tokenEnvVar: 'DISCORD_BOT_TOKEN',
            defaultPrefix: '!',
        };
    }
    async init(config, context) {
        this.ctx = context;
        this.config = {
            dbPath: config['dbPath'] ?? this.config['dbPath'],
            tokenEnvVar: config['tokenEnvVar'] ?? this.config['tokenEnvVar'],
            defaultPrefix: config['defaultPrefix'] ?? this.config['defaultPrefix'],
        };
        this.commandPrefix = this.config.defaultPrefix;
        this.initDatabase();
        // Load persisted configuration
        const cfg = this.db.prepare('SELECT * FROM config WHERE key = ?').get('discord');
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
            }
            catch {
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
    async start() {
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
    async stop() {
        this.ready = false;
        this.ctx.logger.info('DiscordBot stopped');
    }
    async healthCheck() {
        try {
            const sentCount = this.db.prepare('SELECT COUNT(*) as c FROM sent_messages').get().c;
            const receivedCount = this.db.prepare('SELECT COUNT(*) as c FROM received_messages').get().c;
            return {
                healthy: true,
                details: {
                    configured: !!this.token,
                    sentCount,
                    receivedCount,
                    handlers: this.messageHandlers.length,
                },
            };
        }
        catch (err) {
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
    configure(token, clientId, guildId, commandPrefix) {
        this.token = token;
        this.clientId = clientId ?? '';
        this.guildId = guildId ?? null;
        if (commandPrefix)
            this.commandPrefix = commandPrefix;
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
    isReady() {
        return this.ready && !!this.token;
    }
    /**
     * Set the command prefix for text commands.
     */
    setCommandPrefix(prefix) {
        this.commandPrefix = prefix;
        this.db.prepare('UPDATE config SET value = ? WHERE key = ?').run(JSON.stringify({
            token: this.token,
            clientId: this.clientId,
            guildId: this.guildId,
            commandPrefix: this.commandPrefix,
        }), 'discord');
    }
    /**
     * Register a handler for incoming messages.
     *
     * @param handler - Async function called for each message
     */
    onMessage(handler) {
        this.messageHandlers.push(handler);
    }
    /**
     * Send a message to a channel (stub).
     *
     * @param channelId - Discord channel ID
     * @param content - Message text content
     * @returns The sent message object (simulated)
     */
    async sendMessage(channelId, content) {
        if (!this.isReady()) {
            throw new Error('Discord bot not configured. Call configure() first.');
        }
        const msgId = `discord-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
        const now = Date.now();
        // Simulate sending
        const message = {
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
    async receiveMessage(raw) {
        const message = {
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
            }
            catch (err) {
                this.ctx.logger.error('Discord message handler error', {
                    error: err instanceof Error ? err.message : String(err),
                });
            }
        }
    }
    /**
     * Get message history for a channel (stub).
     */
    getHistory(channelId, limit = 50) {
        const rows = this.db
            .prepare(`
      SELECT * FROM received_messages
      WHERE channel_id = ?
      ORDER BY timestamp DESC
      LIMIT ?
    `)
            .all(channelId, limit);
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
    getStats() {
        const sent = this.db.prepare('SELECT COUNT(*) as c FROM sent_messages').get()
            .c;
        const received = this.db.prepare('SELECT COUNT(*) as c FROM received_messages').get().c;
        return {
            configured: this.isReady(),
            messagesSent: sent,
            messagesReceived: received,
            activeHandlers: this.messageHandlers.length,
        };
    }
    /** Log sent message */
    logSentMessage(msg) {
        this.db
            .prepare(`
      INSERT INTO sent_messages (id, channel_id, content, sent_at)
      VALUES (?, ?, ?, ?)
    `)
            .run(msg.id, msg.channelId, msg.content, msg.timestamp);
    }
    /** Log received message */
    logReceivedMessage(msg) {
        this.db
            .prepare(`
      INSERT INTO received_messages (id, channel_id, guild_id, author_id, author_username, author_discriminator, author_bot, content, timestamp, attachments, embeds)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    `)
            .run(msg.id, msg.channelId, msg.guildId ?? null, msg.author.id, msg.author.username, msg.author.discriminator ?? null, msg.author.bot ? 1 : 0, msg.content, msg.timestamp, msg.attachments ? JSON.stringify(msg.attachments) : null, msg.embeds ? JSON.stringify(msg.embeds) : null);
    }
    /** Initialize database and create tables */
    initDatabase() {
        const fullPath = (0, path_1.resolve)(this.config.dbPath);
        if (!(0, fs_1.existsSync)((0, path_1.dirname)(fullPath))) {
            (0, fs_1.mkdirSync)((0, path_1.dirname)(fullPath), { recursive: true });
        }
        this.db = new better_sqlite3_1.default(fullPath);
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
exports.default = new DiscordBotFeature();
//# sourceMappingURL=index.js.map