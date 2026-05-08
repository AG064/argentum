/**
 * Argentum Telegram Integration
 *
 * Based on OpenClaw's Telegram channel pattern:
 * - Grammy bot framework
 * - Token-based auth
 * - User/group allowlists
 * - Markdown formatting
 * - Reaction support
 */

import * as crypto from 'crypto';
import * as fs from 'fs';
import * as path from 'path';

import Database from 'better-sqlite3';
import { Bot, InputFile } from 'grammy';

import { type FeatureContext } from '../../core/plugin-loader';

// ─── Types ───────────────────────────────────────────────────────────────────

export interface TelegramConfig {
  enabled: boolean;
  botToken: string;
  allowFrom: string[]; // ["tg:userId", "tg:userId"]
  groupPolicy: 'allowlist' | 'open' | 'disabled';
  groups: Record<string, { requireMention: boolean }>;
  dmPolicy: 'pairing' | 'open' | 'disabled';
  streaming: 'full' | 'partial' | 'disabled';
  reactionNotifications: 'all' | 'mentions' | 'disabled';
  reactionLevel: 'minimal' | 'standard' | 'verbose';
  markdown: { tables: 'code' | 'markdown' | 'plain' };
}

interface TelegramMessage {
  id: number;
  chatId: number;
  userId: number;
  text: string;
  timestamp: number;
  replyTo?: number;
}

interface ReactionLike {
  emoji?: string;
  custom_emoji_id?: string;
}

interface AllowedUserRow {
  user_id: string;
}

interface AllowedGroupRow {
  group_id: string;
  require_mention: number;
}

interface CountRow {
  c: number;
}

// ─── Feature ─────────────────────────────────────────────────────────────────

class TelegramFeature {
  readonly meta = {
    name: 'telegram',
    version: '0.0.4',
    description: 'Telegram bot integration via Grammy',
    dependencies: ['allowlists'],
  };

  private config: TelegramConfig = {
    enabled: false,
    botToken: '',
    allowFrom: [],
    groupPolicy: 'allowlist',
    groups: {},
    dmPolicy: 'pairing',
    streaming: 'partial',
    reactionNotifications: 'disabled',
    reactionLevel: 'minimal',
    markdown: { tables: 'code' },
  };

