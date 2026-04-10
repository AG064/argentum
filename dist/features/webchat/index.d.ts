/**
 * WebChat Feature
 *
 * Full-featured web UI with Markdown rendering, typing indicators,
 * file upload (drag & drop), WebSocket messaging, dark/light theme,
 * and chat history persistence.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** Webchat configuration */
export interface WebchatConfig {
    enabled: boolean;
    port: number;
    maxConnections: number;
    maxMessageHistory: number;
    maxFileSize: number;
    allowedFileTypes: string[];
    uploadDir: string;
}
/** Chat message structure */
export interface ChatMessage {
    id: string;
    userId: string;
    roomId: string;
    content: string;
    role: 'user' | 'assistant' | 'system';
    timestamp: number;
    attachments?: Attachment[];
    metadata?: Record<string, unknown>;
}
/** File attachment */
export interface Attachment {
    id: string;
    filename: string;
    mimeType: string;
    size: number;
    url: string;
}
/**
 * Webchat feature — full-featured web UI with WebSocket messaging.
 *
 * Security hardening:
 * - Bearer token authentication for WebSocket connections
 * - Room ID and user ID validation (alphanumeric + safe chars only)
 * - Message content validation and length limits
 * - File type validation and size enforcement
 * - Rate limiting per connection
 * - CSP-compatible markdown rendering (no innerHTML for user content)
 * - Audit logging for connections and auth failures
 */
declare class WebchatFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private server;
    private httpServer;
    private clients;
    private messageHistory;
    private typingStates;
    private uploadedFiles;
    private config;
    private authToken;
    private ctx;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /** Handle incoming WebSocket message */
    private handleMessage;
    /** Add message to room history with cap */
    private addMessageToHistory;
    /** Broadcast to all clients in a room */
    private broadcastToRoom;
    /** Send a message as the assistant (for agent responses) */
    sendAssistantMessage(roomId: string, content: string): void;
    private generateId;
}
declare const _default: WebchatFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map