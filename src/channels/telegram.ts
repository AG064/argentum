/**
 * Telegram Channel for AG-Claw
 *
 * Full grammY-based Telegram bot with:
 * - Text and voice message handling
 * - Typing indicators
 * - User allowlisting
 * - Agent integration
 */

import { Bot, type Context, GrammyError, HttpError } from 'grammy';

import {
  type FeatureModule,
  type FeatureContext,
  type FeatureMeta,
  type HealthStatus,
} from '../core/plugin-loader';
import { type Agent } from '../index';

/** Telegram channel configuration */
export interface TelegramChannelConfig {
  enabled: boolean;
  token?: string;
  allowedUsers?: number[];
  allowedChats?: number[];
  blockedChats?: number[];
  parseMode?: 'HTML' | 'Markdown' | 'MarkdownV2';
}

/**
 * Telegram channel — real grammY bot.
 *
 * Handles message routing, typing indicators, voice transcription,
 * and user access control.
 */
class TelegramChannel implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'telegram',
    version: '0.0.2',
    description: 'Telegram channel integration via grammY',
    dependencies: [],
  };

  private config: TelegramChannelConfig = {
    enabled: false,
    parseMode: 'HTML',
  };
  private ctx!: FeatureContext;
  private bot: Bot | null = null;
  private agent: Agent | null = null;
  private running = false;

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = {
      ...this.config,
      ...(config as Partial<TelegramChannelConfig>),
    };

    // Get token from config or env
    if (!this.config.token) {
      this.config.token = process.env.AGCLAW_TELEGRAM_TOKEN ?? process.env.TELEGRAM_BOT_TOKEN;
    }
  }

  async start(): Promise<void> {
    if (!this.config.token) {
      this.ctx.logger.warn('Telegram token not configured, channel will not start');
      return;
    }

    this.bot = new Bot(this.config.token);

    // Register command handlers
    this.registerHandlers();

    // Start bot in background (non-blocking)
    this.bot
      .start({
        onStart: (info) => {
          this.running = true;
          this.ctx.logger.info('Telegram bot started', { username: info.username });
        },
      })
      .catch((err) => {
        this.ctx.logger.error('Telegram bot error', {
          error: err instanceof Error ? err.message : String(err),
        });
        this.running = false;
      });

    this.ctx.logger.info('Telegram channel initializing...');
  }

  async stop(): Promise<void> {
    this.running = false;
    if (this.bot) {
      await this.bot.stop();
      this.bot = null;
    }
  }

  async healthCheck(): Promise<HealthStatus> {
    return {
      healthy: this.running && this.bot !== null,
      message: this.running ? 'Connected' : 'Disconnected',
    };
  }

  /** Set the agent for message handling */
  setAgent(agent: Agent): void {
    this.agent = agent;
  }

  /** Check if a user/chat is allowed */
  private isAllowed(ctx: Context): boolean {
    const allowedUsers = this.config.allowedUsers ?? [];
    const allowedChats = this.config.allowedChats ?? [];
    const blockedChats = this.config.blockedChats ?? [];

    // No restrictions if lists are empty
    if (allowedUsers.length === 0 && allowedChats.length === 0) {
      return !blockedChats.includes(ctx.chat?.id ?? 0);
    }

    const userId = ctx.from?.id;
    const chatId = ctx.chat?.id;

    if (userId && allowedUsers.includes(userId)) return true;
    if (chatId && allowedChats.includes(chatId)) return true;

    return false;
  }

  /** Register bot command and message handlers */
  private registerHandlers(): void {
    if (!this.bot) return;

    // /start command
    this.bot.command('start', async (ctx) => {
      await ctx.reply(
        '🤖 Welcome to AG-Claw!\n\n' +
          'I am an AI assistant. Just send me a message and I will do my best to help.\n\n' +
          '/help - Show available commands',
      );
    });

    // /help command
    this.bot.command('help', async (ctx) => {
      await ctx.reply(
        '🤖 AG-Claw Help\n\n' +
          'Commands:\n' +
          '/start - Welcome message\n' +
          '/help - This help message\n' +
          '/status - Bot status\n\n' +
          'Send any text message and I will respond.\n' +
          'Send a voice message for transcription.',
      );
    });

    // /status command
    this.bot.command('status', async (ctx) => {
      const uptime = Math.floor(process.uptime());
      const memUsage = Math.round(process.memoryUsage().heapUsed / 1024 / 1024);
      await ctx.reply(
        `📊 AG-Claw Status\n\n` +
          `Uptime: ${uptime}s\n` +
          `Memory: ${memUsage}MB\n` +
          `Running: ${this.running ? '✅' : '❌'}`,
      );
    });

    // Text messages
    this.bot.on('message:text', async (ctx) => {
      if (!this.isAllowed(ctx)) {
        this.ctx.logger.warn('Unauthorized message', {
          userId: ctx.from?.id,
          chatId: ctx.chat?.id,
        });
        return;
      }

      const text = ctx.message.text;
      if (!text || text.startsWith('/')) return; // Skip commands

      this.ctx.logger.info('Text message', {
        userId: ctx.from?.id,
        chatId: ctx.chat?.id,
        length: text.length,
      });

      // Typing indicator
      await ctx.replyWithChatAction('typing');

      try {
        if (this.agent) {
          const response = await this.agent.handleMessage(text);
          // Split long messages (Telegram limit is 4096 chars)
          const chunks = this.splitMessage(response, 4000);
          for (const chunk of chunks) {
            await ctx.reply(chunk);
          }
        } else {
          await ctx.reply('Agent not initialized. Please try again later.');
        }
      } catch (err) {
        this.ctx.logger.error('Agent error', {
          error: err instanceof Error ? err.message : String(err),
        });
        await ctx.reply('Sorry, I encountered an error. Please try again.');
      }
    });

    // Voice messages
    this.bot.on('message:voice', async (ctx) => {
      if (!this.isAllowed(ctx)) return;

      this.ctx.logger.info('Voice message', {
        userId: ctx.from?.id,
        chatId: ctx.chat?.id,
        duration: ctx.message.voice.duration,
      });

      try {
        // Download voice file
        const file = await ctx.getFile();
        const fileUrl = `https://api.telegram.org/file/bot${this.config.token}/${file.file_path}`;

        const audioResponse = await fetch(fileUrl);
        const audioBuffer = Buffer.from(await audioResponse.arrayBuffer());

        // Transcribe with Whisper if available
        const openaiKey = process.env.OPENAI_API_KEY;
        if (!openaiKey) {
          await ctx.reply('Voice transcription is not configured (OPENAI_API_KEY not set).');
          return;
        }

        await ctx.replyWithChatAction('typing');

        const formData = new FormData();
        const blob = new Blob([audioBuffer], { type: 'audio/ogg' });
        formData.append('file', blob, 'voice.ogg');
        formData.append('model', 'whisper-1');

        const transcribeResponse = await fetch('https://api.openai.com/v1/audio/transcriptions', {
          method: 'POST',
          headers: { Authorization: `Bearer ${openaiKey}` },
          body: formData,
        });

        if (!transcribeResponse.ok) {
          await ctx.reply('Failed to transcribe voice message.');
          return;
        }

        const { text: transcription } = (await transcribeResponse.json()) as { text: string };
        this.ctx.logger.info('Voice transcribed', { text: transcription.slice(0, 100) });

        // Process with agent
        if (this.agent) {
          const response = await this.agent.handleMessage(transcription);
          await ctx.reply(`🎤 Transcription: ${transcription}\n\n${response}`);
        } else {
          await ctx.reply(`🎤 Transcription: ${transcription}\n\n(Agent not initialized)`);
        }
      } catch (err) {
        this.ctx.logger.error('Voice processing error', {
          error: err instanceof Error ? err.message : String(err),
        });
        await ctx.reply('Sorry, I had trouble processing your voice message.');
      }
    });

    // Error handling
    this.bot.catch((err) => {
      const ctx = err.ctx;
      this.ctx.logger.error('Bot error', {
        updateId: ctx.update.update_id,
        error: err.error instanceof Error ? err.error.message : String(err.error),
      });

      if (err.error instanceof GrammyError) {
        this.ctx.logger.error('Grammy error', { description: err.error.description });
      } else if (err.error instanceof HttpError) {
        this.ctx.logger.error('HTTP error');
      }
    });
  }

  /** Split a message into chunks respecting Telegram limits */
  private splitMessage(text: string, maxLength: number): string[] {
    if (text.length <= maxLength) return [text];

    const chunks: string[] = [];
    let remaining = text;

    while (remaining.length > 0) {
      if (remaining.length <= maxLength) {
        chunks.push(remaining);
        break;
      }

      // Find a good split point (newline, then space)
      let splitAt = remaining.lastIndexOf('\n', maxLength);
      if (splitAt === -1 || splitAt < maxLength / 2) {
        splitAt = remaining.lastIndexOf(' ', maxLength);
      }
      if (splitAt === -1 || splitAt < maxLength / 2) {
        splitAt = maxLength;
      }

      chunks.push(remaining.slice(0, splitAt));
      remaining = remaining.slice(splitAt).trimStart();
    }

    return chunks;
  }
}

export default new TelegramChannel();
