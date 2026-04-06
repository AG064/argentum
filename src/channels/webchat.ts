/**
 * Webchat Channel
 *
 * Web-based chat interface using Server-Sent Events (SSE) for
 * real-time streaming responses.
 */

import {
  type FeatureModule,
  type FeatureContext,
  type FeatureMeta,
  type HealthStatus,
} from '../core/plugin-loader';

/** Webchat channel configuration */
export interface WebchatChannelConfig {
  enabled: boolean;
  port: number;
  corsOrigins: string[];
  sessionTimeoutMs: number;
  maxMessageLength: number;
}

/** Chat session */
interface ChatSession {
  id: string;
  userId: string;
  startedAt: number;
  lastActiveAt: number;
  history: Array<{ role: string; content: string; timestamp: number }>;
}

/** Message handler */
export type WebchatMessageHandler = (sessionId: string, message: string) => Promise<string>;

/**
 * Webchat channel — browser-based chat with SSE streaming.
 *
 * Provides a web interface for chat interactions with session
 * management and streaming response support.
 */
class WebchatChannel implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'webchat-channel',
    version: '0.1.0',
    description: 'Web-based chat with SSE streaming responses',
    dependencies: ['webchat'],
  };

  private config: WebchatChannelConfig = {
    enabled: false,
    port: 3001,
    corsOrigins: ['http://localhost:3001', 'http://127.0.0.1:3001'],
    sessionTimeoutMs: 3600000,
    maxMessageLength: 10000,
  };
  private ctx!: FeatureContext;
  private sessions: Map<string, ChatSession> = new Map();
  private messageHandler: WebchatMessageHandler | null = null;

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...(config as Partial<WebchatChannelConfig>) };
  }

  async start(): Promise<void> {
    // Start session cleanup interval
    setInterval(() => this.cleanupSessions(), 60000);
    this.ctx.logger.info('Webchat channel active', { port: this.config.port });
  }

  async stop(): Promise<void> {
    this.sessions.clear();
  }

  async healthCheck(): Promise<HealthStatus> {
    return {
      healthy: true,
      details: { activeSessions: this.sessions.size },
    };
  }

  /** Register message handler */
  onMessage(handler: WebchatMessageHandler): void {
    this.messageHandler = handler;
  }

  /** Create a new chat session */
  createSession(userId: string): ChatSession {
    const id = `ws_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
    const session: ChatSession = {
      id,
      userId,
      startedAt: Date.now(),
      lastActiveAt: Date.now(),
      history: [],
    };
    this.sessions.set(id, session);
    return session;
  }

  /** Process incoming message */
  async handleMessage(sessionId: string, content: string): Promise<string> {
    if (content.length > this.config.maxMessageLength) {
      return 'Message too long. Please keep messages under 10,000 characters.';
    }

    const session = this.sessions.get(sessionId);
    if (!session) return 'Session not found. Please refresh the page.';

    session.lastActiveAt = Date.now();
    session.history.push({ role: 'user', content, timestamp: Date.now() });

    if (!this.messageHandler) {
      return 'No message handler configured.';
    }

    try {
      const response = await this.messageHandler(sessionId, content);
      session.history.push({ role: 'assistant', content: response, timestamp: Date.now() });
      return response;
    } catch (err) {
      this.ctx.logger.error('Message handler error', {
        sessionId,
        error: err instanceof Error ? err.message : String(err),
      });
      return 'An error occurred processing your message.';
    }
  }

  /** Get session history */
  getHistory(sessionId: string): ChatSession['history'] {
    return this.sessions.get(sessionId)?.history ?? [];
  }

  /** Clean up expired sessions */
  private cleanupSessions(): void {
    const now = Date.now();
    for (const [id, session] of this.sessions) {
      if (now - session.lastActiveAt > this.config.sessionTimeoutMs) {
        this.sessions.delete(id);
      }
    }
  }
}

export default new WebchatChannel();