  private bot: Bot | null = null;
  private ctx!: FeatureContext;
  private db!: Database.Database;
  private messageHandlers: Array<(msg: TelegramMessage) => Promise<void>> = [];

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...config } as TelegramConfig;

    // Initialize database
    const configuredDbPath = config['dbPath'];
    const dbPath = typeof configuredDbPath === 'string' ? configuredDbPath : './data/telegram.db';
    const dir = path.dirname(dbPath);
    if (!fs.existsSync(dir)) fs.mkdirSync(dir, { recursive: true });

    this.db = new Database(dbPath);
    this.db.pragma('journal_mode = WAL');

    // Create tables
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS messages (
        id INTEGER PRIMARY KEY,
        chat_id INTEGER NOT NULL,
        user_id INTEGER NOT NULL,
        text TEXT,
        timestamp INTEGER NOT NULL,
        reply_to INTEGER,
        direction TEXT NOT NULL DEFAULT 'inbound'
      );
      CREATE INDEX IF NOT EXISTS idx_messages_chat ON messages(chat_id);
      CREATE INDEX IF NOT EXISTS idx_messages_ts ON messages(timestamp);

      CREATE TABLE IF NOT EXISTS allowed_users (
        user_id TEXT PRIMARY KEY,
        added_at INTEGER NOT NULL
      );

      CREATE TABLE IF NOT EXISTS allowed_groups (
        group_id TEXT PRIMARY KEY,
        require_mention INTEGER DEFAULT 0,
        added_at INTEGER NOT NULL
      );

      CREATE TABLE IF NOT EXISTS pairing_codes (
        code TEXT PRIMARY KEY,
        user_id TEXT,
        expires_at INTEGER NOT NULL
      );
    `);

    // Load allowed users/groups from config
    for (const userId of this.config.allowFrom) {
      this.db
        .prepare('INSERT OR IGNORE INTO allowed_users (user_id, added_at) VALUES (?, ?)')
        .run(userId, Date.now());
    }
    for (const [groupId, opts] of Object.entries(this.config.groups)) {
      this.db
        .prepare(
          'INSERT OR IGNORE INTO allowed_groups (group_id, require_mention, added_at) VALUES (?, ?, ?)',
        )
        .run(groupId, opts.requireMention ? 1 : 0, Date.now());
    }

    if (this.config.botToken) {
      this.bot = new Bot(this.config.botToken);
      this.setupHandlers();
    }
  }

  private setupHandlers(): void {
    if (!this.bot) return;

    // /start command
    this.bot.command('start', async (ctx) => {
      const userId = `tg:${ctx.from?.id}`;
      if (!this.isAllowed(userId)) {
        await ctx.reply('You are not authorized. Use /pair <code> to request access.');
        return;
      }
      await ctx.reply(
        '**Argentum** is online.\n\n' +
          'Commands:\n' +
          '/status — system status\n' +
          '/features — list features\n' +
          '/help — this message',
        { parse_mode: 'Markdown' },
      );
    });

    // /pair command — request access
    this.bot.command('pair', async (ctx) => {
      const args = ctx.message?.text?.split(' ').slice(1) ?? [];
      if (!args[0]) {
        await ctx.reply('Usage: /pair <code>');
        return;
      }
      const code = args[0].trim();
      const row = this.db
        .prepare('SELECT * FROM pairing_codes WHERE code = ? AND expires_at > ?')
        .get(code, Date.now());
      if (row) {
        const userId = `tg:${ctx.from?.id}`;
        this.db
          .prepare('INSERT OR IGNORE INTO allowed_users (user_id, added_at) VALUES (?, ?)')
          .run(userId, Date.now());
        this.db.prepare('DELETE FROM pairing_codes WHERE code = ?').run(code);
        await ctx.reply('Access granted! Send /start to begin.');
      } else {
        await ctx.reply('Invalid or expired code.');
      }
    });

    // /status command
    this.bot.command('status', async (ctx) => {
      if (!this.isAllowed(`tg:${ctx.from?.id}`)) return;
      const uptime = process.uptime();
      const hours = Math.floor(uptime / 3600);
      const mins = Math.floor((uptime % 3600) / 60);
      await ctx.reply(
        `**Argentum Status**\n` +
          `Uptime: ${hours}h ${mins}m\n` +
          `Memory: ${(process.memoryUsage().heapUsed / 1024 / 1024).toFixed(1)} MB`,
        { parse_mode: 'Markdown' },
      );
    });

    // /features command
    this.bot.command('features', async (ctx) => {
      if (!this.isAllowed(`tg:${ctx.from?.id}`)) return;
      await ctx.reply('Use `argentum tools` on the server to list features.', {
        parse_mode: 'Markdown',
      });
    });

    // /generate_pair — admin generates pairing code
    this.bot.command('generate_pair', async (ctx) => {
      if (!this.isAllowed(`tg:${ctx.from?.id}`)) return;
      const code = crypto.randomBytes(4).toString('hex');
      const expiresAt = Date.now() + 10 * 60 * 1000; // 10 minutes
      this.db
        .prepare('INSERT INTO pairing_codes (code, expires_at) VALUES (?, ?)')
        .run(code, expiresAt);
      await ctx.reply(`Pairing code: \`${code}\`\nValid for 10 minutes.`, {
        parse_mode: 'Markdown',
      });
    });

    // Message handler
    this.bot.on('message:text', async (ctx) => {
      const userId = `tg:${ctx.from?.id}`;
      const chatId = ctx.chat.id;

      // Check permissions
      if (ctx.chat.type === 'private') {
        if (this.config.dmPolicy === 'disabled') return;
        if (this.config.dmPolicy === 'pairing' && !this.isAllowed(userId)) {
          await ctx.reply('Use /pair <code> to request access.');
          return;
        }
      } else {
        // Group chat
        if (this.config.groupPolicy === 'disabled') return;
        if (this.config.groupPolicy === 'allowlist') {
          const groupId = String(chatId);
          const groupRow = this.db
            .prepare('SELECT * FROM allowed_groups WHERE group_id = ?')
            .get(groupId) as AllowedGroupRow | undefined;
          if (!groupRow) return;
          if (groupRow.require_mention && !ctx.message.text.includes(`@${ctx.me.username}`)) return;
        }
      }

      // Store message
      this.db
        .prepare(
          'INSERT INTO messages (id, chat_id, user_id, text, timestamp, reply_to) VALUES (?, ?, ?, ?, ?, ?)',
        )
        .run(
          ctx.message.message_id,
          chatId,
          ctx.from.id,
          ctx.message.text,
          Date.now(),
          ctx.message.reply_to_message?.message_id,
        );

      // Forward to handlers
      const msg: TelegramMessage = {
        id: ctx.message.message_id,
        chatId,
        userId: ctx.from.id,
        text: ctx.message.text,
        timestamp: Date.now(),
        replyTo: ctx.message.reply_to_message?.message_id,
      };

      for (const handler of this.messageHandlers) {
        try {
          await handler(msg);
        } catch (err) {
          this.ctx?.logger?.error?.('Telegram message handler error', { error: String(err) });
        }
      }
    });

    // Reaction handler
    this.bot.on('message_reaction', async (ctx) => {
      if (this.config.reactionNotifications === 'disabled') return;
      const reaction = ctx.messageReaction;
      // Log reactions for analytics
      this.ctx?.logger?.debug?.('Telegram reaction', {
        chatId: reaction.chat.id,
        userId: reaction.user?.id,
        emoji: reaction.new_reaction
          .map((reactionItem) => {
            const item = reactionItem as ReactionLike;
            return item.emoji ?? item.custom_emoji_id ?? 'unknown';
          })
          .join(', '),
      });
    });

    // Error handler
    this.bot.catch((err) => {
      this.ctx?.logger?.error?.('Telegram bot error', { error: err.message });
    });
  }

  private isAllowed(userId: string): boolean {
    const row = this.db.prepare('SELECT * FROM allowed_users WHERE user_id = ?').get(userId);
    return !!row;
  }

  async start(): Promise<void> {
    if (!this.bot || !this.config.enabled) {
      this.ctx?.logger?.warn?.('Telegram not configured or disabled');
      return;
    }
    try {
      await this.bot.start({
        onStart: () => {
          this.ctx?.logger?.info?.('Telegram bot started');
        },
      });
    } catch (err) {
      this.ctx?.logger?.error?.('Failed to start Telegram bot', { error: String(err) });
    }
  }

  async stop(): Promise<void> {
    if (this.bot) {
      await this.bot.stop();
      this.ctx?.logger?.info?.('Telegram bot stopped');
    }
    if (this.db) {
      this.db.close();
    }
  }

  async healthCheck(): Promise<{ healthy: boolean; details: Record<string, unknown> }> {
    try {
      if (!this.bot) return { healthy: false, details: { error: 'Bot not initialized' } };
      const me = await this.bot.api.getMe();
      const msgCount = (this.db.prepare('SELECT COUNT(*) as c FROM messages').get() as CountRow).c;
      return {
        healthy: true,
        details: {
          username: me.username,
          messages: msgCount,
          allowedUsers: this.config.allowFrom.length,
        },
      };
    } catch {
      return { healthy: false, details: { error: 'Cannot reach Telegram API' } };
    }
  }

  // Public API

  onMessage(handler: (msg: TelegramMessage) => Promise<void>): void {
    this.messageHandlers.push(handler);
  }

  async sendMessage(
    chatId: number | string,
    text: string,
    options?: { parseMode?: 'Markdown' | 'MarkdownV2' | 'HTML'; replyTo?: number },
  ): Promise<void> {
    if (!this.bot) throw new Error('Bot not initialized');
    await this.bot.api.sendMessage(chatId, text, {
      parse_mode: options?.parseMode ?? 'Markdown',
      reply_to_message_id: options?.replyTo,
    });
  }

  async sendPhoto(
    chatId: number | string,
    photo: string | Buffer,
    caption?: string,
  ): Promise<void> {
    if (!this.bot) throw new Error('Bot not initialized');
    if (typeof photo === 'string' && (photo.startsWith('http') || photo.startsWith('/'))) {
      await this.bot.api.sendPhoto(chatId, new InputFile(photo), { caption });
    } else {
      await this.bot.api.sendPhoto(chatId, new InputFile(photo as Buffer), { caption });
    }
  }

  async sendVoice(chatId: number | string, voice: Buffer, caption?: string): Promise<void> {
    if (!this.bot) throw new Error('Bot not initialized');
    await this.bot.api.sendVoice(chatId, new InputFile(voice), { caption });
  }

  generatePairingCode(): string {
    const code = crypto.randomBytes(4).toString('hex');
    const expiresAt = Date.now() + 10 * 60 * 1000;
    this.db
      .prepare('INSERT INTO pairing_codes (code, expires_at) VALUES (?, ?)')
      .run(code, expiresAt);
    return code;
  }

  listAllowedUsers(): string[] {
    const rows = this.db
      .prepare<[], AllowedUserRow>('SELECT user_id FROM allowed_users')
      .all();
    return rows.map((r) => r.user_id);
  }

  addUser(userId: string): void {
    this.db
      .prepare('INSERT OR IGNORE INTO allowed_users (user_id, added_at) VALUES (?, ?)')
      .run(userId, Date.now());
  }

  removeUser(userId: string): void {
    this.db.prepare('DELETE FROM allowed_users WHERE user_id = ?').run(userId);
  }
}

export default new TelegramFeature();
