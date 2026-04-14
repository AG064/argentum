"use strict";
/**
 * Webchat Channel
 *
 * Web-based chat interface using Server-Sent Events (SSE) for
 * real-time streaming responses.
 */
Object.defineProperty(exports, "__esModule", { value: true });
/**
 * Webchat channel — browser-based chat with SSE streaming.
 *
 * Provides a web interface for chat interactions with session
 * management and streaming response support.
 */
class WebchatChannel {
    meta = {
        name: 'webchat-channel',
        version: '0.1.0',
        description: 'Web-based chat with SSE streaming responses',
        dependencies: ['webchat'],
    };
    config = {
        enabled: false,
        port: 3001,
        corsOrigins: ['http://localhost:3001', 'http://127.0.0.1:3001'],
        sessionTimeoutMs: 3600000,
        maxMessageLength: 10000,
    };
    ctx;
    sessions = new Map();
    messageHandler = null;
    async init(config, context) {
        this.ctx = context;
        this.config = { ...this.config, ...config };
    }
    async start() {
        // Start session cleanup interval
        setInterval(() => this.cleanupSessions(), 60000);
        this.ctx.logger.info('Webchat channel active', { port: this.config.port });
    }
    async stop() {
        this.sessions.clear();
    }
    async healthCheck() {
        return {
            healthy: true,
            details: { activeSessions: this.sessions.size },
        };
    }
    /** Register message handler */
    onMessage(handler) {
        this.messageHandler = handler;
    }
    /** Create a new chat session */
    createSession(userId) {
        const id = `ws_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
        const session = {
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
    async handleMessage(sessionId, content) {
        if (content.length > this.config.maxMessageLength) {
            return 'Message too long. Please keep messages under 10,000 characters.';
        }
        const session = this.sessions.get(sessionId);
        if (!session)
            return 'Session not found. Please refresh the page.';
        session.lastActiveAt = Date.now();
        session.history.push({ role: 'user', content, timestamp: Date.now() });
        if (!this.messageHandler) {
            return 'No message handler configured.';
        }
        try {
            const response = await this.messageHandler(sessionId, content);
            session.history.push({ role: 'assistant', content: response, timestamp: Date.now() });
            return response;
        }
        catch (err) {
            this.ctx.logger.error('Message handler error', {
                sessionId,
                error: err instanceof Error ? err.message : String(err),
            });
            return 'An error occurred processing your message.';
        }
    }
    /** Get session history */
    getHistory(sessionId) {
        return this.sessions.get(sessionId)?.history ?? [];
    }
    /** Clean up expired sessions */
    cleanupSessions() {
        const now = Date.now();
        for (const [id, session] of this.sessions) {
            if (now - session.lastActiveAt > this.config.sessionTimeoutMs) {
                this.sessions.delete(id);
            }
        }
    }
}
exports.default = new WebchatChannel();
//# sourceMappingURL=webchat.js.map