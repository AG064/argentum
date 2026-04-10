/**
 * Webchat Channel
 *
 * Web-based chat interface using Server-Sent Events (SSE) for
 * real-time streaming responses.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../core/plugin-loader';
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
    history: Array<{
        role: string;
        content: string;
        timestamp: number;
    }>;
}
/** Message handler */
export type WebchatMessageHandler = (sessionId: string, message: string) => Promise<string>;
/**
 * Webchat channel — browser-based chat with SSE streaming.
 *
 * Provides a web interface for chat interactions with session
 * management and streaming response support.
 */
declare class WebchatChannel implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private sessions;
    private messageHandler;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Register message handler */
    onMessage(handler: WebchatMessageHandler): void;
    /** Create a new chat session */
    createSession(userId: string): ChatSession;
    /** Process incoming message */
    handleMessage(sessionId: string, content: string): Promise<string>;
    /** Get session history */
    getHistory(sessionId: string): ChatSession['history'];
    /** Clean up expired sessions */
    private cleanupSessions;
}
declare const _default: WebchatChannel;
export default _default;
//# sourceMappingURL=webchat.d.ts.map