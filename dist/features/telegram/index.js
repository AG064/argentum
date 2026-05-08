"use strict";
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
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const crypto = __importStar(require("crypto"));
const fs = __importStar(require("fs"));
const path = __importStar(require("path"));
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
const grammy_1 = require("grammy");
// ─── Feature ─────────────────────────────────────────────────────────────────
class TelegramFeature {
    meta = {
        name: 'telegram',
        version: '0.0.4',
        description: 'Telegram bot integration via Grammy',
        dependencies: ['allowlists'],
    };
    config = {
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
    bot = null;
    ctx;
    db;
    messageHandlers = [];
    async init(config, context) {
        this.ctx = context;
        this.config = { ...this.config, ...config };
        // Initialize database
        const configuredDbPath = config['dbPath'];
        const dbPath = typeof configuredDbPath === 'string' ? configuredDbPath : './data/telegram.db';
        const dir = path.dirname(dbPath);
        if (!fs.existsSync(dir))
            fs.mkdirSync(dir, { recursive: true });
        this.db = new better_sqlite3_1.default(dbPath);
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
                .prepare('INSERT OR IGNORE INTO allowed_groups (group_id, require_mention, added_at) VALUES (?, ?, ?)')
                .run(groupId, opts.requireMention ? 1 : 0, Date.now());
        }
        if (this.config.botToken) {
            this.bot = new grammy_1.Bot(this.config.botToken);
            this.setupHandlers();
        }
    }
    setupHandlers() {
        if (!this.bot)
            return;
        // /start command
        this.bot.command('start', async (ctx) => {
            const userId = `tg:${ctx.from?.id}`;
            if (!this.isAllowed(userId)) {
                await ctx.reply('You are not authorized. Use /pair <code> to request access.');
                return;
            }
            await ctx.reply('**Argentum** is online.\n\n' +
                'Commands:\n' +
                '/status — system status\n' +
                '/features — list features\n' +
                '/help — this message', { parse_mode: 'Markdown' });
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
            }
            else {
                await ctx.reply('Invalid or expired code.');
            }
        });
        // /status command
        this.bot.command('status', async (ctx) => {
            if (!this.isAllowed(`tg:${ctx.from?.id}`))
                return;
            const uptime = process.uptime();
            const hours = Math.floor(uptime / 3600);
            const mins = Math.floor((uptime % 3600) / 60);
            await ctx.reply(`**Argentum Status**\n` +
                `Uptime: ${hours}h ${mins}m\n` +
                `Memory: ${(process.memoryUsage().heapUsed / 1024 / 1024).toFixed(1)} MB`, { parse_mode: 'Markdown' });
        });
        // /features command
        this.bot.command('features', async (ctx) => {
            if (!this.isAllowed(`tg:${ctx.from?.id}`))
                return;
            await ctx.reply('Use `argentum tools` on the server to list features.', {
                parse_mode: 'Markdown',
            });
        });
        // /generate_pair — admin generates pairing code
        this.bot.command('generate_pair', async (ctx) => {
            if (!this.isAllowed(`tg:${ctx.from?.id}`))
                return;
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
                if (this.config.dmPolicy === 'disabled')
                    return;
                if (this.config.dmPolicy === 'pairing' && !this.isAllowed(userId)) {
                    await ctx.reply('Use /pair <code> to request access.');
                    return;
                }
            }
            else {
                // Group chat
                if (this.config.groupPolicy === 'disabled')
                    return;
                if (this.config.groupPolicy === 'allowlist') {
                    const groupId = String(chatId);
                    const groupRow = this.db
                        .prepare('SELECT * FROM allowed_groups WHERE group_id = ?')
                        .get(groupId);
                    if (!groupRow)
                        return;
                    if (groupRow.require_mention && !ctx.message.text.includes(`@${ctx.me.username}`))
                        return;
                }
            }
            // Store message
            this.db
                .prepare('INSERT INTO messages (id, chat_id, user_id, text, timestamp, reply_to) VALUES (?, ?, ?, ?, ?, ?)')
                .run(ctx.message.message_id, chatId, ctx.from.id, ctx.message.text, Date.now(), ctx.message.reply_to_message?.message_id);
            // Forward to handlers
            const msg = {
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
                }
                catch (err) {
                    this.ctx?.logger?.error?.('Telegram message handler error', { error: String(err) });
                }
            }
        });
        // Reaction handler
        this.bot.on('message_reaction', async (ctx) => {
            if (this.config.reactionNotifications === 'disabled')
                return;
            const reaction = ctx.messageReaction;
            // Log reactions for analytics
            this.ctx?.logger?.debug?.('Telegram reaction', {
                chatId: reaction.chat.id,
                userId: reaction.user?.id,
                emoji: reaction.new_reaction
                    .map((reactionItem) => {
                    const item = reactionItem;
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
    isAllowed(userId) {
        const row = this.db.prepare('SELECT * FROM allowed_users WHERE user_id = ?').get(userId);
        return !!row;
    }
    async start() {
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
        }
        catch (err) {
            this.ctx?.logger?.error?.('Failed to start Telegram bot', { error: String(err) });
        }
    }
    async stop() {
        if (this.bot) {
            await this.bot.stop();
            this.ctx?.logger?.info?.('Telegram bot stopped');
        }
        if (this.db) {
            this.db.close();
        }
    }
    async healthCheck() {
        try {
            if (!this.bot)
                return { healthy: false, details: { error: 'Bot not initialized' } };
            const me = await this.bot.api.getMe();
            const msgCount = this.db.prepare('SELECT COUNT(*) as c FROM messages').get().c;
            return {
                healthy: true,
                details: {
                    username: me.username,
                    messages: msgCount,
                    allowedUsers: this.config.allowFrom.length,
                },
            };
        }
        catch {
            return { healthy: false, details: { error: 'Cannot reach Telegram API' } };
        }
    }
    // Public API
    onMessage(handler) {
        this.messageHandlers.push(handler);
    }
    async sendMessage(chatId, text, options) {
        if (!this.bot)
            throw new Error('Bot not initialized');
        await this.bot.api.sendMessage(chatId, text, {
            parse_mode: options?.parseMode ?? 'Markdown',
            reply_to_message_id: options?.replyTo,
        });
    }
    async sendPhoto(chatId, photo, caption) {
        if (!this.bot)
            throw new Error('Bot not initialized');
        if (typeof photo === 'string' && (photo.startsWith('http') || photo.startsWith('/'))) {
            await this.bot.api.sendPhoto(chatId, new grammy_1.InputFile(photo), { caption });
        }
        else {
            await this.bot.api.sendPhoto(chatId, new grammy_1.InputFile(photo), { caption });
        }
    }
    async sendVoice(chatId, voice, caption) {
        if (!this.bot)
            throw new Error('Bot not initialized');
        await this.bot.api.sendVoice(chatId, new grammy_1.InputFile(voice), { caption });
    }
    generatePairingCode() {
        const code = crypto.randomBytes(4).toString('hex');
        const expiresAt = Date.now() + 10 * 60 * 1000;
        this.db
            .prepare('INSERT INTO pairing_codes (code, expires_at) VALUES (?, ?)')
            .run(code, expiresAt);
        return code;
    }
    listAllowedUsers() {
        const rows = this.db
            .prepare('SELECT user_id FROM allowed_users')
            .all();
        return rows.map((r) => r.user_id);
    }
    addUser(userId) {
        this.db
            .prepare('INSERT OR IGNORE INTO allowed_users (user_id, added_at) VALUES (?, ?)')
            .run(userId, Date.now());
    }
    removeUser(userId) {
        this.db.prepare('DELETE FROM allowed_users WHERE user_id = ?').run(userId);
    }
}
exports.default = new TelegramFeature();
//# sourceMappingURL=index.js.map