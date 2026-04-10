/**
 * Discord Bot Feature
 *
 * Discord bot integration for sending messages and listening to events.
 * Structured as a Discord.js-based bot plugin. Real bot connection not implemented.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
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
    attachments?: Array<{
        id: string;
        filename: string;
        url: string;
    }>;
    embeds?: unknown[];
    referencedMessage?: DiscordMessage;
}
/** Bot configuration */
export interface DiscordBotConfig {
    token: string;
    clientId?: string;
    guildId?: string;
    commandPrefix?: string;
    intents?: string[];
}
/** Feature configuration */
export interface DiscordBotFeatureConfig {
    dbPath?: string;
    tokenEnvVar?: string;
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
declare class DiscordBotFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private db;
    private token;
    private clientId;
    private guildId;
    private commandPrefix;
    private messageHandlers;
    private ready;
    constructor();
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /**
     * Configure the Discord bot.
     *
     * @param token - Bot token from Discord Developer Portal
     * @param clientId - Application client ID (optional)
     * @param guildId - Test guild ID for slash commands (optional)
     * @param commandPrefix - Command prefix (default !)
     */
    configure(token: string, clientId?: string, guildId?: string, commandPrefix?: string): void;
    /**
     * Check if bot is configured and ready.
     */
    isReady(): boolean;
    /**
     * Set the command prefix for text commands.
     */
    setCommandPrefix(prefix: string): void;
    /**
     * Register a handler for incoming messages.
     *
     * @param handler - Async function called for each message
     */
    onMessage(handler: (msg: DiscordMessage) => Promise<void>): void;
    /**
     * Send a message to a channel (stub).
     *
     * @param channelId - Discord channel ID
     * @param content - Message text content
     * @returns The sent message object (simulated)
     */
    sendMessage(channelId: string, content: string): Promise<DiscordMessage>;
    /**
     * Process an incoming Discord message (stub).
     * Would be called by gateway event in real implementation.
     */
    receiveMessage(raw: Partial<DiscordMessage> & {
        id: string;
        channelId: string;
        author: {
            id: string;
            username: string;
        };
    }): Promise<void>;
    /**
     * Get message history for a channel (stub).
     */
    getHistory(channelId: string, limit?: number): DiscordMessage[];
    /**
     * Get bot statistics.
     */
    getStats(): {
        configured: boolean;
        messagesSent: number;
        messagesReceived: number;
        activeHandlers: number;
    };
    /** Log sent message */
    private logSentMessage;
    /** Log received message */
    private logReceivedMessage;
    /** Initialize database and create tables */
    private initDatabase;
}
declare const _default: DiscordBotFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map